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
pub enum StepDefinition {
    Discover,
    Singleton(String),
    Shared(Vec<String>),
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

/// Map [PluginStep](crate::plugin::PluginStep) -> [PluginStep](self::StepDefinition)
#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
pub struct StepsDefinitionMap(Map<PluginStep, StepDefinition>);

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
        let _parsed: StepsDefinitionMap = toml::from_str(toml).unwrap();
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
}
