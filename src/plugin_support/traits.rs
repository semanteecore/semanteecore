use std::ops::Try;

use super::proto::{
    request,
    response::{self, PluginResponse},
};
use crate::plugin_support::flow::FlowError;

pub trait PluginInterface {
    fn name(&self) -> response::Name;

    fn provision_capabilities(&self) -> response::ProvisionCapabilities {
        PluginResponse::from_ok(vec![])
    }

    fn provision(&self, req: request::Provision) -> response::Provision {
        PluginResponse::from_error(FlowError::KeyNotSupported(req.data.clone()).into())
    }

    fn get_default_config(&self) -> response::Config;

    fn set_config(&mut self, req: request::Config) -> response::Null;

    fn methods(&self, _req: request::Methods) -> response::Methods {
        PluginResponse::builder()
            .warning("default methods() implementation called: returning an empty map")
            .body(response::MethodsData::default())
            .build()
    }

    fn pre_flight(&mut self, _params: request::PreFlight) -> response::PreFlight {
        not_implemented_response()
    }

    fn get_last_release(&mut self, _params: request::GetLastRelease) -> response::GetLastRelease {
        not_implemented_response()
    }

    fn derive_next_version(
        &mut self,
        _params: request::DeriveNextVersion,
    ) -> response::DeriveNextVersion {
        not_implemented_response()
    }

    fn generate_notes(&mut self, _params: request::GenerateNotes) -> response::GenerateNotes {
        not_implemented_response()
    }

    fn prepare(&mut self, _params: request::Prepare) -> response::Prepare {
        not_implemented_response()
    }

    fn verify_release(&mut self, _params: request::VerifyRelease) -> response::VerifyRelease {
        not_implemented_response()
    }

    fn commit(&mut self, _params: request::Commit) -> response::Commit {
        not_implemented_response()
    }

    fn publish(&mut self, _params: request::Publish) -> response::Publish {
        not_implemented_response()
    }

    fn notify(&self, _params: request::Notify) -> response::Notify {
        not_implemented_response()
    }
}

fn not_implemented_response<T>() -> PluginResponse<T> {
    PluginResponse::from_error(failure::err_msg("method not implemented"))
}
