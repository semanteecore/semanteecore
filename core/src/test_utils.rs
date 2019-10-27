use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Copied from https://github.com/mitsuhiko/insta/blob/b113499249584cb650150d2d01ed96ee66db6b30/src/runtime.rs#L21-L23
/// License: [Apache-2.0](https://github.com/mitsuhiko/insta/blob/master/LICENSE)
/// Author: Armin Ronacher <armin.ronacher@active-4.com>
lazy_static! {
    static ref WORKSPACES: Mutex<BTreeMap<String, &'static Path>> = Mutex::new(BTreeMap::new());
}

/// Copied from https://github.com/mitsuhiko/insta/blob/b113499249584cb650150d2d01ed96ee66db6b30/src/runtime.rs#L67-L88
/// License: [Apache-2.0](https://github.com/mitsuhiko/insta/blob/master/LICENSE)
/// Author: Armin Ronacher <armin.ronacher@active-4.com>
pub fn get_cargo_workspace(manifest_dir: &str) -> &Path {
    let mut workspaces = WORKSPACES.lock().unwrap();
    if let Some(rv) = workspaces.get(manifest_dir) {
        rv
    } else {
        #[derive(Deserialize)]
        struct Manifest {
            workspace_root: String,
        }
        let output = std::process::Command::new(env!("CARGO"))
            .arg("metadata")
            .arg("--format-version=1")
            .current_dir(manifest_dir)
            .output()
            .unwrap();
        let manifest: Manifest = serde_json::from_slice(&output.stdout).unwrap();
        let path = Box::leak(Box::new(PathBuf::from(manifest.workspace_root)));
        workspaces.insert(manifest_dir.to_string(), path.as_path());
        workspaces.get(manifest_dir).unwrap()
    }
}
