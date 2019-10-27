pub use plugin_api::PluginInterface;

use crate::logger;
use plugin_api::flow::Value;
use plugin_api::proto::response;
use serde::{Deserialize, Serialize};
use std::cell::{Ref, RefCell, RefMut};
use std::convert::TryFrom;
use std::rc::Rc;

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

#[derive(Clone)]
pub struct Plugin {
    pub name: String,
    inner: Rc<RefCell<Box<dyn PluginInterface>>>,
}

impl TryFrom<Box<dyn PluginInterface>> for Plugin {
    type Error = failure::Error;

    fn try_from(inner: Box<dyn PluginInterface>) -> Result<Self, Self::Error> {
        let name = inner.name()?;
        let plugin = Plugin {
            name,
            inner: Rc::new(RefCell::new(inner)),
        };
        Ok(plugin)
    }
}

impl Plugin {
    pub fn new<T: PluginInterface + 'static>(plugin: T) -> Result<Self, failure::Error> {
        Plugin::try_from(Box::new(plugin) as Box<dyn PluginInterface>)
    }

    fn apply<R>(&self, func: impl FnOnce(Ref<Box<dyn PluginInterface>>) -> R) -> R {
        let _span = logger::span(&self.name);
        func(self.inner.borrow())
    }

    fn apply_mut<R>(&mut self, func: impl FnOnce(RefMut<Box<dyn PluginInterface>>) -> R) -> R {
        let _span = logger::span(&self.name);
        func(self.inner.borrow_mut())
    }
}

impl PluginInterface for Plugin {
    fn name(&self) -> response::Name {
        self.apply(|x| x.name())
    }

    fn provision_capabilities(&self) -> response::ProvisionCapabilities {
        self.apply(|x| x.provision_capabilities())
    }

    fn get_value(&self, key: &str) -> response::GetValue {
        self.apply(|x| x.get_value(key))
    }

    fn set_value(&mut self, key: &str, value: Value<serde_json::Value>) -> response::Null {
        self.apply_mut(|mut x| x.set_value(key, value))
    }

    fn get_config(&self) -> response::Config {
        self.apply(|x| x.get_config())
    }

    fn set_config(&mut self, config: serde_json::Value) -> response::Null {
        self.apply_mut(|mut x| x.set_config(config))
    }

    fn reset(&mut self) -> response::Null {
        self.apply_mut(|mut x| x.reset())
    }

    fn methods(&self) -> response::Methods {
        self.apply(|x| x.methods())
    }

    fn pre_flight(&mut self) -> response::Null {
        self.apply_mut(|mut x| x.pre_flight())
    }

    fn get_last_release(&mut self) -> response::Null {
        self.apply_mut(|mut x| x.get_last_release())
    }

    fn derive_next_version(&mut self) -> response::Null {
        self.apply_mut(|mut x| x.derive_next_version())
    }

    fn generate_notes(&mut self) -> response::Null {
        self.apply_mut(|mut x| x.generate_notes())
    }

    fn prepare(&mut self) -> response::Null {
        self.apply_mut(|mut x| x.prepare())
    }

    fn verify_release(&mut self) -> response::Null {
        self.apply_mut(|mut x| x.verify_release())
    }

    fn commit(&mut self) -> response::Null {
        self.apply_mut(|mut x| x.commit())
    }

    fn publish(&mut self) -> response::Null {
        self.apply_mut(|mut x| x.publish())
    }

    fn notify(&self) -> response::Null {
        self.apply(|x| x.notify())
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
