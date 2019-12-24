use crate::runtime::plugin::Plugin;
use plugin_api::{PluginInterface, PluginStep};

pub fn discover(plugin: &Plugin) -> Result<Vec<PluginStep>, failure::Error> {
    let response = plugin.methods()?;
    Ok(response)
}
