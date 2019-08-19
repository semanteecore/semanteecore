#![feature(try_trait, external_doc)]
#![deny(missing_docs)]
#![doc(include = "../README.md")]

#[macro_use]
extern crate strum_macros;
#[macro_use]
extern crate pest_derive;

mod builtin_plugins;
mod config;
mod plugin_runtime;
mod plugin_support;
mod utils;

use crate::config::Config;
use env_logger::fmt::Formatter;
use log::Record;
use plugin_runtime::{Kernel, KernelError};
use std::env;

fn main() {
    if let Err(err) = run() {
        eprintln!("!! Error: {}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<(), failure::Error> {
    init_logger();
    dotenv::dotenv().ok();

    log::info!("semantic.rs 🚀");

    let clap_args = clap::App::new("semantic-rs")
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        .about(clap::crate_description!())
        .arg(
            clap::Arg::with_name("dry")
                .long("dry")
                .help("Execute semantic-rs in dry-run more (no writes or publishes"),
        )
        .get_matches();

    let is_dry_run = clap_args.is_present("dry");

    let config = Config::from_toml("./releaserc.toml", is_dry_run)?;
    let kernel = Kernel::builder(config).build()?;

    if let Err(err) = kernel.run() {
        macro_rules! log_error_and_die {
            ($err:expr) => {{
                log::error!("{}", $err);
                std::process::exit(1);
            }};
        }

        match err.downcast::<KernelError>() {
            Ok(kernel_error) => match kernel_error {
                KernelError::EarlyExit => (),
                _ => log_error_and_die!(kernel_error),
            },
            Err(other_error) => {
                log_error_and_die!(other_error);
            }
        }
    }

    Ok(())
}

fn init_logger() {
    use std::io::Write;

    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }

    let with_prefix =
        |record: &Record, prefix: &'static str, verbose: bool, fmt: &mut Formatter| {
            if !verbose {
                writeln!(fmt, "{}{}", prefix, record.args())
            } else {
                if let Some(module) = record.module_path() {
                    if let Some(line) = record.line() {
                        writeln!(fmt, "{}{}:{}\t{}", prefix, module, line, record.args())
                    } else {
                        writeln!(fmt, "{}{}\t{}", prefix, module, record.args())
                    }
                } else {
                    writeln!(fmt, "{}{}", prefix, record.args())
                }
            }
        };

    env_logger::Builder::from_default_env()
        .format(move |fmt, record| match record.level() {
            log::Level::Info => with_prefix(record, "", false, fmt),
            log::Level::Warn => with_prefix(record, ">> ", false, fmt),
            log::Level::Error => with_prefix(record, "!! ", false, fmt),
            log::Level::Debug => with_prefix(record, "DD ", true, fmt),
            log::Level::Trace => with_prefix(record, "TT ", true, fmt),
        })
        .init();
}
