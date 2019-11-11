#![feature(try_trait)]
extern crate semanteecore_plugin_api as plugin_api;

mod cargo;
use cargo::Cargo;

use std::ops::Try;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use plugin_api::flow::{FlowError, ProvisionCapability, Value};
use plugin_api::keys::{DRY_RUN, FILES_TO_COMMIT, NEXT_VERSION, PROJECT_AND_DEPENDENCIES, PROJECT_ROOT};
use plugin_api::proto::response::{self, PluginResponse};
use plugin_api::proto::ProjectAndDependencies;
use plugin_api::{PluginInterface, PluginStep};
use std::fs;

#[derive(Default)]
pub struct RustPlugin {
    dry_run_guard: Option<DryRunGuard>,
    config: Config,
}

impl RustPlugin {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Serialize, Deserialize)]
struct Config {
    project_root: Value<String>,
    dry_run: Value<bool>,
    token: Value<String>,
    next_version: Value<semver::Version>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            project_root: Value::protected(PROJECT_ROOT),
            dry_run: Value::protected(DRY_RUN),
            token: Value::load_from_env("CARGO_TOKEN"),
            next_version: Value::builder(NEXT_VERSION)
                .required_at(PluginStep::Prepare)
                .protected()
                .build(),
        }
    }
}

impl Drop for RustPlugin {
    fn drop(&mut self) {
        if let Some(guard) = self.dry_run_guard.as_ref() {
            log::info!("rust(dry-run): restoring original state of Cargo.toml");
            if let Err(err) = fs::write(&guard.original_manifest_path, &guard.original_manifest) {
                log::error!("rust(dry-run): failed to restore original manifest, sorry x_x");
                log::error!("{}", err);
                log::info!(
                    "\nOriginal Cargo.toml: \n{}",
                    String::from_utf8_lossy(&guard.original_manifest)
                );
            }
        }
    }
}

struct DryRunGuard {
    original_manifest: Vec<u8>,
    original_manifest_path: PathBuf,
}

impl PluginInterface for RustPlugin {
    fn name(&self) -> response::Name {
        PluginResponse::from_ok("rust".into())
    }

    fn provision_capabilities(&self) -> response::ProvisionCapabilities {
        PluginResponse::from_ok(vec![
            ProvisionCapability::builder(FILES_TO_COMMIT)
                .after_step(PluginStep::Prepare)
                .build(),
            ProvisionCapability::builder(PROJECT_AND_DEPENDENCIES).build(),
        ])
    }

    fn get_value(&self, key: &str) -> response::GetValue {
        let value = match key {
            "files_to_commit" => serde_json::to_value(vec!["Cargo.toml", "Cargo.lock"])?,
            "project_and_dependencies" => {
                serde_json::to_value(project_and_dependencies(self.config.project_root.as_value())?)?
            }
            _other => return PluginResponse::from_error(FlowError::KeyNotSupported(key.to_owned()).into()),
        };
        PluginResponse::from_ok(value)
    }

    fn get_config(&self) -> response::Config {
        PluginResponse::from_ok(serde_json::to_value(&self.config)?)
    }

    fn set_config(&mut self, config: serde_json::Value) -> response::Null {
        self.config = serde_json::from_value(config)?;
        PluginResponse::from_ok(())
    }

    fn reset(&mut self) -> response::Null {
        *self = Self::default();
        PluginResponse::from_ok(())
    }

    fn methods(&self) -> response::Methods {
        let methods = vec![PluginStep::PreFlight, PluginStep::Prepare, PluginStep::VerifyRelease];
        PluginResponse::from_ok(methods)
    }

    fn pre_flight(&mut self) -> response::Null {
        let mut response = PluginResponse::builder();
        response.body(())
    }

    fn prepare(&mut self) -> response::Null {
        let project_root = self.config.project_root.as_value();
        let is_dry_run = *self.config.dry_run.as_value();

        let mut cargo = Cargo::new(project_root)?;

        // If we're in the dry-run mode, we don't wanna change the Cargo.toml manifest,
        // so we save the original state of it, which would be written to
        if is_dry_run {
            log::info!("rust(dry-run): saving original state of Cargo.toml");

            let guard = DryRunGuard {
                original_manifest: cargo.manifest_raw().to_vec(),
                original_manifest_path: cargo.path().to_owned(),
            };

            self.dry_run_guard.replace(guard);
        }

        let next_version = self.config.next_version.as_value();
        cargo.set_version(next_version)?;
        cargo.flush()?;

        PluginResponse::from_ok(())
    }

    fn verify_release(&mut self) -> response::Null {
        let project_root = self.config.project_root.as_value();

        let cargo = Cargo::new(project_root)?;

        log::info!("Packaging new version, please wait...");
        cargo.package()?;
        log::info!("Package created successfully");

        PluginResponse::from_ok(())
    }

    fn publish(&mut self) -> response::Null {
        let project_root = self.config.project_root.as_value();

        let token = self.config.token.as_value();

        let cargo = Cargo::new(project_root)?;

        log::info!("Publishing new version, please wait...");
        cargo.publish(&token)?;
        log::info!("Package published successfully");

        PluginResponse::from_ok(())
    }
}

fn project_and_dependencies(_root: impl AsRef<Path>) -> Result<ProjectAndDependencies, failure::Error> {
    todo!()
}
