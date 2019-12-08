use super::packing::PackGuard;
use super::CommandExecutor;
use crate::test_runner::{TestInfo, TestRunner};
use std::fs::{self, DirEntry};
use std::ops::{Generator, GeneratorState};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(about = "run tests")]
pub struct Test {
    pattern: Option<String>,
    #[structopt(short, long, env = "TEST_THREADS", default_value = "4")]
    // TODO: handle this option
    threads: u32,
}

impl CommandExecutor for Test {
    type Ctx = PathBuf;

    fn execute(self, ctx: &Self::Ctx) -> anyhow::Result<()> {
        // Use the drop-guard to pack repositories back when function returns
        let _pack_guard = PackGuard::unpack(ctx)?;

        let mut tests_generator = self.read_tests(&ctx);
        loop {
            match Pin::new(&mut tests_generator).resume() {
                GeneratorState::Yielded(info) => {
                    // Insert empty line before every test
                    semanteecore::logger::empty_line();
                    TestRunner::run(info)?;
                    continue;
                }
                GeneratorState::Complete(Err(e)) => log::error!("Generator failed: {}", e),
                _ => (),
            }
            break;
        }

        Ok(())
    }
}

trait TestInfoGenerator = Generator<Yield = TestInfo, Return = anyhow::Result<()>>;
trait DirEntryIter = Iterator<Item = DirEntry>;

impl Test {
    fn read_tests<'a>(&'a self, path: &'a Path) -> impl TestInfoGenerator + 'a {
        let contains_pattern = move |dir_entry: &DirEntry| {
            self.pattern.as_ref().map_or(true, |pat| {
                dir_entry.path().to_str().map_or(false, |path| path.contains(pat))
            })
        };

        let filtered_read_dir = |path: &Path| {
            fs::read_dir(path).map(|rd| {
                rd.filter_map(anyhow::Result::ok).filter_map(|entry| {
                    let path = entry.path();
                    let name = path.file_name()?.to_string_lossy().to_string();
                    Some((entry, path, name))
                })
            })
        };

        let dirs_in = move |path: &Path| filtered_read_dir(path).map(|iter| iter.filter(|(_, path, _)| path.is_dir()));

        let releaserc_files_in = move |path: &Path| {
            filtered_read_dir(path).map(|iter| {
                iter.filter(|(_, _, file_name)| file_name.ends_with(".releaserc.toml"))
                    .filter(|(_, path, _)| path.is_file())
            })
        };

        move || {
            // Iterate over domains (1st level)
            for (_, domain_path, domain_name) in dirs_in(path)? {
                // Iterate over tests (2nd level)
                for (test_entry, test_path, test_name) in dirs_in(&domain_path)? {
                    // Skip test if the path doesn't contain the pattern
                    if !contains_pattern(&test_entry) {
                        continue;
                    }

                    for (_, _, subtest_file_name) in releaserc_files_in(&test_path)? {
                        let subtest_name = subtest_file_name.trim_end_matches(".releaserc.toml").to_owned();

                        let diffs_dir = test_path.join("diffs");
                        let artifacts_dir = test_path.join("artifacts").join(&subtest_name);

                        yield TestInfo {
                            path: test_path.clone(),
                            domain: domain_name.clone(),
                            test: test_name.clone(),
                            subtest: subtest_name,
                            subtest_file_name,
                            diffs_dir,
                            artifacts_dir,
                        }
                    }
                }
            }
            Ok(())
        }
    }
}
