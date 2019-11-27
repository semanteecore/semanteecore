use std::path::{Path, PathBuf};
use super::{Graph, Id};

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

type ProjectID = Id<NewProject>;
type ProjectGraph = Graph<NewProject>;

/// A minimal forest of dependencies between projects inside the workspace
#[derive(Deref, DerefMut, Default)]
pub struct WorkspaceDependencyForest {
    roots: Vec<ProjectID>,
    #[deref]
    #[deref_mut]
    forest: ProjectGraph,
}

impl WorkspaceDependencyForest {
    pub fn mirror_vertically(mut self) -> Self {
        self.forest.graph = self.forest.graph.all_edges().map(|(a, b, e)| (b, a, e)).collect();

        let has_no_in_tree_dependencies = |node| {
            self.forest
                .graph
                .neighbors_directed(node, Direction::Incoming)
                .next()
                .is_none()
        };

        self.roots = self
            .forest
            .graph
            .nodes()
            .filter(|&node| has_no_in_tree_dependencies(node))
            .collect();

        self
    }

    pub fn iter_breadth_first(&self) -> impl Iterator<Item = &Project> {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();

        self.roots.iter().for_each(|&id| {
            visited.insert(id);
            queue.push_back(id);
        });

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

impl From<DependencyForest> for WorkspaceDependencyForest {
    fn from(forest: DependencyForest) -> Self {
        let roots = forest.roots;
        let mut forest = forest.forest;

        forest.remove_by(|(id, _)| !roots.contains(&id));

        WorkspaceDependencyForest { roots, forest }
    }
}

/// Combination of dependency trees of every project in the workspace
#[derive(Deref, DerefMut)]
pub struct DependencyForest {
    roots: Vec<ProjectID>,
    #[deref]
    #[deref_mut]
    forest: ProjectGraph,
}

impl DependencyForest {
    pub fn build(config_tree: ConfigTree) -> Result<Self, failure::Error> {
        let mut forest = DependencyForest {
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
        let config = Config::from_path(&releaserc_path, true)?;

        log::debug!("building forest for path {}", releaserc_path.display());

        // TODO: sort out this fuckery
        //
        // SURPRISE: we skip the workspace projects here!
        // That's what the long rebases give you, kids.
        let config = match config {
            Config::Workspace(_) => return Ok(()),
            Config::Monoproject(cfg) => cfg,
        };

        let mut plugins = load_plugins_for_config(&config, None)?;
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

<<<<<<< HEAD
#[cfg(test)]
mod tests {
    use crate::plugin_runtime::graph::releaserc::releaserc_graph;
    use crate::plugin_runtime::graph::workspace::dependency_forest;
=======
// NewProject defines a set of comparison rules that are relevant for the algorithm
#[derive(Deref, DerefMut, AsRef)]
pub struct NewProject(Project);
>>>>>>> 51a6483... feat(core): bottom-to-top dependency graph processing

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
        self.0.name.eq(&other.0.name)
            && self.0.lang.eq(&other.0.lang)
            // Path is not that important, so unless it's known for a fact that the paths are different,
            // we treat them as the same
            && self.0.path.as_ref()
            .map_or(true, |p1| other.0.path.as_ref()
                .map_or(true, |p2| p1.eq(p2)))
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

        let dep_forest = DependencyForest::build(config_tree).unwrap();
        let pg = dep_forest.to_petgraph_map(|node| &node.name);
        println!("dependency_forest:\n{}", Dot::with_config(&pg, PG_CONFIG));

        let workspace_forest = WorkspaceDependencyForest::from(dep_forest);
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
