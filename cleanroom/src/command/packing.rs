use super::CommandExecutor;
use anyhow::Context;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use tar::HeaderMode;

#[derive(StructOpt, Debug)]
#[structopt(about = "pack all test repositories")]
// Note: this is not a tuple struct because StructOpt cannot process unit structs (duh)
pub struct Pack {}

// That's just for convenience of using the abomination right above
impl Pack {
    pub fn new() -> Self {
        Pack {}
    }
}

#[derive(StructOpt, Debug)]
#[structopt(about = "unpack all test repositories")]
// You again...
pub struct Unpack {}

// Duh
impl Unpack {
    pub fn new() -> Self {
        Unpack {}
    }
}

impl CommandExecutor for Pack {
    type Ctx = PathBuf;
    fn execute(self, ctx: &Self::Ctx) -> anyhow::Result<()> {
        walkdir::WalkDir::new(ctx).into_iter().try_for_each(|entry| {
            let entry = entry?;
            let path = entry.path();
            let is_git_dir = path.is_dir() && path.ends_with(".git");
            if is_git_dir {
                if let Some(repo_path) = path.parent() {
                    if repo_path.ends_with("repository") {
                        if let Err(err) = pack_repo(repo_path) {
                            log::error!("Failed to pack repo {}: {}", repo_path.display(), err);
                        }
                    }
                }
            }
            Ok(())
        })
    }
}

impl CommandExecutor for Unpack {
    type Ctx = PathBuf;
    fn execute(self, ctx: &Self::Ctx) -> anyhow::Result<()> {
        walkdir::WalkDir::new(ctx).into_iter().try_for_each(|entry| {
            let entry = entry?;
            let path = entry.path();
            let is_git_tar = path.is_file() && path.ends_with("git.tar");
            if is_git_tar {
                if let Some(repo_path) = path.parent() {
                    // Filter out anything except `repository/git.tar`
                    if repo_path.ends_with("repository") {
                        if let Err(err) = unpack_repo(repo_path) {
                            log::error!("Failed to unpack repo {}: {}", repo_path.display(), err);
                        }
                    }
                }
            }
            Ok(())
        })
    }
}

pub fn pack_repo(repo_path: &Path) -> anyhow::Result<()> {
    log::info!("Packing repository {}", repo_path.display());

    let git_dir_path = repo_path.join(".git");
    let git_tar_path = repo_path.join("git.tar");
    let git_new_tar_path = repo_path.join("git.tar.new");

    let packing_result: anyhow::Result<()> = try {
        let tarball = File::create(&git_new_tar_path)?;
        let mut archive = tar::Builder::new(tarball);
        archive.mode(HeaderMode::Deterministic);
        archive.append_dir_all("./.git", &git_dir_path)?;
        archive.finish()?;
    };

    packing_result.context("Failed to pack git repository")?;

    fs::remove_dir_all(&git_dir_path)
        .with_context(|| format!("Failed to remove {} directory", git_dir_path.display()))?;

    let replace_archive_result: anyhow::Result<()> = try {
        if git_tar_path.exists() {
            fs::remove_file(&git_tar_path)?;
        }
        fs::rename(&git_new_tar_path, &git_tar_path)?;
    };

    replace_archive_result.context("Failed to replace old archive with new one")?;

    Ok(())
}

pub fn unpack_repo(repo_path: &Path) -> anyhow::Result<()> {
    log::info!("Unpacking repository {}", repo_path.display());
    let git_tar_path = repo_path.join("git.tar");

    let packing_result: anyhow::Result<()> = try {
        let tarball = File::open(&git_tar_path)?;
        let mut archive = tar::Archive::new(tarball);
        archive.unpack(repo_path)?;
    };

    packing_result.context("Failed to unpack git repository")?;

    Ok(())
}

pub struct PackGuard<'a>(&'a PathBuf);

impl<'a> PackGuard<'a> {
    pub fn unpack(path: &'a PathBuf) -> anyhow::Result<Self> {
        Unpack::new().execute(path).map(|_| PackGuard(path))
    }
}

impl<'a> Drop for PackGuard<'a> {
    fn drop(&mut self) {
        if let Err(e) = Pack::new().execute(self.0) {
            log::error!("Failed to pack the repositories: {}", e);
        }
    }
}
