use std::fmt::Debug;

use crate::plugin_support::{
    proto::{
        request::{self, PluginRequest},
        response::{self, PluginResponse},
    },
    Plugin, PluginInterface, PluginStep,
};

use crate::config::Map;
use std::collections::HashMap;

pub struct PluginDispatcher {
    env: HashMap<String, String>,
    plugins: Vec<Plugin>,
    map: Map<PluginStep, Vec<usize>>,
}

impl PluginDispatcher {
    pub fn new(plugins: Vec<Plugin>, map: Map<PluginStep, Vec<usize>>) -> Self {
        PluginDispatcher {
            env: std::env::vars().collect(),
            plugins,
            map,
        }
    }

    fn dispatch<RFR: Debug>(
        &self,
        step: PluginStep,
        call_fn: impl Fn(&mut dyn PluginInterface) -> PluginResponse<RFR>,
    ) -> DispatchedMultiResult<PluginResponse<RFR>> {
        let mut response_map = Map::new();

        for plugin in self.mapped_plugins(step) {
            log::info!("Invoking plugin '{}'", plugin.name);
            let response = call_fn(&mut **plugin.as_interface());
            log::debug!("{}: {:?}", plugin.name, response);
            response_map.insert(plugin.name.clone(), response);
        }

        Ok(response_map)
    }

    fn dispatch_singleton<RFR: Debug>(
        &self,
        step: PluginStep,
        call_fn: impl FnOnce(&mut dyn PluginInterface) -> PluginResponse<RFR>,
    ) -> DispatchedSingletonResult<PluginResponse<RFR>> {
        let plugin = self.mapped_singleton(step);
        log::info!("Invoking singleton '{}'", plugin.name);
        let response = call_fn(&mut **plugin.as_interface());
        log::debug!("{}: {:?}", plugin.name, response);
        Ok((plugin.name.clone(), response))
    }

    fn mapped_plugins(&self, step: PluginStep) -> impl Iterator<Item = &Plugin> {
        self.map
            .get(&step)
            .map(Vec::clone)
            .into_iter()
            .flat_map(|ids| ids.into_iter())
            .map(move |id| &self.plugins[id])
    }

    fn mapped_singleton(&self, step: PluginStep) -> &Plugin {
        let no_plugins_found_panic = || -> ! {
            panic!(
                "no plugins matching the singleton step {:?}: this is a bug, aborting.",
                step
            )
        };

        let too_many_plugins_panic = || -> ! {
            panic!(
                "more then one plugin matches the singleton step {:?}: this is a bug, aborting.",
                step
            )
        };

        match self.map.get(&step).map(Vec::as_slice) {
            None | Some([]) => no_plugins_found_panic(),
            Some([single]) => &self.plugins[*single],
            _ => too_many_plugins_panic(),
        }
    }
}

pub type DispatchedMultiResult<T> = Result<Map<String, T>, failure::Error>;
pub type DispatchedSingletonResult<T> = Result<(String, T), failure::Error>;

impl PluginDispatcher {
    pub fn pre_flight(&self) -> DispatchedMultiResult<response::PreFlight> {
        self.dispatch(PluginStep::PreFlight, |p| {
            p.pre_flight(PluginRequest::new_null(&self.env))
        })
    }

    pub fn get_last_release(&self) -> DispatchedSingletonResult<response::GetLastRelease> {
        self.dispatch_singleton(PluginStep::GetLastRelease, move |p| {
            p.get_last_release(PluginRequest::new_null(&self.env))
        })
    }

    pub fn derive_next_version(
        &self,
        current_version: &request::DeriveNextVersionData,
    ) -> DispatchedMultiResult<response::DeriveNextVersion> {
        self.dispatch(PluginStep::DeriveNextVersion, |p| {
            p.derive_next_version(PluginRequest::new(&self.env, current_version))
        })
    }

    pub fn generate_notes(
        &self,
        params: &request::GenerateNotesData,
    ) -> DispatchedMultiResult<response::GenerateNotes> {
        self.dispatch(PluginStep::GenerateNotes, |p| {
            p.generate_notes(PluginRequest::new(&self.env, params))
        })
    }

    pub fn prepare(
        &self,
        params: &request::PrepareData,
    ) -> DispatchedMultiResult<response::Prepare> {
        self.dispatch(PluginStep::Prepare, |p| {
            p.prepare(PluginRequest::new(&self.env, params))
        })
    }

    pub fn verify_release(&self) -> DispatchedMultiResult<response::VerifyRelease> {
        self.dispatch(PluginStep::VerifyRelease, |p| {
            p.verify_release(PluginRequest::new_null(&self.env))
        })
    }

    pub fn commit(
        &self,
        params: &request::CommitData,
    ) -> DispatchedSingletonResult<response::Commit> {
        self.dispatch_singleton(PluginStep::Commit, move |p| {
            p.commit(PluginRequest::new(&self.env, params))
        })
    }

    pub fn publish(
        &self,
        params: &request::PublishData,
    ) -> DispatchedMultiResult<response::Publish> {
        self.dispatch(PluginStep::Publish, |p| {
            p.publish(PluginRequest::new(&self.env, params))
        })
    }

    pub fn notify(&self, params: &request::NotifyData) -> DispatchedMultiResult<response::Notify> {
        self.dispatch(PluginStep::Notify, |p| {
            p.notify(PluginRequest::new(&self.env, params))
        })
    }
}
