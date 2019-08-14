use std::mem;
use std::ops::Try;

use failure::Fail;
use strum::IntoEnumIterator;

use crate::config::{Config, Map, PluginDefinitionMap, StepDefinition};
use crate::plugin_runtime::data_mgr::DataManager;
use crate::plugin_runtime::discovery::CapabilitiesDiscovery;
use crate::plugin_runtime::graph::{collect_plugins_initial_configuration, Action, PluginSequence};
use crate::plugin_runtime::resolver::PluginResolver;
use crate::plugin_runtime::starter::PluginStarter;
use crate::plugin_support::flow::kv::ValueDefinitionMap;
use crate::plugin_support::flow::Value;
use crate::plugin_support::proto::response::PluginResponse;
use crate::plugin_support::proto::Version;
use crate::plugin_support::{Plugin, PluginStep, RawPlugin, RawPluginState};
use std::collections::HashMap;

const STEPS_DRY: &[PluginStep] = &[
    PluginStep::PreFlight,
    PluginStep::GetLastRelease,
    PluginStep::DeriveNextVersion,
    PluginStep::GenerateNotes,
    PluginStep::Prepare,
    PluginStep::VerifyRelease,
];

const STEPS_WET: &[PluginStep] = &[PluginStep::Commit, PluginStep::Publish, PluginStep::Notify];

pub type PluginId = usize;

pub struct Kernel {
    plugins: Vec<Plugin>,
    data_mgr: DataManager,
    sequence: PluginSequence,
    env: HashMap<String, String>,
    hooks: Hooks,
    is_dry_run: bool,
}

impl Kernel {
    pub fn builder(config: Config) -> KernelBuilder {
        KernelBuilder {
            config,
            hooks: Hooks::default(),
        }
    }

    pub fn run(mut self) -> Result<(), failure::Error> {
        for action in self.sequence.into_iter() {
            log::trace!("running action {:?}", action);
            match action {
                Action::Call(id, step) => {
                    let plugin = &self.plugins[id];
                    log::debug!("call {}::{}", plugin.name, step.as_str());
                    let mut callable = plugin.as_interface();
                    match step {
                        PluginStep::PreFlight => callable.pre_flight()?,
                        PluginStep::GetLastRelease => callable.get_last_release()?,
                        PluginStep::DeriveNextVersion => callable.derive_next_version()?,
                        PluginStep::GenerateNotes => callable.generate_notes()?,
                        PluginStep::Prepare => callable.prepare()?,
                        PluginStep::VerifyRelease => callable.verify_release()?,
                        PluginStep::Commit => callable.commit()?,
                        PluginStep::Publish => callable.publish()?,
                        PluginStep::Notify => callable.notify()?,
                    }
                }
                Action::Get(src_id, src_key) => {
                    let value = self.plugins[src_id].as_interface().get_value(&src_key)?;
                    log::debug!("get {}::{} ==> {:?}", self.plugins[src_id].name, src_key, value);
                    let value = Value::builder(&src_key).value(value).build();
                    self.data_mgr.insert_global(src_key, value);
                }
                Action::Set(dst_id, dst_key, src_key) => {
                    let value = self.data_mgr.prepare_value(dst_id, &dst_key, &src_key)?;
                    log::debug!("set {}::{} <== {:?}", self.plugins[dst_id].name, dst_key, value);
                    self.plugins[dst_id].as_interface().set_value(&dst_key, value)?;
                }
                Action::SetValue(dst_id, dst_key, value) => {
                    let value = Value::builder(&dst_key).value(value).build();
                    log::debug!("set {}::{} <== {:?}", self.plugins[dst_id].name, dst_key, value);
                    self.plugins[dst_id].as_interface().set_value(&dst_key, value)?;
                }
                Action::RequireConfigEntry(dst_id, dst_key) => {
                    let value = self.data_mgr.prepare_value_same_key(dst_id, &dst_key)?;
                    log::debug!("set {}::{} <== {:?}", self.plugins[dst_id].name, dst_key, value);
                    self.plugins[dst_id].as_interface().set_value(&dst_key, value)?;
                }
                Action::RequireEnvValue(dst_id, dst_key, src_key) => {
                    let value = self
                        .env
                        .get(&src_key)
                        .ok_or_else(|| KernelError::EnvValueUndefined(src_key.clone()))?;
                    let value = Value::builder(&src_key).value(serde_json::to_value(value)?).build();
                    log::debug!("set {}::{} <== {:?}", self.plugins[dst_id].name, dst_key, value);
                    self.plugins[dst_id].as_interface().set_value(&dst_key, value)?;
                }
                Action::PreStepHook(step) => self.hooks.exec_before(step, &mut self.data_mgr)?,
                Action::PostStepHook(step) => self.hooks.exec_after(step, &mut self.data_mgr)?,
            }
        }

        if self.is_dry_run {
            log::info!(
                "DRY RUN: skipping steps {:?}",
                PluginStep::iter().filter(|s| !s.is_dry()).collect::<Vec<_>>()
            );
        }

        Ok(())
    }
}

pub type Hook = Box<dyn Fn(PluginStep, &mut DataManager) -> Result<(), failure::Error>>;

pub enum HookTarget {
    BeforeStep(PluginStep),
    AfterStep(PluginStep),
    BeforeAnyStep,
    AfterAnyStep,
}

#[derive(Default)]
pub struct Hooks {
    before: Map<PluginStep, Vec<Hook>>,
    after: Map<PluginStep, Vec<Hook>>,
    before_any: Vec<Hook>,
    after_any: Vec<Hook>,
}

impl Hooks {
    pub fn exec_before(&self, step: PluginStep, data_mgr: &mut DataManager) -> Result<(), failure::Error> {
        let hooks = self
            .before
            .get(&step)
            .into_iter()
            .flat_map(|hooks| hooks.iter())
            .chain(self.before_any.iter());

        for hook in hooks {
            hook(step, data_mgr)?;
        }

        Ok(())
    }

    pub fn exec_after(&self, step: PluginStep, data_mgr: &mut DataManager) -> Result<(), failure::Error> {
        let hooks = self
            .after
            .get(&step)
            .into_iter()
            .flat_map(|hooks| hooks.iter())
            .chain(self.after_any.iter());

        for hook in hooks {
            hook(step, data_mgr)?;
        }

        Ok(())
    }
}

pub struct KernelBuilder {
    config: Config,
    hooks: Hooks,
}

impl KernelBuilder {
    pub fn hook<H>(&mut self, target: HookTarget, hook: H) -> &mut Self
    where
        H: Fn(PluginStep, &mut DataManager) -> Result<(), failure::Error> + 'static,
    {
        let hook = Box::new(hook);
        match target {
            HookTarget::BeforeStep(step) => self.hooks.before.entry(step).or_insert_with(Vec::new).push(hook),
            HookTarget::AfterStep(step) => self.hooks.after.entry(step).or_insert_with(Vec::new).push(hook),
            HookTarget::BeforeAnyStep => self.hooks.before_any.push(hook),
            HookTarget::AfterAnyStep => self.hooks.after_any.push(hook),
        }

        self
    }

    pub fn build(&mut self) -> Result<Kernel, failure::Error> {
        // Convert KeyValueDefinitionMap into KeyValue<JsonValue> map
        let cfg = self.config.cfg.clone();
        let cfg: Map<String, Value<serde_json::Value>> = cfg.into();
        let is_dry_run = cfg
            .get("dry_run")
            .and_then(|kv| kv.as_value().as_bool())
            .unwrap_or(true);

        // Move PluginDefinitions out of config and convert them to Plugins
        let plugins = self.config.plugins.clone();
        let plugins = Self::plugin_def_map_to_vec(plugins);

        // Resolve stage
        let plugins = Self::resolve_plugins(plugins)?;
        Self::check_all_resolved(&plugins)?;
        log::info!("All plugins resolved");

        // Starting stage
        let plugins = Self::start_plugins(plugins)?;
        log::info!("All plugins started");

        // Calculate the plugin run sequence
        let sequence = PluginSequence::new(&plugins, &self.config, is_dry_run)?;
        log::info!("Plugin Sequence Graph built successfully");
        log::trace!("graph: {:#?}", sequence);

        // Create data manager
        let data_mgr = DataManager::new(collect_plugins_initial_configuration(&plugins)?, &self.config);

        // Move out hooks
        let hooks = mem::replace(&mut self.hooks, Hooks::default());

        Ok(Kernel {
            env: std::env::vars().collect(),
            plugins,
            data_mgr,
            sequence,
            hooks,
            is_dry_run,
        })
    }

    fn plugin_def_map_to_vec(plugins: PluginDefinitionMap) -> Vec<RawPlugin> {
        plugins
            .into_iter()
            .map(|(name, def)| RawPlugin::new(name, RawPluginState::Unresolved(def.into_full())))
            .collect()
    }

    fn resolve_plugins(plugins: Vec<RawPlugin>) -> Result<Vec<RawPlugin>, failure::Error> {
        log::info!("Resolving plugins");
        let resolver = PluginResolver::new();
        let plugins = plugins
            .into_iter()
            .map(|p| resolver.resolve(p))
            .collect::<Result<_, _>>()?;
        Ok(plugins)
    }

    fn start_plugins(plugins: Vec<RawPlugin>) -> Result<Vec<Plugin>, failure::Error> {
        log::info!("Starting plugins");
        let starter = PluginStarter::new();
        let plugins = plugins
            .into_iter()
            .map(|p| starter.start(p))
            .collect::<Result<_, _>>()?;
        Ok(plugins)
    }

    fn check_all_resolved(plugins: &[RawPlugin]) -> Result<(), failure::Error> {
        let unresolved = Self::list_not_resolved_plugins(plugins);
        if unresolved.is_empty() {
            Ok(())
        } else {
            Err(KernelError::FailedToResolvePlugins(unresolved).into())
        }
    }

    fn list_not_resolved_plugins(plugins: &[RawPlugin]) -> Vec<String> {
        Self::list_all_plugins_that(plugins, |plugin| match plugin.state() {
            RawPluginState::Unresolved(_) => true,
            RawPluginState::Resolved(_) | RawPluginState::Started(_) => false,
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
}

#[derive(Fail, Debug)]
pub enum KernelError {
    #[fail(display = "failed to resolve some modules: \n{:#?}", _0)]
    FailedToResolvePlugins(Vec<String>),
    #[fail(display = "failed to start some modules: \n{:#?}", _0)]
    FailedToStartPlugins(Vec<String>),
    #[fail(display = "no plugins is capable of satisfying a non-null step {:?}", _0)]
    NoPluginsForStep(PluginStep),
    #[fail(
        display = "step {:?} requested plugin {:?}, but it does not implement this step",
        _0, 1
    )]
    PluginDoesNotImplementStep(PluginStep, String),
    #[fail(display = "required data '{}' was not provided by the previous steps", _0)]
    MissingRequiredData(&'static str),
    #[fail(display = "'{}' is undefined in releaserc.toml", _0)]
    ConfigEntryUndefined(String),
    #[fail(display = "cannot determine current version due to value conflict {:?}", _0)]
    CurrentVersionConflict(Vec<serde_json::Value>),
    #[fail(display = "environment value must be set: {}", _0)]
    EnvValueUndefined(String),
    #[fail(display = "Kernel finished early")]
    EarlyExit,
}
