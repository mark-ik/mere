// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! App-scope graph registry — kurbo/host-neutral port of the legacy
//! gpui-side `mere_host::graph_registry`.
//!
//! Every live `kernel::graph::Graph` in the application sits in
//! this map, keyed by `frame::GraphId`. Panes in a `FrameLayout`
//! carry a `graph_id` on each leaf; the substrate path looks each
//! `graph_id` up here when projecting per-pane content (orrery,
//! workbench tile strip, gloss).
//!
//! Multi-graph-in-window: a single `FrameLayout` can hold leaves
//! with different `graph_id`s. The host walks the layout, looks each
//! up here, renders each pane against its bound graph. Multi-window
//! (Cmd-N) clones a handle to the same registry so windows sharing
//! a `graph_id` see the same `Graph`.
//!
//! v0 stores graphs by value. A future revision may wrap each entry
//! in `Arc<RwLock<Graph>>` (or similar) so multiple panes can read
//! the same graph concurrently while a single writer mutates it;
//! v0's substrate path is single-threaded, so by-value is enough.

use std::collections::HashMap;

use frame::GraphId;
use kernel::graph::Graph;

/// Holds every live graph the app is currently presenting, keyed
/// by `GraphId`.
#[derive(Default)]
pub struct GraphRegistry {
    graphs: HashMap<GraphId, Graph>,
}

impl GraphRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a graph under `id`. Returns the previously-stored
    /// graph if any (usually `None`).
    pub fn insert(&mut self, id: GraphId, graph: Graph) -> Option<Graph> {
        self.graphs.insert(id, graph)
    }

    /// Lookup. Returns `None` when nothing is registered under `id`.
    pub fn get(&self, id: GraphId) -> Option<&Graph> {
        self.graphs.get(&id)
    }

    /// Mutable lookup for in-place edits (add nodes, edges, etc.).
    pub fn get_mut(&mut self, id: GraphId) -> Option<&mut Graph> {
        self.graphs.get_mut(&id)
    }

    /// Remove and return the graph at `id`, if any.
    pub fn remove(&mut self, id: GraphId) -> Option<Graph> {
        self.graphs.remove(&id)
    }

    /// Iterate every `(GraphId, &Graph)` currently registered.
    pub fn iter(&self) -> impl Iterator<Item = (GraphId, &Graph)> {
        self.graphs.iter().map(|(k, v)| (*k, v))
    }

    /// Count of registered graphs.
    pub fn len(&self) -> usize {
        self.graphs.len()
    }

    /// True if no graphs are registered.
    pub fn is_empty(&self) -> bool {
        self.graphs.is_empty()
    }

    /// True iff a graph is registered under `id`.
    pub fn contains(&self, id: GraphId) -> bool {
        self.graphs.contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_id() -> GraphId {
        GraphId::new()
    }

    #[test]
    fn new_registry_is_empty() {
        let reg = GraphRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn insert_get_round_trip() {
        let mut reg = GraphRegistry::new();
        let id = fresh_id();
        reg.insert(id, Graph::new());
        assert!(reg.contains(id));
        assert_eq!(reg.len(), 1);
        assert!(reg.get(id).is_some());
    }

    #[test]
    fn remove_drops_the_graph() {
        let mut reg = GraphRegistry::new();
        let id = fresh_id();
        reg.insert(id, Graph::new());
        let removed = reg.remove(id);
        assert!(removed.is_some());
        assert!(!reg.contains(id));
        assert!(reg.is_empty());
    }

    #[test]
    fn get_mut_allows_in_place_edits() {
        let mut reg = GraphRegistry::new();
        let id = fresh_id();
        reg.insert(id, Graph::new());
        let g = reg.get_mut(id).unwrap();
        g.add_node(
            "test://node".to_string(),
            kernel::geometry::PortablePoint::new(0.0, 0.0),
        );
        assert_eq!(reg.get(id).unwrap().node_count(), 1);
    }
}
