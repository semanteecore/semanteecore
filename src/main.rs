use semanteecore::Args;
use structopt::StructOpt;

fn main() {
    let args = Args::from_args();
    if let Err(err) = semanteecore::run(args) {
        eprintln!("!! Error: {}", err);
        std::process::exit(1);
    }
}
