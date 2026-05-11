/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tear-out gestures — spawn a new host window seeded by the active
//! tile of the current window. Three modes, all action-driven (v0:
//! palette / shortcut; Part 2 of the phase wires a drag gesture on
//! top of these methods).
//!
//! Modes:
//!
//! 1. **New graph, minimized orrery** — Mint a fresh graph, copy the
//!    tile's node (by URL), open a window with the workbench
//!    dominating the layout (95/5 split) and a thin orrery strip on
//!    the right. Donor window is unchanged.
//!
//! 2. **New graph, visible orrery** — Same as (1) but 50/50 split so
//!    the new graph is immediately editable in the orrery.
//!
//! 3. **Sticky-note** — Move semantics: close the donor's tile, open
//!    a workbench-only window keyed to the **donor's graph**.
//!    Sticky-note edits propagate live because both windows hold
//!    handles to the same `Entity<Graph>` via the registry. Phase 3
//!    layers diff capture / cross-graph rekey on top of this so the
//!    sticky-note can carry its own graph and merge changes back.

use gpui::Context;
use mere_frame::{
    FrameId, FrameLayout, PaneContent, PaneId, PaneNode, SplitAxis,
};

use crate::graph_registry::GraphRegistry;
use crate::host_helpers::ensure_node_for_address;
use crate::tiles::TileManager;
use crate::HostRoot;

impl HostRoot {
    /// Tear out the active tile into a new window backed by a
    /// freshly-minted graph. `minimized` controls whether the new
    /// window's orrery occupies a thin strip (`true`, 95/5 split) or
    /// half the window (`false`, 50/50 split).
    ///
    /// Donor unchanged — this is a copy, not a move. The donor's
    /// tile/node stay in place; the new graph contains a node with
    /// the same URL.
    pub(crate) fn tear_out_tile_to_new_graph(
        &mut self,
        minimized: bool,
        cx: &mut Context<Self>,
    ) {
        let Some((url, doc)) = self.active_tile_url_and_doc(cx) else {
            tracing::debug!("tear_out_to_new_graph: no active tile to tear out");
            return;
        };

        let registry = self.registry.clone();
        let event_buffer = self.event_buffer.clone();
        let (new_graph_id, new_graph) = GraphRegistry::create_graph(&registry, cx);
        let new_node = new_graph.update(cx, |g, gcx| {
            let key = ensure_node_for_address(g, &url);
            gcx.notify();
            key
        });

        let ratio = if minimized { 0.95 } else { 0.5 };
        let layout = FrameLayout {
            id: FrameId::new(if minimized { "tearout-min" } else { "tearout" }),
            label: format!("Tear-out: {url}"),
            root: PaneNode::Split {
                axis: SplitAxis::Horizontal,
                ratio,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(1),
                    content: PaneContent::Workbench,
                    graph_id: new_graph_id,
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(2),
                    content: PaneContent::Orrery,
                    graph_id: new_graph_id,
                }),
            },
        };

        let mut tiles = TileManager::new();
        tiles.open_or_focus(new_node, doc);

        tracing::info!(
            ?new_graph_id,
            url = %url,
            minimized,
            "tearing out tile to new window with new graph"
        );
        crate::bootstrap::open_tearout_window(
            cx,
            registry,
            new_graph_id,
            new_graph,
            layout,
            tiles,
            url,
            event_buffer,
        );
    }

    /// Tear out the active tile into a **sticky-note window** —
    /// workbench-only, no orrery. The new window references the
    /// **donor's graph** via the registry, so edits propagate live.
    ///
    /// Move semantics: the tile is removed from the donor's
    /// workbench. The node itself stays in the graph (the orrery in
    /// the donor still shows it); only the open-tile binding moves.
    pub(crate) fn tear_out_tile_sticky_note(&mut self, cx: &mut Context<Self>) {
        let Some(donor_pane) = self.active_workbench else {
            tracing::debug!("tear_out_sticky_note: no active workbench");
            return;
        };
        let donor_graph_id = self
            .frame_layout
            .iter_leaves()
            .find(|(p, _, _)| *p == donor_pane)
            .map(|(_, _, gid)| gid)
            .unwrap_or(self.graph_id);
        let Some(donor_graph) = self.registry.read(cx).get(donor_graph_id).cloned() else {
            tracing::warn!(?donor_graph_id, "sticky-note: donor graph missing from registry");
            return;
        };

        // Snapshot the active tile's node + document + URL before we
        // mutate the donor. After this borrow ends we can call
        // close_tile safely.
        let Some((url, doc, node, active_idx)) = self.active_tile_full_snapshot(cx)
        else {
            tracing::debug!("tear_out_sticky_note: no active tile to tear out");
            return;
        };

        // Donor mutation: drop the open-tile binding.
        self.close_tile_in(donor_pane, active_idx, cx);

        // New window: workbench-only leaf bound to the donor graph.
        let layout = FrameLayout {
            id: FrameId::new("sticky-note"),
            label: format!("Sticky note: {url}"),
            root: PaneNode::Leaf {
                pane_id: PaneId(1),
                content: PaneContent::Workbench,
                graph_id: donor_graph_id,
            },
        };
        let mut tiles = TileManager::new();
        tiles.open_or_focus(node, doc);

        let registry = self.registry.clone();
        let event_buffer = self.event_buffer.clone();
        tracing::info!(
            ?donor_graph_id,
            url = %url,
            "tearing out tile as sticky-note (graph shared with donor)"
        );
        crate::bootstrap::open_tearout_window(
            cx,
            registry,
            donor_graph_id,
            donor_graph,
            layout,
            tiles,
            url,
            event_buffer,
        );
    }

    /// Look up the active tile's URL + cloned document, resolved
    /// against whichever graph the active workbench is currently
    /// bound to. Returns `None` if there's no active workbench, no
    /// active tile, or the URL can't be resolved.
    fn active_tile_url_and_doc(
        &self,
        cx: &Context<Self>,
    ) -> Option<(String, inker::EngineDocument)> {
        let pane_id = self.active_workbench?;
        let graph_id = self
            .frame_layout
            .iter_leaves()
            .find(|(p, _, _)| *p == pane_id)
            .map(|(_, _, gid)| gid)?;
        let tiles = self.active_tiles()?;
        let node = tiles.active_node()?;
        let doc = tiles.active_document()?.clone();
        let graph = self.registry.read(cx).get(graph_id).cloned()?;
        let url = graph.read(cx).get_node(node).map(|n| n.url().to_string())?;
        Some((url, doc))
    }

    /// Same as `active_tile_url_and_doc` but also returns the
    /// active tile's `NodeKey` and index in the workbench's tile
    /// list — the bits a sticky-note tear-out needs to (a) seed the
    /// recipient TileManager and (b) close the donor's tile.
    fn active_tile_full_snapshot(
        &self,
        cx: &Context<Self>,
    ) -> Option<(
        String,
        inker::EngineDocument,
        mere_kernel::graph::NodeKey,
        usize,
    )> {
        let pane_id = self.active_workbench?;
        let graph_id = self
            .frame_layout
            .iter_leaves()
            .find(|(p, _, _)| *p == pane_id)
            .map(|(_, _, gid)| gid)?;
        let tiles = self.active_tiles()?;
        let node = tiles.active_node()?;
        let idx = tiles.active_index()?;
        let doc = tiles.active_document()?.clone();
        let graph = self.registry.read(cx).get(graph_id).cloned()?;
        let url = graph.read(cx).get_node(node).map(|n| n.url().to_string())?;
        Some((url, doc, node, idx))
    }
}
