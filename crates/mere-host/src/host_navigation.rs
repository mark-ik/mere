/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Navigation + tile + chrome action methods on `HostRoot`.
//!
//! Pulled out of `lib.rs` so the file stays under the size ceiling
//! and the "what does pressing Enter / Reload / Cmd-N / a tile click
//! actually do?" logic lives in one focused module.

use gpui::Context;
use mere_frame::{
    GraphId, InsertSide, PaneContent, PaneId, PaneNode, SplitChoice,
};
use mere_kernel::graph::Graph;

use crate::demo::build_demo_application_tree;
use crate::graph_registry::GraphRegistry;
use crate::host_helpers::{ensure_node_for_address, ensure_node_for_address_near, error_document};
use crate::layout_config::save_chrome_layout;
use crate::loader;
use crate::HostRoot;

impl HostRoot {
    /// Drive a navigation. The single funnel every navigation source
    /// flows through — omnibar submit, Reload, Go Back, Go Forward,
    /// future link clicks. Loads the address, updates the omnibar
    /// text, optionally pushes onto history, refreshes the app tree.
    pub(crate) fn navigate_to(
        &mut self,
        address: String,
        push_history: bool,
        cx: &mut Context<Self>,
    ) {
        self.toolbar.location = address.clone();
        let mirror = address.clone();
        self.omnibar_input
            .update(cx, |input, cx| input.set_content(mirror, cx));

        // Find the active workbench's graph_id so the new node
        // lands in the right graph (not always the window's
        // primary). If no workbench is active, fall back to
        // primary.
        let target_graph_id = self
            .active_workbench
            .and_then(|pid| {
                self.frame_layout
                    .iter_leaves()
                    .find(|(p, _, _)| *p == pid)
                    .map(|(_, _, gid)| gid)
            })
            .unwrap_or(self.graph_id);
        let Some(graph_entity) = self.registry.read(cx).get(target_graph_id).cloned() else {
            tracing::warn!(?target_graph_id, "navigate_to: target graph not in registry");
            return;
        };

        // Anchor new nodes to whatever tile is currently active so
        // the graph reflects the navigation path. When no tile is
        // open (last close, fresh start), the new node lands at the
        // origin with no edge.
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
                "created graph node for new address"
            );
        }

        let document = match loader::load(&self.engine_registry, &self.engine_policy, &address) {
            Ok(doc) => {
                tracing::info!(
                    address = %address,
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
        if let Some(tiles) = self.active_tiles_mut() {
            tiles.open_or_focus(node, document);
        } else {
            tracing::warn!("navigate_to with no active workbench; document discarded");
        }

        if push_history {
            self.history.truncate(self.history_cursor + 1);
            let last_matches = self.history.last().map(|s| s == &address).unwrap_or(false);
            if !last_matches {
                self.history.push(address);
                self.history_cursor = self.history.len() - 1;
            }
        }
        self.toolbar.can_go_back = self.history_cursor > 0;
        self.toolbar.can_go_forward = self.history_cursor + 1 < self.history.len();

        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Switch the active workbench to the tile at `index`. Syncs
    /// the omnibar / toolbar to that tile's address.
    pub(crate) fn focus_tile(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(pane_id) = self.active_workbench else {
            return;
        };
        self.focus_tile_in(pane_id, index, cx);
    }

    /// Switch the workbench at `pane_id` to its tile at `index`.
    /// Used by tile clicks in any workbench (the click handler
    /// captures the pane_id explicitly).
    pub(crate) fn focus_tile_in(
        &mut self,
        pane_id: PaneId,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let node = self
            .panes
            .get_mut(&pane_id)
            .and_then(|s| s.as_workbench_mut())
            .and_then(|w| w.tiles.focus_index(index));
        let Some(node) = node else {
            return;
        };
        let graph_id = self
            .frame_layout
            .iter_leaves()
            .find(|(pid, _, _)| *pid == pane_id)
            .map(|(_, _, gid)| gid);
        let url = graph_id
            .and_then(|gid| self.registry.read(cx).get(gid).cloned())
            .and_then(|graph| graph.read(cx).get_node(node).map(|n| n.url().to_string()));
        if let Some(url) = url {
            self.toolbar.location = url.clone();
            self.omnibar_input
                .update(cx, |input, cx| input.set_content(url, cx));
        }
        self.active_workbench = Some(pane_id);
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Close the tile at `index` in the active workbench.
    pub(crate) fn close_tile(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(pane_id) = self.active_workbench else {
            return;
        };
        self.close_tile_in(pane_id, index, cx);
    }

    /// Close the tile at `index` in the workbench at `pane_id`.
    pub(crate) fn close_tile_in(
        &mut self,
        pane_id: PaneId,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(workbench) = self
            .panes
            .get_mut(&pane_id)
            .and_then(|s| s.as_workbench_mut())
        else {
            return;
        };
        let new_active = workbench.tiles.close_index(index);
        let url = new_active.and_then(|node| {
            self.graph
                .read(cx)
                .get_node(node)
                .map(|n| n.url().to_string())
        });
        if let Some(url) = url {
            self.toolbar.location = url.clone();
            self.omnibar_input
                .update(cx, |input, cx| input.set_content(url, cx));
        } else if new_active.is_none() {
            self.toolbar.location.clear();
            self.omnibar_input
                .update(cx, |input, cx| input.set_content("", cx));
        }
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Recompute `app_tree` from the current shared graph snapshot
    /// + per-window state. Centralised here so the borrow dance
    /// around `self.graph.read(cx)` lives in one place.
    pub(crate) fn rebuild_app_tree(&mut self, cx: &mut Context<Self>) {
        let active_doc = self
            .active_tiles()
            .and_then(|t| t.active_document())
            .cloned();
        let frame_layout = &self.frame_layout;
        let graph_snapshot: &Graph = self.graph.read(cx);
        self.app_tree =
            build_demo_application_tree(active_doc.as_ref(), graph_snapshot, frame_layout);
    }

    pub(crate) fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.history_cursor == 0 {
            tracing::debug!("go-back: at oldest entry, no-op");
            return;
        }
        self.history_cursor -= 1;
        let addr = self.history[self.history_cursor].clone();
        self.navigate_to(addr, false, cx);
    }

    pub(crate) fn go_forward(&mut self, cx: &mut Context<Self>) {
        if self.history_cursor + 1 >= self.history.len() {
            tracing::debug!("go-forward: at newest entry, no-op");
            return;
        }
        self.history_cursor += 1;
        let addr = self.history[self.history_cursor].clone();
        self.navigate_to(addr, false, cx);
    }

    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        if self.history.is_empty() {
            return;
        }
        let addr = self.history[self.history_cursor].clone();
        self.navigate_to(addr, false, cx);
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
        let event_buffer = self.event_buffer.clone();
        let (graph_id, _) = GraphRegistry::create_graph_seeded(&registry, cx, |g, _| {
            ensure_node_for_address(g, "mere://intro");
        });
        tracing::info!(?graph_id, "opening host window with fresh graph");
        crate::bootstrap::open_host_window(cx, registry, graph_id, event_buffer);
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
        let (graph_id, _) = GraphRegistry::create_graph_seeded(&registry, cx, |g, _| {
            ensure_node_for_address(g, "mere://intro");
        });
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
