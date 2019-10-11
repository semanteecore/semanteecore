use crate::plugin_support::{Plugin, PluginStep};

pub fn discover<'a>(plugin: &Plugin<'a>) -> Result<Vec<PluginStep>, failure::Error> {
    let response = plugin.methods()?;
    Ok(response)
}
