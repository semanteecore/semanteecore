pub mod plugin_def;
pub mod step_def;
pub mod value_def;

pub use self::plugin_def::{PluginDefinition, PluginDefinitionMap};
pub use self::step_def::{StepDefinition, StepsDefinitionMap};
pub use self::value_def::{ValueDefinition, ValueDefinitionMap};

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use failure::Fail;
use linked_hash_map::LinkedHashMap;
use serde::Deserialize;

/// Map type override used in configs
///
/// LinkedHashMap is used 'cause it preserves original declaration order
/// from the configuration file
pub type Map<K, V> = LinkedHashMap<K, V>;

/// Workspace definition table
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Workspace {
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub members: Vec<PathBuf>,
    #[serde(default)]
    pub ignore: Vec<String>,
}

/// Base structure to parse `releaserc.toml` into
#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default)]
    pub cfg: ValueDefinitionMap,
    #[serde(default)]
    pub workspace: Option<Workspace>,
    #[serde(default)]
    pub plugins: PluginDefinitionMap,
    #[serde(default)]
    pub steps: StepsDefinitionMap,
}

impl Config {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, failure::Error> {
        let config_path = path.as_ref();
        let mut file = File::open(config_path).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => Error::FileNotFound.into(),
            _other => failure::Error::from(err),
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let contents = contents.trim();

        toml::from_str(contents).map_err(failure::Error::from)
    }
}

#[derive(Fail, Debug, PartialEq, Eq)]
pub enum Error {
    #[fail(display = "releaserc.toml not found in the project root")]
    FileNotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parse_layer_config() {
        let toml = r#"
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
        let filepath = concat!(env!("CARGO_MANIFEST_DIR"), "/../releaserc.toml");
        eprintln!("filepath: {}", filepath);
        Config::from_path(filepath).unwrap();
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
        namespace = "semanteecore"
        dockerfile = ".docker/Dockerfile"
        name = "semanteecore"
        tag = "latest"
        binary_path = "target/release/semanteecore"
        cleanup = true
        build_cmd = "from:language:build_cmd"
        exec_cmd = "/bin/semanteecore"
        "#;

        let parsed: Config = toml::from_str(toml).unwrap();

        drop(parsed)
    }
}
