use plugin_api::{PluginInterface, PluginStep};
use crate::runtime::plugin::Plugin;

pub fn discover(plugin: &Plugin) -> Result<Vec<PluginStep>, failure::Error> {
    let response = plugin.methods()?;
    Ok(response)
}
