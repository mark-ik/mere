//! The generic graph: [`Graph<N, E>`].

use std::collections::HashMap;

use petgraph::stable_graph::{EdgeIndex, NodeIndex, StableGraph};
use petgraph::visit::{EdgeRef, IntoNodeReferences};
use petgraph::Direction;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::caps::{Classified, Identified, Labeled};
use crate::taxonomy::RelationClass;

/// A stable handle to a node. Survives the removal of other nodes.
pub type NodeKey = NodeIndex;

/// A stable handle to an edge.
pub type EdgeKey = EdgeIndex;

/// A directed graph of nodes `N` and edges `E`.
///
/// Built on petgraph's `StableGraph` (keys survive other removals). The one
/// requirement on `N` is [`Identified`]: the graph maintains an id-to-key index so
/// a node can be found by its stable identity, not only by position. Capability
/// traits on `N` and `E` unlock the richer queries (see the methods bounded on
/// [`Labeled`], [`Classified`], and friends).
pub struct Graph<N: Identified, E> {
    inner: StableGraph<N, E>,
    by_id: HashMap<N::Id, NodeKey>,
}

impl<N: Identified, E> Default for Graph<N, E> {
    fn default() -> Self {
        Self {
            inner: StableGraph::default(),
            by_id: HashMap::new(),
        }
    }
}

impl<N: Identified, E> Graph<N, E> {
    /// A fresh, empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a node, indexed by its identity. If a node with the same id already
    /// exists, its payload is replaced in place (an upsert) and the existing key
    /// is returned, so identity is stable across re-inserts.
    pub fn insert(&mut self, node: N) -> NodeKey {
        let id = node.id().clone();
        if let Some(&key) = self.by_id.get(&id) {
            if let Some(slot) = self.inner.node_weight_mut(key) {
                *slot = node;
            }
            key
        } else {
            let key = self.inner.add_node(node);
            self.by_id.insert(id, key);
            key
        }
    }

    /// The node at `key`.
    pub fn node(&self, key: NodeKey) -> Option<&N> {
        self.inner.node_weight(key)
    }

    /// The node at `key`, mutably. Mutating the id field directly desyncs the
    /// index; re-`insert` instead to change identity.
    pub fn node_mut(&mut self, key: NodeKey) -> Option<&mut N> {
        self.inner.node_weight_mut(key)
    }

    /// The key for a node identity, if present.
    pub fn key_of(&self, id: &N::Id) -> Option<NodeKey> {
        self.by_id.get(id).copied()
    }

    /// The node with the given identity, if present.
    pub fn get(&self, id: &N::Id) -> Option<&N> {
        self.key_of(id).and_then(|key| self.node(key))
    }

    /// Remove the node at `key` and its incident edges, returning the payload.
    pub fn remove(&mut self, key: NodeKey) -> Option<N> {
        let removed = self.inner.remove_node(key);
        if let Some(node) = &removed {
            self.by_id.remove(node.id());
        }
        removed
    }

    /// Connect `from` to `to` with an edge payload, returning its key. Parallel
    /// edges are allowed (this is a multigraph).
    pub fn connect(&mut self, from: NodeKey, to: NodeKey, edge: E) -> EdgeKey {
        self.inner.add_edge(from, to, edge)
    }

    /// The edge at `key`.
    pub fn edge(&self, key: EdgeKey) -> Option<&E> {
        self.inner.edge_weight(key)
    }

    /// The edge at `key`, mutably.
    pub fn edge_mut(&mut self, key: EdgeKey) -> Option<&mut E> {
        self.inner.edge_weight_mut(key)
    }

    /// Remove the edge at `key`, returning its payload.
    pub fn disconnect(&mut self, key: EdgeKey) -> Option<E> {
        self.inner.remove_edge(key)
    }

    /// The number of nodes.
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// The number of edges.
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Every node, as `(key, payload)`.
    pub fn nodes<'a>(&'a self) -> impl Iterator<Item = (NodeKey, &'a N)> + 'a {
        self.inner.node_references().map(|(key, node)| (key, node))
    }

    /// The nodes reachable by one outgoing edge from `key`.
    pub fn out_neighbors<'a>(&'a self, key: NodeKey) -> impl Iterator<Item = NodeKey> + 'a {
        self.inner.neighbors_directed(key, Direction::Outgoing)
    }

    /// Every outgoing edge from `key`, as `(edge, target, payload)`.
    pub fn out_edges<'a>(
        &'a self,
        key: NodeKey,
    ) -> impl Iterator<Item = (EdgeKey, NodeKey, &'a E)> + 'a {
        self.inner
            .edges_directed(key, Direction::Outgoing)
            .map(|edge| (edge.id(), edge.target(), edge.weight()))
    }

    /// The nodes whose payload satisfies `pred`.
    pub fn filter_nodes<'a, P>(&'a self, mut pred: P) -> impl Iterator<Item = (NodeKey, &'a N)> + 'a
    where
        P: FnMut(&N) -> bool + 'a,
    {
        self.nodes().filter(move |(_, node)| pred(node))
    }

    /// Every edge incident to `key`, in either direction, deduplicated (a self-loop
    /// appears once). Used by the edit spine to reap edge bookkeeping when a node is
    /// removed.
    pub fn incident_edges(&self, key: NodeKey) -> Vec<EdgeKey> {
        let mut edges: Vec<EdgeKey> = self
            .inner
            .edges_directed(key, Direction::Outgoing)
            .chain(self.inner.edges_directed(key, Direction::Incoming))
            .map(|edge| edge.id())
            .collect();
        edges.sort();
        edges.dedup();
        edges
    }
}

// Queries unlocked by node capabilities.
impl<N: Identified + Labeled, E> Graph<N, E> {
    /// Every node carrying `tag`.
    pub fn nodes_tagged<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = (NodeKey, &'a N)> + 'a {
        self.filter_nodes(move |node| node.tags().iter().any(|t| t == tag))
    }
}

// Queries unlocked by edge capabilities.
impl<N: Identified, E: Classified> Graph<N, E> {
    /// The outgoing edges from `key` whose relation is `class`, as
    /// `(edge, target)`.
    pub fn out_edges_of_class<'a>(
        &'a self,
        key: NodeKey,
        class: &'a RelationClass,
    ) -> impl Iterator<Item = (EdgeKey, NodeKey)> + 'a {
        self.out_edges(key)
            .filter(move |(_, _, edge)| &edge.class() == class)
            .map(|(edge, target, _)| (edge, target))
    }
}

// Serialization delegates to the inner graph; the id index is rebuilt on load, so
// it is never part of the serialized form and cannot drift from the nodes.
impl<N, E> Serialize for Graph<N, E>
where
    N: Identified + Serialize,
    E: Serialize,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.inner.serialize(serializer)
    }
}

impl<'de, N, E> Deserialize<'de> for Graph<N, E>
where
    N: Identified + Deserialize<'de>,
    E: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let inner = StableGraph::<N, E>::deserialize(deserializer)?;
        let mut by_id = HashMap::new();
        for key in inner.node_indices() {
            if let Some(node) = inner.node_weight(key) {
                by_id.insert(node.id().clone(), key);
            }
        }
        Ok(Self { inner, by_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::Predicated;
    use crate::container::{Container, Relation};
    use crate::taxonomy::{Recognized, RelationClass};

    fn seed() -> Graph<Container, Relation> {
        let mut g = Graph::new();
        let a = g.insert(
            Container::new("a")
                .with_address("https://a.test/")
                .with_title("Article A")
                .with_tag("research"),
        );
        let b = g.insert(Container::new("b").with_title("Article B").with_tag("research"));
        let c = g.insert(Container::new("c").with_tag("aside"));
        g.connect(a, b, Relation::new(RelationClass::recognized(Recognized::Cites)));
        g.connect(a, c, Relation::new(RelationClass::app("mere", 1)));
        g
    }

    #[test]
    fn builds_and_counts() {
        let g = seed();
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn lookup_by_identity() {
        let g = seed();
        let key = g.key_of(&"a".to_string()).expect("a present");
        assert_eq!(g.node(key).unwrap().title(), Some("Article A"));
        assert!(g.get(&"missing".to_string()).is_none());
    }

    #[test]
    fn insert_upserts_on_identity() {
        let mut g = seed();
        let before = g.node_count();
        let key = g.insert(Container::new("a").with_title("Renamed A"));
        assert_eq!(g.node_count(), before, "same id does not add a node");
        assert_eq!(g.node(key).unwrap().title(), Some("Renamed A"));
    }

    #[test]
    fn query_neighbors_and_out_edges() {
        let g = seed();
        let a = g.key_of(&"a".to_string()).unwrap();
        assert_eq!(g.out_neighbors(a).count(), 2);
        let cites: Vec<_> = g
            .out_edges_of_class(a, &RelationClass::recognized(Recognized::Cites))
            .collect();
        assert_eq!(cites.len(), 1, "one cites edge from a");
        let (edge, target) = cites[0];
        assert_eq!(g.node(target).unwrap().id, "b");
        assert_eq!(g.edge(edge).unwrap().predicate(), Some("urn:chart:rel:cites"));
    }

    #[test]
    fn filter_by_tag_via_labeled_capability() {
        let g = seed();
        let mut tagged: Vec<_> = g.nodes_tagged("research").map(|(_, n)| n.id.clone()).collect();
        tagged.sort();
        assert_eq!(tagged, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn remove_drops_node_and_deindexes() {
        let mut g = seed();
        let a = g.key_of(&"a".to_string()).unwrap();
        g.remove(a);
        assert_eq!(g.node_count(), 2);
        assert!(g.get(&"a".to_string()).is_none(), "identity index updated");
    }

    #[test]
    fn serde_round_trip_rebuilds_the_id_index() {
        let g = seed();
        let json = serde_json::to_string(&g).unwrap();
        let back: Graph<Container, Relation> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.node_count(), 3);
        assert_eq!(back.edge_count(), 2);
        // The id index is not serialized; lookup working proves it was rebuilt.
        let a = back.get(&"a".to_string()).expect("a found by id after load");
        assert_eq!(a.title(), Some("Article A"));
    }
}
