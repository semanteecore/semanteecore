pub mod response;

use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};
use std::path::PathBuf;

pub type GitRevision = String;

pub type Warning = String;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Version {
    pub rev: GitRevision,
    pub semver: Option<semver::Version>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum NewVersion {
    Revision(GitRevision),
    RevisionAndSemver(GitRevision, semver::Version),
    Semver(semver::Version),
    SemverReq(semver::VersionReq),
    String(String),
}

impl Display for NewVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NewVersion::Revision(rev) => write!(f, "rev={}", rev),
            NewVersion::RevisionAndSemver(rev, semver) => write!(f, "{} ({})", semver, rev),
            NewVersion::Semver(semver) => write!(f, "{}", semver),
            NewVersion::SemverReq(req) => write!(f, "{}", req),
            NewVersion::String(string) => write!(f, "{}", string),
        }
    }
}

impl From<String> for NewVersion {
    /// Parse String into Version
    ///
    /// 1. Try to parse semver::VersionReq
    /// 2. If 1 failed, try to parse semver::Version
    /// 3. If 2 failed, construct Version::String
    fn from(s: String) -> Self {
        if let Ok(v) = s.parse::<semver::VersionReq>() {
            return NewVersion::SemverReq(v);
        }

        if let Ok(v) = s.parse::<semver::Version>() {
            return NewVersion::Semver(v);
        }

        NewVersion::String(s)
    }
}

impl From<semver::Version> for NewVersion {
    fn from(v: semver::Version) -> Self {
        NewVersion::Semver(v)
    }
}

impl From<semver::VersionReq> for NewVersion {
    fn from(v: semver::VersionReq) -> Self {
        NewVersion::SemverReq(v)
    }
}

pub type ProjectAndDependencies = (Project, Vec<Project>);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub name: String,
    pub version: Option<NewVersion>,
    pub lang: Option<String>,
    pub path: Option<PathBuf>,
}

impl Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)?;
        self.version.iter().try_for_each(|v| write!(f, ":{}", v))?;
        self.path.iter().try_for_each(|p| write!(f, " [{}]", p.display()))
    }
}
