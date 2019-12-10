mod state;
mod workdir;

use self::state::*;
use crate::test_runner::workdir::WorkDir;
use anyhow::{bail, Context};
use git2::DiffFormat;
use serde::{Serialize, Serializer};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestInfo {
    pub path: PathBuf,
    pub domain: String,
    pub test: String,
    pub subtest: String,
    pub subtest_file_name: String,
    pub diffs_dir: PathBuf,
    pub artifacts_dir: PathBuf,
}

pub struct TestRunner<S>(S);

impl<S> TestRunner<S> {
    fn with_state(state: S) -> Self {
        TestRunner(state)
    }
}

/// Preparation stage:
/// Given TestInfo as initial state,
/// 1. collect metadata about initial repository state
/// 2. create workdir and populate it with copies of test items
impl TestRunner<Initial<'_>> {
    pub fn run(semanteecore_path: &Path, info: TestInfo) -> anyhow::Result<()> {
        let runner = TestRunner(Initial {
            semanteecore_path,
            info,
        });

        runner.do_run()
    }

    fn do_run(self) -> anyhow::Result<()> {
        let info = &self.0.info;
        let test_path = &info.path;
        let workdir = WorkDir::create(info)?;

        // Load git index of initial repo
        let repo = git2::Repository::open(&*workdir).context("failed to open git repository")?;

        let index = repo.index().context("failed to load current git index")?;

        // Copy subtest releaserc file into workdir
        let subtest_path = test_path.join(&info.subtest_file_name);
        let releaserc_path = workdir.join("releaserc.toml");
        fs::copy(&subtest_path, &releaserc_path).context("failed to copy subtest releaserc.toml workdir")?;

        // Create subtest artifacts directory
        fs::create_dir(&info.artifacts_dir).ok();

        // Load env (optional)
        let env_path = info.path.join("env");
        if env_path.exists() {
            dotenv::from_path(&env_path).context("Failed to load test env file")?;
        }

        // Progress the state of runner
        let next_state = self.0.progress(InitialToPrepared { workdir, index });

        TestRunner::with_state(next_state).do_run()
    }
}

impl TestRunner<Prepared<'_>> {
    fn do_run(self) -> anyhow::Result<()> {
        let info = self.0.info();
        let semanteecore_path = self.0.semanteecore_path();
        let workdir = self.0.workdir();

        // Run semanteecore
        log::info!("testing {}::{}::{}", info.domain, info.test, info.subtest);

        let status = Command::new(semanteecore_path)
            .args(&["--path", workdir.path().to_str().unwrap()])
            .status()
            .context("failed to run semanteecore")?;

        // If semanteecore have failed, fail the test
        if !status.success() {
            bail!("semanteecore exited with error");
        }

        // Load new index, after semanteecore did some changes
        let repo = git2::Repository::open(workdir.path())?;
        let index = repo.index()?;

        // Progress to the next state of the runner
        let next_state = self.0.progress(PreparedIntoProcessed { repo, index });

        TestRunner::with_state(next_state).do_run()
    }
}

impl TestRunner<Processed> {
    fn do_run(self) -> anyhow::Result<()> {
        self.check_diffs()?;
        self.check_artifacts()
    }

    fn check_diffs(&self) -> anyhow::Result<()> {
        let info = self.0.info();

        // Get the diff, and print it to string
        let diff = self
            .0
            .repo()
            .diff_index_to_index(self.0.old_index(), self.0.new_index(), None)?;
        let mut new_diff = String::new();
        diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
            match line.origin() {
                '+' | '-' | ' ' => new_diff.push(line.origin()),
                _ => {}
            }
            new_diff.push_str(str::from_utf8(line.content()).unwrap());
            true
        })?;

        let diffs_dir = &info.diffs_dir;
        let diff_name = format!("{}.diff", info.subtest);
        match_or_create(diffs_dir, &diff_name, &new_diff)
    }

    fn check_artifacts(&self) -> anyhow::Result<()> {
        self.check_tags_artifact()?;
        Ok(())
    }

    fn check_tags_artifact(&self) -> anyhow::Result<()> {
        let repo = self.0.repo();
        let artifacts_dir = &self.0.info().artifacts_dir;
        let tags = repo.tag_names(None)?;
        let contents = serde_json::to_string_pretty(&SerIter::from(tags.iter()))?;
        match_or_create(artifacts_dir, "tags.json", &contents)
    }
}

fn match_or_create(base_path: &Path, filename: &str, new_contents: &str) -> anyhow::Result<()> {
    let file_path = base_path.join(filename);

    if file_path.exists() {
        let old_contents = fs::read_to_string(&file_path)?;
        let (d, _) = text_diff::diff(&old_contents, new_contents, "");
        if d != 0 {
            text_diff::print_diff(&old_contents, new_contents, "");
            // TODO: support running full test suite regardless of whether tests fail or not
            bail!(
                "New version of {} doesn't match the previous snapshot",
                file_path.display()
            );
        }
    } else {
        log::warn!("previous snapshot was not found for {}", filename);
        log::warn!(
            "please manually check if the snapshot is correct at {}",
            file_path.display()
        );
        fs::write(&file_path, new_contents.as_bytes())
            .with_context(|| format!("Failed to write {}", file_path.display()))?;
    }

    Ok(())
}

// This serde helper struct allows to avoid collecting iterator into serde_json::Value,
// through consuming iterator in the serialization process directly
struct SerIter<I>(RefCell<I>);

impl<I> From<I> for SerIter<I> {
    fn from(iter: I) -> Self {
        SerIter(RefCell::new(iter))
    }
}

// Clippy fires false-positive
#[allow(clippy::while_let_on_iterator)]
impl<I, T> Serialize for SerIter<I>
where
    T: Serialize,
    I: Iterator<Item = T>,
{
    fn serialize<S>(&self, s: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(None)?;
        let mut iter = self.0.borrow_mut();
        while let Some(item) = iter.next() {
            seq.serialize_element(&item)?;
        }
        seq.end()
    }
}
