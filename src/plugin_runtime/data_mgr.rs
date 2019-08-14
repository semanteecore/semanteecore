use failure::Fail;

use crate::config::{Config, Map};
use crate::plugin_runtime::graph::{DestKey, SourceKey};
use crate::plugin_support::flow::{FlowError, Value};
use std::collections::HashSet;

pub struct DataManager {
    configs: Vec<Map<String, Value<serde_json::Value>>>,
    global: Map<String, Vec<serde_json::Value>>,
}

impl DataManager {
    pub fn new(configs: Vec<Map<String, Value<serde_json::Value>>>, releaserc: &Config) -> Self {
        DataManager {
            configs,
            global: releaserc
                .cfg
                .iter()
                .filter(|(_, v)| v.is_value())
                .map(|(k, v)| (k.to_owned(), vec![v.as_value().clone()]))
                .collect(),
        }
    }

    pub fn insert_global(&mut self, key: String, value: Value<serde_json::Value>) {
        if value.is_ready() {
            let vec = self.global.entry(key).or_insert_with(Vec::new);

            let value = value.as_value();
            if !vec.contains(value) {
                vec.push(value.clone());
            }
        }
    }

    // TODO: merging techniques agnostic of destination data type
    pub fn prepare_value(
        &self,
        dst_id: usize,
        dst_key: &DestKey,
        src_key: &SourceKey,
    ) -> Result<Value<serde_json::Value>, failure::Error> {
        let values = self
            .global
            .get(src_key)
            .ok_or_else(|| DataManagerError::DataNotAvailable(src_key.clone()))?;

        let value = match values.len() {
            0 => None,
            1 => Some(values.iter().nth(0).unwrap().clone()),
            multiple => Some(serde_json::to_value(multiple)?),
        };

        if let Some(value) = value {
            Ok(Value::builder(&src_key).value(value).build())
        } else {
            Err(DataManagerError::DataNotAvailable(src_key.clone()).into())
        }
    }

    pub fn prepare_value_same_key(
        &self,
        dst_id: usize,
        dst_key: &DestKey,
    ) -> Result<Value<serde_json::Value>, failure::Error> {
        self.prepare_value(dst_id, dst_key, dst_key)
    }
}

#[derive(Fail, Debug)]
pub enum DataManagerError {
    #[fail(display = "no data available for key {}", _0)]
    DataNotAvailable(String),
}
