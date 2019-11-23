use crate::config::Config;
use crate::plugin_support::proto::ProjectAndDependencies;
use petgraph::prelude::NodeIndex;
use petgraph::Graph;
use super::{Graph, Id};
use std::fs;
use std::path::{Path, PathBuf};

pub type ReleaseRcGraph = Graph<PathBuf>;
pub type NodeId = Id<PathBuf>;

#[cfg(target_os = "linux")]
fn check_same_file(a: fs::Metadata, b: fs::Metadata) -> bool {
    use std::os::linux::fs::MetadataExt;
    a.st_ino() == b.st_ino()
}

#[cfg(target_os = "macos")]
fn check_same_file(a: fs::Metadata, b: fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    a.st_ino() == b.st_ino()
}

#[cfg(target_os = "windows")]
fn check_same_file(a: fs::Metadata, b: fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    a.creation_time() == b.creation_time()
}

pub fn releaserc_graph(root: impl AsRef<Path>, use_relative_path: bool) -> Result<ReleaseRcGraph, failure::Error> {
    use std::env;

    let mut root = root.as_ref().to_owned();

    if use_relative_path {
        // Check that it's safe to use relative path from here on
        let current_dir = env::current_dir()?;
        assert!(check_same_file(root.metadata()?, current_dir.metadata()?));

        // Convert to relative path
        root = PathBuf::from("./");
    }

    // Check that releaserc.toml exists in root
    assert!(root.join("releaserc.toml").exists());

    let mut graph = Graph::new();
    let mut node_stack = Vec::new();

    recursive_walk(root, &mut graph, &mut node_stack)?;

    Ok(graph)
}

fn recursive_walk(
    dir_path: impl AsRef<Path>,
    graph: &mut ReleaseRcGraph,
    node_stack: &mut Vec<NodeId>,
) -> Result<(), failure::Error> {
    use std::fs::read_dir;

    let dir_path = dir_path.as_ref();
    let mut pushed_node = false;

    let read_dir = match read_dir(&dir_path) {
        Ok(rd) => rd,
        Err(e) => {
            log::warn!("failed to read directory {}: {}", dir_path.display(), e);
            return Ok(());
        }
    };

    let mut entries: Vec<_> = read_dir.filter_map(Result::ok).collect();

    entries.sort_by(|a, b| {
        let a_is_file = a.file_type().unwrap().is_file();
        let b_is_file = b.file_type().unwrap().is_file();
        b_is_file.cmp(&a_is_file)
    });

    for entry in entries {
        let entry_type = entry.file_type()?;

        if entry_type.is_dir() {
            let path = entry.path();
            recursive_walk(path, graph, node_stack)?;
            continue;
        }

        if (entry_type.is_file() || entry_type.is_symlink()) && entry.file_name() == "releaserc.toml" {
            let node_idx = entry
                .path()
                .parent()
                .map(ToOwned::to_owned)
                .map(|path| graph.add_node(path));

            match (node_stack.last(), node_idx) {
                (Some(parent_idx), Some(node_idx)) => {
                    graph.add_edge(*parent_idx, node_idx);
                }
                _ => (),
            }

            if let Some(node_idx) = node_idx {
                node_stack.push(node_idx);
                pushed_node = true;
            }
        }
    }

    if pushed_node == true {
        node_stack.pop();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::dot::Dot;
    use serial_test_derive::serial;
    use std::fs::{self, File};

    const PG_CONFIG: &[petgraph::dot::Config] = &[petgraph::dot::Config::EdgeNoLabel];

    fn pushd(path: impl AsRef<Path>) -> PushdGuard {
        let path = path.as_ref();
        std::env::set_current_dir(path).unwrap();
        PushdGuard(path.to_owned())
    }

    struct PushdGuard(PathBuf);

    impl Drop for PushdGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).unwrap();
        }
    }

    #[test]
    #[serial(current_dir)]
    fn build_releaserc_graph_simple() {
        let dir = tempfile::tempdir().unwrap();
        let _g = pushd(dir.path());
        File::create(dir.path().join("releaserc.toml")).unwrap();
        let graph = releaserc_graph(dir.path(), true).unwrap();
        let rendered = graph.dot_with_config(PG_CONFIG);
        println!("{}", rendered);
        assert_eq!(
            rendered,
            r#"digraph {
    0 [label="\"./\""]
}
"#
        );
    }

    #[test]
    #[serial(current_dir)]
    fn build_releaserc_graph_wrong_file_type() {
        let dir = tempfile::tempdir().unwrap();
        let _g = pushd(dir.path());
        fs::create_dir(dir.path().join("releaserc.toml")).unwrap();
        let graph = releaserc_graph(dir.path(), true).unwrap();
        let rendered = graph.dot_with_config(PG_CONFIG);
        println!("{}", rendered);
        assert_eq!(
            rendered,
            r#"digraph {
}
"#
        );
    }

    #[test]
    #[serial(current_dir)]
    fn find_roots_nested() {
        let dir = tempfile::tempdir().unwrap();
        let _g = pushd(dir.path());

        let expected = &[dir.path().to_owned(), dir.path().join("one"), dir.path().join("two")];

        for d in expected {
            if !d.exists() {
                fs::create_dir(d).unwrap();
            }
            File::create(d.join("releaserc.toml")).unwrap();
        }

        let graph = releaserc_graph(dir.path(), true).unwrap();
        let rendered = graph.dot_with_config(PG_CONFIG);
        println!("{}", rendered);
        assert_eq!(
            rendered,
            r#"digraph {
    0 [label="\"./\""]
    1 [label="\"./one\""]
    2 [label="\"./two\""]
    0 -> 1
    0 -> 2
}
"#
        );
    }

    #[test]
    #[serial(current_dir)]
    #[should_panic]
    fn find_roots_no_releaserc_in_root() {
        let dir = tempfile::tempdir().unwrap();
        let _g = pushd(dir.path());
        let _graph = releaserc_graph(dir.path(), true).unwrap();
    }

    #[test]
    #[serial(current_dir)]
    fn find_roots_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let _g = pushd(dir.path());

        let orig_file_path = dir.path().join("releaserc.toml");
        File::create(&orig_file_path).unwrap();

        let expected = &[dir.path().to_owned(), dir.path().join("one"), dir.path().join("two")];

        for d in expected {
            if !d.exists() {
                fs::create_dir(d).unwrap();
            }
            let file_path = d.join("releaserc.toml");
            if !file_path.exists() {
                symlink::symlink_file(&orig_file_path, &file_path).unwrap();
            }
        }

        let graph = releaserc_graph(dir.path(), true).unwrap();
        let rendered = graph.dot_with_config(PG_CONFIG);
        println!("{}", rendered);
        assert_eq!(
            rendered,
            r#"digraph {
    0 [label="\"./\""]
    1 [label="\"./one\""]
    2 [label="\"./two\""]
    0 -> 1
    0 -> 2
}
"#
        );
    }
}
