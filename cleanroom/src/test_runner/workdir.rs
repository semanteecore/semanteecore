use derive_more::Deref;
use fs_extra::dir::CopyOptions;
use std::fs;
use std::path::{Path, PathBuf};

use super::TestInfo;

#[derive(Deref)]
pub struct WorkDir {
    workdir_path: PathBuf,
}

impl WorkDir {
    // WorkDir is a copy of <..>/test/repository directory
    pub fn create(meta: &TestInfo) -> anyhow::Result<Self> {
        // Derive unique work dir name
        let dir_name = format!("__workdir_{}", meta.subtest);
        let workdir_path = meta.path.join(dir_name);

        // Remove old work dir, if it exists
        if workdir_path.exists() {
            fs::remove_dir_all(&workdir_path)?;
        }

        // Copy `repository` into workdir
        let repo_path = meta.path.join("repository");
        let options = CopyOptions {
            overwrite: true,
            skip_exist: false,
            buffer_size: 0,
            copy_inside: true,
            depth: 0,
        };
        fs_extra::dir::copy(&repo_path, &workdir_path, &options)?;

        Ok(WorkDir { workdir_path })
    }

    pub fn path(&self) -> &Path {
        &self.workdir_path
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        if fs::remove_dir_all(&self.workdir_path).is_err() {
            log::error!(
                "failed to remove work directory '{}', please remove it manually",
                self.workdir_path.display()
            );
        }
    }
}
