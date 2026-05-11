/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! App-scope graph registry.
//!
//! Every `Entity<Graph>` in the application lives in this map,
//! keyed by [`mere_frame::GraphId`]. Panels (orrery, workbench,
//! gloss, apparatus) carry a `graph_id` on their `FrameLayout`
//! leaf; the host resolves that ID against this registry to get
//! the live `Entity<Graph>` to render.
//!
//! Multi-window today: the registry is shared across every host
//! window via a single `Entity<GraphRegistry>` clone, so
//! `Cmd-N`-opened windows referencing the same `graph_id` see the
//! same graph. Multi-window with *different* `graph_id`s = each
//! window operates on its own graph.
//!
//! Multi-graph-in-window (Phase 1B): a single `FrameLayout` can
//! contain leaves with different `graph_id`s. The host walks the
//! layout, looks each up here, renders the matching panel against
//! its graph.

use std::collections::HashMap;

use gpui::{App, AppContext, Context, Entity};
use mere_frame::GraphId;
use mere_kernel::graph::Graph;

/// Holds every live graph in the application. Wrap in
/// `Entity<GraphRegistry>` and clone the handle to share across
/// windows.
#[derive(Default)]
pub struct GraphRegistry {
    graphs: HashMap<GraphId, Entity<Graph>>,
}

impl GraphRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an existing graph entity under `id`. Used at startup
    /// to seed the first graph (so its initial nodes are already in
    /// place before any window opens).
    pub fn insert(&mut self, id: GraphId, graph: Entity<Graph>) {
        self.graphs.insert(id, graph);
    }

    /// Resolve a `GraphId` to its live entity, if registered.
    pub fn get(&self, id: GraphId) -> Option<&Entity<Graph>> {
        self.graphs.get(&id)
    }

    /// Iterate every registered `(id, &entity)` pair. Used by the
    /// settings palette / "switch graph" UI to enumerate options.
    pub fn iter(&self) -> impl Iterator<Item = (GraphId, &Entity<Graph>)> {
        self.graphs.iter().map(|(k, v)| (*k, v))
    }

    pub fn len(&self) -> usize {
        self.graphs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.graphs.is_empty()
    }

    /// Create a new, empty graph and register it under a freshly
    /// minted `GraphId`. Returns the new ID + handle so callers can
    /// open a window referencing it. The registry itself notifies
    /// observers so any UI element listing graphs refreshes.
    pub fn create_graph(
        registry: &Entity<GraphRegistry>,
        cx: &mut App,
    ) -> (GraphId, Entity<Graph>) {
        let id = GraphId::new();
        let graph_entity = cx.new(|_| Graph::new());
        registry.update(cx, |reg, rcx| {
            reg.graphs.insert(id, graph_entity.clone());
            rcx.notify();
        });
        tracing::info!(?id, "created new graph in registry");
        (id, graph_entity)
    }

    /// Same as [`create_graph`] but the new graph is seeded by
    /// running `seed` on the freshly-constructed `Graph` before
    /// it's published to the registry. Useful when a new window
    /// wants its graph to already contain a particular node (e.g.
    /// the intro page).
    pub fn create_graph_seeded(
        registry: &Entity<GraphRegistry>,
        cx: &mut App,
        seed: impl FnOnce(&mut Graph, &mut Context<Graph>),
    ) -> (GraphId, Entity<Graph>) {
        let id = GraphId::new();
        let graph_entity = cx.new(|gcx| {
            let mut g = Graph::new();
            seed(&mut g, gcx);
            g
        });
        registry.update(cx, |reg, rcx| {
            reg.graphs.insert(id, graph_entity.clone());
            rcx.notify();
        });
        tracing::info!(?id, "created seeded graph in registry");
        (id, graph_entity)
    }
}
