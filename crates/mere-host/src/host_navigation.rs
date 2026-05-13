/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Navigation + chrome + panel-summon action methods on `HostRoot`.
//!
//! Pulled out of `lib.rs` so the file stays under the size ceiling.
//! Sibling module [`crate::tile_ops`] owns the focus-tile /
//! close-tile / app-tree-rebuild operations; this file keeps the
//! navigation flow, history cursor, chrome cycling, new-window, and
//! panel-summon methods.

use gpui::Context;
use graph_tree::{NavAction, Provenance};
use mere_frame::{
    GraphId, InsertSide, PaneContent, PaneId, PaneNode, SplitChoice,
};
use mere_host_runtime::NavigateMode;
use crate::persistence::save_frame_layout;

use crate::graph_registry::GraphRegistry;
use crate::host_helpers::{ensure_node_for_address, ensure_node_for_address_near, error_document};
use crate::layout_config::save_chrome_layout;
use crate::loader;
use crate::HostRoot;

impl HostRoot {
    /// Drive a navigation. The single funnel every navigation
    /// source flows through — omnibar submit, link clicks, future
    /// drop-on-tile gestures.
    ///
    /// `mode` picks the semantics:
    /// - [`NavigateMode::WithinTile`] (default): extend the active
    ///   tile's history; no new kernel node. Falls through to
    ///   `NewTile` if no tile is active.
    /// - [`NavigateMode::NewTile`]: mint a new anchor node and open
    ///   it as a new tile.
    ///
    /// Per the node-per-tile design committed 2026-05-11. Reload /
    /// back / forward bypass this and operate directly on the
    /// active tile's history (see [`Self::go_back`], etc.).
    pub(crate) fn navigate_to(
        &mut self,
        address: String,
        mode: NavigateMode,
        cx: &mut Context<Self>,
    ) {
        let resolved_mode = if mode == NavigateMode::WithinTile
            && self
                .active_tiles()
                .and_then(|t| t.active_node())
                .is_none()
        {
            // No active tile means there's nothing to push history
            // into. Escalate to new-tile so the user's first omnibar
            // submit creates a tile.
            NavigateMode::NewTile
        } else {
            mode
        };

        let document = match loader::load(&self.engine_registry, &self.engine_policy, &address) {
            Ok(doc) => {
                tracing::info!(
                    address = %address,
                    mode = ?resolved_mode,
                    engine = %doc.provenance.source_kind.as_deref().unwrap_or("?"),
                    "loaded document"
                );
                doc
            }
            Err(e) => {
                tracing::warn!(address = %address, error = %e, "failed to load address");
                error_document(&address, &e)
            }
        };

        match resolved_mode {
            NavigateMode::WithinTile => {
                if let Some(tiles) = self.active_tiles_mut() {
                    tiles.push_history(address.clone(), document);
                    tracing::debug!(address = %address, "tile.navigated_within");
                } else {
                    tracing::warn!("within-tile navigate with no active workbench; discarded");
                }
            }
            NavigateMode::NewTile => {
                let target_graph_id = self
                    .active_workbench
                    .and_then(|pid| {
                        self.frame_layout
                            .iter_leaves()
                            .find(|(p, _, _)| *p == pid)
                            .map(|(_, _, gid)| gid)
                    })
                    .unwrap_or(self.graph_id);
                let Some(graph_entity) =
                    self.registry.read(cx).get(target_graph_id).cloned()
                else {
                    tracing::warn!(
                        ?target_graph_id,
                        "navigate_to: target graph not in registry"
                    );
                    return;
                };
                // Anchor new nodes to whatever tile is currently
                // active so the graph reflects the navigation path.
                let anchor = self.active_tiles().and_then(|t| t.active_node());
                let (node, created) = graph_entity.update(cx, |g, gcx| {
                    let result = ensure_node_for_address_near(g, &address, anchor);
                    gcx.notify();
                    result
                });
                if created {
                    tracing::debug!(
                        ?node,
                        ?anchor,
                        ?target_graph_id,
                        address = %address,
                        reason = "new_tile",
                        "node.created"
                    );
                }
                let active_pane = self.active_workbench;
                if let Some(workbench) = active_pane
                    .and_then(|pid| self.panes.get_mut(&pid))
                    .and_then(|s| s.as_workbench_mut())
                {
                    workbench
                        .tiles
                        .open_or_focus(node, address.clone(), document);
                    tracing::debug!(?node, address = %address, "tile.opened");

                    // v1 — lineage facet. Record this new anchor in
                    // the workbench's graph-tree, with provenance
                    // reflecting how the user got here.
                    let provenance = match anchor {
                        Some(source) if source != node => Provenance::Traversal {
                            source,
                            edge_kind: Some("user-spawned-tile".to_string()),
                        },
                        // First tile of the session, or a re-anchor
                        // on an existing node — both register as
                        // anchors in the tree.
                        _ => Provenance::Anchor,
                    };
                    let result = workbench.tree.apply(NavAction::Attach {
                        member: node,
                        provenance: provenance.clone(),
                    });
                    if result.structure_changed {
                        tracing::debug!(
                            to = ?node,
                            from = ?anchor,
                            edge_kind = "user-spawned-tile",
                            ?provenance,
                            "lineage.edge_added"
                        );
                    }
                } else {
                    tracing::warn!(
                        "new-tile navigate with no active workbench; document discarded"
                    );
                }

                // Persist the session — new anchor minted, lineage
                // updated. Cheap (JSON write, debounced is a future
                // optimisation); important so a crash doesn't lose
                // the user's tile work.
                let registry = self.registry.clone();
                let manifests = self.manifests.clone();
                crate::session_persist::flush_session(
                    &registry,
                    &manifests,
                    target_graph_id,
                    cx,
                );
            }
        }

        // Toolbar reflects the active tile's current URL +
        // back/forward availability.
        let (location, can_back, can_forward) = self
            .active_tiles()
            .map(|t| {
                (
                    t.active_url().unwrap_or(&address).to_string(),
                    t.active_can_go_back(),
                    t.active_can_go_forward(),
                )
            })
            .unwrap_or_else(|| (address.clone(), false, false));
        self.toolbar.location = location.clone();
        self.omnibar_input
            .update(cx, |input, cx| input.set_content(location, cx));
        self.toolbar.can_go_back = can_back;
        self.toolbar.can_go_forward = can_forward;

        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Move the active tile back one entry in its within-tile
    /// history. v0: pre-fetched documents are still cached, so this
    /// is free; if we evict caches later, we'd re-fetch here.
    pub(crate) fn go_back(&mut self, cx: &mut Context<Self>) {
        let Some(tiles) = self.active_tiles_mut() else {
            return;
        };
        let Some(new_url) = tiles.active_go_back() else {
            tracing::debug!("go-back: tile at oldest entry, no-op");
            return;
        };
        self.sync_omnibar_to_active_tile(cx);
        tracing::debug!(url = %new_url, "tile.history_back");
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Move the active tile forward one entry in its within-tile
    /// history.
    pub(crate) fn go_forward(&mut self, cx: &mut Context<Self>) {
        let Some(tiles) = self.active_tiles_mut() else {
            return;
        };
        let Some(new_url) = tiles.active_go_forward() else {
            tracing::debug!("go-forward: tile at newest entry, no-op");
            return;
        };
        self.sync_omnibar_to_active_tile(cx);
        tracing::debug!(url = %new_url, "tile.history_forward");
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Reload the active tile's current URL — re-fetches the
    /// document and replaces the cached one at the current cursor.
    /// Does **not** mint a new node.
    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self
            .active_tiles()
            .and_then(|t| t.active_url().map(|s| s.to_string()))
        else {
            tracing::debug!("reload: no active tile, no-op");
            return;
        };
        let document = match loader::load(&self.engine_registry, &self.engine_policy, &url) {
            Ok(doc) => doc,
            Err(e) => {
                tracing::warn!(address = %url, error = %e, "reload failed");
                error_document(&url, &e)
            }
        };
        if let Some(tiles) = self.active_tiles_mut() {
            tiles.refresh_active(document);
        }
        tracing::debug!(%url, "tile.reloaded");
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Sync the omnibar text + back/forward toolbar state to the
    /// active tile's current URL + history availability. Called
    /// after any operation that may have shifted the active tile's
    /// cursor (back, forward, focus_tile, close_tile).
    pub(crate) fn sync_omnibar_to_active_tile(&mut self, cx: &mut Context<Self>) {
        let (location, can_back, can_forward) = self
            .active_tiles()
            .map(|t| {
                (
                    t.active_url().map(|s| s.to_string()).unwrap_or_default(),
                    t.active_can_go_back(),
                    t.active_can_go_forward(),
                )
            })
            .unwrap_or_default();
        self.toolbar.location = location.clone();
        self.omnibar_input
            .update(cx, |input, cx| input.set_content(location, cx));
        self.toolbar.can_go_back = can_back;
        self.toolbar.can_go_forward = can_forward;
    }

    pub(crate) fn cycle_shellbar_position(&mut self, cx: &mut Context<Self>) {
        self.chrome_layout.shellbar = self.chrome_layout.shellbar.cycle();
        tracing::info!(
            position = ?self.chrome_layout.shellbar,
            "shellbar position cycled"
        );
        save_chrome_layout(&self.chrome_layout);
        cx.notify();
    }

    pub(crate) fn cycle_workbench_strip_position(&mut self, cx: &mut Context<Self>) {
        self.chrome_layout.workbench_strip.0 = self.chrome_layout.workbench_strip.0.cycle();
        tracing::info!(
            position = ?self.chrome_layout.workbench_strip.0,
            "workbench strip position cycled"
        );
        save_chrome_layout(&self.chrome_layout);
        cx.notify();
    }

    /// Open a host window referencing a **fresh graph** registered
    /// under a new `GraphId`. Per Mark's framing: `Cmd-N` = new
    /// graph in new window, not "duplicate current window."
    /// Tear-out is the gesture for sharing a graph across windows
    /// (Phase 2).
    pub(crate) fn open_new_window(&mut self, cx: &mut Context<Self>) {
        let registry = self.registry.clone();
        let manifests = self.manifests.clone();
        let event_buffer = self.event_buffer.clone();
        let (_session_id, graph_id, _) = GraphRegistry::create_graph_seeded(
            &registry,
            Some(&manifests),
            cx,
            |g, _| {
                ensure_node_for_address(g, "mere://intro");
            },
        );
        tracing::info!(?graph_id, "opening host window with fresh graph");
        crate::bootstrap::open_host_window(cx, registry, manifests, graph_id, event_buffer);
    }

    /// Toggle a panel of the given content kind in this window. If
    /// one already exists, close the first occurrence; otherwise
    /// summon a new one bound to the window's primary graph.
    pub(crate) fn toggle_panel(
        &mut self,
        content: PaneContent,
        cx: &mut Context<Self>,
    ) {
        let existing = self
            .leaf_path_for_content(&content)
            .map(|(path, _)| path);
        if let Some(path) = existing {
            tracing::info!(?content, ?path, "closing existing panel");
            self.frame_layout.close_leaf(&path);
        } else {
            tracing::info!(?content, "summoning new panel for primary graph");
            self.summon_panel(content, self.graph_id);
        }
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Summon a new orrery panel bound to a *fresh* graph minted
    /// into the registry. Drops the new leaf along the right edge
    /// of the current frame.
    pub(crate) fn summon_orrery_for_new_graph(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let registry = self.registry.clone();
        let manifests = self.manifests.clone();
        let (_session_id, graph_id, _) = GraphRegistry::create_graph_seeded(
            &registry,
            Some(&manifests),
            cx,
            |g, _| {
                ensure_node_for_address(g, "mere://intro");
            },
        );
        tracing::info!(?graph_id, "summoning orrery for fresh graph in current window");
        self.summon_panel(PaneContent::Orrery, graph_id);
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Summon an orrery panel bound to an **existing** graph from
    /// the registry. Used by the graph switcher to bring a
    /// previously-closed graph back into the current window.
    pub(crate) fn summon_orrery_for_graph(
        &mut self,
        graph_id: GraphId,
        cx: &mut Context<Self>,
    ) {
        // If this graph already has any panel in the window, no-op
        // (no point summoning a second orrery for the same graph
        // unless the user explicitly wants split-views; that comes
        // from `+◯` not the switcher).
        let already_present = self
            .frame_layout
            .iter_leaves()
            .any(|(_, _, gid)| gid == graph_id);
        if already_present {
            tracing::debug!(?graph_id, "graph already in this window; not summoning duplicate");
            self.graph_switcher_open = false;
            cx.notify();
            return;
        }
        tracing::info!(?graph_id, "summoning orrery for existing graph in current window");
        self.summon_panel(PaneContent::Orrery, graph_id);
        self.graph_switcher_open = false;
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Toggle the graph switcher dropdown.
    pub(crate) fn toggle_graph_switcher(&mut self, cx: &mut Context<Self>) {
        self.graph_switcher_open = !self.graph_switcher_open;
        cx.notify();
    }

    /// Derive a display name for a graph from its node contents.
    /// Tries the first non-intro node's URL, falls back to a short
    /// UUID prefix.
    pub(crate) fn graph_display_name(
        &self,
        graph_id: GraphId,
        cx: &Context<Self>,
    ) -> String {
        let Some(graph) = self.registry.read(cx).get(graph_id).cloned() else {
            return format!("(missing {:?})", graph_id.as_uuid());
        };
        let g = graph.read(cx);
        if let Some(url) = g
            .nodes()
            .map(|(_, n)| n.url().to_string())
            .find(|u| u != "mere://intro" && !u.is_empty())
        {
            return url;
        }
        let s = graph_id.as_uuid().to_string();
        format!("Graph {}", &s[..8])
    }

    /// Close the panel at `pane_id`. For an **orrery**, this is
    /// special: per the orrery-as-root invariant, the orrery
    /// represents the window's session for its graph, so closing it
    /// also closes every other panel in this window bound to the
    /// same `graph_id`. For non-orrery panels, just remove the
    /// single leaf.
    ///
    /// If the resulting layout would become empty (the closed leaf
    /// was the only one), the underlying `close_leaf` refuses to
    /// remove it — the window keeps its last panel. The user can
    /// close the window through the OS to fully release the graph.
    /// Move `source` to be adjacent to `target` on `side`. Wraps
    /// `FrameLayout::reparent_leaf` with path-by-pane-id lookup
    /// + app-tree rebuild + layout save. v0 of pane drag-rearrange
    /// per the pane-UX brief §1.
    pub(crate) fn reparent_pane(
        &mut self,
        source: PaneId,
        target: PaneId,
        side: InsertSide,
        cx: &mut Context<Self>,
    ) {
        if source == target {
            return;
        }
        let Some(source_path) = self.path_for_pane(source) else {
            tracing::warn!(?source, "reparent_pane: source not in layout");
            return;
        };
        let Some(target_path) = self.path_for_pane(target) else {
            tracing::warn!(?target, "reparent_pane: target not in layout");
            return;
        };
        let ok = self
            .frame_layout
            .reparent_leaf(&source_path, &target_path, side);
        if !ok {
            tracing::debug!(
                ?source,
                ?target,
                ?side,
                "reparent_pane: frame layout refused the move"
            );
            return;
        }
        tracing::info!(?source, ?target, ?side, "pane.reparented");
        save_frame_layout(&self.frame_layout);
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    pub(crate) fn close_pane(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        let Some((content, graph_id)) = self
            .frame_layout
            .iter_leaves()
            .find(|(pid, _, _)| *pid == pane_id)
            .map(|(_, content, gid)| (content.clone(), gid))
        else {
            return;
        };

        if matches!(content, PaneContent::Orrery) {
            // Cascade: collect every pane_id in this window bound to
            // this orrery's graph, then close each.
            let cascade: Vec<PaneId> = self
                .frame_layout
                .iter_leaves()
                .filter(|(_, _, gid)| *gid == graph_id)
                .map(|(pid, _, _)| pid)
                .collect();
            tracing::info!(
                ?graph_id,
                count = cascade.len(),
                "closing orrery cascades to dependent panels"
            );
            for pid in cascade {
                self.close_single_leaf(pid);
            }
        } else {
            self.close_single_leaf(pane_id);
        }
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Close one leaf by `pane_id`, no cascade. Used by
    /// `close_pane` (after computing the cascade list) and any
    /// future "close this specific leaf" call sites.
    fn close_single_leaf(&mut self, pane_id: PaneId) {
        let Some(path) = self.path_for_pane(pane_id) else {
            return;
        };
        if !self.frame_layout.close_leaf(&path) {
            tracing::debug!(?pane_id, "close_leaf refused (likely root)");
        }
    }

    /// Find the `SplitPath` to the leaf with `pane_id`.
    fn path_for_pane(&self, pane_id: PaneId) -> Option<Vec<SplitChoice>> {
        fn walk(
            node: &PaneNode,
            path: &mut Vec<SplitChoice>,
            target: PaneId,
        ) -> Option<Vec<SplitChoice>> {
            match node {
                PaneNode::Leaf { pane_id, .. } if *pane_id == target => Some(path.clone()),
                PaneNode::Leaf { .. } => None,
                PaneNode::Split { first, second, .. } => {
                    path.push(SplitChoice::First);
                    if let Some(hit) = walk(first, path, target) {
                        return Some(hit);
                    }
                    path.pop();
                    path.push(SplitChoice::Second);
                    if let Some(hit) = walk(second, path, target) {
                        return Some(hit);
                    }
                    path.pop();
                    None
                }
            }
        }
        let mut path = Vec::new();
        walk(&self.frame_layout.root, &mut path, pane_id)
    }

    /// Append a new leaf of `content` (bound to `graph_id`) along
    /// the right edge of the frame. Picks the rightmost leaf in
    /// layout order as the anchor, then inserts to its right.
    fn summon_panel(&mut self, content: PaneContent, graph_id: GraphId) {
        let pane_id = self.mint_pane_id();
        let new_leaf = PaneNode::Leaf {
            pane_id,
            content,
            graph_id,
        };
        // Find the path to the rightmost-then-deepest leaf so the
        // new panel slots in at the visual right edge.
        let anchor_path = rightmost_leaf_path(&self.frame_layout.root);
        let ok = self
            .frame_layout
            .summon_leaf(&anchor_path, InsertSide::Right, new_leaf);
        if !ok {
            tracing::warn!(?anchor_path, "summon_leaf failed; layout unchanged");
        }
    }

    /// Find the first leaf with the given content kind in layout
    /// order. Returns `(path, pane_id)` for the matching leaf.
    fn leaf_path_for_content(&self, content: &PaneContent) -> Option<(Vec<SplitChoice>, PaneId)> {
        fn walk(
            node: &PaneNode,
            path: &mut Vec<SplitChoice>,
            target: &PaneContent,
        ) -> Option<(Vec<SplitChoice>, PaneId)> {
            match node {
                PaneNode::Leaf { pane_id, content, .. } if content == target => {
                    Some((path.clone(), *pane_id))
                }
                PaneNode::Leaf { .. } => None,
                PaneNode::Split { first, second, .. } => {
                    path.push(SplitChoice::First);
                    if let Some(hit) = walk(first, path, target) {
                        return Some(hit);
                    }
                    path.pop();
                    path.push(SplitChoice::Second);
                    if let Some(hit) = walk(second, path, target) {
                        return Some(hit);
                    }
                    path.pop();
                    None
                }
            }
        }
        let mut path = Vec::new();
        walk(&self.frame_layout.root, &mut path, content)
    }

    fn mint_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }
}

/// Walk to the rightmost-deepest leaf — at each split, prefer
/// `Second`. Returns the path to that leaf. Used to pick a default
/// insertion anchor that lands new panels along the window's right
/// edge.
fn rightmost_leaf_path(root: &PaneNode) -> Vec<SplitChoice> {
    let mut path = Vec::new();
    let mut node = root;
    loop {
        match node {
            PaneNode::Leaf { .. } => return path,
            PaneNode::Split { second, .. } => {
                path.push(SplitChoice::Second);
                node = second;
            }
        }
    }
}
