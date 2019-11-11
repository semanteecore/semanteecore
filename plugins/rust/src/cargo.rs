use std::fs;
use std::path::{Path, PathBuf};

use cargo_metadata::{Metadata, MetadataCommand};
use cargo_toml::Manifest;

use plugin_api::command::PipedCommand;

pub struct Cargo {
    path: PathBuf,
    manifest_raw: Vec<u8>,
    manifest: Manifest,
    metadata: Metadata,
}

impl Cargo {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, failure::Error> {
        let path = path.as_ref().to_path_buf();
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
        log::info!("Setting new version '{}' in Cargo.toml", version);

        let package = self
            .manifest
            .package
            .as_mut()
            .ok_or_else(|| failure::format_err!("[package] section must be present in Cargo.toml"))?;

        package.version = version.to_string();

        Ok(())
    }

    pub fn flush(&self) -> Result<(), failure::Error> {
        let toml = toml::to_string_pretty(&self.manifest)?;
        fs::write(&self.path, toml.as_bytes())?;
        Ok(())
    }
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
