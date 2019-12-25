pub mod data_mgr;
pub mod discovery;
pub mod kernel;
pub mod plugin;
pub mod resolver;
pub mod sequence;
pub mod starter;
pub mod util;
pub mod graph;

pub use self::kernel::{Error, Kernel};

pub use crate::runtime::plugin::Plugin;
use plugin_api::PluginStep;

pub type PluginId = usize;

pub type Injection = (Plugin, InjectionTarget);

#[derive(Copy, Clone, Debug)]
pub enum InjectionTarget {
    BeforeStep(PluginStep),
    AfterStep(PluginStep),
}
