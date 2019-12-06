use super::workdir::WorkDir;
use super::TestInfo;
use getset::Getters;
use git2::{Index, Repository};
use std::path::Path;

pub trait Progress {
    type Target;
    type Data;
    fn progress(self, data: Self::Data) -> Self::Target;
}

pub struct Initial<'a> {
    pub semanteecore_path: &'a Path,
    pub info: TestInfo,
}

pub struct InitialToPrepared {
    pub workdir: WorkDir,
    pub index: Index,
}

impl<'a> Progress for Initial<'a> {
    type Target = Prepared<'a>;
    type Data = InitialToPrepared;

    fn progress(self, data: Self::Data) -> Self::Target {
        Prepared {
            semanteecore_path: self.semanteecore_path,
            info: self.info,
            workdir: data.workdir,
            index: data.index,
        }
    }
}

#[derive(Getters)]
pub struct Prepared<'a> {
    #[get = "pub"]
    semanteecore_path: &'a Path,
    #[get = "pub"]
    info: TestInfo,
    #[get = "pub"]
    workdir: WorkDir,
    #[get = "pub"]
    index: Index,
}

pub struct PreparedIntoProcessed {
    pub repo: Repository,
    pub index: Index,
}

impl<'a> Progress for Prepared<'a> {
    type Target = Processed;
    type Data = PreparedIntoProcessed;

    fn progress(self, data: Self::Data) -> Self::Target {
        Processed {
            info: self.info,
            workdir: self.workdir,
            old_index: self.index,
            repo: data.repo,
            new_index: data.index,
        }
    }
}

#[derive(Getters)]
pub struct Processed {
    #[get = "pub"]
    info: TestInfo,
    #[get = "pub"]
    workdir: WorkDir,
    #[get = "pub"]
    old_index: Index,
    #[get = "pub"]
    repo: Repository,
    #[get = "pub"]
    new_index: Index,
}
