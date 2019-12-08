use cleanroom::command::{Command, Test};
use cleanroom::{run, Args};
use std::path::PathBuf;

#[test]
fn all() -> anyhow::Result<()> {
    run(Args {
        test_subjects: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test_subjects")),
        cmd: Command::Test(Test {
            pattern: None,
            threads: 0,
        }),
    })
}
