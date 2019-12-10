#![feature(generators, generator_trait)]
#![feature(trait_alias)]
#![feature(try_blocks)]

pub mod command;
pub mod test_runner;

pub use self::command::{Cleanroom, CommandExecutor};
pub use command::Cleanroom as Args;

pub fn init_logger(v_count: Option<u8>, silent: Option<bool>) {
    // Reserved for possible future change of defaults
    let v_count = v_count.unwrap_or_default();
    let silent = silent.unwrap_or_default();

    semanteecore::logger::init_logger(v_count, silent).ok();
}

pub fn run(args: Args) -> anyhow::Result<()> {
    args.execute(&())
}
