use std::collections::{HashMap, HashSet};
use std::mem;
use std::rc::Rc;

use failure::Fail;

use crate::config::{CfgMap, Config, Map, PluginDefinitionMap, StepDefinition};
use crate::plugin::discovery::{CapabilitiesDiscovery, Discovery as _};
use crate::plugin::proto::request::PluginRequest;
use crate::plugin::proto::response::PluginResponse;
use crate::plugin::proto::Version;
use crate::plugin::resolver::PluginResolver;
use crate::plugin::starter::PluginStarter;
use crate::plugin::{
    Plugin, PluginDispatcher, PluginName, PluginState, PluginStep, ResolvedPlugin,
};

const STEPS_ORDER: &[PluginStep] = &[
    PluginStep::PreFlight,
    PluginStep::GetLastRelease,
    PluginStep::DeriveNextVersion,
    PluginStep::GenerateNotes,
    PluginStep::Prepare,
    PluginStep::VerifyRelease,
    PluginStep::Commit,
    PluginStep::Publish,
    PluginStep::Notify,
];

pub struct Kernel {
    dispatcher: PluginDispatcher,
}

impl Kernel {
    pub fn builder(config: Config) -> KernelBuilder {
        KernelBuilder {
            config,
            additional_plugins: vec![],
        }
    }

    pub fn run(self) -> Result<(), failure::Error> {
        let mut data = KernelData::default();

        // Run through the steps
        for step in STEPS_ORDER {
            step.execute(&self, &mut data).map_err(|err| {
                log::error!("Step {:?} failed", step);
                err
            })?
        }

        Ok(())
    }
}

pub struct KernelBuilder {
    config: Config,
    additional_plugins: Vec<Plugin>,
}

impl KernelBuilder {
    pub fn plugin(&mut self, plugin: Plugin) -> &mut Self {
        self.additional_plugins.push(plugin);
        self
    }

    pub fn build(&mut self) -> Result<Kernel, failure::Error> {
        // Move PluginDefinitions out of config and convert them to Plugins
        let plugins = mem::replace(&mut self.config.plugins, Map::new());
        let mut plugins = Self::plugin_def_map_to_vec(plugins);

        // Append plugins from config to additional plugins
        // Order matters here 'cause additional plugins
        // MUST run before external plugins from Config
        self.additional_plugins.extend(plugins.drain(..));
        let plugins = mem::replace(&mut self.additional_plugins, Vec::new());

        // Resolve stage
        let plugins = Self::resolve_plugins(plugins)?;
        Self::check_all_resolved(&plugins)?;
        log::info!("All plugins resolved");

        // Starting stage
        let plugins = Self::start_plugins(plugins)?;
        Self::check_all_started(&plugins)?;
        log::info!("All plugins started");

        // Discovering plugins capabilities
        let capabilities = Self::discover_capabilities(&self.config.cfg, &plugins)?;

        // Building a steps to plugins map
        let steps_to_plugins =
            Self::build_steps_to_plugins_map(&self.config, plugins, capabilities)?;

        // Create a dispatcher
        let cfg_map = mem::replace(&mut self.config.cfg, CfgMap::new());
        let dispatcher = PluginDispatcher::new(cfg_map, steps_to_plugins);

        Ok(Kernel { dispatcher })
    }

    fn plugin_def_map_to_vec(plugins: PluginDefinitionMap) -> Vec<Plugin> {
        plugins
            .into_iter()
            .map(|(name, def)| Plugin::new(name, PluginState::Unresolved(def.into_full())))
            .collect()
    }

    fn resolve_plugins(plugins: Vec<Plugin>) -> Result<Vec<Plugin>, failure::Error> {
        log::info!("Resolving plugins");
        let resolver = PluginResolver::new();
        let plugins = plugins
            .into_iter()
            .map(|p| resolver.resolve(p))
            .collect::<Result<_, _>>()?;
        Ok(plugins)
    }

    fn start_plugins(plugins: Vec<Plugin>) -> Result<Vec<Plugin>, failure::Error> {
        log::info!("Starting plugins");
        let starter = PluginStarter::new();
        let plugins = plugins
            .into_iter()
            .map(|p| starter.start(p))
            .collect::<Result<_, _>>()?;
        Ok(plugins)
    }

    fn discover_capabilities(
        cfg_map: &CfgMap,
        plugins: &[Plugin],
    ) -> Result<Map<PluginStep, Vec<PluginName>>, failure::Error> {
        let discovery = CapabilitiesDiscovery::new();
        let mut capabilities = Map::new();

        for plugin in plugins {
            let plugin_caps = discovery.discover(cfg_map, &plugin)?;
            for step in plugin_caps {
                capabilities
                    .entry(step)
                    .or_insert_with(|| Vec::new())
                    .push(plugin.name().clone());
            }
        }

        Ok(capabilities)
    }

    fn build_steps_to_plugins_map(
        config: &Config,
        plugins: Vec<Plugin>,
        capabilities: Map<PluginStep, Vec<PluginName>>,
    ) -> Result<Map<PluginStep, Vec<Rc<Plugin>>>, failure::Error> {
        let mut map = Map::new();

        let plugins: Vec<Rc<Plugin>> = plugins.into_iter().map(Rc::new).collect();

        fn copy_plugins_matching(
            plugins: &[Rc<Plugin>],
            names: &[impl AsRef<str>],
        ) -> Vec<Rc<Plugin>> {
            plugins
                .iter()
                .filter(|p| {
                    names
                        .iter()
                        .map(AsRef::as_ref)
                        .find(|n| n == p.name())
                        .is_some()
                })
                .map(Rc::clone)
                .collect::<Vec<_>>()
        }

        // TODO: Store plugins in Arc links to make it possible to have copies in different steps
        for (step, step_def) in config.steps.iter() {
            match step_def {
                StepDefinition::Discover => {
                    let names = capabilities.get(&step);

                    let plugins = if let Some(names) = names {
                        copy_plugins_matching(&plugins, &names[..])
                    } else {
                        Vec::new()
                    };

                    if plugins.is_empty() {
                        log::warn!("Step '{}' is marked for auto-discovery, but no plugin implements this method", step.as_str());
                    }

                    map.insert(*step, plugins);
                }
                StepDefinition::Singleton(plugin) => {
                    let names = capabilities
                        .get(&step)
                        .ok_or(KernelError::NoPluginsForStep(*step))?;

                    if !names.contains(&plugin) {
                        Err(KernelError::PluginDoesNotImplementStep(
                            *step,
                            plugin.to_string(),
                        ))?
                    }

                    let plugins = copy_plugins_matching(&plugins, &[plugin]);
                    assert_eq!(plugins.len(), 1);

                    map.insert(*step, plugins);
                }
                StepDefinition::Shared(list) => {
                    if list.is_empty() {
                        continue;
                    };

                    let names = capabilities
                        .get(&step)
                        .ok_or(KernelError::NoPluginsForStep(*step))?;

                    for plugin in list {
                        if !names.contains(&plugin) {
                            Err(KernelError::PluginDoesNotImplementStep(
                                *step,
                                plugin.to_string(),
                            ))?
                        }
                    }

                    let plugins = copy_plugins_matching(&plugins, &list[..]);
                    assert_eq!(plugins.len(), list.len());

                    map.insert(*step, plugins);
                }
            }
        }

        Ok(map)
    }

    fn check_all_resolved(plugins: &[Plugin]) -> Result<(), failure::Error> {
        let unresolved = Self::list_not_resolved_plugins(plugins);
        if unresolved.is_empty() {
            Ok(())
        } else {
            Err(KernelError::FailedToResolvePlugins(unresolved).into())
        }
    }

    fn check_all_started(plugins: &[Plugin]) -> Result<(), failure::Error> {
        let not_started = Self::list_not_started_plugins(plugins);
        if not_started.is_empty() {
            Ok(())
        } else {
            Err(KernelError::FailedToStartPlugins(not_started).into())
        }
    }

    fn list_not_resolved_plugins(plugins: &[Plugin]) -> Vec<PluginName> {
        Self::list_all_plugins_that(plugins, |plugin| match plugin.state() {
            PluginState::Unresolved(_) => true,
            PluginState::Resolved(_) | PluginState::Started(_) => false,
        })
    }

    fn list_not_started_plugins(plugins: &[Plugin]) -> Vec<PluginName> {
        Self::list_all_plugins_that(plugins, |plugin| match plugin.state() {
            PluginState::Unresolved(_) | PluginState::Resolved(_) => true,
            PluginState::Started(_) => false,
        })
    }

    fn list_all_plugins_that(
        plugins: &[Plugin],
        filter: impl Fn(&Plugin) -> bool,
    ) -> Vec<PluginName> {
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
}

#[derive(Fail, Debug)]
enum KernelError {
    #[fail(display = "failed to resolve some modules: \n{:#?}", _0)]
    FailedToResolvePlugins(Vec<PluginName>),
    #[fail(display = "failed to start some modules: \n{:#?}", _0)]
    FailedToStartPlugins(Vec<PluginName>),
    #[fail(
        display = "no plugins is capable of satisfying a non-null step {:?}",
        _0
    )]
    NoPluginsForStep(PluginStep),
    #[fail(
        display = "step {:?} requested plugin {:?}, but it does not implement this step",
        _0, 1
    )]
    PluginDoesNotImplementStep(PluginStep, PluginName),
    #[fail(
        display = "required data '{}' was not provided by the previous steps",
        _0
    )]
    MissingRequiredData(&'static str),
}

#[derive(Default)]
struct KernelData {
    last_version: Option<Version>,
}

impl KernelData {
    fn set_last_version(&mut self, version: Version) {
        self.last_version = Some(version)
    }

    fn require_last_version(&self) -> Result<&Version, failure::Error> {
        Ok(Self::_require(|| self.last_version.as_ref())?)
    }

    fn _require<T>(query_fn: impl Fn() -> Option<T>) -> Result<T, failure::Error> {
        let data = query_fn().ok_or_else(|| KernelError::MissingRequiredData("last_version"))?;
        Ok(data)
    }
}

type KernelRoutineResult<T> = Result<T, failure::Error>;

trait KernelRoutine {
    fn execute(&self, kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()>;

    fn pre_flight(kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        execute_request(|| kernel.dispatcher.pre_flight(), all_responses_into_result)?;

        Ok(())
    }

    fn get_last_release(kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        let (plugin_name, response) = kernel.dispatcher.get_last_release()?;
        let response = response.into_result()?;
        data.set_last_version(response);
        Ok(())
    }

    fn derive_next_version(kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        unimplemented!()
    }

    fn generate_notes(kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        unimplemented!()
    }

    fn prepare(kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        unimplemented!()
    }

    fn verify_release(kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        unimplemented!()
    }

    fn commit(kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        unimplemented!()
    }

    fn publish(kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        unimplemented!()
    }

    fn notify(kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        unimplemented!()
    }
}

impl KernelRoutine for PluginStep {
    fn execute(&self, kernel: &Kernel, data: &mut KernelData) -> KernelRoutineResult<()> {
        match self {
            PluginStep::PreFlight => PluginStep::pre_flight(kernel, data),
            PluginStep::GetLastRelease => PluginStep::get_last_release(kernel, data),
            PluginStep::DeriveNextVersion => PluginStep::derive_next_version(kernel, data),
            PluginStep::GenerateNotes => PluginStep::generate_notes(kernel, data),
            PluginStep::Prepare => PluginStep::prepare(kernel, data),
            PluginStep::VerifyRelease => PluginStep::verify_release(kernel, data),
            PluginStep::Commit => PluginStep::commit(kernel, data),
            PluginStep::Publish => PluginStep::publish(kernel, data),
            PluginStep::Notify => PluginStep::notify(kernel, data),
        }
    }
}

fn execute_request<RF, RFR, MF, MFR>(request_fn: RF, merge_fn: MF) -> Result<MFR, failure::Error>
where
    RF: Fn() -> Result<RFR, failure::Error>,
    MF: Fn(RFR) -> Result<MFR, failure::Error>,
{
    let response = request_fn()?;
    let merged = merge_fn(response)?;
    Ok(merged)
}

fn all_responses_into_result<T>(
    responses: Map<PluginName, PluginResponse<T>>,
) -> Result<Map<PluginName, T>, failure::Error> {
    responses
        .into_iter()
        .map(|(name, r)| {
            r.into_result()
                .map_err(|err| failure::format_err!("Plugin {:?} raised error: {}", name, err))
                .map(|data| (name, data))
        })
        .collect()
}