pub mod command;
pub mod flow;
pub mod keys;
pub mod proto;
pub mod traits;

pub use self::traits::PluginInterface;

use serde::{Deserialize, Serialize};
use std::cell::{RefCell, RefMut};
use strum::IntoEnumIterator;
use crate::plugin_support::proto::response;
use crate::plugin_support::flow::Value;
use crate::logger;

pub struct RawPlugin {
    name: String,
    state: RawPluginState,
}

impl RawPlugin {
    pub fn new(name: String, state: RawPluginState) -> Self {
        RawPlugin { name, state }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn state(&self) -> &RawPluginState {
        &self.state
    }

    pub fn decompose(self) -> (String, RawPluginState) {
        (self.name, self.state)
    }
}

pub enum RawPluginState {
    Unresolved(UnresolvedPlugin),
    Resolved(ResolvedPlugin),
}

pub struct Plugin<'a> {
    pub name: String,
    inner: MaybeOwnedCallable<'a>
}

enum MaybeOwnedCallable<'a> {
    Owned(Box<dyn PluginInterface + 'static>),
    Borrowed(&'a mut (dyn PluginInterface + 'static))
}

impl From<Box<dyn PluginInterface>> for MaybeOwnedCallable<'_> {
    fn from(x: Box<dyn PluginInterface>) -> Self {
        MaybeOwnedCallable::Owned(x)
    }
}
impl<'a> From<&'a mut (dyn PluginInterface + 'static)> for MaybeOwnedCallable<'a> {
    fn from(x: &'a mut (dyn PluginInterface + 'static)) -> Self {
        MaybeOwnedCallable::Borrowed(x)
    }
}

impl<'a> AsRef<dyn PluginInterface + 'a> for MaybeOwnedCallable<'a> {
    fn as_ref(&self) -> &(dyn PluginInterface + 'a) {
        match self {
            MaybeOwnedCallable::Owned(call) => &**call,
            MaybeOwnedCallable::Borrowed(call) => &**call,
        }
    }
}

impl<'a> AsMut<dyn PluginInterface + 'a> for MaybeOwnedCallable<'a> {
    fn as_mut(&mut self) -> &mut (dyn PluginInterface + 'a) {
        match self {
            MaybeOwnedCallable::Owned(call) => &mut **call,
            MaybeOwnedCallable::Borrowed(call) => *call,
        }
    }
}

impl Plugin<'static> {
    pub fn from_box(plugin: Box<dyn PluginInterface>) -> Result<Self, failure::Error> {
        Plugin::construct(plugin)
    }
}

impl<'a> Plugin<'a> {
    pub fn from_ref(plugin: &'a mut (dyn PluginInterface + 'static)) -> Result<Self, failure::Error> {
        Plugin::construct(plugin)
    }

    fn construct<T: Into<MaybeOwnedCallable<'a>>>(plugin: T) -> Result<Self, failure::Error> {
        let plugin = plugin.into();
        let name = plugin.as_ref().name()?;
        let plugin = Plugin {
            name,
            inner: plugin,
        };
        Ok(plugin)
    }

    fn wrap<R>(&self, func: impl FnOnce(&dyn PluginInterface) -> R) -> R {
        let _span = logger::span(&self.name);
        func(self.inner.as_ref())
    }

    fn wrap_mut<R>(&mut self, func: impl FnOnce(&mut dyn PluginInterface) -> R) -> R {
        let _span = logger::span(&self.name);
        func(self.inner.as_mut())
    }
}

impl<'a> PluginInterface for Plugin<'a> {
    fn name(&self) -> response::Name {
        self.wrap(|x| x.name())
    }

    fn provision_capabilities(&self) -> response::ProvisionCapabilities {
        self.wrap(|x| x.provision_capabilities())
    }

    fn get_value(&self, key: &str) -> response::GetValue {
        self.wrap(|x| x.get_value(key))
    }

    fn set_value(&mut self, key: &str, value: Value<serde_json::Value>) -> response::Null {
        self.wrap_mut(|x| x.set_value(key, value))
    }

    fn get_config(&self) -> response::Config {
        self.wrap(|x| x.get_config())
    }

    fn set_config(&mut self, config: serde_json::Value) -> response::Null {
        self.wrap_mut(|x| x.set_config(config))
    }

    fn reset(&mut self) -> response::Null {
        self.wrap_mut(|x| x.reset())
    }

    fn methods(&self) -> response::Methods {
        self.wrap(|x| x.methods())
    }

    fn pre_flight(&mut self) -> response::Null {
        self.wrap_mut(|x| x.pre_flight())
    }

    fn get_last_release(&mut self) -> response::Null {
        self.wrap_mut(|x| x.get_last_release())
    }

    fn derive_next_version(&mut self) -> response::Null {
        self.wrap_mut(|x| x.derive_next_version())
    }

    fn generate_notes(&mut self) -> response::Null {
        self.wrap_mut(|x| x.generate_notes())
    }

    fn prepare(&mut self) -> response::Null {
        self.wrap_mut(|x| x.prepare())
    }

    fn verify_release(&mut self) -> response::Null {
        self.wrap_mut(|x| x.verify_release())
    }

    fn commit(&mut self) -> response::Null {
        self.wrap_mut(|x| x.commit())
    }

    fn publish(&mut self) -> response::Null {
        self.wrap_mut(|x| x.publish())
    }

    fn notify(&self) -> response::Null {
        self.wrap(|x| x.notify())
    }
}

impl RawPluginState {
    pub fn is_resolved(&self) -> bool {
        match self {
            RawPluginState::Resolved(_) => true,
            _ => false,
        }
    }

    pub fn as_unresolved(&self) -> Option<&UnresolvedPlugin> {
        match self {
            RawPluginState::Unresolved(unresolved) => Some(unresolved),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(tag = "location")]
#[serde(rename_all = "lowercase")]
pub enum UnresolvedPlugin {
    Builtin,
    Cargo { package: String, version: String },
}

pub enum ResolvedPlugin {
    Builtin(Box<dyn PluginInterface>),
}

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Copy,
    Clone,
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Hash,
    EnumString,
    EnumIter,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PluginStep {
    PreFlight,
    GetLastRelease,
    DeriveNextVersion,
    GenerateNotes,
    Prepare,
    VerifyRelease,
    Commit,
    Publish,
    Notify,
}

impl PluginStep {
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    pub fn kind(self) -> PluginStepKind {
        match self {
            PluginStep::PreFlight
            | PluginStep::DeriveNextVersion
            | PluginStep::Prepare
            | PluginStep::VerifyRelease
            | PluginStep::Publish
            | PluginStep::Notify => PluginStepKind::Shared,
            PluginStep::GetLastRelease | PluginStep::GenerateNotes | PluginStep::Commit => PluginStepKind::Singleton,
        }
    }

    pub fn dry_steps() -> impl Iterator<Item = PluginStep> {
        PluginStep::iter().filter(|s| s.is_dry())
    }

    pub fn wet_steps() -> impl Iterator<Item = PluginStep> {
        PluginStep::iter().filter(|s| s.is_wet())
    }

    pub fn is_dry(self) -> bool {
        match self {
            PluginStep::PreFlight
            | PluginStep::GetLastRelease
            | PluginStep::DeriveNextVersion
            | PluginStep::GenerateNotes
            | PluginStep::Prepare
            | PluginStep::VerifyRelease => true,
            PluginStep::Publish | PluginStep::Notify | PluginStep::Commit => false,
        }
    }

    pub fn is_wet(self) -> bool {
        !self.is_dry()
    }
}

#[derive(Copy, Clone, Debug)]
pub enum PluginStepKind {
    Singleton,
    Shared,
}
