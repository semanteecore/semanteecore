#![feature(generators, generator_trait)]
#![feature(trait_alias)]
#![feature(try_blocks)]

use structopt::StructOpt;

mod command;
mod test_runner;
use self::command::{Cleanroom, CommandExecutor};

fn main() -> anyhow::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    semanteecore::logger::init_logger(0, false).map_err(|e| e.compat())?;

    let _span = semanteecore::logger::span("cleanroom");

    let opt = Cleanroom::from_args();
    opt.execute(&())
}
