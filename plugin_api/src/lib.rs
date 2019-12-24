#![feature(try_trait)]
#[macro_use]
extern crate strum_macros;

pub mod command;
pub mod flow;
pub mod keys;
pub mod proto;
pub mod utils;

use std::collections::HashMap;
use std::ops::Try;

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::flow::{FlowError, Value};
use crate::proto::response::{self, PluginResponse};

pub trait PluginInterface {
    fn name(&self) -> response::Name;

    fn provision_capabilities(&self) -> response::ProvisionCapabilities {
        PluginResponse::from_ok(vec![])
    }

    fn get_value(&self, key: &str) -> response::GetValue {
        PluginResponse::from_error(FlowError::KeyNotSupported(key.to_owned()).into())
    }

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

    fn get_config(&self) -> response::Config;

    fn set_config(&mut self, config: serde_json::Value) -> response::Null;

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

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Copy,
    Clone,
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Hash,
    EnumString,
    EnumIter,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PluginStep {
    PreFlight,
    GetLastRelease,
    DeriveNextVersion,
    GenerateNotes,
    Prepare,
    VerifyRelease,
    Commit,
    Publish,
    Notify,
}

impl PluginStep {
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    pub fn kind(self) -> PluginStepKind {
        match self {
            PluginStep::PreFlight
            | PluginStep::DeriveNextVersion
            | PluginStep::Prepare
            | PluginStep::VerifyRelease
            | PluginStep::Publish
            | PluginStep::Notify => PluginStepKind::Shared,
            PluginStep::GetLastRelease | PluginStep::GenerateNotes | PluginStep::Commit => PluginStepKind::Singleton,
        }
    }

    pub fn dry_steps() -> impl Iterator<Item = PluginStep> {
        PluginStep::iter().filter(|s| s.is_dry())
    }

    pub fn wet_steps() -> impl Iterator<Item = PluginStep> {
        PluginStep::iter().filter(|s| s.is_wet())
    }

    pub fn is_dry(self) -> bool {
        match self {
            PluginStep::PreFlight
            | PluginStep::GetLastRelease
            | PluginStep::DeriveNextVersion
            | PluginStep::GenerateNotes
            | PluginStep::Prepare
            | PluginStep::VerifyRelease => true,
            PluginStep::Publish | PluginStep::Notify | PluginStep::Commit => false,
        }
    }

    pub fn is_wet(self) -> bool {
        !self.is_dry()
    }
}

#[derive(Copy, Clone, Debug)]
pub enum PluginStepKind {
    Singleton,
    Shared,
}
