pub mod releaserc;
//pub mod workspace;

use id_arena::Arena;
use safe_graph::Graph as SafeGraph;
use std::marker::PhantomData;

pub use id_arena::Id;

pub struct DefaultAllocStrategy;
pub struct UniqAllocStrategy;

pub struct Graph<N, A = DefaultAllocStrategy> {
    nodes: Arena<N>,
    graph: SafeGraph<Id<N>, ()>,
    _casper: PhantomData<fn() -> A>,
}

impl<N, A> Default for Graph<N, A> {
    fn default() -> Self {
        Graph {
            nodes: Arena::new(),
            graph: SafeGraph::new(),
            _casper: PhantomData,
        }
    }
}

impl<N, A> Graph<N, A> {
    pub fn new() -> Self {
        Self::default()
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

impl<N> Graph<N, DefaultAllocStrategy> {
    pub fn add_node(&mut self, node: N) -> Id<N> {
        let id = self.nodes.alloc(node);
        self.graph.add_node(id)
    }
}

impl<N> Graph<N, UniqAllocStrategy> {
    pub fn uniq() -> Self {
        Graph::default()
    }
}

impl<N> Graph<N, UniqAllocStrategy>
where
    N: Eq,
{
    pub fn add_node(&mut self, node: N) -> Id<N> {
        let id = self.node_idx_unchecked(&node).unwrap_or_else(|| self.nodes.alloc(node));

        self.graph.add_node(id)
    }
}

impl<N, A> Graph<N, A>
where
    N: Eq,
{
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
impl<N, A> Graph<N, A>
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
            .filter_map(|id| Some((id, self.nodes.get(id)?)))
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
    // TODO: test UniqAllocStrategy
    // TODO: test node_idx
}
