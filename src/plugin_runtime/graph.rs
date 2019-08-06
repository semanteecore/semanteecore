use crate::config::Map;
use crate::plugin_runtime::kernel::PluginId;
use crate::plugin_support::flow::kv::{Key, ValueDefinitionMap, ValueState};
use crate::plugin_support::flow::{Availability, ProvisionCapability, ProvisionRequest, Value};
use crate::plugin_support::{Plugin, PluginStep};
use std::collections::{HashSet, VecDeque};

pub enum Action {
    Call(PluginId, PluginStep),
    DataQuery(PluginId, Key),
    ConfigQuery(PluginId, Key),
    DataProvision(PluginId, Key),
}

pub struct PluginSequence {
    seq: Vec<Action>,
}

impl PluginSequence {
    pub fn new(plugins: &[Plugin]) -> Result<Self, failure::Error> {
        // First -- collect data from plugins
        let names = collect_plugins_names(plugins);
        let configs = collect_plugins_initial_configuration(plugins)?;
        let caps = collect_plugins_provision_capabilities(plugins)?;

        // Then delegate that data to a builder
        let builder = PluginSequenceBuilder {
            names,
            configs,
            caps,
        };

        builder.build()
    }

    fn iter(&self) -> impl Iterator<Item = &Action> {
        self.seq.iter()
    }

    fn into_iter(self) -> impl Iterator<Item = Action> {
        self.seq.into_iter()
    }
}

struct PluginSequenceBuilder {
    names: Vec<String>,
    configs: Vec<Map<String, Value<serde_json::Value>>>,
    caps: Vec<Vec<ProvisionCapability>>,
}

impl PluginSequenceBuilder {
    fn build(self) -> Result<PluginSequence, failure::Error> {
        let mut seq = Vec::new();

        // For each step we need to solve a dependency graph or fail to do so.
        // TODO: replace with iter over variants
        let steps = &[
            PluginStep::PreFlight,
            PluginStep::DeriveNextVersion,
            PluginStep::GetLastRelease,
            PluginStep::GenerateNotes,
            PluginStep::Prepare,
            PluginStep::VerifyRelease,
            PluginStep::Commit,
            PluginStep::Publish,
            PluginStep::Notify,
        ];

        for &step in steps {
            let builder = StepSequenceBuilder::new(step, &self.names, &self.configs, &self.caps);
            let step_seq = builder.build();
            seq.extend(step_seq.into_iter());
        }

        Ok(PluginSequence { seq })
    }
}

struct StepSequenceBuilder<'a> {
    step: PluginStep,
    names: &'a [String],
    configs: &'a [Map<String, Value<serde_json::Value>>],
    caps: &'a [Vec<ProvisionCapability>],

    unresolved: Vec<Vec<(String, String)>>,
    available_key_to_plugins: Map<String, Vec<PluginId>>,
    same_step_key_to_plugins: Map<String, Vec<PluginId>>,
    future_key_to_plugins: Map<String, Vec<(PluginId, Availability)>>,
}

impl<'a> StepSequenceBuilder<'a> {
    fn new(
        step: PluginStep,
        names: &'a [String],
        configs: &'a [Map<String, Value<serde_json::Value>>],
        caps: &'a [Vec<ProvisionCapability>],
    ) -> Self {
        // Collect unresolved keys
        // Here are 2 keys for every plugin:
        // - destination: the key in the plugin config
        // - source: the key advertised by the plugin
        let unresolved: Vec<Vec<(String, String)>> = configs
            .iter()
            .enumerate()
            .map(|(dest_id, config)| {
                config
                    .iter()
                    .filter_map(|(dest_key, value)| match &value.state {
                        ValueState::Ready(_) => None,
                        ValueState::NeedsProvision(pr) => match pr.required_at {
                            Some(required_at) => {
                                if required_at > step {
                                    None
                                } else {
                                    Some((dest_key.clone(), pr.key.clone()))
                                }
                            }
                            None => Some((dest_key.clone(), pr.key.clone())),
                        },
                    })
                    .collect()
            })
            .collect();

        // Collect a few maps from keys to plugins to make life easier
        let mut available_key_to_plugins = Map::new();
        let mut same_step_key_to_plugins = Map::new();
        let mut future_key_to_plugins = Map::new();
        caps.iter().enumerate().for_each(|(source_id, caps)| {
            caps.iter().for_each(|cap| {
                let (available, same_step, future) = match cap.when {
                    Availability::Always => (true, false, false),
                    Availability::AfterStep(after) => (after < step, after == step, after > step),
                };

                if available {
                    available_key_to_plugins
                        .entry(cap.key.clone())
                        .or_insert(Vec::new())
                        .push(source_id);
                }

                if same_step {
                    same_step_key_to_plugins
                        .entry(cap.key.clone())
                        .or_insert(Vec::new())
                        .push(source_id);
                }

                if future {
                    future_key_to_plugins
                        .entry(cap.key.clone())
                        .or_insert(Vec::new())
                        .push((source_id, cap.when));
                }
            })
        });

        StepSequenceBuilder {
            step,
            names,
            configs,
            caps,
            unresolved,
            available_key_to_plugins,
            same_step_key_to_plugins,
            future_key_to_plugins,
        }
    }

    fn build(mut self) -> Vec<Action> {
        let mut seq = VecDeque::new();

        // Resolve what's trivially available right now
        let unresolved: Vec<Vec<(&String, &String)>> = self
            .unresolved
            .iter()
            .enumerate()
            .map(|(dest_id, keys)| {
                keys.iter()
                    .filter_map(|(dest_key, source_key)| {
                        if let Some(plugins) = self.available_key_to_plugins.get(source_key) {
                            seq.extend(
                                plugins
                                    .iter()
                                    .filter(|&&source_id| source_id != dest_id)
                                    .map(|source_id| {
                                        Action::DataQuery(*source_id, source_key.clone())
                                    }),
                            );
                            seq.push_back(Action::DataProvision(dest_id, dest_key.to_owned()));
                            None
                        } else {
                            Some((dest_key, source_key))
                        }
                    })
                    .collect()
            })
            .collect();

        // What's left unresolved is either inner-step dependencies,
        // where one plugin in the current step depends on data provided by another after running the same step
        // or data that should be available from the config
        //
        // Firstly let's resolve the config values
        let unresolved: Vec<Vec<(&String, &String)>> = unresolved.into_iter().enumerate().map(|(dest_id, keys)| {
            keys.into_iter().filter_map(|(dest_key, source_key)| {
                // Key must be resolved within the current step
                if self.same_step_key_to_plugins.contains_key(source_key) {
                    Some((dest_key, source_key))
                    // Key is not available now, but would be in future steps.
                } else if let Some(plugins) = self.future_key_to_plugins.get(source_key) {
                    let dest_plugin_name = &self.names[dest_id];
                    log::error!("Plugin {:?} requested key {:?}", dest_plugin_name, source_key);
                    for (source_id, when) in plugins {
                        let source_plugin_name = &self.names[*source_id];
                        log::error!("Matching source plugin {:?} can supply this key only after step {:?}, and the current step is {:?}", source_plugin_name, when, self.step);
                    }
                    log::error!("The releaserc.toml entry cfg.{}.{} must be defined to proceed", dest_plugin_name, dest_key);
                    seq.push_front(Action::ConfigQuery(dest_id, source_key.clone()));
                    None
                    // Key cannot be supplied by plugins and must be defined in releaserc.toml
                } else {
                    seq.push_front(Action::ConfigQuery(dest_id, dest_key.clone()));
                    None
                }
            }).collect()
        }).collect();

        // The next part is determining the sequence of running the plugins, and
        // since we do not do any reorders (as order is always determined by releaserc.toml)
        // this is not very hard
        //
        // If order is incorrect, that's a hard error

        // First option: every key is resolved. Then we just generate a number of Call actions.
        if unresolved.iter().all(Vec::is_empty) {
            seq.extend(
                (0..self.names.len())
                    .into_iter()
                    .map(|id| Action::Call(id, self.step)),
            );
        } else {
            // Second option: there are some inter-step resolutions being necessary,
            // so we check that the defined sequence of plugins is adequate for provisioning data
            let mut became_available = Map::new();
            for (dest_id, unresolved_keys) in unresolved.into_iter().enumerate() {
                for cap in &self.caps[dest_id] {
                    let available = match cap.when {
                        Availability::Always => true,
                        Availability::AfterStep(after) => after <= self.step,
                    };

                    if available {
                        became_available
                            .entry(cap.key.clone())
                            .or_insert(Vec::new())
                            .push(dest_id);
                    }
                }

                for (dest_key, source_key) in unresolved_keys {
                    if let Some(plugins) = became_available.get(source_key) {
                        seq.extend(
                            plugins
                                .iter()
                                .filter(|&&source_id| source_id != dest_id)
                                .map(|source_id| Action::DataQuery(*source_id, source_key.clone())),
                        );
                        seq.push_back(Action::DataProvision(dest_id, dest_key.clone()));
                    } else {
                        let dest_plugin_name = &self.names[dest_id];
                        log::error!(
                            "Plugin {:?} requested key {:?}",
                            dest_plugin_name,
                            source_key
                        );
                        for source_id in self.same_step_key_to_plugins.get(source_key).expect("at this point only same-step keys should be unresolved. This is a bug.") {
                            let source_plugin_name = &self.names[*source_id];
                            log::error!("Matching source plugin {:?} supplies this key at the current step ({:?}) but it's set to run after plugin {:?} in releaserc.toml", source_plugin_name, self.step, dest_plugin_name);
                        }
                        log::error!(
                            "Reorder the plugins in releaserc.toml or define the key manually."
                        );
                        log::error!(
                            "The releaserc.toml entry cfg.{}.{} must be defined to proceed.",
                            dest_plugin_name,
                            dest_key
                        );
                        seq.push_front(Action::ConfigQuery(dest_id, source_key.clone()));
                    }
                }

                seq.push_back(Action::Call(dest_id, self.step));
            }
        }

        seq.into()
    }
}

fn collect_plugins_names(plugins: &[Plugin]) -> Vec<String> {
    plugins.iter().map(|p| p.name.clone()).collect()
}

fn collect_plugins_initial_configuration(
    plugins: &[Plugin],
) -> Result<Vec<Map<String, Value<serde_json::Value>>>, failure::Error> {
    let mut configs = Vec::new();

    for (id, plugin) in plugins.iter().enumerate() {
        let plugin_config = serde_json::from_value(plugin.as_interface().get_default_config()?)?;

        configs.push(plugin_config);
    }

    Ok(configs)
}

fn collect_plugins_provision_capabilities(
    plugins: &[Plugin],
) -> Result<Vec<Vec<ProvisionCapability>>, failure::Error> {
    let mut caps = Vec::new();

    for (id, plugin) in plugins.iter().enumerate() {
        let plugin_caps = plugin.as_interface().provision_capabilities()?;

        caps.push(plugin_caps);
    }

    Ok(caps)
}
