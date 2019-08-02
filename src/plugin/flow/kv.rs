use std::mem;

use serde::{Deserialize, Serialize};

use super::{ProvisionRequest, Scope};
use crate::plugin::PluginStep;

pub type Key = String;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyValue<T> {
    /// Whether user can override this value in releaserc.toml
    pub protected: bool,
    pub state: KeyValueState<T>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum KeyValueState<T> {
    NeedsProvision(ProvisionRequest),
    UserDefined(Key),
    Ready(Key, T),
}

impl<T> KeyValue<T> {
    pub fn builder(key: &str) -> KeyValueBuilder<T> {
        KeyValueBuilder::new(key)
    }

    pub fn as_value(&self) -> &T {
        match &self.state {
            KeyValueState::Ready(_, v) => v,
            KeyValueState::UserDefined(key) =>
                panic!("Key {:?} is required to be user-defined in releaserc.toml, but it is not.\n\
                        This is a data flow manager bug, please consider opening an issue at https://github.com/etclabscore/semantic-rs/issues/new", key),
            KeyValueState::NeedsProvision(pr) =>
                panic!("Value was requested, but haven't yet been provisioned (request: {:?}). \n \
                        This is a data flow manager bug, please consider opening an issue at https://github.com/etclabscore/semantic-rs/issues/new", pr),
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
                state: KeyValueState::Ready(key, value),
            }
        } else if self.user_defined {
            KeyValue {
                protected: false,
                state: KeyValueState::UserDefined(key),
            }
        } else {
            KeyValue {
                protected: self.protected,
                state: KeyValueState::NeedsProvision(ProvisionRequest {
                    scope: std::mem::replace(&mut self.scope, Scope::Global),
                    required_at: self.required_at.take(),
                    key,
                }),
            }
        }
    }
}

impl<T: Default> KeyValueBuilder<T> {
    pub fn default_value(&mut self) -> &mut Self {
        self.value = Some(Default::default());
        self
    }
}
