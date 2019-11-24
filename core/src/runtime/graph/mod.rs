pub mod releaserc;
//pub mod workspace;

use id_arena::Arena;
use safe_graph::Graph as SafeGraph;
use std::marker::PhantomData;

pub use id_arena::Id;

pub struct Graph<N> {
    nodes: Arena<N>,
    graph: SafeGraph<Id<N>, ()>,
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
        self.graph.add_edge(a, b, ());
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
}

impl<N> Graph<N>
where
    N: Eq,
{
    pub fn add_node(&mut self, node: N) -> Id<N> {
        let id = self.node_idx_unchecked(&node).unwrap_or_else(|| self.nodes.alloc(node));

        self.graph.add_node(id)
    }

    pub fn node_idx(&self, node: &N) -> Option<Id<N>> {
        let id = self.node_idx_unchecked(node)?;

        // Node may be deleted from the graph without deallocation
        if self.graph.contains_node(id) {
            Some(id)
        } else {
            None
        }
    }

    #[rustfmt::skip]
    fn node_idx_unchecked(&self, node: &N) -> Option<Id<N>> {
        self.nodes.iter()
            .find(|(_id, n)| *n == node)
            .map(|(id, _)| id)
    }
}

#[cfg(feature = "emit-graphviz")]
use petgraph::{dot, dot::Dot, Graph as PetGraph};

#[cfg(feature = "emit-graphviz")]
impl<N> Graph<N>
where
    N: std::fmt::Debug,
{
    pub fn dot(&self) -> String {
        self.dot_with_config(&[])
    }

    pub fn dot_with_config(&self, config: &[dot::Config]) -> String {
        let pg = self.petgraph();
        format!("{:?}", Dot::with_config(&pg, config))
    }

    pub fn petgraph(&self) -> petgraph::Graph<&N, ()> {
        use std::collections::BTreeMap;

        let mut pg = PetGraph::new();

        let id_mapping: BTreeMap<_, _> = self
            .graph
            .nodes()
            .filter_map(|id| self.nodes.get(id).map(move |node| (id, node)))
            .map(|(id, node_ref)| (id, pg.add_node(node_ref)))
            .collect();

        let arena_id_to_petgraph_id = |id| id_mapping.get(&id).expect("invalid arena id: pergraph id not found");

        self.graph.all_edges().for_each(|(x, y, ..)| {
            let xpg = arena_id_to_petgraph_id(x);
            let ypg = arena_id_to_petgraph_id(y);
            pg.add_edge(*xpg, *ypg, ());
        });

        pg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::collection::{size_range, vec};
    use proptest::prelude::*;
    use proptest_derive::Arbitrary;
    use std::fmt::Debug;
    use std::iter;

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
        fn as_petgraph(mut nodes in any_with::<Vec<i8>>(size_range(0..1000).lift())) {
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
                .for_each(|(a, b)| { pg.add_edge(*a, *b, ()); });

            let graph_dot = graph.dot();
            let pg_dot = format!("{:?}", petgraph::dot::Dot::new(&pg));

            prop_assert_eq!(graph_dot, pg_dot);
        }

    }
}
