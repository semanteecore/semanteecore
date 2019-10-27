use serde::{Deserialize, Serialize};
use std::mem;

use super::ProvisionRequest;
use crate::PluginStep;

pub type Key = String;

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Value<T> {
    /// Whether user can override this value in releaserc.toml
    #[serde(default)]
    pub protected: bool,
    pub key: Key,
    pub state: ValueState<T>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum ValueState<T> {
    NeedsProvision(ProvisionRequest),
    // Data is available (either provisioned or defined in releaserc.toml)
    Ready(T),
}

impl<T> Value<T> {
    pub fn builder(key: &str) -> ValueBuilder<T> {
        ValueBuilder::new(key)
    }

    pub fn as_value(&self) -> &T {
        match &self.state {
            ValueState::Ready(v) => v,
            ValueState::NeedsProvision(pr) =>
                panic!("Value for key {:?} was requested, but haven't yet been provisioned (request: {:?}). \n \
                        This is a data flow manager bug, please consider opening an issue at https://github.com/semanteecore/semanteecore/issues/new", self.key, pr),
        }
    }

    #[allow(dead_code)]
    pub fn as_value_mut(&mut self) -> &mut T {
        match &mut self.state {
            ValueState::Ready(v) => v,
            ValueState::NeedsProvision(pr) =>
                panic!("Value for key {:?} was requested, but haven't yet been provisioned (request: {:?}). \n \
                        This is a data flow manager bug, please consider opening an issue at https://github.com/semanteecore/semanteecore/issues/new", self.key, pr),
        }
    }

    pub fn is_ready(&self) -> bool {
        match &self.state {
            ValueState::Ready(_) => true,
            ValueState::NeedsProvision(_) => false,
        }
    }

    // Convenience constructors

    /// Makes a `Value` with a given key which requires provision.
    pub fn from_key(key: &str) -> Self {
        ValueBuilder::new(key).build()
    }

    /// Makes a protected `Value` with a given key which requires provision.
    pub fn protected(key: &str) -> Self {
        ValueBuilder::new(key).protected().build()
    }

    /// Makes a `Value` with default content.
    /// Resulting `Value` doesn't require provision.
    pub fn with_default_value(key: &str) -> Self
    where
        T: Default,
    {
        ValueBuilder::new(key).default_value().build()
    }

    /// Makes a `Value` with a given key which requires provision at given step.
    pub fn required_at(key: &str, step: PluginStep) -> Self {
        ValueBuilder::new(key).required_at(step).build()
    }

    /// Makes a `Value` with a given key and given underlying `T`.
    /// Resulting `Value` doesn't require provision.
    pub fn with_value(key: &str, value: T) -> Self {
        ValueBuilder::new(key).value(value).build()
    }

    /// Makes a `Value` with a given key and with contents to be resolved from evironment.
    /// Resulting `Value` requires provision.
    pub fn load_from_env(key: &str) -> Self {
        ValueBuilder::new(key).load_from_env().build()
    }
}

pub struct ValueBuilder<T> {
    protected: bool,
    key: String,
    value: Option<T>,
    from_env: bool,
    required_at: Option<PluginStep>,
}

impl<T> ValueBuilder<T> {
    pub fn new(key: &str) -> Self {
        ValueBuilder {
            protected: false,
            key: key.to_owned(),
            value: None,
            from_env: false,
            required_at: None,
        }
    }

    pub fn protected(&mut self) -> &mut Self {
        self.protected = true;
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

    #[allow(clippy::wrong_self_convention)]
    pub fn load_from_env(&mut self) -> &mut Self {
        self.from_env = true;
        self
    }

    pub fn build(&mut self) -> Value<T> {
        let key = mem::replace(&mut self.key, String::new());

        if let Some(value) = self.value.take() {
            Value {
                protected: self.protected,
                key,
                state: ValueState::Ready(value),
            }
        } else {
            Value {
                protected: self.protected,
                key: key.clone(),
                state: ValueState::NeedsProvision(ProvisionRequest {
                    required_at: self.required_at.take(),
                    from_env: self.from_env,
                    key,
                }),
            }
        }
    }
}

impl<T: Default> ValueBuilder<T> {
    pub fn default_value(&mut self) -> &mut Self {
        self.value = Some(Default::default());
        self
    }
}
