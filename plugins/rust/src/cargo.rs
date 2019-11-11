use std::fs;
use std::path::{Path, PathBuf};
use std::str::{self, FromStr};

use cargo_metadata::{Metadata, MetadataCommand};
use cargo_toml::Manifest;
use failure::Fail;

use plugin_api::command::PipedCommand;

pub struct Cargo {
    path: PathBuf,
    // TODO Use temporary work directory for dry-run
    // BODY This will allow to get rid of dirty hacks in rust and clog plugins, as well as it would fix a bug with git https-forcing
    manifest_raw: Vec<u8>,
    manifest: Manifest,
    metadata: Metadata,
}

impl Cargo {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, failure::Error> {
        let path = path.as_ref().join("Cargo.toml");
        let manifest_raw = load_manifest_raw(&path)?;
        let manifest = Manifest::from_slice(&manifest_raw)?;
        let metadata = load_metadata(&path)?;
        Ok(Cargo {
            path,
            manifest_raw,
            manifest,
            metadata,
        })
    }

    pub fn manifest_raw(&self) -> &[u8] {
        &self.manifest_raw
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn generate_lockfile(&self) -> Result<(), failure::Error> {
        generate_lockfile(&self.path)
    }

    pub fn refresh(&mut self) -> Result<(), failure::Error> {
        // Reload metadata after updating lockfile
        *self = Self::new(&self.path)?;
        Ok(())
    }

    pub fn package(&self) -> Result<(), failure::Error> {
        let args = &[
            "package",
            "--allow-dirty",
            "--manifest-path",
            &self.path.display().to_string(),
        ];

        PipedCommand::new("cargo", args).join(log::Level::Info)
    }

    pub fn publish(&self, token: &str) -> Result<(), failure::Error> {
        let args = &[
            "publish",
            "--manifest-path",
            &self.path.display().to_string(),
            "--token",
            token,
        ];

        PipedCommand::new("cargo", args).join(log::Level::Info)
    }

    pub fn set_version(&mut self, version: &semver::Version) -> Result<(), failure::Error> {
        use toml_edit::{decorated, Item, Value};
        log::info!("Setting new version '{}' in Cargo.toml", version);

        let manifest = str::from_utf8(&self.manifest_raw)?;
        let mut document = toml_edit::Document::from_str(&manifest)?;

        let new_version = version.to_string();

        let version_value = document
            .as_table_mut()
            .entry("package")
            .as_table_mut()
            .ok_or(Error::InvalidManifest("[package] section is missing"))?
            .entry("version")
            .or_insert(Item::Value(Value::from("")))
            .as_value_mut()
            .unwrap();

        let decor = version_value.decor();
        let new_version_value = decorated(Value::from(new_version.as_str()), decor.prefix(), decor.suffix());
        *version_value = new_version_value;

        let new_manifest = document.to_string_in_original_order();
        fs::write(&self.path, &new_manifest)?;

        Ok(())
    }
}

pub fn generate_lockfile(path: impl AsRef<Path>) -> Result<(), failure::Error> {
    let path = path.as_ref().display().to_string();
    let args = &["generate-lockfile", "--manifest-path", &path];

    PipedCommand::new("cargo", args).join(log::Level::Info)
}

pub fn load_manifest_raw(path: impl AsRef<Path>) -> Result<Vec<u8>, failure::Error> {
    let path = path.as_ref();
    let contents = fs::read(path)
        .map_err(|e| failure::format_err!("failed to read Cargo.toml file at '{}': {}", path.display(), e))?;
    Ok(contents)
}

pub fn load_manifest(path: impl AsRef<Path>) -> Result<Manifest, failure::Error> {
    let raw_manifest = load_manifest_raw(path)?;
    let manifest = Manifest::from_slice(&raw_manifest)?;
    Ok(manifest)
}

pub fn load_metadata(path: impl AsRef<Path>) -> Result<Metadata, failure::Error> {
    let mut cmd = MetadataCommand::new();
    cmd.manifest_path(path);
    let metadata = cmd.exec()?;
    Ok(metadata)
}

#[derive(Fail, Debug)]
enum Error {
    #[fail(display = "ill-formed Cargo.toml manifest: {}", _0)]
    InvalidManifest(&'static str),
}
