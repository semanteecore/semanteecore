use serde::{Deserialize, Serialize};

use super::Map;
use crate::runtime::plugin::UnresolvedPlugin;

/// Map PluginName -> PluginDefinition
pub type PluginDefinitionMap = Map<String, PluginDefinition>;

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
    use crate::runtime::plugin::UnresolvedPlugin;
    use plugin_api::PluginStep;

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
        let _full = short.into_full();
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
}
