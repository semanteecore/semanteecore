use std::mem;
use std::ops::Try;

use failure::Fail;

use crate::config::{Config, Map, PluginDefinitionMap, StepDefinition};
use crate::plugin_runtime::discovery::CapabilitiesDiscovery;
use crate::plugin_runtime::dispatcher::PluginDispatcher;
use crate::plugin_runtime::resolver::PluginResolver;
use crate::plugin_runtime::starter::PluginStarter;
use crate::plugin_support::flow::kv::KeyValueDefinitionMap;
use crate::plugin_support::flow::KeyValue;
use crate::plugin_support::proto::Version;
use crate::plugin_support::proto::{request, response::PluginResponse};
use crate::plugin_support::{Plugin, PluginStep, RawPlugin, RawPluginState};

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
    step_map: Map<PluginStep, Vec<PluginId>>,
    is_dry_run: bool,
}

impl Kernel {
    pub fn builder(config: Config) -> KernelBuilder {
        KernelBuilder {
            config,
            additional_plugins: vec![],
        }
    }

    pub fn run(self) -> Result<(), failure::Error> {
        let dispatcher = PluginDispatcher::new(&self.plugins, &self.step_map);
        let mut data = KernelData::default();

        let mut run_step = |step: PluginStep| -> Result<(), failure::Error> {
            log::info!("Running step '{}'", step.as_str());

            step.execute(&dispatcher, &mut data).map_err(|err| {
                log::error!("Step {:?} failed", step);
                err
            })?;

            if data.should_finish_early {
                Err(KernelError::EarlyExit)?
            } else {
                Ok(())
            }
        };

        // Run through the "dry" steps
        for &step in STEPS_DRY {
            run_step(step)?;
        }

        if self.is_dry_run {
            log::info!("DRY RUN: skipping steps {:?}", STEPS_WET);
        } else {
            for &step in STEPS_WET {
                run_step(step)?
            }
        }

        Ok(())
    }
}

pub struct KernelBuilder {
    config: Config,
    additional_plugins: Vec<RawPlugin>,
}

impl KernelBuilder {
    pub fn plugin(&mut self, plugin: RawPlugin) -> &mut Self {
        self.additional_plugins.push(plugin);
        self
    }

    pub fn build(&mut self) -> Result<Kernel, failure::Error> {
        // Convert KeyValueDefinitionMap into KeyValue<JsonValue> map
        let cfg = mem::replace(&mut self.config.cfg, KeyValueDefinitionMap::default());
        let cfg: Map<String, KeyValue<serde_json::Value>> = cfg.into();
        let is_dry_run = cfg
            .get("dry_run")
            .and_then(|kv| kv.as_value().as_bool())
            .unwrap_or(true);

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
        log::info!("All plugins started");

        // Discovering plugins capabilities
        let capabilities = Self::discover_capabilities(&plugins)?;

        // Building a steps to plugins map
        let step_map = Self::build_steps_to_plugin_ids_map(&self.config, &plugins, capabilities)?;

        Ok(Kernel {
            plugins,
            step_map,
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

    fn discover_capabilities(
        plugins: &[Plugin],
    ) -> Result<Map<PluginStep, Vec<String>>, failure::Error> {
        let discovery = CapabilitiesDiscovery::new();
        let mut capabilities = Map::new();

        for plugin in plugins {
            let plugin_caps = discovery.discover(&plugin)?;
            for step in plugin_caps {
                capabilities
                    .entry(step)
                    .or_insert_with(Vec::new)
                    .push(plugin.name.clone());
            }
        }

        Ok(capabilities)
    }

    fn build_steps_to_plugin_ids_map(
        config: &Config,
        plugins: &[Plugin],
        capabilities: Map<PluginStep, Vec<String>>,
    ) -> Result<Map<PluginStep, Vec<usize>>, failure::Error> {
        let mut map = Map::new();

        fn collect_ids_of_plugins_matching(
            plugins: &[Plugin],
            names: &[impl AsRef<str>],
        ) -> Vec<usize> {
            plugins
                .iter()
                .enumerate()
                .filter_map(|(id, p)| {
                    names
                        .iter()
                        .map(AsRef::as_ref)
                        .find(|&n| n == p.name)
                        .map(|_| id)
                })
                .collect::<Vec<_>>()
        }

        // TODO: Store plugins in Arc links to make it possible to have copies in different steps
        for (step, step_def) in config.steps.iter() {
            match step_def {
                StepDefinition::Discover => {
                    let names = capabilities.get(&step);

                    let ids = if let Some(names) = names {
                        collect_ids_of_plugins_matching(&plugins[..], &names[..])
                    } else {
                        Vec::new()
                    };

                    if ids.is_empty() {
                        log::warn!("Step '{}' is marked for auto-discovery, but no plugin implements this method", step.as_str());
                    }

                    map.insert(*step, ids);
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

                    let ids = collect_ids_of_plugins_matching(&plugins, &[plugin]);
                    assert_eq!(ids.len(), 1);

                    map.insert(*step, ids);
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

                    let ids = collect_ids_of_plugins_matching(&plugins, &list[..]);
                    assert_eq!(ids.len(), list.len());

                    map.insert(*step, ids);
                }
            }
        }

        Ok(map)
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

    fn list_all_plugins_that(
        plugins: &[RawPlugin],
        filter: impl Fn(&RawPlugin) -> bool,
    ) -> Vec<String> {
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
    #[fail(
        display = "no plugins is capable of satisfying a non-null step {:?}",
        _0
    )]
    NoPluginsForStep(PluginStep),
    #[fail(
        display = "step {:?} requested plugin {:?}, but it does not implement this step",
        _0, 1
    )]
    PluginDoesNotImplementStep(PluginStep, String),
    #[fail(
        display = "required data '{}' was not provided by the previous steps",
        _0
    )]
    MissingRequiredData(&'static str),
    #[fail(display = "Kernel finished early")]
    EarlyExit,
}

#[derive(Default)]
struct KernelData {
    last_version: Option<Version>,
    next_version: Option<semver::Version>,
    changelog: Option<String>,
    files_to_commit: Option<Vec<String>>,
    tag_name: Option<String>,
    should_finish_early: bool,
}

impl KernelData {
    fn set_last_version(&mut self, version: Version) {
        self.last_version = Some(version)
    }

    fn set_next_version(&mut self, version: semver::Version) {
        self.next_version = Some(version)
    }

    fn set_changelog(&mut self, changelog: String) {
        self.changelog = Some(changelog)
    }

    fn set_files_to_commit(&mut self, files: Vec<String>) {
        self.files_to_commit = Some(files);
    }

    fn set_tag_name(&mut self, tag_name: String) {
        self.tag_name = Some(tag_name);
    }

    fn require_last_version(&self) -> Result<&Version, failure::Error> {
        Ok(require("last_version", || self.last_version.as_ref())?)
    }

    fn require_next_version(&self) -> Result<&semver::Version, failure::Error> {
        Ok(require("next_version", || self.next_version.as_ref())?)
    }

    fn require_changelog(&self) -> Result<&str, failure::Error> {
        Ok(require("changelog", || self.changelog.as_ref())?)
    }

    fn require_files_to_commit(&self) -> Result<&[String], failure::Error> {
        Ok(require("files_to_commit", || {
            self.files_to_commit.as_ref()
        })?)
    }

    fn requite_tag_name(&self) -> Result<&str, failure::Error> {
        Ok(require("tag_name", || self.tag_name.as_ref())?)
    }
}

fn require<T>(desc: &'static str, query_fn: impl Fn() -> Option<T>) -> Result<T, failure::Error> {
    let data = query_fn().ok_or(KernelError::MissingRequiredData(desc))?;
    Ok(data)
}

type KernelRoutineResult<T> = Result<T, failure::Error>;

trait KernelRoutine {
    fn execute(
        &self,
        dispatcher: &PluginDispatcher,
        data: &mut KernelData,
    ) -> KernelRoutineResult<()>;

    fn pre_flight(
        dispatcher: &PluginDispatcher,
        _data: &mut KernelData,
    ) -> KernelRoutineResult<()> {
        execute_request(|| dispatcher.pre_flight())?;
        Ok(())
    }

    fn get_last_release(
        dispatcher: &PluginDispatcher,
        data: &mut KernelData,
    ) -> KernelRoutineResult<()> {
        let (_, response) = dispatcher.get_last_release()?;
        let response = response.into_result()?;
        data.set_last_version(response);
        Ok(())
    }

    fn derive_next_version(
        dispatcher: &PluginDispatcher,
        data: &mut KernelData,
    ) -> KernelRoutineResult<()> {
        let responses =
            execute_request(|| dispatcher.derive_next_version(data.require_last_version()?))?;

        let next_version = responses
            .into_iter()
            .map(|(_, v)| v)
            .max()
            .expect("iterator from response map cannot be empty: this is a bug, aborting.");

        let is_same_versions = {
            let last_version = data.require_last_version()?;
            last_version
                .semver
                .as_ref()
                .map(|v| v == &next_version)
                .unwrap_or(false)
        };

        data.set_next_version(next_version);

        if is_same_versions {
            log::info!("Next version would be the same as previous");
            log::info!("You're all set, no release is required!");
            data.should_finish_early = true;
        }

        Ok(())
    }

    fn generate_notes(
        dispatcher: &PluginDispatcher,
        data: &mut KernelData,
    ) -> KernelRoutineResult<()> {
        let params = request::GenerateNotesData {
            start_rev: data.require_last_version()?.rev.clone(),
            new_version: data.require_next_version()?.clone(),
        };

        let responses = execute_request(|| dispatcher.generate_notes(&params))?;

        let changelog = responses.values().fold(String::new(), |mut summary, part| {
            summary.push_str(part);
            summary
        });

        log::info!("Would write the following changelog: ");
        log::info!("--------- BEGIN CHANGELOG ----------");
        log::info!("{}", changelog);
        log::info!("---------- END CHANGELOG -----------");

        data.set_changelog(changelog);

        Ok(())
    }

    fn prepare(dispatcher: &PluginDispatcher, data: &mut KernelData) -> KernelRoutineResult<()> {
        let responses = execute_request(|| dispatcher.prepare(data.require_next_version()?))?;

        let changed_files = responses
            .into_iter()
            .flat_map(|(_, v)| v.into_iter())
            .collect();

        data.set_files_to_commit(changed_files);

        Ok(())
    }

    fn verify_release(
        dispatcher: &PluginDispatcher,
        _data: &mut KernelData,
    ) -> KernelRoutineResult<()> {
        execute_request(|| dispatcher.verify_release())?;
        Ok(())
    }

    fn commit(dispatcher: &PluginDispatcher, data: &mut KernelData) -> KernelRoutineResult<()> {
        let params = request::CommitData {
            files_to_commit: data.require_files_to_commit()?.to_owned(),
            version: data.require_next_version()?.clone(),
            changelog: data.require_changelog()?.to_owned(),
        };

        let (_, response) = dispatcher.commit(&params)?;

        let tag_name = response.into_result()?;

        data.set_tag_name(tag_name);

        Ok(())
    }

    fn publish(dispatcher: &PluginDispatcher, data: &mut KernelData) -> KernelRoutineResult<()> {
        let params = request::PublishData {
            tag_name: data.requite_tag_name()?.to_owned(),
            changelog: data.require_changelog()?.to_owned(),
        };

        execute_request(|| dispatcher.publish(&params))?;
        Ok(())
    }

    fn notify(dispatcher: &PluginDispatcher, _data: &mut KernelData) -> KernelRoutineResult<()> {
        execute_request(|| dispatcher.notify(&()))?;
        Ok(())
    }
}

impl KernelRoutine for PluginStep {
    fn execute(
        &self,
        dispatcher: &PluginDispatcher,
        data: &mut KernelData,
    ) -> KernelRoutineResult<()> {
        match self {
            PluginStep::PreFlight => PluginStep::pre_flight(dispatcher, data),
            PluginStep::GetLastRelease => PluginStep::get_last_release(dispatcher, data),
            PluginStep::DeriveNextVersion => PluginStep::derive_next_version(dispatcher, data),
            PluginStep::GenerateNotes => PluginStep::generate_notes(dispatcher, data),
            PluginStep::Prepare => PluginStep::prepare(dispatcher, data),
            PluginStep::VerifyRelease => PluginStep::verify_release(dispatcher, data),
            PluginStep::Commit => PluginStep::commit(dispatcher, data),
            PluginStep::Publish => PluginStep::publish(dispatcher, data),
            PluginStep::Notify => PluginStep::notify(dispatcher, data),
        }
    }
}

fn execute_request<F, T>(request_fn: F) -> Result<Map<String, T>, failure::Error>
where
    F: FnOnce() -> Result<Map<String, PluginResponse<T>>, failure::Error>,
{
    request_fn()?
        .into_iter()
        .map(|(name, r)| {
            r.into_result()
                .map_err(|err| failure::format_err!("Plugin {:?} raised error: {}", name, err))
                .map(|data| (name, data))
        })
        .collect()
}
