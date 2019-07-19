use std::fmt::Debug;

use super::{
    proto::{
        request::{self, PluginRequest},
        response::{self, PluginResponse},
        Version,
    },
    PluginStep,
};

use crate::config::{CfgMap, Map};
use crate::plugin::{Plugin, PluginInterface};

pub struct PluginDispatcher {
    config: CfgMap,
    plugins: Vec<Plugin>,
    map: Map<PluginStep, Vec<usize>>,
}

impl PluginDispatcher {
    pub fn new(config: CfgMap, plugins: Vec<Plugin>, map: Map<PluginStep, Vec<usize>>) -> Self {
        PluginDispatcher {
            config,
            plugins,
            map,
        }
    }

    fn dispatch<R, F>(
        &mut self,
        step: PluginStep,
        mut call_fn: F,
    ) -> DispatchedMultiResult<PluginResponse<R>>
    where
        R: Debug,
        F: FnMut(&mut dyn PluginInterface) -> PluginResponse<R>,
    {
        let response_map = self
            .map_plugins(step, |p| {
                log::info!("Invoking plugin '{}'", p.name);
                let response = call_fn(p.as_interface_mut());
                log::debug!("{}: {:?}", p.name, response);
                (p.name.clone(), response)
            })
            .collect();

        Ok(response_map)
    }

    fn dispatch_singleton<R, F>(
        &mut self,
        step: PluginStep,
        call_fn: F,
    ) -> DispatchedSingletonResult<PluginResponse<R>>
    where
        R: Debug,
        F: FnOnce(&mut dyn PluginInterface) -> PluginResponse<R>,
    {
        let name_and_response = self.map_singleton(step, |p| {
            log::info!("Invoking singleton '{}'", p.name);
            let response = call_fn(p.as_interface_mut());
            log::debug!("{}: {:?}", p.name, response);
            (p.name.clone(), response)
        });

        Ok(name_and_response)
    }

    fn map_plugins<'a, F, R>(
        &'a mut self,
        step: PluginStep,
        mut map_fn: F,
    ) -> impl Iterator<Item = R> + 'a
    where
        F: FnMut(&mut Plugin) -> R + 'a,
    {
        self.map
            .get(&step)
            .map(Vec::clone)
            .into_iter()
            .flat_map(|ids| ids.into_iter())
            .map(move |id| map_fn(&mut self.plugins[id]))
    }

    fn map_singleton<F, R>(&mut self, step: PluginStep, map_fn: F) -> R
    where
        F: FnOnce(&mut Plugin) -> R,
    {
        let no_plugins_found_panic = || {
            panic!(
                "no plugins matching the singleton step {:?}: this is a bug, aborting.",
                step
            )
        };
        let too_many_plugins_panic = || {
            panic!(
                "more then one plugin matches the singleton step {:?}: this is a bug, aborting.",
                step
            )
        };

        let ids = self.map.get(&step).unwrap_or_else(no_plugins_found_panic);

        if ids.is_empty() {
            no_plugins_found_panic();
        }

        if ids.len() != 1 {
            too_many_plugins_panic();
        }

        map_fn(&mut self.plugins[ids[0]])
    }
}

pub type DispatchedMultiResult<T> = Result<Map<String, T>, failure::Error>;
pub type DispatchedSingletonResult<T> = Result<(String, T), failure::Error>;

impl PluginDispatcher {
    pub fn pre_flight(&mut self) -> DispatchedMultiResult<response::PreFlight> {
        let cfg = self.config.clone();
        self.dispatch(PluginStep::PreFlight, move |p| {
            p.pre_flight(PluginRequest::with_default_data(cfg.clone()))
        })
    }

    pub fn get_last_release(&mut self) -> DispatchedSingletonResult<response::GetLastRelease> {
        let cfg = self.config.clone();
        self.dispatch_singleton(PluginStep::GetLastRelease, move |p| {
            p.get_last_release(PluginRequest::with_default_data(cfg))
        })
    }

    pub fn derive_next_version(
        &mut self,
        current_version: Version,
    ) -> DispatchedMultiResult<response::DeriveNextVersion> {
        let cfg = self.config.clone();
        self.dispatch(PluginStep::DeriveNextVersion, move |p| {
            p.derive_next_version(PluginRequest::new(cfg.clone(), current_version.clone()))
        })
    }

    pub fn generate_notes(
        &mut self,
        params: request::GenerateNotesData,
    ) -> DispatchedMultiResult<response::GenerateNotes> {
        let cfg = self.config.clone();
        self.dispatch(PluginStep::GenerateNotes, move |p| {
            p.generate_notes(PluginRequest::new(cfg.clone(), params.clone()))
        })
    }

    pub fn prepare(
        &mut self,
        params: request::PrepareData,
    ) -> DispatchedMultiResult<response::Prepare> {
        let cfg = self.config.clone();
        self.dispatch(PluginStep::Prepare, move |p| {
            p.prepare(PluginRequest::new(cfg.clone(), params.clone()))
        })
    }

    pub fn verify_release(&mut self) -> DispatchedMultiResult<response::VerifyRelease> {
        let cfg = self.config.clone();
        self.dispatch(PluginStep::VerifyRelease, move |p| {
            p.verify_release(PluginRequest::with_default_data(cfg.clone()))
        })
    }

    pub fn commit(
        &mut self,
        params: request::CommitData,
    ) -> DispatchedSingletonResult<response::Commit> {
        let cfg = self.config.clone();
        self.dispatch_singleton(PluginStep::Commit, move |p| {
            p.commit(PluginRequest::new(cfg, params))
        })
    }

    pub fn publish(
        &mut self,
        params: request::PublishData,
    ) -> DispatchedMultiResult<response::Publish> {
        let cfg = self.config.clone();
        self.dispatch(PluginStep::Publish, move |p| {
            p.publish(PluginRequest::new(cfg.clone(), params.clone()))
        })
    }

    pub fn notify(
        &mut self,
        params: request::NotifyData,
    ) -> DispatchedMultiResult<response::Notify> {
        let cfg = self.config.clone();
        self.dispatch(PluginStep::Notify, move |p| {
            p.notify(PluginRequest::new(cfg.clone(), params))
        })
    }
}
