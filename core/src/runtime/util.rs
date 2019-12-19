use failure::Fail;

use crate::config::{Monoproject, PluginDefinitionMap};
use crate::runtime::plugin::{Plugin, RawPlugin, RawPluginState};
use crate::runtime::resolver::PluginResolver;
use crate::runtime::starter::PluginStarter;
use crate::runtime::Injection;

pub fn load_plugins(config: &Monoproject) -> Result<Vec<Plugin>, failure::Error> {
    load_plugins_with_injections(config, vec![])
}

pub fn load_plugins_with_injections(
    config: &Monoproject,
    injections: Vec<Injection>,
) -> Result<Vec<Plugin>, failure::Error> {
    // Move PluginDefinitions out of config and convert them to Plugins
    let plugins = config.plugins.clone();
    let plugins = plugin_def_map_to_vec(plugins);

    // Resolve stage
    let plugins = resolve_plugins(plugins)?;
    check_all_resolved(&plugins)?;
    log::debug!("all plugins resolved");

    // Starting stage
    let mut plugins = start_plugins(plugins)?;
    log::debug!("all plugins started");

    // Prepend injected plugins to plugin list
    let plugins_to_inject = injections.into_iter().map(|(plugin, _)| plugin);
    plugins.splice(..0, plugins_to_inject);

    Ok(plugins)
}

fn plugin_def_map_to_vec(plugins: PluginDefinitionMap) -> Vec<RawPlugin> {
    plugins
        .into_iter()
        .map(|(name, def)| RawPlugin::new(name, RawPluginState::Unresolved(def.into_full())))
        .collect()
}

fn resolve_plugins(plugins: Vec<RawPlugin>) -> Result<Vec<RawPlugin>, failure::Error> {
    log::debug!("resolving plugins...");
    let resolver = PluginResolver::new();
    let plugins = plugins
        .into_iter()
        .map(|p| resolver.resolve(p))
        .collect::<Result<_, _>>()?;
    Ok(plugins)
}

fn start_plugins(plugins: Vec<RawPlugin>) -> Result<Vec<Plugin>, failure::Error> {
    log::debug!("starting plugins...");
    let starter = PluginStarter::new();
    let plugins = plugins
        .into_iter()
        .map(|p| starter.start(p))
        .collect::<Result<_, _>>()?;
    Ok(plugins)
}

fn check_all_resolved(plugins: &[RawPlugin]) -> Result<(), failure::Error> {
    let unresolved = list_not_resolved_plugins(plugins);
    if unresolved.is_empty() {
        Ok(())
    } else {
        Err(Error::FailedToResolvePlugins(unresolved).into())
    }
}

fn list_not_resolved_plugins(plugins: &[RawPlugin]) -> Vec<String> {
    list_all_plugins_that(plugins, |plugin| match plugin.state() {
        RawPluginState::Unresolved(_) => true,
        RawPluginState::Resolved(_) => false,
    })
}

fn list_all_plugins_that(plugins: &[RawPlugin], filter: impl Fn(&RawPlugin) -> bool) -> Vec<String> {
    plugins
        .iter()
        .filter_map(|plugin| {
            if filter(plugin) {
                Some(plugin.name().clone())
            } else {
                None
            }
        })
        .collect()
}

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "failed to resolve some modules: \n{:#?}", _0)]
    FailedToResolvePlugins(Vec<String>),
}
