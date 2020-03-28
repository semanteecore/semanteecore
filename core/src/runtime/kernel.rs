use failure::Fail;
use strum::IntoEnumIterator;

use crate::config::{Map, Monoproject};
use crate::runtime::data_mgr::DataManager;
use crate::runtime::sequence::{ActionKind, PluginSequence};
use crate::runtime::util::load_plugins;
use crate::runtime::InjectionTarget;
use crate::runtime::Plugin;
use plugin_api::flow::Value;
use plugin_api::{PluginInterface, PluginStep};
use std::collections::HashMap;

pub struct Kernel {
    plugins: Vec<Plugin>,
    data_mgr: DataManager,
    sequence: PluginSequence,
    env: HashMap<String, String>,
    is_dry_run: bool,
}

impl Kernel {
    pub fn builder(config: Monoproject) -> KernelBuilder {
        KernelBuilder::new(config)
    }

    pub fn run(mut self) -> Result<(), failure::Error> {
        for action in self.sequence.into_iter() {
            log::trace!("running action {:?}", action);
            let id = action.id();
            match action.into_kind() {
                ActionKind::Call(step) => {
                    let plugin = &mut self.plugins[id];
                    log::debug!("call {}::{}", plugin.name, step.as_str());
                    match step {
                        PluginStep::PreFlight => plugin.pre_flight()?,
                        PluginStep::GetLastRelease => plugin.get_last_release()?,
                        PluginStep::DeriveNextVersion => plugin.derive_next_version()?,
                        PluginStep::GenerateNotes => plugin.generate_notes()?,
                        PluginStep::Prepare => plugin.prepare()?,
                        PluginStep::VerifyRelease => plugin.verify_release()?,
                        PluginStep::Commit => plugin.commit()?,
                        PluginStep::Publish => plugin.publish()?,
                        PluginStep::Notify => plugin.notify()?,
                    }
                }
                ActionKind::Get(src_key) => {
                    let plugin = &self.plugins[id];
                    let value = plugin.get_value(&src_key)?;
                    log::debug!("get {}::{} ==> {:?}", self.plugins[id].name, src_key, value);
                    let value = Value::builder(&src_key).value(value).build();
                    self.data_mgr.insert_global(src_key, value);
                }
                ActionKind::Set(dst_key, src_key) => {
                    let value = self.data_mgr.prepare_value(id, &dst_key, &src_key)?;
                    log::debug!("set {}::{} <== {:?}", self.plugins[id].name, dst_key, value);
                    let plugin = &mut self.plugins[id];
                    plugin.set_value(&dst_key, value)?;
                }
                ActionKind::SetValue(dst_key, value) => {
                    let value = Value::builder(&dst_key).value(value).build();
                    log::debug!("set {}::{} <== {:?}", self.plugins[id].name, dst_key, value);
                    self.plugins[id].set_value(&dst_key, value)?;
                }
                ActionKind::RequireConfigEntry(dst_key) => {
                    let value = self.data_mgr.prepare_value_same_key(id, &dst_key)?;
                    log::debug!("set {}::{} <== {:?}", self.plugins[id].name, dst_key, value);
                    self.plugins[id].set_value(&dst_key, value)?;
                }
                ActionKind::RequireEnvValue(dst_key, src_key) => {
                    let value = self
                        .env
                        .get(&src_key)
                        .ok_or_else(|| Error::EnvValueUndefined(src_key.clone()))?;
                    let value = Value::builder(&src_key).value(serde_json::to_value(value)?).build();
                    log::debug!("set {}::{} <== {:?}", self.plugins[id].name, dst_key, value);
                    self.plugins[id].set_value(&dst_key, value)?;
                }
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

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins[..]
    }
}

pub struct KernelBuilder {
    config: Monoproject,
    injections: Vec<(Plugin, InjectionTarget)>,
}

impl KernelBuilder {
    pub fn new(config: Monoproject) -> Self {
        KernelBuilder {
            config,
            injections: Vec::new(),
        }
    }

    pub fn inject(&mut self, plugin: Plugin, target: InjectionTarget) -> &mut Self {
        self.injections.push((plugin, target));
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

        // Load and start the plugins
        // We skip the injected plugins here 'cause there's a custom chaining logic required for Sequence
        let plugins = load_plugins(&self.config)?;

        // Injection stage
        let injections = std::mem::replace(&mut self.injections, Vec::new());
        let mut injection_defs = Vec::new();
        let mut injected_plugins = Vec::new();
        for (id, (plugin, target)) in injections.into_iter().enumerate() {
            injected_plugins.push(plugin);
            injection_defs.push((id, target));
        }

        // Prepend injected plugins to plugin list
        injected_plugins.extend(plugins.into_iter());
        let plugins = injected_plugins;

        // Calculate the plugin run sequence
        let sequence = PluginSequence::new(&plugins, &self.config, injection_defs, is_dry_run)?;
        log::debug!("plugin Sequence Graph built successfully");
        log::trace!("graph: {:#?}", sequence);

        // Create data manager
        let data_mgr = DataManager::new(&self.config);

        Ok(Kernel {
            env: std::env::vars().collect(),
            plugins,
            data_mgr,
            sequence,
            is_dry_run,
        })
    }
}

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "environment value must be set: {}", _0)]
    EnvValueUndefined(String),
}
