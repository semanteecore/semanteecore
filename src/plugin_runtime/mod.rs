pub mod discovery;
pub mod dispatcher;
pub mod data_mgr;
pub mod graph;
pub mod kernel;
pub mod resolver;
pub mod starter;

pub use self::kernel::{Error, Kernel};
use crate::plugin_support::Plugin;

struct PluginContainer<'a> {
    injected: Vec<Plugin<'a>>,
    loaded: Vec<Plugin<'static>>
}