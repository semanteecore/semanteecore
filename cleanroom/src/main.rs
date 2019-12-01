#![feature(generators, generator_trait)]
#![feature(trait_alias)]
#![feature(try_blocks)]

use structopt::StructOpt;

mod command;
mod test_runner;
use self::command::{Cleanroom, CommandExecutor};

use std::io::Write;

fn main() -> anyhow::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    env_logger::Builder::from_default_env()
        .format(|f, r| writeln!(f, "[{}] {}", r.level(), r.args()))
        .init();

    let opt = Cleanroom::from_args();
    println!("{:#?}", opt);
    opt.execute(&())
}
