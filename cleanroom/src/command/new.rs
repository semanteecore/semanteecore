use super::packing::pack_repo;
use super::CommandExecutor;
use anyhow::{bail, Context};
use std::fs;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(about = "create new entity")]
pub enum New {
    Domain(NewDomain),
    Test(NewTest),
    Subtest(NewSubTest),
}

#[derive(StructOpt, Debug)]
#[structopt(about = "create new domain")]
pub struct NewDomain {
    name: String,
}

#[derive(StructOpt, Debug)]
#[structopt(about = "create new test")]
pub struct NewTest {
    domain: String,
    name: String,
}

#[derive(StructOpt, Debug)]
#[structopt(about = "create new subtest")]
pub struct NewSubTest {
    domain: String,
    test: String,
    name: String,
}

impl CommandExecutor for New {
    type Ctx = PathBuf;
    fn execute(self, ctx: &Self::Ctx) -> anyhow::Result<()> {
        match self {
            New::Domain(x) => x.execute(ctx),
            New::Test(x) => x.execute(ctx),
            New::Subtest(x) => x.execute(ctx),
        }
    }
}

impl CommandExecutor for NewDomain {
    type Ctx = PathBuf;
    fn execute(self, ctx: &Self::Ctx) -> anyhow::Result<()> {
        try_create_dir(ctx.join(self.name))?;
        Ok(())
    }
}

impl CommandExecutor for NewTest {
    type Ctx = PathBuf;
    fn execute(self, ctx: &Self::Ctx) -> anyhow::Result<()> {
        let domain_path = ctx.join(&self.domain);

        if !domain_path.exists() {
            bail!("Domain {} does not exist, cannot create test", self.domain);
        }

        let test_path = try_create_dir(domain_path.join(&self.name))?;
        try_create_dir(test_path.join("artifacts"))?;
        try_create_dir(test_path.join("diffs"))?;
        let repo_path = try_create_dir(test_path.join("repository"))?;
        init_repo(&repo_path)?;
        pack_repo(&repo_path)?;

        Ok(())
    }
}

impl CommandExecutor for NewSubTest {
    type Ctx = PathBuf;
    fn execute(self, ctx: &Self::Ctx) -> anyhow::Result<()> {
        let test_path = ctx.join(&self.domain).join(&self.test);

        if !test_path.exists() {
            bail!("Test {} does not exist, cannot create subtest", self.test);
        }

        let subtest_file_name = format!("{}.releaserc.toml", self.name);
        let subtest_path = test_path.join(subtest_file_name);
        if subtest_path.exists() {
            bail!("Subtest {} already exists", self.name);
        }

        let template = include_str!("../../resources/subtest_releaserc_template.toml");
        fs::write(&subtest_path, template.as_bytes())
            .with_context(|| format!("Failed to create file {}", subtest_path.display()))?;

        try_create_dir(test_path.join("artifacts").join(&self.name))?;

        Ok(())
    }
}

fn try_create_dir(path: PathBuf) -> anyhow::Result<PathBuf> {
    fs::create_dir(&path)
        .with_context(|| format!("Failed to create directory {}", path.display()))
        .map(|_| path)
}

fn init_repo(repo_path: &Path) -> anyhow::Result<()> {
    git2::Repository::init(repo_path)
        .context("Failed to init repository")
        .map(|_| ())
}
