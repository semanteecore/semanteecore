use std::path::{Path, PathBuf};

use petgraph::prelude::NodeIndex;
use petgraph::Graph;

use crate::config::Config;
use crate::plugin_runtime::graph::releaserc::ReleaseRcGraph;
use crate::plugin_runtime::util::load_plugins_for_config;
use crate::plugin_support::flow::{Availability, Value};
use crate::plugin_support::keys::{PROJECT_AND_DEPENDENCIES, PROJECT_ROOT};
use crate::plugin_support::proto::{Project, ProjectAndDependencies};
use crate::plugin_support::{Plugin, PluginInterface};
//
//pub fn workspace_tree(releaserc_graph: ReleaseRcGraph) -> Result<(), failure::Error> {
//    let forest = dependency_forest(releaserc_graph)?;
//
//    forest
//
//    Ok(())
//}

#[derive(Debug)]
struct DependencyTree {
    root: NodeIndex<u32>,
    tree: Graph<Project, ()>,
}

type DependencyForest = Vec<DependencyTree>;

fn dependency_forest(releaserc_graph: ReleaseRcGraph) -> Result<DependencyForest, failure::Error> {
    let subforests = releaserc_graph
        .into_nodes_edges()
        .0
        .into_iter()
        .map(|node| subforest(&node.weight))
        .collect::<Result<Vec<Vec<DependencyTree>>, _>>()?;

    let forest = subforests.into_iter().flat_map(|sub| sub.into_iter()).collect();

    Ok(forest)
}

fn subforest(root: impl AsRef<Path>) -> Result<Vec<DependencyTree>, failure::Error> {
    let releaserc_path = root.as_ref().join("releaserc.toml");
    let config = Config::from_path(&releaserc_path, true)?;

    log::debug!("building subforest for path {}", releaserc_path.display());

    // TODO: sort out this fuckery
    //
    // SURPRISE: we skip the workspace projects here!
    // That's what the long rebases give you, kids.
    let config = match config {
        Config::Workspace(_) => return Ok(vec![]),
        Config::Monoproject(cfg) => cfg,
    };

    let mut plugins = load_plugins_for_config(&config, None)?;
    let plugins = filter_usable_plugins(&mut plugins)?;

    if plugins.is_empty() {
        return Err(failure::format_err!(
            "no plugin supports monorepo projects, cannot proceed"
        ));
    }

    let project_root = Value::with_value(PROJECT_ROOT, serde_json::to_value(root.as_ref())?);

    plugins
        .into_iter()
        .map(|plugin| dependency_tree(plugin, project_root.clone()))
        .collect()
}

fn dependency_tree(
    plugin: &mut Plugin,
    project_root: Value<serde_json::Value>,
) -> Result<DependencyTree, failure::Error> {
    plugin.set_value(PROJECT_ROOT, project_root)?;
    let value = plugin.get_value(PROJECT_AND_DEPENDENCIES)?;
    let (root, dependencies): ProjectAndDependencies = serde_json::from_value(value)?;

    let mut tree = Graph::new();

    let root = tree.add_node(root);
    for dep in dependencies {
        let dep = tree.add_node(dep);
        tree.add_edge(root, dep, ());
    }

    Ok(DependencyTree { root, tree })
}

fn filter_usable_plugins(plugins: &mut [Plugin]) -> Result<Vec<&mut Plugin>, failure::Error> {
    let mut filtered = Vec::new();
    for plugin in plugins {
        // Get keys that plugin can provision
        let caps = plugin.provision_capabilities()?;

        // Iterate through capabilities to find the PROJECTS_PATHS key
        let mut can_provision_project_structure = false;
        for cap in caps {
            if cap.key == PROJECT_AND_DEPENDENCIES {
                // Key must be available always
                if cap.when == Availability::Always {
                    can_provision_project_structure = true;
                } else {
                    log::warn!("invalid configuration of plugin {}", plugin.name);
                    log::warn!(
                        "key {:?} must have {:?}",
                        PROJECT_AND_DEPENDENCIES,
                        Availability::Always
                    );
                }
            }
        }

        if can_provision_project_structure {
            filtered.push(plugin)
        }
    }

    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use crate::plugin_runtime::graph::releaserc::releaserc_graph;
    use crate::plugin_runtime::graph::workspace::dependency_forest;

    use petgraph::dot::Dot;
    use std::path::{Path, PathBuf};

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
    fn semanteecore() {
        let root = crate::test_utils::get_cargo_workspace(env!("CARGO_MANIFEST_DIR"));
        println!("{}", root.display());
        let _guard = pushd(root);

        let releaserc_graph = releaserc_graph(root, true).unwrap();
        let rendered = format!("{:?}", Dot::with_config(&releaserc_graph, PG_CONFIG));
        println!("releaserc_graph:\n{}", rendered);

        let dep_forest = dependency_forest(releaserc_graph).unwrap();
        for tree in dep_forest {
            let root = tree.tree.node_weight(tree.root).unwrap();
            let rendered = format!("{:?}", Dot::with_config(&tree.tree, PG_CONFIG));
            println!("dep_tree({}):\n{}", root.name, rendered);
        }
    }
}
