pub mod releaserc;
pub mod workspace;

use derive_more::Display;
use id_arena::{Arena, Id};
use safe_graph::Graph as SafeGraph;
use std::fmt::{self, Debug, Display};

#[derive(Debug, Display, Clone, Copy)]
pub struct NullEdge;

pub struct Graph<N> {
    nodes: Arena<N>,
    graph: SafeGraph<Id<N>, NullEdge>,
}

impl<N: Debug> Debug for Graph<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut dbg_set = f.debug_set();
        self.graph
            .nodes()
            // Note:
            //   unknown ID is a bug, but, since it's debug print,
            //   panicking here can cause panic-while-panicking situation in tests.
            .filter_map(|node| self.nodes.get(node))
            .for_each(|node| {
                dbg_set.entry(node);
            });
        dbg_set.finish()
    }
}

impl<N> Default for Graph<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N> Graph<N> {
    pub fn new() -> Self {
        Graph {
            nodes: Arena::new(),
            graph: SafeGraph::new(),
        }
    }

    pub fn add_edge(&mut self, a: Id<N>, b: Id<N>) {
        self.graph.add_edge(a, b, NullEdge);
    }

    pub fn nodes<'a>(&'a self) -> impl Iterator<Item = &'a N> + 'a {
        self.graph
            .nodes()
            .map(move |id| self.nodes.get(id).expect("unknown NodeID"))
    }

    pub fn node_weight(&self, id: Id<N>) -> Option<&N> {
        if self.graph.contains_node(id) {
            self.nodes.get(id)
        } else {
            None
        }
    }

    pub fn node_weight_mut(&mut self, id: Id<N>) -> Option<&mut N> {
        if self.graph.contains_node(id) {
            self.nodes.get_mut(id)
        } else {
            None
        }
    }

    pub fn remove_by(&mut self, should_remove: impl Fn((Id<N>, &N)) -> bool) {
        let mut new = SafeGraph::new();

        for id in self.graph.nodes() {
            let node = self.nodes.get(id).expect("invalid NodeID");
            if !should_remove((id, node)) {
                new.add_node(id);
            }
        }

        for (a, b, _) in self.graph.all_edges() {
            if new.contains_node(a) && new.contains_node(b) {
                new.add_edge(a, b, NullEdge);
            }
        }

        self.graph = new;
    }
}

impl<N> Graph<N>
where
    N: PartialEq,
{
    pub fn add_node(&mut self, node: N) -> Id<N> {
        let id = self.node_idx_unchecked(&node).unwrap_or_else(|| self.nodes.alloc(node));

        self.graph.add_node(id)
    }

    pub fn node_idx(&self, node: &N) -> Option<Id<N>> {
        let id = self.node_idx_unchecked(node)?;

        // Node may be deleted from the graph without deallocation
        Some(id).filter(|_| self.graph.contains_node(id))
    }

    fn node_idx_unchecked(&self, node: &N) -> Option<Id<N>> {
        self.nodes.iter().find_map(|(id, n)| Some(id).filter(|_| n == node))
    }
}

#[cfg(feature = "emit-graphviz")]
mod emit_graphviz {
    use super::*;
    use petgraph::{
        dot,
        dot::{Config, Dot},
        Graph as PetGraph,
    };

    impl<N> Graph<N>
    where
        N: Debug,
    {
        pub fn to_petgraph_map<'a, U>(&'a self, map_fn: impl Fn(&'a N) -> U) -> petgraph::Graph<U, NullEdge> {
            use std::collections::BTreeMap;

            let mut pg = PetGraph::new();

            let id_mapping: Vec<_> = self
                .nodes
                .iter()
                .filter(|(id, _)| self.graph.contains_node(*id))
                .map(|(id, node_ref)| (id, pg.add_node(map_fn(node_ref))))
                .collect();

            assert!(id_mapping.is_sorted_by_key(|(id, _)| *id));

            let arena_id_to_petgraph_id = |arena_id: Id<N>| {
                let idx = id_mapping
                    .binary_search_by_key(&arena_id, |(id, _)| *id)
                    .expect("invalid arena id: pergraph id not found");
                id_mapping[idx].1
            };

            self.graph.all_edges().for_each(|(x, y, ..)| {
                let xpg = arena_id_to_petgraph_id(x);
                let ypg = arena_id_to_petgraph_id(y);
                pg.add_edge(xpg, ypg, NullEdge);
            });

            pg
        }

        pub fn to_petgraph(&self) -> petgraph::Graph<&N, NullEdge> {
            self.to_petgraph_map(|x| x)
        }
    }

    pub trait ToDot {
        fn to_dot(&self) -> String {
            self.to_dot_with_config(&[])
        }

        fn to_dot_with_config(&self, config: &[dot::Config]) -> String;
    }

    impl<N: Debug> ToDot for Graph<N> {
        default fn to_dot_with_config(&self, config: &[Config]) -> String {
            self.to_petgraph().to_dot_with_config(config)
        }
    }

    impl<N: Debug + Display> ToDot for Graph<N> {
        fn to_dot_with_config(&self, config: &[Config]) -> String {
            self.to_petgraph().to_dot_with_config(config)
        }
    }

    impl<N: Debug, E: Debug> ToDot for petgraph::Graph<N, E> {
        default fn to_dot_with_config(&self, config: &[Config]) -> String {
            format!("{:?}", Dot::with_config(self, config))
        }
    }

    impl<N: Display + Debug, E: Display + Debug> ToDot for petgraph::Graph<N, E> {
        fn to_dot_with_config(&self, config: &[Config]) -> String {
            format!("{}", Dot::with_config(self, config))
        }
    }
}

#[cfg(feature = "emit-graphviz")]
pub use emit_graphviz::*;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::collection::size_range;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn insert_and_query(ref nodes in any_with::<Vec<i8>>(size_range(0..1000).lift())) {
            let mut graph = Graph::new();
            for node in nodes {
                let id = graph.add_node(node);
                let query_id = graph.node_idx(&node);
                prop_assert_eq!(Some(id), query_id);

                let query_node = graph.node_weight(id);
                prop_assert_eq!(Some(&node), query_node);
            }
        }

        #[test]
        #[cfg(feature = "emit-graphviz")]
        fn to_petgraph(mut nodes in any_with::<Vec<i8>>(size_range(0..1000).lift())) {
            // Get rid of repetitions 'cause insertion behaviour may vary
            nodes.sort();
            nodes.dedup();

            let mut graph = Graph::new();
            let mut pg = petgraph::Graph::new();

            let graph_ids: Vec<_> = nodes.iter().map(|n| graph.add_node(n)).collect();
            let pg_ids: Vec<_> = nodes.iter().map(|n| pg.add_node(n)).collect();

            graph_ids.iter().zip(graph_ids.iter().rev())
                .for_each(|(a, b)| graph.add_edge(*a, *b));

            pg_ids.iter().zip(pg_ids.iter().rev())
                .for_each(|(a, b)| { pg.add_edge(*a, *b, NullEdge); });

            let graph_dot = graph.to_dot();
            let pg_dot = format!("{:?}", petgraph::dot::Dot::new(&pg));

            prop_assert_eq!(graph_dot, pg_dot);
        }
    }
}
