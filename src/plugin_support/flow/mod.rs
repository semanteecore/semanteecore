pub mod kv;
pub use kv::Value;

use failure::Fail;
use serde::{Deserialize, Serialize};
use std::mem;

use super::PluginStep;

#[derive(Debug, Clone, Copy)]
pub enum Availability {
    Always,
    AfterStep(PluginStep),
}

impl Default for Availability {
    fn default() -> Self {
        Availability::Always
    }
}

pub struct ProvisionCapability {
    pub when: Availability,
    pub key: String,
}

impl ProvisionCapability {
    pub fn builder(key: &str) -> ProvisionCapabilityBuilder {
        ProvisionCapabilityBuilder {
            when: Availability::default(),
            key: key.to_owned(),
        }
    }
}

pub struct ProvisionCapabilityBuilder {
    when: Availability,
    key: String,
}

impl ProvisionCapabilityBuilder {
    pub fn after_step(&mut self, step: PluginStep) -> &mut Self {
        self.when = Availability::AfterStep(step);
        self
    }

    pub fn build(&mut self) -> ProvisionCapability {
        ProvisionCapability {
            when: mem::replace(&mut self.when, Default::default()),
            key: mem::replace(&mut self.key, String::new()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProvisionRequest {
    pub required_at: Option<PluginStep>,
    pub key: String,
}

#[derive(Fail, Debug, Clone)]
pub enum FlowError {
    #[fail(
        display = "key {:?} is not available for querying yet, its availability is {:?}",
        _0, _1
    )]
    DataNotAvailableYet(String, Availability),
    #[fail(display = "key {:?} is supported for querying", _0)]
    KeyNotSupported(String),
}
