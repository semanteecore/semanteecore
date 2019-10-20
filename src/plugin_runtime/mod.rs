pub mod data_mgr;
pub mod discovery;
pub mod graph;
pub mod kernel;
pub mod resolver;
pub mod starter;
pub mod util;

pub use self::kernel::{Error, Kernel};

use crate::plugin_support::{Plugin, PluginStep};

pub type PluginId = usize;

pub type Injection = (Plugin, InjectionTarget);

#[derive(Copy, Clone, Debug)]
pub enum InjectionTarget {
    BeforeStep(PluginStep),
    AfterStep(PluginStep),
}
