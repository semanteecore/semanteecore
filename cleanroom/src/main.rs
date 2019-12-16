use cleanroom::Args;
use structopt::StructOpt;

fn main() -> anyhow::Result<()> {
    let opt = Args::from_args();
    cleanroom::run(opt)
}
