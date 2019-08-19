use std::fs::File;
use std::io::Read;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use failure::Fail;
use linked_hash_map::LinkedHashMap;
use serde::{de::Deserializer, de::Error as _, Deserialize, Serialize};

use crate::plugin_support::flow::kv::{ValueDefinition, ValueDefinitionMap};
use crate::plugin_support::{PluginStep, PluginStepKind, UnresolvedPlugin};

/// Map type override used in configs
///
/// LinkedHashMap is used 'cause it preserves original declaration order
/// from the configuration file
pub type Map<K, V> = LinkedHashMap<K, V>;

/// Map PluginName -> PluginDefinition
pub type PluginDefinitionMap = Map<String, PluginDefinition>;

/// Map [PluginStep](crate::plugin::PluginStep) -> [PluginStep](self::StepDefinition)
#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
pub struct StepsDefinitionMap(Map<PluginStep, StepDefinition>);

/// Base structure to parse `releaserc.toml` into
#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub plugins: PluginDefinitionMap,
    pub steps: StepsDefinitionMap,
    #[serde(default)]
    pub cfg: ValueDefinitionMap,
}

fn default_project_root() -> ValueDefinition {
    let root = PathBuf::from("./");
    let root = root.canonicalize().ok();
    let root = root.and_then(|p| p.to_str().map(String::from));

    if let Some(path) = root {
        ValueDefinition::Value(serde_json::Value::String(path))
    } else {
        panic!(
            "failed to derive project_root from environment, please set cfg.project_root manually in releaserc.toml"
        );
    }
}

fn default_dry_run() -> ValueDefinition {
    ValueDefinition::Value(serde_json::Value::Bool(false))
}

impl Config {
    pub fn from_toml<P: AsRef<Path>>(path: P, is_dry_run: bool) -> Result<Self, failure::Error> {
        let mut file = File::open(path).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => ConfigError::FileNotFound.into(),
            _other => failure::Error::from(err),
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let contents = contents.trim();
        let mut config: Config = toml::from_str(contents)?;

        config.check_step_arguments_correctness()?;

        config.cfg.entry("dry_run".to_owned()).or_insert_with(|| {
            if is_dry_run {
                ValueDefinition::Value(is_dry_run.into())
            } else {
                default_dry_run()
            }
        });

        config
            .cfg
            .entry("project_root".into())
            .or_insert_with(default_project_root);

        Ok(config)
    }

    fn check_step_arguments_correctness(&self) -> Result<(), failure::Error> {
        for (step, def) in self.steps.iter() {
            match def {
                // If step is defined as singleton in the config,
                // as that's the most permissive kind,
                // we can use it for both singleton and shared steps
                StepDefinition::Singleton(_) => (),
                StepDefinition::Shared(_) | StepDefinition::Discover => match step.kind() {
                    PluginStepKind::Shared => (),
                    PluginStepKind::Singleton => Err(ConfigError::WrongStepKind {
                        expected: PluginStepKind::Singleton,
                        got: PluginStepKind::Shared,
                    })?,
                },
            }
        }
        Ok(())
    }
}

#[derive(Fail, Debug)]
pub enum ConfigError {
    #[fail(display = "releaserc.toml not found in the project root")]
    FileNotFound,
    #[fail(display = "step defined as {:?}, expected {:?}", got, expected)]
    WrongStepKind {
        expected: PluginStepKind,
        got: PluginStepKind,
    },
    #[fail(display = "project root path is not set")]
    MissingProjectRootPath,
    #[fail(display = "expected a table for key {}, found {}", _0, _1)]
    PluginConfigIsNotTable(String, String),
    #[fail(display = "dry run flag is not set")]
    MissingDryRunFlag,
    #[fail(display = "changelog path is undefined")]
    MissingChangelogPath,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
/// Plugin definition may be defined as a fully-qualified configuration as [UnresolvedPlugin](crate::plugin::UnresolvedPlugin)
/// or as a short alias, defining the source where the plugin may be resolved from (builtin/crates/npm...)
///
/// In case of using the short definition, the fully-qualified definition would be derived automatically (and possibly incorrectly)
pub enum PluginDefinition {
    Full(UnresolvedPlugin),
    Short(String),
}

/// Step definition variants
///
///  - Singletone (only one plugin allowed to fill the step)
///  - Multiple plugins in a sequence
///  - Discover (use automatic discovery mechanism and use this plugin for every method it implements)
///
/// The sequence of plugin execution in case of `discovery` would be defined by
/// the sequence of plugin definitions in the `plugins` table.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepDefinition {
    Discover,
    Singleton(String),
    Shared(Vec<String>),
}

impl<'de> Deserialize<'de> for StepsDefinitionMap {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use std::str::FromStr;
        let raw_map: Map<String, StepDefinition> = Deserialize::deserialize(de)?;
        let mut map = Map::new();

        for (key, value) in raw_map {
            let key = PluginStep::from_str(&key).map_err(D::Error::custom)?;
            map.insert(key, value);
        }

        Ok(StepsDefinitionMap(map))
    }
}

impl Deref for StepsDefinitionMap {
    type Target = Map<PluginStep, StepDefinition>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StepsDefinitionMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de> Deserialize<'de> for StepDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Debug)]
        #[serde(untagged)]
        enum StepDefinitionRaw {
            Unit(String),
            Array(Vec<String>),
        }

        let raw = StepDefinitionRaw::deserialize(deserializer)?;

        match raw {
            StepDefinitionRaw::Unit(name) => match name.as_str() {
                "discover" => Ok(StepDefinition::Discover),
                _other => Ok(StepDefinition::Singleton(name)),
            },
            StepDefinitionRaw::Array(names) => Ok(StepDefinition::Shared(names)),
        }
    }
}

impl PluginDefinition {
    pub fn into_full(self) -> UnresolvedPlugin {
        match self {
            PluginDefinition::Full(full) => full,
            PluginDefinition::Short(short) => match short.as_str() {
                "builtin" => UnresolvedPlugin::Builtin,
                other => panic!("unknown short plugin alias: '{}'", other),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_builtin_plugin_full_definition() {
        let toml = "name = { location = \"builtin\" }";
        let parsed: PluginDefinitionMap = toml::from_str(toml).unwrap();

        let plugin = parsed.get("name").expect("plugin 'name' not found in parsed map");

        assert_eq!(&PluginDefinition::Full(UnresolvedPlugin::Builtin), plugin);
    }

    #[test]
    fn parse_builtin_plugin_short_definition() {
        let toml = "name = \"builtin\"";
        let parsed: PluginDefinitionMap = toml::from_str(toml).unwrap();

        let plugin = parsed.get("name").expect("plugin 'name' not found in parsed map");

        assert_eq!(&PluginDefinition::Short("builtin".into()), plugin);
    }

    #[test]
    fn plugin_definition_builtin_into_full() {
        let short = PluginDefinition::Short("builtin".into());
        let full = short.into_full();
        assert_eq!(UnresolvedPlugin::Builtin, full);
    }

    #[test]
    #[should_panic]
    fn plugin_definition_invalid_into_full() {
        let short = PluginDefinition::Short("invalid".into());
        let full = short.into_full();
    }

    #[test]
    fn plugin_list_builtin_full() {
        let toml = r#"
            git = { location = "builtin" }
            clog = { location = "builtin" }
            github = { location = "builtin" }
            rust = { location = "builtin" }
        "#;

        let expected: PluginDefinitionMap = vec![
            ("git", UnresolvedPlugin::Builtin),
            ("clog", UnresolvedPlugin::Builtin),
            ("github", UnresolvedPlugin::Builtin),
            ("rust", UnresolvedPlugin::Builtin),
        ]
        .into_iter()
        .map(|(name, state)| (name.to_string(), PluginDefinition::Full(state)))
        .collect();

        let parsed: PluginDefinitionMap = toml::from_str(toml).unwrap();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn plugin_list_builtin_short() {
        let toml = r#"
            git = "builtin"
            clog = "builtin"
            github = "builtin"
            rust = "builtin"
        "#;

        let expected: PluginDefinitionMap = ["git", "clog", "github", "rust"]
            .into_iter()
            .map(|name| (name.to_string(), PluginDefinition::Short("builtin".to_string())))
            .collect();

        let parsed: PluginDefinitionMap = toml::from_str(toml).unwrap();

        assert_eq!(parsed, expected);
    }

    #[test]
    // NOTE: will fail without the `preserve_order` feature of `toml`
    //       or with Map being not LinkedHashMap
    fn plugin_order_stabilify() {
        let toml = r#"
            git = "builtin"
            clog = "builtin"
            github = "builtin"
            rust = "builtin"
        "#;

        let expected = &["git", "clog", "github", "rust"];

        let parsed: PluginDefinitionMap = toml::from_str(toml).unwrap();

        let parsed_keys: Vec<&str> = parsed.keys().map(String::as_str).collect();

        assert_eq!(&parsed_keys[..], expected);
    }

    #[test]
    fn parse_step() {
        let toml = r#"pre_flight = ["git", "github", "rust"]"#;
        let expected_list = ["git", "github", "rust"]
            .iter()
            .map(|&s| String::from(s))
            .collect::<Vec<_>>();
        let expected = StepDefinition::Shared(expected_list);
        let mut expected_map = Map::new();
        expected_map.insert(PluginStep::PreFlight, expected);
        let parsed: StepsDefinitionMap = toml::from_str(toml).unwrap();
        assert_eq!(*parsed, expected_map);
    }

    #[test]
    fn parse_step_discover() {
        let toml = r#"pre_flight = "discover""#;
        let expected = StepDefinition::Discover;
        let mut expected_map = Map::new();
        expected_map.insert(PluginStep::PreFlight, expected);
        let parsed: StepsDefinitionMap = toml::from_str(toml).unwrap();
        assert_eq!(*parsed, expected_map);
    }

    #[test]
    #[should_panic]
    fn parse_step_invalid_key() {
        let toml = r#"invalid = "discover""#;
        let parsed: StepsDefinitionMap = toml::from_str(toml).unwrap();
    }

    #[test]
    fn parse_step_map() {
        let toml = r#"
            pre_flight = ["git", "github", "rust"]
            get_last_release = "git"
            derive_next_version = [ "clog" ]
            generate_notes = "discover"
            prepare = ["rust"]
            verify_release = ["rust"]
            commit = "git"
            publish = [ "github" ]
            notify = "discover"
        "#;

        let owned = |s: &str| s.to_owned();
        let singleton = |s: &str| StepDefinition::Singleton(owned(s));
        let plugins = |s: &[&str]| StepDefinition::Shared(s.iter().map(|&s| owned(s)).collect());

        let expected = [
            (PluginStep::PreFlight, plugins(&["git", "github", "rust"])),
            (PluginStep::GetLastRelease, singleton("git")),
            (PluginStep::DeriveNextVersion, plugins(&["clog"])),
            (PluginStep::GenerateNotes, StepDefinition::Discover),
            (PluginStep::Prepare, plugins(&["rust"])),
            (PluginStep::VerifyRelease, plugins(&["rust"])),
            (PluginStep::Commit, singleton("git")),
            (PluginStep::Publish, plugins(&["github"])),
            (PluginStep::Notify, StepDefinition::Discover),
        ]
        .iter()
        .cloned()
        .collect();

        let expected = StepsDefinitionMap(expected);

        let parsed: StepsDefinitionMap = toml::from_str(toml).unwrap();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_global_cfg_section() {
        let toml = r#"
            [cfg]
            one = 1
            two = 2
        "#;

        #[derive(Deserialize, Debug)]
        struct Global {
            cfg: Map<String, toml::Value>,
        }

        let mut expected = Map::new();
        expected.insert("one".into(), toml::Value::Integer(1));
        expected.insert("two".into(), toml::Value::Integer(2));

        let parsed: Global = toml::from_str(toml).unwrap();

        assert_eq!(parsed.cfg, expected);
    }

    #[test]
    fn parse_plugin_cfg_section() {
        use toml::map::Map;

        let toml = r#"
            [cfg.git]
            three = 3
            four = 4
        "#;

        #[derive(Deserialize, Debug)]
        struct Global {
            cfg: Map<String, toml::Value>,
        }

        let mut expected = Map::new();
        expected.insert("three".into(), toml::Value::Integer(3));
        expected.insert("four".into(), toml::Value::Integer(4));

        let parsed: Global = toml::from_str(toml).unwrap();
        let parsed_git = parsed
            .cfg
            .get("git")
            .expect("no 'git' in 'cfg' section")
            .as_table()
            .expect("'git' is not a table");

        assert_eq!(parsed_git, &expected);
    }

    #[test]
    fn parse_full_config() {
        let toml = r#"
            [plugins]
            # Fully qualified definition
            git = { location = "builtin" }
            # Short definition
            clog = "builtin"
            github = "builtin"
            rust = "builtin"

            [steps]
            # Shared step
            pre_flight = ["git", "github", "rust"]
            # Singleton step
            get_last_release = "git"
            # Analyze the changes and derive the appropriate version bump
            # In case of different results, the most major would be taken
            derive_next_version = [ "clog" ]
            # Notes from each step would be appended to the notes of previous one
            # `discover` is a reserved keyword for deriving the step runners through OpenRPC Service Discovery
            # the succession of runs in this case will be determined by the succession in the `plugins` list
            generate_notes = "discover"
            # Prepare the release (pre-release step for intermediate artifacts generation)
            prepare = ["rust"]
            # Check the release before publishing
            verify_release = ["rust"]
            # Commit & push changes to the VCS
            commit = "git"
            # Publish to various platforms
            publish = [ "github" ]
            # Post-release step to notify users about release (e.g leave comments in issues resolved in this release)
            notify = "discover"

            [cfg]
            # Global configuration

            [cfg.git]
            # Per-plugin configuration
            user_name = "Mike Lubinets"
            user_email = "me@mkl.dev"
            branch = "master"
            force_https = true

            [cfg.github]
            assets = [
                "bin/*",
                "Changelog.md"
            ]
        "#;

        let parsed: Config = toml::from_str(toml).unwrap();

        drop(parsed)
    }

    #[test]
    fn read_full_config_from_file() {
        let filepath = concat!(env!("CARGO_MANIFEST_DIR"), "/releaserc.toml");
        eprintln!("filepath: {}", filepath);
        Config::from_toml(filepath, true).unwrap();
    }

    #[test]
    fn parse_full_config_with_data_flow_queries() {
        let toml = r#"
        [plugins]
        # Fully qualified definition
        git = { location = "builtin" }
        # Short definition
        clog = "builtin"
        #github = "builtin"
        #rust = "builtin"
        #docker = "builtin"

        [steps]
        # Shared step
        pre_flight = "discover"
        # Singleton step
        get_last_release = "git"
        # Analyze the changes and derive the appropriate version bump
        # In case of different results, the most major would be taken
        derive_next_version = [ "clog" ]
        # Notes from each step would be appended to the notes of previous one
        # `discover` is a reserved keyword for deriving the step runners through OpenRPC Service Discovery
        # the succession of runs in this case will be determined by the succession in the `plugins` list
        generate_notes = "clog"
        # Prepare the release (pre-release step for intermediate artifacts generation)
        prepare = "discover"
        # Check the release before publishing
        verify_release = "discover"
        # Commit & push changes to the VCS
        commit = "git"
        # Publish to various platforms
        publish = []
        # Post-release step to notify users about release (e.g leave comments in issues resolved in this release)
        notify = "discover"

        [cfg]
        # Global configuration

        [cfg.clog]
        # Ignore commits like feat(ci) cause it makes no sense to issue a release for improvements in CI config
        ignore = ["ci"]

        [cfg.git]
        # Per-plugin configuration
        user_name = "Mike Lubinets"
        user_email = "me@mkl.dev"
        branch = "master"
        force_https = true

        [cfg.github]
        assets = [
            "/workspace/bin/*",
            "Changelog.md"
        ]

        [cfg.docker]
        repo_url = "from:vcs:git_clone_url"
        repo_branch = "from:vcs:git_branch"

        [[cfg.docker.images]]
        registry = "dockerhub"
        namespace = "etclabscore"
        dockerfile = ".docker/Dockerfile"
        name = "semantic-rs"
        tag = "latest"
        binary_path = "target/release/semantic-rs"
        cleanup = true
        build_cmd = "from:language:build_cmd"
        exec_cmd = "/bin/semantic-rs"
        "#;

        let parsed: Config = toml::from_str(toml).unwrap();

        drop(parsed)
    }
}
