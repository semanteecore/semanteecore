use std::ops::Try;

use super::proto::response::{self, PluginResponse};
use crate::plugin_support::flow::{FlowError, Value};
use std::collections::HashMap;

pub trait PluginInterface {
    /// Get the human-readable name of the plugin
    fn name(&self) -> response::Name;

    /// Get list of keys plugin is capable of provisioning on verious execution steps
    fn provision_capabilities(&self) -> response::ProvisionCapabilities {
        PluginResponse::from_ok(vec![])
    }

    /// Get a value advertised in PluginInterface::provision_capabilities
    fn get_value(&self, key: &str) -> response::GetValue {
        PluginResponse::from_error(FlowError::KeyNotSupported(key.to_owned()).into())
    }

    /// Set a key-value pair in the plugin configuration
    ///
    /// This method is provided and uses the PluginInterface::get_config and PluginInterface::set_config
    /// in order to merge the before and after configuration states
    fn set_value(&mut self, key: &str, value: Value<serde_json::Value>) -> response::Null {
        if log::log_enabled!(log::Level::Trace) {
            let name = self.name()?;
            log::trace!("Setting {}::{} = {:?}", name, key, value);
        }

        let config_json = self.get_config()?;
        let mut config_map: HashMap<String, Value<serde_json::Value>> = serde_json::from_value(config_json)?;
        config_map.insert(key.to_owned(), value);
        let config_json = serde_json::to_value(config_map)?;

        self.set_config(config_json)
    }

    /// Returns plugin configuration encoded as JSON object
    fn get_config(&self) -> response::Config;

    /// Called to override plugin configuration
    fn set_config(&mut self, config: serde_json::Value) -> response::Null;

    /// Called when plugin is required to reset its inner state to initial configuration
    fn reset(&mut self) -> response::Null;

    /// Get list of methods this plugin implements
    fn methods(&self) -> response::Methods {
        PluginResponse::builder()
            .warning("default methods() implementation called: returning an empty map")
            .body(response::MethodsData::default())
    }

    fn pre_flight(&mut self) -> response::Null {
        not_implemented_response()
    }

    fn get_last_release(&mut self) -> response::Null {
        not_implemented_response()
    }

    fn derive_next_version(&mut self) -> response::Null {
        not_implemented_response()
    }

    fn generate_notes(&mut self) -> response::Null {
        not_implemented_response()
    }

    fn prepare(&mut self) -> response::Null {
        not_implemented_response()
    }

    fn verify_release(&mut self) -> response::Null {
        not_implemented_response()
    }

    fn commit(&mut self) -> response::Null {
        not_implemented_response()
    }

    fn publish(&mut self) -> response::Null {
        not_implemented_response()
    }

    fn notify(&self) -> response::Null {
        not_implemented_response()
    }
}

fn not_implemented_response<T>() -> PluginResponse<T> {
    PluginResponse::from_error(failure::err_msg("method not implemented"))
}
