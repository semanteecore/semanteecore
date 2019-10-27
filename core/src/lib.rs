#![feature(try_trait, external_doc)]
#![doc(include = "../../README.md")]

#[macro_use]
extern crate pest_derive;
extern crate semanteecore_plugin_api as plugin_api;

pub mod builtin_plugins;
pub mod config;
pub mod logger;
pub mod runtime;
pub mod utils;

#[cfg(test)]
pub mod test_utils;
