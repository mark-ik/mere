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

use gpui::{Context, IntoElement, Render, Window, div, prelude::*, px, rgb};
use mere_frame::{
    FrameId, FrameLayout, PaneContent, PaneId, PaneNode, SplitAxis,
};
use mere_host_runtime::TileManager;

use crate::graph_registry::GraphRegistry;
use crate::host_helpers::find_or_create_node_for_address;
use crate::HostRoot;

/// Re-export so call sites (and ide-completable references) stay
/// `crate::tearout::TileDragPayload` — the type itself lives in the
/// portable runtime crate.
pub use mere_host_runtime::TileDragPayload;

/// gpui view rendered as the drag visual while a tile is being
/// pulled out. v0: a small labelled chip following the cursor.
///
/// Stays in `mere-host` (not in `mere-host-runtime`) because it's a
/// gpui `Render` impl — the host adapter owns paint, the runtime
/// owns state.
pub struct DraggedTileLabel {
    pub text: String,
}

/// Drag visual for a pane being relocated. Same shape as
/// `DraggedTileLabel`, just visually distinct (pane chrome darker
/// + label = pane kind tag) so users see which gesture is active.
pub struct DraggedPaneLabel {
    pub text: String,
}

impl gpui::Render for DraggedPaneLabel {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        gpui::div()
            .bg(gpui::rgb(0x3a4060))
            .text_color(gpui::rgb(0xf0f0f0))
            .px_3()
            .py(gpui::px(4.0))
            .text_xs()
            .border_1()
            .border_color(gpui::rgb(0x6090d0))
            .rounded_md()
            .shadow_lg()
            .child(format!("\u{2630} {}", self.text))
    }
}

impl Render for DraggedTileLabel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .bg(rgb(0x223a55))
            .text_color(rgb(0xf0f0f0))
            .px_3()
            .py(px(4.0))
            .text_xs()
            .border_1()
            .border_color(rgb(0x5680c0))
            .rounded_md()
            .shadow_lg()
            .child(self.text.clone())
    }
}

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
        let manifests = self.manifests.clone();
        let event_buffer = self.event_buffer.clone();
        let (_new_session_id, new_graph_id, new_graph) =
            GraphRegistry::create_graph(&registry, Some(&manifests), cx);
        let new_node = new_graph.update(cx, |g, gcx| {
            let (key, _was_created) = find_or_create_node_for_address(g, &url, None);
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
        tiles.open_or_focus(new_node, url.clone(), doc);

        tracing::info!(
            ?new_graph_id,
            url = %url,
            minimized,
            "tearing out tile to new window with new graph"
        );
        crate::bootstrap::open_tearout_window(
            cx,
            registry,
            manifests,
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
    ///
    /// Thin wrapper around [`Self::tear_out_tile_sticky_note_for`]
    /// that resolves the active workbench + active tile index.
    /// Drag-driven tear-out goes through `_for` directly with the
    /// payload's pane_id + index, since "active" at drop time may
    /// have moved.
    pub(crate) fn tear_out_tile_sticky_note(&mut self, cx: &mut Context<Self>) {
        let Some(donor_pane) = self.active_workbench else {
            tracing::debug!("tear_out_sticky_note: no active workbench");
            return;
        };
        let Some(active_idx) = self
            .panes
            .get(&donor_pane)
            .and_then(|s| s.as_workbench())
            .and_then(|w| w.tiles.active_index())
        else {
            tracing::debug!("tear_out_sticky_note: no active tile to tear out");
            return;
        };
        self.tear_out_tile_sticky_note_for(donor_pane, active_idx, cx);
    }

    /// Tear out the active tile as a **branch** — Phase 3
    /// implementation. Creates a new graphlet inside the donor's
    /// forme (via the workbench's `GraphTree<NodeKey>` from
    /// node-per-tile v1), opens a new window pointed at the same
    /// donor session+graph but with the new graphlet as the leaf's
    /// initial focus. Donor unchanged.
    ///
    /// Per the [tear-out operations brief](../../../design_docs/mere_docs/research/2026-05-11_tearout_operations_brief.md)
    /// §4.2.
    pub(crate) fn tear_out_tile_as_branch_for(
        &mut self,
        donor_pane: PaneId,
        tile_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(donor_graph_id) = self
            .frame_layout
            .iter_leaves()
            .find(|(p, _, _)| *p == donor_pane)
            .map(|(_, _, gid)| gid)
        else {
            tracing::warn!(?donor_pane, "branch tear-out: pane not in layout");
            return;
        };
        let Some(donor_graph) = self.registry.read(cx).get(donor_graph_id).cloned() else {
            tracing::warn!(?donor_graph_id, "branch tear-out: donor graph missing");
            return;
        };
        let Some((url, doc, node)) =
            self.indexed_tile_snapshot(donor_pane, tile_index, donor_graph_id, cx)
        else {
            tracing::debug!(?donor_pane, tile_index, "branch tear-out: no tile to branch from");
            return;
        };

        // Mint a new graphlet in the donor's workbench tree
        // anchored at the tile's node. Binding records the parent
        // spec when the donor's tree already has a graphlet for
        // this node; otherwise it's an unlinked session graphlet.
        let new_graphlet_id = self
            .panes
            .get_mut(&donor_pane)
            .and_then(|s| s.as_workbench_mut())
            .map(|workbench| {
                let next_id = workbench.tree.graphlets().len() as u32;
                let parent_spec = workbench
                    .tree
                    .graphlet_of(&node)
                    .map(|g| forme::GraphletSpec {
                        kind: g.kind.clone().unwrap_or(forme::GraphletKind::Session),
                        anchors: g.anchors.iter().map(|n| format!("{n:?}")).collect(),
                        primary_anchor: g.primary_anchor.as_ref().map(|n| format!("{n:?}")),
                        selectors: Vec::new(),
                    });
                let binding = match parent_spec {
                    Some(spec) => forme::GraphletBinding::Forked {
                        parent_spec: spec,
                        reason: "tearout-branch".to_string(),
                    },
                    None => forme::GraphletBinding::UnlinkedSession,
                };
                let graphlet = forme::GraphletRef {
                    id: next_id,
                    anchors: vec![node],
                    primary_anchor: Some(node),
                    binding,
                    kind: Some(forme::GraphletKind::Session),
                };
                workbench.tree.add_graphlet(graphlet);
                next_id
            });

        let Some(graphlet_id) = new_graphlet_id else {
            tracing::warn!(?donor_pane, "branch tear-out: donor pane not a workbench");
            return;
        };

        tracing::info!(
            ?donor_graph_id,
            graphlet_id,
            url = %url,
            "session.branched (donor session shared)"
        );

        // Open the branch window: workbench-only, bound to donor
        // graph (same session — branches live inside the donor
        // session per the brief's §4.2).
        let layout = FrameLayout {
            id: FrameId::new("branch"),
            label: format!("Branch: {url}"),
            root: PaneNode::Leaf {
                pane_id: PaneId(1),
                content: PaneContent::Workbench,
                graph_id: donor_graph_id,
            },
        };
        let mut tiles = TileManager::new();
        tiles.open_or_focus(node, url.clone(), doc);

        let registry = self.registry.clone();
        let manifests = self.manifests.clone();
        let event_buffer = self.event_buffer.clone();
        crate::bootstrap::open_tearout_window(
            cx,
            registry,
            manifests,
            donor_graph_id,
            donor_graph,
            layout,
            tiles,
            url,
            event_buffer,
        );
    }

    /// Sticky-note tear-out for an **explicit** `(donor_pane,
    /// tile_index)` pair. Used by drag-drop where the active tile at
    /// drop time may not be the one the user grabbed.
    pub(crate) fn tear_out_tile_sticky_note_for(
        &mut self,
        donor_pane: PaneId,
        tile_index: usize,
        cx: &mut Context<Self>,
    ) {
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

        // Snapshot the indexed tile's node + document + URL before
        // we mutate the donor. After this borrow ends we can call
        // close_tile safely.
        let Some((url, doc, node)) =
            self.indexed_tile_snapshot(donor_pane, tile_index, donor_graph_id, cx)
        else {
            tracing::debug!(
                ?donor_pane,
                tile_index,
                "tear_out_sticky_note_for: no tile at that index to tear out"
            );
            return;
        };

        // Donor mutation: drop the open-tile binding.
        self.close_tile_in(donor_pane, tile_index, cx);

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
        tiles.open_or_focus(node, url.clone(), doc);

        let registry = self.registry.clone();
        let manifests = self.manifests.clone();
        let event_buffer = self.event_buffer.clone();
        tracing::info!(
            ?donor_graph_id,
            url = %url,
            "tearing out tile as sticky-note (graph shared with donor)"
        );
        crate::bootstrap::open_tearout_window(
            cx,
            registry,
            manifests,
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

    /// Snapshot the URL + cloned document + `NodeKey` for the tile
    /// at `(pane_id, tile_index)`, resolved against `graph_id`.
    /// Returns `None` if the pane isn't a workbench, the index is
    /// out of range, or the URL can't be resolved.
    fn indexed_tile_snapshot(
        &self,
        pane_id: PaneId,
        tile_index: usize,
        graph_id: mere_frame::GraphId,
        cx: &Context<Self>,
    ) -> Option<(String, inker::EngineDocument, mere_kernel::graph::NodeKey)> {
        let workbench = self.panes.get(&pane_id)?.as_workbench()?;
        let node = *workbench.tiles.open_tiles().get(tile_index)?;
        let doc = workbench.tiles.document_for(node)?.clone();
        let graph = self.registry.read(cx).get(graph_id).cloned()?;
        let url = graph.read(cx).get_node(node).map(|n| n.url().to_string())?;
        Some((url, doc, node))
    }
}
