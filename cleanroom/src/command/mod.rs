mod new;
mod packing;
mod test;

use self::new::New;
use self::packing::{Pack, Unpack};
use self::test::Test;

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(about = "semanteecore test suite")]
pub struct Cleanroom {
    #[structopt(parse(from_os_str), skip = "./test_subjects")]
    test_subjects: PathBuf,
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt, Debug)]
#[structopt(about = "command to execute")]
pub enum Command {
    New(New),
    Test(Test),
    Pack(Pack),
    Unpack(Unpack),
}

pub trait CommandExecutor {
    type Ctx;
    fn execute(self, ctx: &Self::Ctx) -> anyhow::Result<()>;
}

impl CommandExecutor for Cleanroom {
    type Ctx = ();

    fn execute(self, _ctx: &()) -> anyhow::Result<()> {
        let path = self.test_subjects;
        self.cmd.execute(&path)
    }
}

impl CommandExecutor for Command {
    type Ctx = PathBuf;
    fn execute(self, ctx: &Self::Ctx) -> anyhow::Result<()> {
        match self {
            Command::New(new) => new.execute(ctx),
            Command::Pack(pack) => pack.execute(ctx),
            Command::Unpack(unpack) => unpack.execute(ctx),
            Command::Test(test) => test.execute(ctx),
        }
    }
}
