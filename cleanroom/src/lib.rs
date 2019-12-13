#![feature(generators, generator_trait)]
#![feature(trait_alias)]
#![feature(try_blocks)]

// TODO Document cleanroom library crate

pub mod command;
pub mod test_runner;

pub use self::command::{Cleanroom, CommandExecutor};
pub use command::Cleanroom as Args;

pub fn init_logger_with(v_count: u8, silent: bool) {
    semanteecore::logger::init_logger(v_count, silent).ok();
}

pub fn init_logger() {
    init_logger_with(0, false);
}

pub fn run(args: Args) -> anyhow::Result<()> {
    args.execute(&())
}
