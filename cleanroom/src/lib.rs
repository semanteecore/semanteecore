#![feature(generators, generator_trait)]
#![feature(trait_alias)]
#![feature(try_blocks)]

pub mod command;
pub mod test_runner;

pub use self::command::{Cleanroom, CommandExecutor};
pub use command::Cleanroom as Args;

pub fn run(args: Args) -> anyhow::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    let _span = semanteecore::logger::span("cleanroom");
    semanteecore::logger::init_logger(0, false)
        .map_err(|e| log::warn!("{}", e))
        .ok();

    args.execute(&())
}
