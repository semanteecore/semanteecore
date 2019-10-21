pub mod response;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type GitRevision = String;

pub type Warning = String;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Version {
    pub rev: GitRevision,
    pub semver: Option<semver::Version>,
}

pub type ProjectAndDependencies = (Project, Vec<Project>);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub name: String,
    pub version: Option<String>,
    pub lang: Option<String>,
    pub path: Option<PathBuf>,
}
