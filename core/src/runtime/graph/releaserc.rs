use std::cmp::Reverse;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use derive_more::{Deref, DerefMut};

use super::{Graph, Id, UniqAllocStrategy};

#[derive(Deref, DerefMut)]
pub struct ConfigTree {
    root: Id<PathBuf>,
    #[deref]
    #[deref_mut]
    graph: ConfigGraph,
}

impl ConfigTree {
    pub fn build(root: impl AsRef<Path>, convert_to_relative_path: bool) -> Result<ConfigTree, failure::Error> {
        use std::env;

        let root = root.as_ref().to_path_buf();

        // Check that releaserc.toml exists in root
        let releaserc_file_path = root.join("releaserc.toml");
        if !releaserc_file_path.exists() || !releaserc_file_path.is_file() {
            return Err(failure::format_err!(
                "releaserc.toml not found in {} or is not a file",
                root.display()
            ));
        }

        let mut graph = Graph::uniq();
        let mut node_stack = Vec::new();

        let graph_root;
        let absolute = if convert_to_relative_path {
            graph_root = PathBuf::from("./");
            Some(root.as_ref())
        } else {
            graph_root = root.clone();
            None
        };

        let graph_root_id = graph.add_node(graph_root.clone());

        recursive_walk(absolute, &root, &mut graph, &mut node_stack)?;

        println!("root = {}", root.display());
        println!("graph_root = {}", graph_root.display());
        println!("{}", graph.dot());

        Ok(ConfigTree {
            root: graph_root_id,
            graph,
        })
    }

    pub fn root(&self) -> &PathBuf {
        self.graph
            .node_weight(self.root)
            .expect("root path not found in the graph")
    }
}

type ConfigGraph = Graph<PathBuf, UniqAllocStrategy>;
type NodeId = Id<PathBuf>;

fn recursive_walk(
    absolute_root: Option<&Path>,
    dir_path: impl AsRef<Path>,
    graph: &mut ConfigGraph,
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

    entries.sort_by_key(|e| Reverse(e.file_type().unwrap().is_file()));

    for entry in entries {
        let entry_type = entry.file_type()?;

        if entry_type.is_dir() {
            let path = entry.path();
            recursive_walk(absolute_root.clone(), path, graph, node_stack)?;
            continue;
        }

        if (entry_type.is_file() || entry_type.is_symlink()) && entry.file_name() == "releaserc.toml" {
            let node_idx = entry
                .path()
                .parent()
                .and_then(|p| {
                    if let Some(absolute) = absolute_root {
                        p.strip_prefix(absolute).map(|p| Path::new(".").join(p)).ok()
                    } else {
                        Some(p.to_owned())
                    }
                })
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
        let tree = ConfigTree::build(dir.path(), true).unwrap();
        let rendered = tree.dot_with_config(PG_CONFIG);
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
        let tree = ConfigTree::build(dir.path(), true);
        assert!(tree.is_err())
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

        let tree = ConfigTree::build(dir.path(), true).unwrap();
        let rendered = tree.dot_with_config(PG_CONFIG);
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
    fn find_roots_no_releaserc_in_root() {
        let dir = tempfile::tempdir().unwrap();
        let _g = pushd(dir.path());
        let tree = ConfigTree::build(dir.path(), true);
        assert!(tree.is_err())
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

        let tree = ConfigTree::build(dir.path(), true).unwrap();
        let rendered = tree.dot_with_config(PG_CONFIG);
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
