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
use mere_frame::{GraphId, SessionId};
use mere_host_runtime::{GraphSessionManifest, ManifestStore};
use mere_kernel::graph::Graph;

/// Holds every live graph in the application. Wrap in
/// `Entity<GraphRegistry>` and clone the handle to share across
/// windows.
///
/// Each registered graph is associated with a [`SessionId`] (v0:
/// 1:1 with the graph's `GraphId`). Session metadata
/// ([`GraphSessionManifest`]) lives in a sibling
/// `Entity<ManifestStore>` at app scope — the registry holds the
/// runtime graph entities; the manifest store holds durable
/// session identity. Together they describe a session's full live
/// state.
#[derive(Default)]
pub struct GraphRegistry {
    graphs: HashMap<GraphId, Entity<Graph>>,
    /// Reverse index — which session a graph belongs to. v0 maps
    /// 1:1 with `GraphId`; later phases (sub-graphs, branches that
    /// share a session) will see one session map to multiple
    /// graphs.
    graph_to_session: HashMap<GraphId, SessionId>,
}

impl GraphRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an existing graph entity under `id`. Used at startup
    /// to seed the first graph (so its initial nodes are already in
    /// place before any window opens). Implicitly associates the
    /// graph with a fresh `SessionId` (1:1 in v0); use
    /// [`Self::insert_with_session`] when the caller already has a
    /// `SessionId` from a manifest.
    pub fn insert(&mut self, id: GraphId, graph: Entity<Graph>) {
        let session_id = SessionId::new();
        self.graphs.insert(id, graph);
        self.graph_to_session.insert(id, session_id);
    }

    /// Insert a graph entity with an explicit session association.
    /// Used by manifest-restore where the session id is known from
    /// the on-disk manifest.
    pub fn insert_with_session(
        &mut self,
        id: GraphId,
        graph: Entity<Graph>,
        session_id: SessionId,
    ) {
        self.graphs.insert(id, graph);
        self.graph_to_session.insert(id, session_id);
    }

    /// Look up the `SessionId` associated with a `GraphId`.
    pub fn session_for_graph(&self, id: GraphId) -> Option<SessionId> {
        self.graph_to_session.get(&id).copied()
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
    /// minted `GraphId` + `SessionId`. Returns the IDs + handle so
    /// callers can open a window referencing it. The registry
    /// itself notifies observers so any UI element listing graphs
    /// refreshes.
    ///
    /// If `manifest_store` is provided, a fresh
    /// [`GraphSessionManifest`] is also inserted into it (which
    /// marks the session dirty for the next flush). Callers that
    /// don't want manifest tracking (e.g. tests) pass `None`.
    pub fn create_graph(
        registry: &Entity<GraphRegistry>,
        manifest_store: Option<&Entity<ManifestStore>>,
        cx: &mut App,
    ) -> (SessionId, GraphId, Entity<Graph>) {
        Self::create_graph_seeded(registry, manifest_store, cx, |_, _| {})
    }

    /// Same as [`create_graph`] but the new graph is seeded by
    /// running `seed` on the freshly-constructed `Graph` before
    /// it's published to the registry. Useful when a new window
    /// wants its graph to already contain a particular node (e.g.
    /// the intro page).
    pub fn create_graph_seeded(
        registry: &Entity<GraphRegistry>,
        manifest_store: Option<&Entity<ManifestStore>>,
        cx: &mut App,
        seed: impl FnOnce(&mut Graph, &mut Context<Graph>),
    ) -> (SessionId, GraphId, Entity<Graph>) {
        let session_id = SessionId::new();
        let graph_id = GraphId::new();
        let graph_entity = cx.new(|gcx| {
            let mut g = Graph::new();
            seed(&mut g, gcx);
            g
        });
        registry.update(cx, |reg, rcx| {
            reg.graphs.insert(graph_id, graph_entity.clone());
            reg.graph_to_session.insert(graph_id, session_id);
            rcx.notify();
        });
        if let Some(manifests) = manifest_store {
            manifests.update(cx, |store, mcx| {
                store.insert(GraphSessionManifest::new(session_id, graph_id));
                mcx.notify();
            });
            tracing::info!(?session_id, ?graph_id, "session.created");
        } else {
            tracing::info!(?session_id, ?graph_id, "session.created (no manifest store)");
        }
        (session_id, graph_id, graph_entity)
    }
}
