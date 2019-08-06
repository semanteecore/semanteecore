use crate::config::Map;
use crate::plugin_runtime::kernel::PluginId;
use crate::plugin_support::flow::kv::{Key, ValueState};
use crate::plugin_support::flow::{Availability, ProvisionCapability, Value};
use crate::plugin_support::{Plugin, PluginStep};
use std::collections::VecDeque;
use strum::IntoEnumIterator;

pub type SourceKey = Key;
pub type DestKey = Key;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Call(PluginId, PluginStep),
    DataQuery(PluginId, SourceKey),
    ConfigQuery(PluginId, DestKey),
    DataProvision(PluginId, DestKey, SourceKey),
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

        for step in PluginStep::iter() {
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

    unresolved: Vec<Vec<(DestKey, SourceKey)>>,
    available_key_to_plugins: Map<SourceKey, Vec<PluginId>>,
    same_step_key_to_plugins: Map<SourceKey, Vec<PluginId>>,
    future_key_to_plugins: Map<SourceKey, Vec<(PluginId, Availability)>>,
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
        let unresolved = configs
            .iter()
            .map(|config| {
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

    fn build(self) -> Vec<Action> {
        let mut seq = VecDeque::new();

        let unresolved = self.borrow_unresolved();

        // First -- resolve data that's trivially available from the previous step
        let unresolved = self.resolve_already_available(&mut seq, unresolved);

        // What's left unresolved is either
        // - inner-step dependencies, where one plugin in the current step depends on data provided by another after running the same step
        // - future-step dependencies, where data would only be available in future steps (then data should be in config)
        // - or data that should be available from the config, but is not there
        // Let's filter out the later 2 categories
        let unresolved = self.resolve_should_be_in_config(&mut seq, unresolved);

        // The next part is determining the sequence of running the plugins, and
        // since we do not do any reorders (as order is always determined by releaserc.toml)
        // this is not very hard
        //
        // If order is incorrect, that's an error and plugins should either be reordered
        // or the key should be defined in config manually
        self.resolve_same_step_and_build_call_sequence(&mut seq, unresolved);

        seq.into()
    }

    // Resolve data that's trivially available (Availability::Always or available since previous step)
    fn resolve_already_available<'b>(
        &self,
        seq: &mut VecDeque<Action>,
        unresolved: Vec<Vec<(&'b DestKey, &'b SourceKey)>>,
    ) -> Vec<Vec<(&'b DestKey, &'b SourceKey)>> {
        unresolved
            .into_iter()
            .enumerate()
            .map(|(dest_id, keys)| {
                keys.into_iter()
                    .filter_map(|(dest_key, source_key)| {
                        if let Some(plugins) = self.available_key_to_plugins.get(source_key) {
                            seq.extend(
                                plugins
                                    .iter()
                                    .filter(|&&source_id| source_id != dest_id)
                                    .map(|source_id| {
                                        Action::DataQuery(*source_id, Clone::clone(source_key))
                                    }),
                            );
                            seq.push_back(Action::DataProvision(
                                dest_id,
                                dest_key.clone(),
                                source_key.clone(),
                            ));
                            None
                        } else {
                            Some((dest_key, source_key))
                        }
                    })
                    .collect()
            })
            .collect()
    }

    // Resolve data that should be in config but isn't there
    fn resolve_should_be_in_config<'b>(
        &self,
        seq: &mut VecDeque<Action>,
        unresolved: Vec<Vec<(&'b DestKey, &'b SourceKey)>>,
    ) -> Vec<Vec<(&'b DestKey, &'b SourceKey)>> {
        unresolved.into_iter().enumerate().map(|(dest_id, keys)| {
            keys.into_iter().filter_map(|(dest_key, source_key)| {
                // Key must be resolved within the current step
                if self.same_step_key_to_plugins.contains_key(source_key) {
                    Some((dest_key, source_key))
                } else if let Some(plugins) = self.future_key_to_plugins.get(source_key) {
                    // Key is not available now, but would be in future steps.
                    let dest_plugin_name = &self.names[dest_id];
                    log::error!("Plugin {:?} requested key {:?}", dest_plugin_name, source_key);
                    for (source_id, when) in plugins {
                        let source_plugin_name = &self.names[*source_id];
                        log::error!("Matching source plugin {:?} can supply this key only after step {:?}, and the current step is {:?}", source_plugin_name, when, self.step);
                    }
                    log::error!("The releaserc.toml entry cfg.{}.{} must be defined to proceed", dest_plugin_name, dest_key);
                    seq.push_front(Action::ConfigQuery(dest_id, source_key.clone()));
                    None
                } else {
                    // Key cannot be supplied by plugins and must be defined in releaserc.toml
                    seq.push_front(Action::ConfigQuery(dest_id, dest_key.clone()));
                    None
                }
            }).collect()
        }).collect()
    }

    // Resolve data that should be in config but isn't there
    fn resolve_same_step_and_build_call_sequence<'b>(
        &self,
        seq: &mut VecDeque<Action>,
        unresolved: Vec<Vec<(&'b DestKey, &'b SourceKey)>>,
    ) {
        // First option: every key is resolved. Then we just generate a number of Call actions.
        if unresolved.iter().all(Vec::is_empty) {
            seq.extend((0..self.names.len()).map(|id| Action::Call(id, self.step)));

            return;
        }

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
                    seq.push_back(Action::DataProvision(
                        dest_id,
                        dest_key.clone(),
                        source_key.to_owned(),
                    ));
                } else {
                    let dest_plugin_name = &self.names[dest_id];
                    log::error!(
                        "Plugin {:?} requested key {:?}",
                        dest_plugin_name,
                        source_key
                    );
                    for source_id in self.same_step_key_to_plugins.get(source_key).expect(
                        "at this point only same-step keys should be unresolved. This is a bug.",
                    ) {
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

    fn borrow_unresolved(&self) -> Vec<Vec<(&DestKey, &SourceKey)>> {
        self.unresolved
            .iter()
            .map(|list| list.iter().map(|(key, value)| (key, value)).collect())
            .collect()
    }
}

fn collect_plugins_names(plugins: &[Plugin]) -> Vec<String> {
    plugins.iter().map(|p| p.name.clone()).collect()
}

fn collect_plugins_initial_configuration(
    plugins: &[Plugin],
) -> Result<Vec<Map<String, Value<serde_json::Value>>>, failure::Error> {
    let mut configs = Vec::new();

    for plugin in plugins.iter() {
        let plugin_config = serde_json::from_value(plugin.as_interface().get_default_config()?)?;

        configs.push(plugin_config);
    }

    Ok(configs)
}

fn collect_plugins_provision_capabilities(
    plugins: &[Plugin],
) -> Result<Vec<Vec<ProvisionCapability>>, failure::Error> {
    let mut caps = Vec::new();

    for plugin in plugins.iter() {
        let plugin_caps = plugin.as_interface().provision_capabilities()?;

        caps.push(plugin_caps);
    }

    Ok(caps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_support::flow::{FlowError, ProvisionRequest};
    use crate::plugin_support::{
        proto::{
            request,
            response::{self, PluginResponse},
        },
        PluginInterface,
    };
    use std::ops::Try;

    fn dependent_provider_plugins() -> Vec<Plugin> {
        vec![
            Plugin::new(Box::new(self::test_plugins::Dependent)).unwrap(),
            Plugin::new(Box::new(self::test_plugins::Provider)).unwrap(),
        ]
    }

    #[test]
    fn collect_names() {
        let plugins = dependent_provider_plugins();
        let names = collect_plugins_names(&plugins);
        assert_eq!(2, names.len());
        for (id, plugin) in plugins.iter().enumerate() {
            assert_eq!(&plugin.name, &names[id]);
        }
    }

    #[test]
    fn collect_configs() {
        let plugins = dependent_provider_plugins();
        let configs = collect_plugins_initial_configuration(&plugins).unwrap();
        assert_eq!(2, configs.len());

        // Check dependent config
        let dependent_map = &configs[0];
        assert_eq!(dependent_map.len(), 1);
        assert!(dependent_map.contains_key("dest_key"));
        let dest_key_value = dependent_map.get("dest_key").unwrap();
        assert_eq!(
            dest_key_value.state,
            ValueState::NeedsProvision(ProvisionRequest {
                required_at: None,
                key: "source_key".to_string()
            })
        );

        // check provider config
        assert_eq!(configs[1].len(), 0);
    }

    #[test]
    fn collect_caps() {
        let plugins = dependent_provider_plugins();
        let caps = collect_plugins_provision_capabilities(&plugins).unwrap();
        assert_eq!(
            caps,
            vec![
                vec![],
                vec![ProvisionCapability::builder("source_key").build()]
            ]
        );
    }

    #[test]
    fn build_sequence_for_dependent_provider() {
        let PluginSequence { seq } = PluginSequence::new(&dependent_provider_plugins()).unwrap();

        let correct_seq: Vec<Action> = PluginStep::iter()
            .flat_map(|step| {
                vec![
                    Action::DataQuery(1, "source_key".into()),
                    Action::DataProvision(0, "dest_key".into(), "source_key".into()),
                    Action::Call(0, step),
                    Action::Call(1, step),
                ]
                .into_iter()
            })
            .collect();

        assert_eq!(seq, correct_seq);
    }

    mod resolve {
        use super::*;

        mod already_available {
            use super::*;

            #[test]
            fn all_available() {
                let step = PluginStep::PreFlight;
                let names = vec!["one".into(), "two".into()];
                let configs = vec![
                    vec![("one_dst".into(), Value::builder("two_src").build())]
                        .into_iter()
                        .collect(),
                    vec![("two_dst".into(), Value::builder("one_src").build())]
                        .into_iter()
                        .collect(),
                ];
                let caps = vec![
                    vec![ProvisionCapability::builder("one_src").build()],
                    vec![ProvisionCapability::builder("two_src").build()],
                ];

                let ssb = StepSequenceBuilder::new(step, &names, &configs, &caps);
                let unresolved = ssb.borrow_unresolved();
                let mut seq = VecDeque::new();

                let unresolved = ssb.resolve_already_available(&mut seq, unresolved);
                assert_eq!(unresolved, vec![vec![], vec![]]);
                assert_eq!(
                    Vec::from(seq),
                    vec![
                        Action::DataQuery(1, "two_src".into()),
                        Action::DataProvision(0, "one_dst".into(), "two_src".into()),
                        Action::DataQuery(0, "one_src".into()),
                        Action::DataProvision(1, "two_dst".into(), "one_src".into()),
                    ]
                );
            }

            #[test]
            fn same_key() {
                let step = PluginStep::PreFlight;
                let names = vec!["one".into(), "two".into()];
                let configs = vec![
                    vec![("dst".into(), Value::builder("src").build())]
                        .into_iter()
                        .collect(),
                    vec![("dst".into(), Value::builder("src").build())]
                        .into_iter()
                        .collect(),
                ];
                let caps = vec![
                    vec![ProvisionCapability::builder("src").build()],
                    vec![ProvisionCapability::builder("src").build()],
                ];

                let ssb = StepSequenceBuilder::new(step, &names, &configs, &caps);
                let unresolved = ssb.borrow_unresolved();
                let mut seq = VecDeque::new();

                let unresolved = ssb.resolve_already_available(&mut seq, unresolved);
                assert_eq!(unresolved, vec![vec![], vec![]]);
                assert_eq!(
                    Vec::from(seq),
                    vec![
                        Action::DataQuery(1, "src".into()),
                        Action::DataProvision(0, "dst".into(), "src".into()),
                        Action::DataQuery(0, "src".into()),
                        Action::DataProvision(1, "dst".into(), "src".into()),
                    ]
                );
            }

            #[test]
            fn all_not_available() {
                let step = PluginStep::PreFlight;
                let names = vec!["one".into(), "two".into()];
                let configs = vec![
                    vec![("one_dst".into(), Value::builder("two_src").build())]
                        .into_iter()
                        .collect(),
                    vec![("two_dst".into(), Value::builder("one_src").build())]
                        .into_iter()
                        .collect(),
                ];
                let caps = vec![
                    vec![ProvisionCapability::builder("one_src")
                        .after_step(PluginStep::DeriveNextVersion)
                        .build()],
                    vec![ProvisionCapability::builder("two_src")
                        .after_step(PluginStep::Commit)
                        .build()],
                ];

                let ssb = StepSequenceBuilder::new(step, &names, &configs, &caps);
                let unresolved = ssb.borrow_unresolved();
                let mut seq = VecDeque::new();

                let unresolved = ssb.resolve_already_available(&mut seq, unresolved);
                assert_eq!(
                    unresolved,
                    vec![
                        vec![(&"one_dst".into(), &"two_src".into())],
                        vec![(&"two_dst".into(), &"one_src".into())],
                    ]
                );
                assert_eq!(Vec::from(seq), vec![]);
            }

            #[test]
            fn partially_not_available() {
                let step = PluginStep::PreFlight;
                let names = vec!["one".into(), "two".into()];
                let configs = vec![
                    vec![("one_dst".into(), Value::builder("two_src").build())]
                        .into_iter()
                        .collect(),
                    vec![("two_dst".into(), Value::builder("one_src").build())]
                        .into_iter()
                        .collect(),
                ];
                let caps = vec![
                    vec![ProvisionCapability::builder("one_src").build()],
                    vec![ProvisionCapability::builder("two_src")
                        .after_step(PluginStep::Commit)
                        .build()],
                ];

                let ssb = StepSequenceBuilder::new(step, &names, &configs, &caps);
                let unresolved = ssb.borrow_unresolved();
                let mut seq = VecDeque::new();

                let unresolved = ssb.resolve_already_available(&mut seq, unresolved);
                assert_eq!(
                    unresolved,
                    vec![vec![(&"one_dst".into(), &"two_src".into())], vec![],]
                );
                assert_eq!(
                    Vec::from(seq),
                    vec![
                        Action::DataQuery(0, "one_src".into()),
                        Action::DataProvision(1, "two_dst".into(), "one_src".into()),
                    ]
                );
            }

            #[test]
            fn all_not_needed() {
                let step = PluginStep::PreFlight;
                let names = vec!["one".into(), "two".into()];
                let configs = vec![
                    vec![(
                        "one_dst".into(),
                        Value::builder("two_src")
                            .required_at(PluginStep::Commit)
                            .build(),
                    )]
                    .into_iter()
                    .collect(),
                    vec![(
                        "two_dst".into(),
                        Value::builder("one_src")
                            .required_at(PluginStep::GenerateNotes)
                            .build(),
                    )]
                    .into_iter()
                    .collect(),
                ];
                let caps = vec![
                    vec![ProvisionCapability::builder("one_src").build()],
                    vec![ProvisionCapability::builder("two_src").build()],
                ];

                let ssb = StepSequenceBuilder::new(step, &names, &configs, &caps);
                let unresolved = ssb.borrow_unresolved();
                let mut seq = VecDeque::new();

                let unresolved = ssb.resolve_already_available(&mut seq, unresolved);
                assert_eq!(unresolved, vec![vec![], vec![]]);
                assert_eq!(Vec::from(seq), vec![]);
            }

            #[test]
            fn partially_not_needed() {
                let step = PluginStep::PreFlight;
                let names = vec!["one".into(), "two".into()];
                let configs = vec![
                    vec![(
                        "one_dst".into(),
                        Value::builder("two_src")
                            .required_at(PluginStep::Commit)
                            .build(),
                    )]
                    .into_iter()
                    .collect(),
                    vec![("two_dst".into(), Value::builder("one_src").build())]
                        .into_iter()
                        .collect(),
                ];
                let caps = vec![
                    vec![ProvisionCapability::builder("one_src").build()],
                    vec![ProvisionCapability::builder("two_src").build()],
                ];

                let ssb = StepSequenceBuilder::new(step, &names, &configs, &caps);
                let unresolved = ssb.borrow_unresolved();
                let mut seq = VecDeque::new();

                let unresolved = ssb.resolve_already_available(&mut seq, unresolved);
                assert_eq!(unresolved, vec![vec![], vec![]]);
                assert_eq!(
                    Vec::from(seq),
                    vec![
                        Action::DataQuery(0, "one_src".into()),
                        Action::DataProvision(1, "two_dst".into(), "one_src".into()),
                    ]
                );
            }
        }
    }

    mod test_plugins {
        use super::*;
        use serde::{Deserialize, Serialize};

        pub struct Dependent;

        #[derive(Serialize, Deserialize, Debug)]
        struct DependentConfig {
            dest_key: Value<String>,
        }

        impl PluginInterface for Dependent {
            fn name(&self) -> response::Name {
                PluginResponse::from_ok("dependent".into())
            }

            fn get_default_config(&self) -> response::Config {
                PluginResponse::from_ok(
                    serde_json::to_value(DependentConfig {
                        dest_key: Value::builder("source_key").build(),
                    })
                    .unwrap(),
                )
            }

            fn set_config(&mut self, req: request::Config) -> response::Null {
                let config: DependentConfig = serde_json::from_value(req.data.clone()).unwrap();
                assert_eq!(config.dest_key.as_value(), "value");
                PluginResponse::from_ok(())
            }
        }

        pub struct Provider;

        impl PluginInterface for Provider {
            fn name(&self) -> response::Name {
                PluginResponse::from_ok("provider".into())
            }

            fn provision_capabilities(&self) -> response::ProvisionCapabilities {
                PluginResponse::from_ok(vec![ProvisionCapability::builder("source_key").build()])
            }

            fn provision(&self, req: request::Provision) -> response::Provision {
                match req.data.as_str() {
                    "source_key" => PluginResponse::from_ok(serde_json::to_value("value").unwrap()),
                    other => PluginResponse::from_error(
                        FlowError::KeyNotSupported(other.to_owned()).into(),
                    ),
                }
            }

            fn get_default_config(&self) -> response::Config {
                PluginResponse::from_ok(serde_json::Value::Object(serde_json::Map::default()))
            }

            fn set_config(&mut self, req: request::Config) -> response::Null {
                unimplemented!()
            }
        }
    }

}
