use super::{ProvisionRequest, Scope};
use crate::config::Map;
use crate::plugin_support::PluginStep;
use pest::Parser;
use serde::{
    de::{DeserializeOwned, Error as _},
    Deserialize, Deserializer, Serialize,
};
use std::io::{BufWriter, Cursor};
use std::mem;

pub type Key = String;

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct KeyValue<T> {
    /// Whether user can override this value in releaserc.toml
    #[serde(default)]
    pub protected: bool,
    pub key: Key,
    pub state: KeyValueState<T>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum KeyValueState<T> {
    NeedsProvision(ProvisionRequest),
    UserDefined,
    Ready(T),
}

impl<T> KeyValue<T> {
    pub fn builder(key: &str) -> KeyValueBuilder<T> {
        KeyValueBuilder::new(key)
    }

    pub fn as_value(&self) -> &T {
        match &self.state {
            KeyValueState::Ready(v) => v,
            KeyValueState::UserDefined =>
                panic!("Key {:?} is required to be user-defined in releaserc.toml, but it is not.\n\
                        This is a data flow manager bug, please consider opening an issue at https://github.com/etclabscore/semantic-rs/issues/new", self.key),
            KeyValueState::NeedsProvision(pr) =>
                panic!("Value for key {:?} was requested, but haven't yet been provisioned (request: {:?}). \n \
                        This is a data flow manager bug, please consider opening an issue at https://github.com/etclabscore/semantic-rs/issues/new", self.key, pr),
        }
    }

    pub fn as_value_mut(&mut self) -> &mut T {
        match &mut self.state {
            KeyValueState::Ready(v) => v,
            KeyValueState::UserDefined =>
                panic!("Key {:?} is required to be user-defined in releaserc.toml, but it is not.\n\
                        This is a data flow manager bug, please consider opening an issue at https://github.com/etclabscore/semantic-rs/issues/new", self.key),
            KeyValueState::NeedsProvision(pr) =>
                panic!("Value for key {:?} was requested, but haven't yet been provisioned (request: {:?}). \n \
                        This is a data flow manager bug, please consider opening an issue at https://github.com/etclabscore/semantic-rs/issues/new", self.key, pr),
        }
    }
}

pub struct KeyValueBuilder<T> {
    protected: bool,
    user_defined: bool,
    scope: Scope,
    key: String,
    value: Option<T>,
    required_at: Option<PluginStep>,
}

impl<T> KeyValueBuilder<T> {
    pub fn new(key: &str) -> Self {
        KeyValueBuilder {
            protected: false,
            user_defined: false,
            scope: Scope::Global,
            key: key.to_owned(),
            value: None,
            required_at: None,
        }
    }

    pub fn protected(&mut self) -> &mut Self {
        if self.user_defined {
            panic!("Key definition cannot be protected and user defined at the same time: protected means that user cannot override the key");
        }
        self.protected = true;
        self
    }

    pub fn user_defined(&mut self) -> &mut Self {
        if self.protected {
            panic!("Key definition cannot be protected and user defined at the same time: protected means that user cannot override the key");
        }
        self.user_defined = true;
        self
    }

    pub fn scope(&mut self, scope: Scope) -> &mut Self {
        self.scope = scope;
        self
    }

    pub fn value(&mut self, value: T) -> &mut Self {
        self.value = Some(value);
        self
    }

    pub fn required_at(&mut self, step: PluginStep) -> &mut Self {
        self.required_at = Some(step);
        self
    }

    pub fn build(&mut self) -> KeyValue<T> {
        let key = mem::replace(&mut self.key, String::new());

        if let Some(value) = self.value.take() {
            KeyValue {
                protected: self.protected,
                key,
                state: KeyValueState::Ready(value),
            }
        } else if self.user_defined {
            KeyValue {
                protected: false,
                key,
                state: KeyValueState::UserDefined,
            }
        } else {
            KeyValue {
                protected: self.protected,
                key: key.clone(),
                state: KeyValueState::NeedsProvision(ProvisionRequest {
                    scope: std::mem::replace(&mut self.scope, Scope::Global),
                    required_at: self.required_at.take(),
                    key,
                }),
            }
        }
    }
}

struct KeyValueDefinitionMap(Map<String, ValueDefinition>);

impl Into<Map<String, KeyValue<serde_json::Value>>> for KeyValueDefinitionMap {
    fn into(self) -> Map<String, KeyValue<serde_json::Value>> {
        let mut map = Map::new();
        for (key, value) in self.0 {
            let kv = match value {
                ValueDefinition::Value(v) => KeyValue::builder(&key).value(v).build(),
                ValueDefinition::From {
                    scope,
                    required_at,
                    key,
                } => {
                    let mut kv = KeyValue::builder(&key);
                    if let Some(step) = required_at {
                        kv.required_at(step);
                    }
                    kv.scope(scope).build()
                }
            };
            map.insert(key, kv);
        }
        map
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ValueDefinition {
    From {
        scope: Scope,
        required_at: Option<PluginStep>,
        key: String,
    },
    Value(serde_json::Value),
}

impl<'de> Deserialize<'de> for KeyValueDefinitionMap {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use std::str::FromStr;
        let raw_map: Map<String, serde_json::Value> = Deserialize::deserialize(de)?;
        let mut map = Map::new();

        for (key, value) in raw_map {
            if let Some(value) = value.as_str() {
                let parsed = parse_value_definition(value).map_err(D::Error::custom)?;
                map.insert(key, parsed);
            } else {
                map.insert(key, ValueDefinition::Value(value));
            }
        }

        Ok(KeyValueDefinitionMap(map))
    }
}

#[derive(Parser)]
#[grammar = "../grammar/dataflow.pest"]
struct ValueDefinitionParser;

fn parse_value_definition(value: &str) -> Result<ValueDefinition, failure::Error> {
    use std::str::FromStr;

    let pairs = ValueDefinitionParser::parse(Rule::value_def, value)
        .map_err(|e| failure::err_msg(format!("{}", e)))?
        .next()
        .unwrap();

    let mut scope = Scope::Global;
    let mut required_at = None;
    let mut key = String::new();

    for pair in pairs.into_inner() {
        let pair = dbg!(pair);
        match pair.as_rule() {
            Rule::value => {
                return Ok(ValueDefinition::Value(serde_json::Value::String(
                    pair.as_str().into(),
                )))
            }
            Rule::scope => {
                scope = Scope::from_str(pair.as_str())?;
            }
            Rule::required_at_step => {
                required_at = Some(PluginStep::from_str(pair.as_str())?);
            }
            Rule::key => {
                key = pair.as_str().into();
            }
            _ => (),
        }
    }

    Ok(ValueDefinition::From {
        scope,
        required_at,
        key,
    })
}

impl<T: Default> KeyValueBuilder<T> {
    pub fn default_value(&mut self) -> &mut Self {
        self.value = Some(Default::default());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Display;

    #[test]
    fn build() {
        let kv: KeyValue<()> = KeyValue::builder("key").build();
        assert_eq!(kv.protected, false);
        assert_eq!(kv.key, "key");
        assert_eq!(
            kv.state,
            KeyValueState::NeedsProvision(ProvisionRequest {
                scope: Scope::Global,
                required_at: None,
                key: "key".to_string()
            })
        );
    }

    #[test]
    fn build_protected() {
        let kv: KeyValue<()> = KeyValue::builder("key").protected().build();
        assert_eq!(kv.protected, true);
        assert_eq!(kv.key, "key");
        assert_eq!(
            kv.state,
            KeyValueState::NeedsProvision(ProvisionRequest {
                scope: Scope::Global,
                required_at: None,
                key: "key".to_string()
            })
        );
    }

    #[test]
    fn build_scoped() {
        let kv: KeyValue<()> = KeyValue::builder("key").scope(Scope::Analysis).build();
        assert_eq!(kv.protected, false);
        assert_eq!(kv.key, "key");
        assert_eq!(
            kv.state,
            KeyValueState::NeedsProvision(ProvisionRequest {
                scope: Scope::Analysis,
                required_at: None,
                key: "key".to_string()
            })
        );
    }

    #[test]
    fn build_required_at() {
        let kv: KeyValue<()> = KeyValue::builder("key")
            .required_at(PluginStep::Commit)
            .build();
        assert_eq!(kv.protected, false);
        assert_eq!(kv.key, "key");
        assert_eq!(
            kv.state,
            KeyValueState::NeedsProvision(ProvisionRequest {
                scope: Scope::Global,
                required_at: Some(PluginStep::Commit),
                key: "key".to_string()
            })
        );
    }

    #[test]
    fn build_ready_default_value() {
        let kv: KeyValue<bool> = KeyValue::builder("key").default_value().build();
        assert_eq!(kv.protected, false);
        assert_eq!(kv.key, "key");
        assert_eq!(kv.state, KeyValueState::Ready(false));
    }

    #[test]
    fn build_ready_custom_value() {
        let kv = KeyValue::builder("key").value("value").build();
        assert_eq!(kv.protected, false);
        assert_eq!(kv.key, "key");
        assert_eq!(kv.state, KeyValueState::Ready("value"));
    }

    #[test]
    fn build_user_defined() {
        let kv: KeyValue<()> = KeyValue::builder("key").user_defined().build();
        assert_eq!(kv.protected, false);
        assert_eq!(kv.key, "key".to_string());
        assert_eq!(kv.state, KeyValueState::UserDefined);
    }

    #[test]
    #[should_panic]
    fn build_user_defined_and_protected() {
        let kv: KeyValue<()> = KeyValue::builder("key").user_defined().protected().build();
    }

    #[test]
    fn as_value() {
        let kv = KeyValue::builder("key").value("value").build();
        kv.as_value();
    }

    #[test]
    #[should_panic]
    fn as_value_user_defined() {
        let kv: KeyValue<()> = KeyValue::builder("key").user_defined().build();
        kv.as_value();
    }

    #[test]
    #[should_panic]
    fn as_value_needs_provision() {
        let kv: KeyValue<()> = KeyValue::builder("key").build();
        kv.as_value();
    }

    #[test]
    fn as_value_mut() {
        let mut kv = KeyValue::builder("key").value("value").build();
        kv.as_value_mut();
    }

    #[test]
    #[should_panic]
    fn as_value_mut_user_defined() {
        let mut kv: KeyValue<()> = KeyValue::builder("key").user_defined().build();
        kv.as_value_mut();
    }

    #[test]
    #[should_panic]
    fn as_value_mut_needs_provision() {
        let mut kv: KeyValue<()> = KeyValue::builder("key").build();
        kv.as_value_mut();
    }

    #[test]
    fn serialize_deserialize_ready() {
        let kv = KeyValue {
            protected: false,
            key: "key".into(),
            state: KeyValueState::Ready("value"),
        };

        let serialized = serde_json::to_string(&kv).unwrap();
        let deserialized: KeyValue<&str> = serde_json::from_str(&serialized).unwrap();

        assert_eq!(kv, deserialized);
    }

    fn pretty_print_error_and_panic(err: impl Display) {
        eprintln!("{}", err);
        panic!("test failed");
    }

    #[test]
    fn parse_value_definition_value() {
        let v: ValueDefinition = parse_value_definition(r#"false"#)
            .map_err(pretty_print_error_and_panic)
            .unwrap();

        assert_eq!(
            v,
            ValueDefinition::Value(serde_json::Value::String("false".into()))
        );
    }

    #[test]
    fn parse_value_definition_from_key() {
        let v: ValueDefinition = parse_value_definition(r#"from:key"#)
            .map_err(pretty_print_error_and_panic)
            .unwrap();

        assert_eq!(
            v,
            ValueDefinition::From {
                scope: Scope::Global,
                required_at: None,
                key: "key".into()
            }
        );
    }

    #[test]
    fn parse_value_definition_from_key_with_scope() {
        let v: ValueDefinition = parse_value_definition(r#"from:vcs:key"#)
            .map_err(pretty_print_error_and_panic)
            .unwrap();

        assert_eq!(
            v,
            ValueDefinition::From {
                scope: Scope::VCS,
                required_at: None,
                key: "key".into()
            }
        );
    }

    #[test]
    fn parse_value_definition_from_full() {
        let v: ValueDefinition = parse_value_definition(r#"from:vcs:required_at=commit:key"#)
            .map_err(pretty_print_error_and_panic)
            .unwrap();

        assert_eq!(
            v,
            ValueDefinition::From {
                scope: Scope::VCS,
                required_at: Some(PluginStep::Commit),
                key: "key".into()
            }
        );
    }

    #[test]
    fn deserialize_value_definition_string() {
        let toml = r#"key = "false""#;
        let kvmap: KeyValueDefinitionMap = toml::from_str(toml).unwrap();
        assert_eq!(kvmap.0.len(), 1);
        let v = kvmap.0.values().next().unwrap();

        assert_eq!(
            v,
            &ValueDefinition::Value(serde_json::Value::String("false".into()))
        );
    }

    #[test]
    fn deserialize_value_definition_not_string() {
        let toml = r#"key = false"#;
        let kvmap: KeyValueDefinitionMap = toml::from_str(toml).unwrap();
        assert_eq!(kvmap.0.len(), 1);
        let v = kvmap.0.values().next().unwrap();

        assert_eq!(v, &ValueDefinition::Value(serde_json::Value::Bool(false)));
    }

    #[test]
    fn deserialize_value_definition_complex_value() {
        #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
        struct Value {
            one: i32,
            two: bool,
            three: String,
            four: Vec<u32>,
        }

        let value = Value {
            one: 1,
            two: true,
            three: "three".to_owned(),
            four: vec![1, 2, 3, 4],
        };

        let value_toml = r#"key = { one = 1, two = true, three = "three", four = [1, 2, 3, 4] }"#;

        let kvmap: KeyValueDefinitionMap = toml::from_str(value_toml).unwrap();
        assert_eq!(kvmap.0.len(), 1);
        let v = kvmap.0.values().next().unwrap();

        let parsed: Value = match v {
            ValueDefinition::From { .. } => panic!("expected Value, got From"),
            ValueDefinition::Value(value) => serde_json::from_value(value.clone()).unwrap(),
        };

        assert_eq!(value, parsed);
    }
}
