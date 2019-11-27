use super::{Graph, Id};
use std::path::Path;

use crate::config::Config;
use crate::runtime::graph::releaserc::ConfigTree;
use crate::runtime::plugin::Plugin;
use crate::runtime::util::load_plugins_for_config;
use derive_more::{AsRef, Deref, DerefMut};
use plugin_api::flow::{Availability, Value};
use plugin_api::keys::{PROJECT_AND_DEPENDENCIES, PROJECT_ROOT};
use plugin_api::proto::{Project, ProjectAndDependencies};
use plugin_api::PluginInterface;
use safe_graph::edge::Direction;
use std::collections::{BTreeSet, VecDeque};
use std::convert::AsRef;
use std::fmt::{self, Debug, Display};
use std::mem;

type ProjectID = Id<NewProject>;
type ProjectGraph = Graph<NewProject>;

/// A minimal forest of dependencies between projects inside the workspace
#[derive(Deref, DerefMut, Default)]
pub struct WorkspaceDepForest {
    roots: Vec<ProjectID>,
    #[deref]
    #[deref_mut]
    forest: ProjectGraph,
}

impl WorkspaceDepForest {
    pub fn mirror_vertically(mut self) -> Self {
        // Reverse the direction of the edges: (a -> b) => (b -> a)
        self.forest.graph = self.forest.graph.all_edges().map(|(a, b, edge)| (b, a, edge)).collect();

        // Move the roots out, cause otherwise borrowck is not happy
        let mut roots = mem::replace(&mut self.roots, Vec::new());

        let has_no_in_tree_dependencies = |node| {
            self.forest
                .graph
                .neighbors_directed(node, Direction::Incoming)
                .next()
                .is_none()
        };

        roots.clear();
        roots.extend(
            self.forest
                .graph
                .nodes()
                .filter(|&node| has_no_in_tree_dependencies(node)),
        );

        self.roots = roots;
        self
    }

    // TODO: benchmark against HashMap with some fast hasher
    pub fn iter_breadth_first(&self) -> impl Iterator<Item = &Project> {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();

        visited.extend(self.roots.iter().copied());
        queue.extend(self.roots.iter().copied());

        std::iter::from_fn(move || {
            let next_node_id = queue.pop_front()?;
            let next_node = self.forest.node_weight(next_node_id).expect("unknown NodeID");

            let graph = &self.forest.graph;

            for neighbor_id in graph.neighbors_directed(next_node_id, Direction::Outgoing) {
                if !visited.contains(&neighbor_id) {
                    let has_all_dependencies_satisfied = graph
                        .neighbors_directed(neighbor_id, Direction::Incoming)
                        .all(|id| visited.contains(&id));

                    if has_all_dependencies_satisfied {
                        visited.insert(neighbor_id);
                        queue.push_back(neighbor_id);
                    }
                }
            }

            Some(&next_node.0)
        })
    }
}

impl From<DepForest> for WorkspaceDepForest {
    fn from(forest: DepForest) -> Self {
        let roots = forest.roots;
        let mut forest = forest.forest;

        forest.remove_by(|(id, _)| !roots.contains(&id));

        WorkspaceDepForest { roots, forest }
    }
}

/// Combination of dependency trees of every project in the workspace
#[derive(Deref, DerefMut)]
pub struct DepForest {
    roots: Vec<ProjectID>,
    #[deref]
    #[deref_mut]
    forest: ProjectGraph,
}

impl DepForest {
    pub fn build(config_tree: ConfigTree) -> Result<Self, failure::Error> {
        let mut forest = DepForest {
            roots: Vec::new(),
            forest: Graph::new(),
        };

        config_tree
            .nodes()
            .try_for_each(|node| forest.handle_project_path(node))?;

        Ok(forest)
    }

    fn handle_project_path(&mut self, root: impl AsRef<Path>) -> Result<(), failure::Error> {
        let releaserc_path = root.as_ref().join("releaserc.toml");
        let config = Config::from_toml(&releaserc_path, true)?;

        log::debug!("building forest for path {}", releaserc_path.display());

        // Early return if the releaserc.toml instance is but a mere config layer
        // incapable of harnessing the deadly and unmatched power of plugins
        // FIXME: that should definitely be handled somewhere else
        let plugins_cfg = match config.plugins_cfg() {
            None => return Ok(()),
            Some(cfg) => cfg,
        };

        let mut plugins = load_plugins_for_config(plugins_cfg, None)?;
        let plugins = filter_usable_plugins(&mut plugins)?;

        if plugins.is_empty() {
            return Err(failure::format_err!(
                "no plugin supports monorepo projects, cannot proceed"
            ));
        }

        plugins
            .into_iter()
            .try_for_each(|plugin| self.process_project_with_plugin(plugin, root.as_ref()))
    }

    fn process_project_with_plugin(
        &mut self,
        plugin: &mut Plugin,
        project_root: impl AsRef<Path>,
    ) -> Result<(), failure::Error> {
        let project_root = Value::with_value(PROJECT_ROOT, serde_json::to_value(project_root.as_ref())?);
        plugin.set_value(PROJECT_ROOT, project_root)?;

        let response = plugin.get_value(PROJECT_AND_DEPENDENCIES)?;
        let (root, dependencies): ProjectAndDependencies = serde_json::from_value(response)?;

        let root = self.add_node(NewProject(root));
        for dep in dependencies {
            let dep = self.add_node(NewProject(dep));
            self.add_edge(root, dep);
        }

        self.roots.push(root);

        Ok(())
    }
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

// NewProject defines a set of comparison rules that are relevant for the algorithm
#[derive(Deref, DerefMut, AsRef)]
pub struct NewProject(Project);

impl Debug for NewProject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for NewProject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[rustfmt::skip]
impl PartialEq for NewProject {
    fn eq(&self, other: &Self) -> bool {
        let paths_are_equal = || {
            let p1 = self.0.path.as_ref();
            let p2 = other.0.path.as_ref();
            // Path is not that important, so unless it's known for a fact that the paths are different,
            // we treat them as the same
            if let (Some(p1), Some(p2)) = (p1, p2) {
                p1 == p2
            } else {
                true
            }
        };

        self.0.name.eq(&other.0.name)
            && self.0.lang.eq(&other.0.lang)
            && paths_are_equal()
    }
}

#[cfg(test)]
#[cfg(feature = "emit-graphviz")]
mod tests_with_pg {
    use super::*;
    use crate::runtime::graph::releaserc::ConfigTree;
    use crate::runtime::graph::ToDot;
    use crate::test_utils::pushd;
    use petgraph::dot::{Config, Dot};
    use std::path::{Path, PathBuf};

    const PG_CONFIG: &[Config] = &[Config::EdgeNoLabel];

    #[test]
    #[ignore]
    fn semanteecore() {
        let root = crate::test_utils::get_cargo_workspace(env!("CARGO_MANIFEST_DIR"));
        println!("{}", root.display());
        let _guard = pushd(root);

        let config_tree = ConfigTree::build(root, true).unwrap();
        println!("releaserc_graph:\n{}", config_tree.to_dot_with_config(PG_CONFIG));

        let dep_forest = DepForest::build(config_tree).unwrap();
        let pg = dep_forest.to_petgraph_map(|node| &node.name);
        println!("dependency_forest:\n{}", Dot::with_config(&pg, PG_CONFIG));

        let workspace_forest = WorkspaceDepForest::from(dep_forest);
        let pg = workspace_forest.to_petgraph_map(|node| &node.name);
        println!("workspace_dependency_forest:\n{}", Dot::with_config(&pg, PG_CONFIG));

        let workspace_forest_mirrored = workspace_forest.mirror_vertically();
        let pg = workspace_forest_mirrored.to_petgraph_map(|node| &node.name);
        println!(
            "workspace_dependency_forest_mirrored:\n{}",
            Dot::with_config(&pg, PG_CONFIG)
        );

        let dispatch_sequence: Vec<_> = workspace_forest_mirrored
            .iter_breadth_first()
            .map(|project| format!("{}", project))
            .collect();
        println!("dispatch_sequence:\n{:#?};", dispatch_sequence);
    }
}
