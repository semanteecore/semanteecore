pub mod kv;
pub use kv::KeyValue;

use failure::Fail;
use serde::{Deserialize, Serialize};
use std::mem;

use super::PluginStep;

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Scope {
    /// Global semanic-rs data scope for universally shared values
    Global,
    /// Local scope is the releaserc.toml plugin configuration sub-table
    Local,
    /// Scope for language-support plugins (e.g rust, node, go)
    Language,
    /// Scope for vcs-support plugins (e.g git, svn, pijul)
    VCS,
    /// Scope for project-analysis plugins (e.g clog)
    Analysis,
    /// Scope for release-target support plugins (e.g github, docker)
    Release,
    /// Any custom scope (use with caution)
    Custom(String),
}

impl Default for Scope {
    fn default() -> Self {
        Scope::Global
    }
}

#[derive(Debug, Clone)]
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
    pub scope: Scope,
    pub when: Availability,
    pub key: String,
}

impl ProvisionCapability {
    pub fn builder(key: &str) -> ProvisionCapabilityBuilder {
        ProvisionCapabilityBuilder {
            scope: Scope::default(),
            when: Availability::default(),
            key: key.to_owned(),
        }
    }
}

pub struct ProvisionCapabilityBuilder {
    scope: Scope,
    when: Availability,
    key: String,
}

impl ProvisionCapabilityBuilder {
    pub fn scope(&mut self, scope: Scope) -> &mut Self {
        self.scope = scope;
        self
    }

    pub fn after_step(&mut self, step: PluginStep) -> &mut Self {
        self.when = Availability::AfterStep(step);
        self
    }

    pub fn build(&mut self) -> ProvisionCapability {
        ProvisionCapability {
            scope: mem::replace(&mut self.scope, Default::default()),
            when: mem::replace(&mut self.when, Default::default()),
            key: mem::replace(&mut self.key, String::new()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProvisionRequest {
    pub scope: Scope,
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
