#![feature(try_trait, external_doc)]
#![doc(include = "../README.md")]

#[macro_use]
extern crate strum_macros;
#[macro_use]
extern crate pest_derive;

pub mod builtin_plugins;
pub mod config;
pub mod logger;
pub mod plugin_runtime;
pub mod plugin_support;
pub mod utils;

use crate::builtin_plugins::{early_exit, EarlyExitPlugin};
use crate::config::Config;
use crate::plugin_runtime::kernel::InjectionTarget;
use crate::plugin_support::PluginStep;
use plugin_runtime::Kernel;
use std::env;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "semanticore 🚀")]
pub struct Args {
    // Dry Run mode: no publishing
    #[structopt(short, long)]
    pub dry: bool,
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: u8,
    /// Silent mode: no logs
    #[structopt(short, long)]
    pub silent: bool,
}

pub fn run(args: Args) -> Result<(), failure::Error> {
    dotenv::dotenv().ok();

    logger::init_logger(args.verbose, args.silent)?;

    log::info!("semanteecore 🚀");

    let config = Config::from_toml("./releaserc.toml", args.dry)?;

    let kernel = Kernel::builder(config)
        .inject_plugin(
            EarlyExitPlugin::new(),
            InjectionTarget::AfterStep(PluginStep::DeriveNextVersion),
        )
        .build()?;

    if let Err(err) = kernel.run() {
        macro_rules! log_error_and_die {
            ($err:expr) => {{
                log::error!("{}", $err);
                std::process::exit(1);
            }};
        }

        match err.downcast::<early_exit::Error>() {
            Ok(ee_error) => match ee_error {
                early_exit::Error::EarlyExit(_) => (),
            },
            Err(other_error) => {
                log_error_and_die!(other_error);
            }
        }
    }

    Ok(())
}