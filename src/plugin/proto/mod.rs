pub mod request;
pub mod response;

use serde::{Serialize, Deserialize};

pub type GitRevision = String;

pub type Null = ();

pub type ReleaseNotes = String;

pub type MethodName = String;

pub type Warning = String;

pub type Error = String;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Version {
    pub rev: GitRevision,
    pub semver: Option<semver::Version>,
}
