use std::ops::{Deref, DerefMut};

use serde::{de::Deserializer, de::Error as _, Deserialize, Serialize};

use super::Map;
use plugin_api::PluginStep;

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
pub enum Definition {
    Discover,
    Singleton(String),
    Shared(Vec<String>),
}

impl<'de> Deserialize<'de> for Definition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Debug)]
        #[serde(untagged)]
        enum DefinitionRaw {
            Unit(String),
            Array(Vec<String>),
        }

        let raw = DefinitionRaw::deserialize(deserializer)?;

        match raw {
            DefinitionRaw::Unit(name) => match name.as_str() {
                "discover" => Ok(Definition::Discover),
                _other => Ok(Definition::Singleton(name)),
            },
            DefinitionRaw::Array(names) => Ok(Definition::Shared(names)),
        }
    }
}

/// Map [PluginStep](crate::plugin::PluginStep) -> [PluginStep](self::Definition)
#[derive(Serialize, Debug, Clone, Eq, PartialEq, Default)]
pub struct DefinitionMap(Map<PluginStep, Definition>);

impl<'de> Deserialize<'de> for DefinitionMap {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use std::str::FromStr;
        let raw_map: Map<String, Definition> = Deserialize::deserialize(de)?;
        let mut map = Map::new();

        for (key, value) in raw_map {
            let key = PluginStep::from_str(&key).map_err(D::Error::custom)?;
            map.insert(key, value);
        }

        Ok(DefinitionMap(map))
    }
}

impl Deref for DefinitionMap {
    type Target = Map<PluginStep, Definition>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DefinitionMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plugin_api::PluginStep;

    #[test]
    fn parse_step() {
        let toml = r#"pre_flight = ["git", "github", "rust"]"#;
        let expected_list = ["git", "github", "rust"]
            .iter()
            .map(|&s| String::from(s))
            .collect::<Vec<_>>();
        let expected = Definition::Shared(expected_list);
        let mut expected_map = Map::new();
        expected_map.insert(PluginStep::PreFlight, expected);
        let parsed: DefinitionMap = toml::from_str(toml).unwrap();
        assert_eq!(*parsed, expected_map);
    }

    #[test]
    fn parse_step_discover() {
        let toml = r#"pre_flight = "discover""#;
        let expected = Definition::Discover;
        let mut expected_map = Map::new();
        expected_map.insert(PluginStep::PreFlight, expected);
        let parsed: DefinitionMap = toml::from_str(toml).unwrap();
        assert_eq!(*parsed, expected_map);
    }

    #[test]
    #[should_panic]
    fn parse_step_invalid_key() {
        let toml = r#"invalid = "discover""#;
        let _parsed: DefinitionMap = toml::from_str(toml).unwrap();
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
        let singleton = |s: &str| Definition::Singleton(owned(s));
        let plugins = |s: &[&str]| Definition::Shared(s.iter().map(|&s| owned(s)).collect());

        let expected = [
            (PluginStep::PreFlight, plugins(&["git", "github", "rust"])),
            (PluginStep::GetLastRelease, singleton("git")),
            (PluginStep::DeriveNextVersion, plugins(&["clog"])),
            (PluginStep::GenerateNotes, Definition::Discover),
            (PluginStep::Prepare, plugins(&["rust"])),
            (PluginStep::VerifyRelease, plugins(&["rust"])),
            (PluginStep::Commit, singleton("git")),
            (PluginStep::Publish, plugins(&["github"])),
            (PluginStep::Notify, Definition::Discover),
        ]
        .iter()
        .cloned()
        .collect();

        let expected = DefinitionMap(expected);

        let parsed: DefinitionMap = toml::from_str(toml).unwrap();

        assert_eq!(parsed, expected);
    }
}
