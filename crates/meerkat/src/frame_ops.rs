/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Navigation, content, session, and chrome-drain operations for
//! [`Shell`](super::Shell). Factored from `main.rs` to keep files under the
//! workspace 600-LOC ceiling.

use std::collections::HashMap;

use forme::GraphMemberId;
use frame::{
    GraphId, InsertSide, PaneContent, PaneId, PaneNode, SessionId, SplitAxis, SplitChoice,
};
use kernel::graph::{
    ContainmentSubKind, Graph, ProvenanceSubKind, RelationKind, SemanticSubKind,
};
use meerkat::command::Command;
use meerkat::shell_eval::{CommandShell, ShellContext};
use meerkat::{Chrome, CommsIntent, ContextAction, ContextItem};
use orrery::{NodeShape, NodeState};
use platen_view::WorkbenchAction;
use session_runtime::{
    PersistedSettings, ShellbarEdge, SwitcherThumbnailOptions, ViewIntent, build_switcher_thumbnail,
    content_store, frame_layout_store, manifest::GraphSessionManifest, session_graph_store,
    settings_store, view_intent_store,
};
use super::switcher::{SWITCHER_THUMB_H, SWITCHER_THUMB_W};

use super::observability::{ObservabilitySnapshot, Severity};
use super::{
    DEFAULT_FRAME, DEFAULT_PANE, FALLBACK_TOOLBAR_H, GRAPH_PANE, WindowCtx, apparatus, comms_host,
    fetch, frame_view, roster, sync,
};

/// Map a `relate("…")` kind word to a [`SemanticSubKind`]. A small curated set
/// of the relations a user reaches for by hand; anything unrecognized falls back
/// to `UserGrouped` (the honest default — a human asserted it).
fn relation_kind_from_str(s: &str) -> SemanticSubKind {
    match s.trim().to_ascii_lowercase().as_str() {
        "cites" | "cite" => SemanticSubKind::Cites,
        "quotes" | "quote" => SemanticSubKind::Quotes,
        "summarizes" | "summary" => SemanticSubKind::Summarizes,
        "elaborates" | "elaborate" => SemanticSubKind::Elaborates,
        "example" | "example_of" | "exampleof" => SemanticSubKind::ExampleOf,
        "supports" | "support" => SemanticSubKind::Supports,
        "contradicts" | "contradict" => SemanticSubKind::Contradicts,
        "questions" | "question" => SemanticSubKind::Questions,
        "same" | "same_entity" | "sameentity" => SemanticSubKind::SameEntityAs,
        "duplicate" | "duplicate_of" | "duplicateof" => SemanticSubKind::DuplicateOf,
        "hyperlink" | "link" => SemanticSubKind::Hyperlink,
        _ => SemanticSubKind::UserGrouped,
    }
}

impl WindowCtx<'_> {
    /// The URL of whatever is in focus: in the tiled view the focused tile's node,
    /// in the orrery the focused node. `None` when nothing is focused.
    pub(super) fn current_focus_url(&self) -> Option<String> {
        if self.workbench_active() {
            let member = self.view.focused_tile?;
            self.orrery
                .graph()
                .get_node_by_id(member)
                .map(|(_, node)| node.url().to_string())
        } else {
            self.orrery.focused_url().map(str::to_string)
        }
    }

    /// Point the omnibar at the focused tile / node (the address bar follows focus),
    /// but only when that focus actually changed and the user isn't editing the
    /// omnibar (no chrome field holds the caret) — so it never clobbers typing.
    pub(super) fn sync_location(&mut self) {
        let url = self.current_focus_url();
        if url == self.view.shown_location {
            return;
        }
        self.view.shown_location = url.clone();
        if let (Some(url), None) = (url, self.view.runner.focus()) {
            self.view.runner.update(move |c| c.show_location(&url));
            self.view.request_redraw();
        }
    }

    /// Sync the graph to the chrome's current navigation target when it changes.
    ///
    /// Per-node navigation (the node-lineage model): a node is a browsing surface.
    /// If one is focused — the focused tile in Tree, the selected node in
    /// Cartography — the omnibar **navigates it in place** (`navigate_member`):
    /// the node's URL and within-node history advance, no new node is minted, so
    /// it stays right where it is in the workbench. Only when nothing is focused
    /// (empty workbench / no selection) does this mint a node (`visit`), the
    /// first-context case. Called after any input that can navigate (omnibar
    /// submit, suggestion / palette).
    pub(super) fn sync_orrery(&mut self) {
        let loc = self.view.runner.state().content_location().to_string();
        // Ctrl/Cmd-Enter: open the address as a *new node* linked from the focused
        // one. Handled before the change guard below, since duplicates are welcome
        // (the node-identity model) — opening the current page as a new node is a
        // valid branch, not a no-op.
        if self.view.runner.state().open_as_new_node {
            self.view.runner.update(|c| c.open_as_new_node = false);
            let origin = self.nav_target_member();
            let new_member = self.orrery.open_member_as_new_node(origin, &loc);
            // With the workbench active, tile the new node: stack it into the
            // focused tile's slot as the active tab (or a fresh slot when nothing is
            // focused), and focus it so the next navigation targets it. Otherwise
            // the orrery has selected it, so it shows as the focused-node card.
            if self.workbench_active() {
                let stacked = origin.is_some_and(|o| self.view.workbench.open_in_slot_of(new_member, o));
                if !stacked {
                    self.view.workbench.open_tile(new_member);
                }
                self.view.focused_tile = Some(new_member);
            } else {
                // Cartography: opening a node is deliberate — show it as a live
                // card straight away (passive focus shows only the snapshot).
                self.view.live_previews.insert(new_member);
            }
            self.ensure_content(&loc);
            self.view.content_location = loc;
            self.save_session();
            self.view.request_redraw();
            return;
        }
        if loc == self.view.content_location {
            return;
        }
        match self.nav_target_member() {
            Some(member) => {
                self.orrery.navigate_member(member, &loc);
                self.view.scroll.remove(&member); // a new page starts at the top
            }
            None => {
                self.orrery.visit(&loc);
            }
        }
        // Navigating is deliberate: with the orrery active, show the target as a
        // live card (passive focus shows only the snapshot). With the workbench
        // active the node already tiled, so no card.
        if !self.workbench_active() {
            if let Some(member) = self.focused_member() {
                self.view.live_previews.insert(member);
            }
        }
        self.ensure_content(&loc);
        self.view.content_location = loc;
        self.save_session();
        self.view.request_redraw();
    }

    /// The node per-node navigation acts on: the focused tile in Tree, the single
    /// selected node in Cartography. `None` when nothing is focused.
    fn nav_target_member(&self) -> Option<GraphMemberId> {
        if self.workbench_active() {
            self.view.focused_tile
        } else {
            self.orrery.focused_member()
        }
    }

    /// Apply a queued back/forward step to the **focused node's own** history:
    /// move its cursor (no new node, no fork), mirror the revealed page into the
    /// chrome (content target + omnibar) so the following `sync_orrery` sees no
    /// change and does not record a fresh visit, and refetch it. Drained each input
    /// pass, before `sync_orrery`.
    pub(super) fn drain_history_step(&mut self) {
        let Some(step) = self.view.runner.state().history_step else {
            return;
        };
        self.view.runner.update(|c| c.history_step = None);
        let Some(member) = self.nav_target_member() else {
            return;
        };
        let url = match step {
            meerkat::HistoryStep::Back => self.orrery.member_history_back(member),
            meerkat::HistoryStep::Forward => self.orrery.member_history_forward(member),
        };
        let Some(url) = url else {
            return;
        };
        self.view.scroll.remove(&member); // the revealed page starts at the top
        self.view.content_location = url.clone();
        self.ensure_content(&url);
        self.view.runner.update(|c| {
            c.content_location = url.clone();
            c.show_location(&url);
        });
        self.save_session();
        self.view.request_redraw();
    }

    /// Reflect the focused node's history onto the toolbar's back/forward
    /// enabled-state, so the buttons track whichever node is focused. Cheap; a
    /// no-op when unchanged. Called each render.
    pub(super) fn sync_nav_buttons(&mut self) {
        let (can_back, can_forward) = match self.nav_target_member() {
            Some(m) => (
                self.orrery.member_can_back(m),
                self.orrery.member_can_forward(m),
            ),
            None => (false, false),
        };
        let paused = self.orrery.physics_paused();
        let (cur_back, cur_forward, cur_paused) = {
            let c = self.view.runner.state();
            (c.toolbar.can_go_back, c.toolbar.can_go_forward, c.physics_paused)
        };
        if cur_back == can_back && cur_forward == can_forward && cur_paused == paused {
            return;
        }
        self.view.runner.update(move |c| {
            c.toolbar.can_go_back = can_back;
            c.toolbar.can_go_forward = can_forward;
            c.physics_paused = paused;
        });
        self.view.request_redraw();
    }

    /// Apply a pending pause/play toggle the toolbar button queued (the chrome can't
    /// reach the orrery). Mirrors `drain_history_step`'s shape. (Physics pause.)
    pub(super) fn drain_physics_toggle(&mut self) {
        let mut toggled = false;
        self.view.runner.update(|c| toggled = c.take_physics_toggle());
        if toggled {
            self.orrery.toggle_physics_paused();
            self.sync_nav_buttons();
            self.view.request_redraw();
        }
    }

    /// Persist the session (graph + camera view-intent) under the session dir.
    /// Best-effort: a write failure is logged, not fatal. Called after each
    /// navigation and on window close.
    pub(super) fn save_session(&mut self) {
        let graph_file = self.shared.session.session_dir.join(session_graph_store::GRAPH_FILE);
        if let Err(err) = session_graph_store::save(&graph_file, self.orrery.graph()) {
            tracing::warn!(%err, path = ?graph_file, "failed to persist the session graph");
        }
        let intent = ViewIntent {
            camera: Some(super::camera_to_snapshot(self.orrery.camera())),
            focus: self.orrery.focused_url().map(str::to_string),
            ..Default::default()
        };
        if let Err(err) = view_intent_store::save_view_intent(
            &self.shared.session.session_dir,
            DEFAULT_FRAME,
            DEFAULT_PANE,
            &intent,
        ) {
            tracing::warn!(%err, dir = ?self.shared.session.session_dir, "failed to persist the view intent");
        }
        // The content frame's pane layout (which panes are open + split ratios) is
        // **window-scoped** (Model B, MG5): it persists at the shared root and stays
        // put across session switches, so a graph swap re-sources the panes without
        // rearranging them.
        if let Err(err) = frame_layout_store::save_frame_layout(&self.shared.session.mere_root, &self.view.frame_layout)
        {
            tracing::warn!(%err, dir = ?self.shared.session.mere_root, "failed to persist the frame layout");
        }
        // Record the save in the active session's manifest (advances `updated_at`,
        // the switcher's recency key) and flush the registry. (Multi-graph MG1.)
        let active = self.shared.session.active_session_id;
        self.shared.session.manifests.update(active, |_| {});
        if let Err(err) = self.shared.session.manifests.flush_dirty() {
            tracing::warn!(%err, "failed to flush the session registry");
        }
        // Keep the active session's switcher thumbnail live as its graph grows
        // (cheap; no disk read, unlike the full refresh on a session change).
        let opts = SwitcherThumbnailOptions {
            width: SWITCHER_THUMB_W,
            height: SWITCHER_THUMB_H,
            ..SwitcherThumbnailOptions::default()
        };
        let thumb = build_switcher_thumbnail(self.orrery.graph(), opts);
        self.shared.session.session_thumbnails.insert(self.shared.session.active_session_id, thumb);
    }

    /// Begin renaming `id`: seed the switcher edit buffer from its current label, so
    /// editing starts from the shown name (display or derived). (Host text path.)
    pub(super) fn start_rename(&mut self, id: SessionId) {
        if self.shared.session.manifests.get(id).is_none() {
            return;
        }
        let seed = self.shared.session.session_labels.get(&id).cloned().unwrap_or_default();
        self.view.renaming = Some((id, seed));
        self.view.request_redraw();
    }

    /// Commit the in-progress rename: a non-empty name sets the session's display
    /// name; an empty one clears it (the label reverts to the derived one). Persists
    /// the manifest and refreshes the labels. (Host text path.)
    pub(super) fn commit_rename(&mut self) {
        let Some((id, name)) = self.view.renaming.take() else {
            return;
        };
        let trimmed = name.trim().to_string();
        let display = (!trimmed.is_empty()).then_some(trimmed);
        self.shared.session.manifests.update(id, |m| m.display_name = display);
        if let Err(err) = self.shared.session.manifests.flush_dirty() {
            tracing::warn!(%err, "failed to flush the renamed session manifest");
        }
        self.refresh_session_thumbnails();
        self.view.request_redraw();
    }

    /// Drop the in-progress rename without saving (Escape, or an interaction that
    /// moves on). A no-op when not renaming. (Host text path.)
    pub(super) fn cancel_rename(&mut self) {
        if self.view.renaming.take().is_some() {
            self.view.request_redraw();
        }
    }

    /// Append typed `ch` to the rename buffer. No-op when not renaming. (Host text.)
    pub(super) fn rename_push(&mut self, ch: &str) {
        if let Some((_, buf)) = self.view.renaming.as_mut() {
            buf.push_str(ch);
            self.view.request_redraw();
        }
    }

    /// Delete the last char of the rename buffer (Backspace). (Host text path.)
    pub(super) fn rename_backspace(&mut self) {
        if let Some((_, buf)) = self.view.renaming.as_mut() {
            buf.pop();
            self.view.request_redraw();
        }
    }

    /// Rebuild the per-session switcher thumbnails **and labels**: the active
    /// session from the **live** orrery graph, each inactive session from its cold
    /// `graph.json`. The label is the user's display name, else one derived from the
    /// graph. Drops entries for closed sessions. Called on every session or graph
    /// change (cheap small-graph walks; no per-frame disk reads). (Multi-graph MG4 /
    /// host text path.)
    pub(super) fn refresh_session_thumbnails(&mut self) {
        let opts = SwitcherThumbnailOptions {
            width: SWITCHER_THUMB_W,
            height: SWITCHER_THUMB_H,
            ..SwitcherThumbnailOptions::default()
        };
        let ids: Vec<SessionId> = self.shared.session.manifests.iter().map(|(id, _)| id).collect();
        let live: std::collections::HashSet<SessionId> = ids.iter().copied().collect();
        self.shared.session.session_thumbnails.retain(|id, _| live.contains(id));
        self.shared.session.session_labels.retain(|id, _| live.contains(id));
        for id in ids {
            // A user-set display name wins; otherwise derive a short label.
            let display_name = self.shared.session.manifests
                .get(id)
                .and_then(|m| m.display_name.clone())
                .filter(|n| !n.trim().is_empty());
            let (thumb, label) = if id == self.shared.session.active_session_id {
                let g = self.orrery.graph();
                let label = display_name.unwrap_or_else(|| derive_session_label(g));
                (build_switcher_thumbnail(g, opts), label)
            } else {
                let dir = self.shared.session.mere_root
                    .join("sessions")
                    .join(id.as_uuid().to_string());
                let graph = session_graph_store::load(&dir.join(session_graph_store::GRAPH_FILE))
                    .ok()
                    .flatten()
                    .unwrap_or_else(Graph::new);
                let label = display_name.unwrap_or_else(|| derive_session_label(&graph));
                (build_switcher_thumbnail(&graph, opts), label)
            };
            self.shared.session.session_thumbnails.insert(id, thumb);
            self.shared.session.session_labels.insert(id, label);
        }
    }

    /// Make the focused node's content available. A network address already in
    /// this session's content map is left as-is; otherwise a durable cache hit is
    /// shown without re-fetching (so a reload need not hit the network), and a
    /// miss marks it `Loading` and spawns a fetch.
    pub(super) fn ensure_content(&mut self, url: &str) {
        if !fetch::is_fetchable(url) || self.shared.content.pages.contains_key(url) {
            return;
        }
        if let Some(stored) = self.load_cached(url) {
            self.shared.content.pages.insert(
                url.to_string(),
                fetch::ContentState::Ready(super::fetched_from(stored)),
            );
            return;
        }
        self.shared.content.pages
            .insert(url.to_string(), fetch::ContentState::Loading);
        self.shared.observability
            .record_actor("fetch", "started", Some(url.to_string()));
        self.shared.content.fetch_handle
            .command(fetch::FetchCommand::Page(url.to_string()));
    }

    /// Toggle between the orrery (Cartography) and the tiled workbench (Tree).
    /// Entering Tree seeds the open set from the focused node and its graph
    /// neighbors, so the tiled view reflects the node you toggled on; exiting
    /// clears it. The constellation reconciles its actors to the resulting needed
    /// set on the next frame — spawning the tiles, reaping what's no longer shown
    /// (background-flagged nodes excepted).
    pub(super) fn toggle_workbench(&mut self) {
        // Clear the omnibar suggestions dropdown so it doesn't hang over the tiles.
        self.view.runner.update(Chrome::close_suggestions);
        if self.workbench_open() {
            self.close_workbench();
            self.view.request_redraw();
            return;
        }
        // Summon the workbench pane beside the orrery, then tile the selection.
        self.open_workbench();
        self.view.workbench.clear_tiles();
        {
            for member in self.selection_working_set() {
                self.view.workbench.open_tile(member);
            }
            // Focus the node the open was seeded from (the primary selection), so the
            // omnibar shows its URL; fall back to the first opened tile.
            self.view.focused_tile = self
                .orrery
                .selected_members()
                .first()
                .copied()
                .or_else(|| self.view.workbench.open_members().first().copied());
        }
        self.view.request_redraw();
    }

    /// The members a selection-driven open acts on. A multi-selection is its own
    /// nodes (opened in splits). A single selection expands to the **active tabs in
    /// that node's graphlet** — its connected component intersected with the warm-tab
    /// set, plus the node itself — so you gather the live cluster around it. An empty
    /// selection yields nothing. Shared by entering the workbench and the right-click
    /// menu.
    pub(super) fn selection_working_set(&self) -> Vec<GraphMemberId> {
        let selected = self.orrery.selected_members();
        if selected.len() > 1 {
            return selected; // multi-select → the selection
        }
        match selected.first() {
            Some(&focus) => self
                .orrery
                .connected_members(focus)
                .into_iter()
                .filter(|m| *m == focus || self.shared.content.constellation.is_active(*m))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Open the right-click context menu over the current selection's working set,
    /// at window `(x, y)`. A no-op when nothing is selected (no set to act on). A
    /// single-member set offers one "open tile"; a larger set offers splits vs a
    /// stack. The host remembers the set; the chrome renders the rows.
    pub(super) fn open_context_menu_at(&mut self, x: f32, y: f32) {
        let set = self.selection_working_set();
        if set.is_empty() {
            // No selection (typically a right-click on empty canvas): offer "Add
            // node" at the cursor. Remember the content-band cursor point so AddNode
            // mints the node under it; leave context_set empty (no member set). A
            // node-hit-test for true spatial emptiness is a refinement.
            self.view.context_origin = Some(self.orrery_point(x, y));
            self.view.context_set.clear();
            let items = vec![
                ContextItem::new("Add node", ContextAction::AddNode),
                ContextItem::new("Add field", ContextAction::AddField),
            ];
            self.view.runner.update(move |c| c.open_context_menu(x, y, items));
            self.view.request_redraw();
            return;
        }
        // A selection-based menu never mints at the cursor; clear any stale anchor.
        self.view.context_origin = None;
        let items = if set.len() == 1 {
            vec![ContextItem::new("Open tile", ContextAction::OpenSplits)]
        } else {
            let mut items = vec![
                ContextItem::new("Open in splits", ContextAction::OpenSplits),
                ContextItem::new("Open in a stack", ContextAction::Stack),
            ];
            // Relate is pairwise — offer it only for exactly two selected nodes.
            if set.len() == 2 {
                items.push(ContextItem::new("Relate", ContextAction::Relate));
            }
            items
        };
        self.view.context_set = set;
        self.view.runner
            .update(move |c| c.open_context_menu(x, y, items));
        self.view.request_redraw();
    }

    /// Dismiss the context menu (an outside click / Escape), dropping its set and
    /// the add-node cursor anchor.
    pub(super) fn close_context_menu(&mut self) {
        self.view.context_set.clear();
        self.view.context_origin = None;
        self.view.runner.update(Chrome::close_context_menu);
        self.view.request_redraw();
    }

    /// Open the shellbar move menu at `(x, y)` — four entries, one per edge,
    /// with the current edge marked. (Shellbar F2.2.)
    pub(super) fn open_shellbar_menu_at(&mut self, x: f32, y: f32) {
        let current = self.shared.presentation.shellbar_edge;
        let items: Vec<ContextItem> = [
            (ShellbarEdge::Left, "Move shellbar to left"),
            (ShellbarEdge::Right, "Move shellbar to right"),
            (ShellbarEdge::Top, "Move shellbar to top"),
            (ShellbarEdge::Bottom, "Move shellbar to bottom"),
        ]
        .iter()
        .map(|&(edge, label)| {
            let label = if edge == current {
                format!("{label} \u{2713}") // ✓ marks current position
            } else {
                label.to_string()
            };
            ContextItem::new(label, ContextAction::ShellbarMove(edge))
        })
        .collect();
        self.view.runner.update(move |c| c.open_context_menu(x, y, items));
        self.view.request_redraw();
    }

    /// Run a pending context-menu action the chrome captured: open the menu's
    /// member set as splits or as one stack, switching into the tiled (Tree)
    /// projection first if needed.
    pub(super) fn drain_pending_context(&mut self) {
        let Some(action) = self.view.runner.state().pending_context else {
            return;
        };
        self.view.runner.update(|c| c.pending_context = None);
        // Shellbar move: redock the strip to the chosen edge and persist. No
        // member set involved — return before the orrery-tile logic below.
        if let ContextAction::ShellbarMove(edge) = action {
            self.shared.presentation.shellbar_edge = edge;
            self.view.centered = false; // orrery band changed; recenter once
            self.view.toolbar_h = 0;   // re-measure (band height may change if Top/Bottom)
            self.persist_settings();
            self.view.request_redraw();
            return;
        }
        // Relate the two selected nodes — no tile / member-set logic, like the
        // shellbar move above.
        if let ContextAction::Relate = action {
            if self.orrery.assert_selected_relation(SemanticSubKind::UserGrouped) {
                self.save_session();
            }
            self.view.request_redraw();
            return;
        }
        // Add a fresh node. From the empty-space right-click it lands at the saved
        // cursor anchor; from the add-pill (no anchor) it mints at the default
        // position. No member set.
        if let ContextAction::AddNode = action {
            let url = "mere://welcome";
            match self.view.context_origin.take() {
                Some(origin) => {
                    let _ = self.orrery.add_node_at(origin, url);
                }
                None => {
                    let _ = self.orrery.open_member_as_new_node(None, url);
                }
            }
            self.ensure_content(url);
            self.save_session();
            self.view.request_redraw();
            return;
        }
        // Add a tile (the add-pill's "Add tile"): mint a node and open it as a tile,
        // summoning the workbench pane — the same body as WorkbenchAction::NewTile.
        if let ContextAction::AddTile = action {
            self.open_workbench();
            let url = "mere://welcome";
            let member = self.orrery.open_member_as_new_node(None, url);
            self.view.workbench.open_tile(member);
            self.view.focused_tile = Some(member);
            self.ensure_content(url);
            self.save_session();
            self.view.request_redraw();
            return;
        }
        // Add a session (the add-pill's "Add session"): a cross-window new-graph op
        // the host queues — `create_session` is on `Shell`, not `WindowCtx`.
        if let ContextAction::AddSession = action {
            self.commands.push(super::ShellCommand::CreateSession);
            return;
        }
        // Place a field region. From the empty-space right-click it lands at the saved
        // cursor anchor; from the add-pill (no anchor) it places at the orrery view
        // center. No member set. (Field regions P0.)
        if let ContextAction::AddField = action {
            let anchor = match self.view.context_origin.take() {
                Some(origin) => origin,
                None => {
                    let r = self.orrery_leaf_rect();
                    self.orrery_point((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0)
                }
            };
            let _ = self.orrery.add_field_at(anchor);
            self.save_session();
            self.view.request_redraw();
            return;
        }
        let set = std::mem::take(&mut self.view.context_set);
        if set.is_empty() {
            return;
        }
        // These open tiles, so summon the workbench pane (closing the suggestions
        // dropdown on the way in, like Ctrl+T does).
        if !self.workbench_open() {
            self.view.runner.update(Chrome::close_suggestions);
        }
        self.open_workbench();
        match action {
            ContextAction::OpenSplits => {
                self.view.workbench.open_split(&set);
            }
            ContextAction::Stack => {
                self.view.workbench.open_stack(&set);
            }
            ContextAction::ShellbarMove(_)
            | ContextAction::Relate
            | ContextAction::AddNode
            | ContextAction::AddTile
            | ContextAction::AddSession
            | ContextAction::AddField => {
                unreachable!("handled above")
            }
        }
        self.view.request_redraw();
    }

    /// Delete the focused node from the graph and reap its activation (the actor
    /// winds down on drop). A no-op when zero or many nodes are focused. Deletion
    /// removes the node's data; deactivation just stops its actor — this does
    /// both, because the node itself is gone.
    pub(super) fn delete_focused_node(&mut self) {
        if let Some(member) = self.orrery.remove_focused() {
            self.view.live_previews.remove(&member);
            self.shared.content.constellation.reap(member);
            self.save_session();
            self.view.request_redraw();
        }
    }

    /// Toggle the focused node's background flag: when set, its actor keeps
    /// running after focus moves away (the headless-active state for background
    /// work). A no-op when nothing is focused or the focused node has no live
    /// actor yet (focused but not rendered — press again once it has).
    pub(super) fn toggle_focus_background(&mut self) {
        let Some(member) = self.focused_member() else {
            return;
        };
        let next = !self.shared.content.constellation.is_background(member);
        if self.shared.content.constellation.set_background(member, next) {
            tracing::info!(%member, background = next, "toggled node background");
            self.view.request_redraw();
        }
    }

    /// Toggle the focused node's compatibility view: render it through the
    /// system WebView (the scrying pool) instead of a content actor. Pinning
    /// also opens the live card so the WebView is visible immediately; the
    /// node's actor (if any) is reaped since the WebView replaces it.
    /// (Scrying tile plan, X1; session-local pin — the durable `compat_mode`
    /// node field takes over in X3.)
    pub(super) fn toggle_focus_compat(&mut self) {
        let Some(member) = self.focused_member() else {
            return;
        };
        // The pin is shared session state; the WebView that serves it is this
        // window's per-view pool. Toggle the shared pin, reap the local tile.
        let on = if self.shared.content.compat_pins.remove(&member) {
            self.view.scrying.reap(member);
            false
        } else {
            self.shared.content.compat_pins.insert(member);
            true
        };
        if on {
            self.view.live_previews.insert(member);
            self.shared.content.constellation.reap(member);
        } else if self.view.scrying_input_focus == Some(member) {
            self.view.scrying_input_focus = None; // unpinned the tile that held the keyboard
        }
        tracing::info!(%member, compat = on, "toggled compatibility view");
        self.view.request_redraw();
    }

    /// Retry the focused node's page fetch. Unlike `ensure_content`, this bypasses
    /// the durable cache so Steward can ask the live fetch actor to try again.
    pub(super) fn retry_focused_content(&mut self) {
        let Some(url) = self.current_focus_url() else {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                super::observability::Severity::Warn,
                "retry focused content: no focused node",
            );
            return;
        };
        if !fetch::is_fetchable(&url) {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                super::observability::Severity::Warn,
                format!("retry focused content: not fetchable: {url}"),
            );
            return;
        }
        self.shared.content.pages
            .insert(url.clone(), fetch::ContentState::Loading);
        self.shared.observability
            .record_actor("fetch", "started", Some(url.clone()));
        self.shared.content.fetch_handle.command(fetch::FetchCommand::Page(url));
        self.view.request_redraw();
    }

    /// Stop the focused live operation. In Cartography this demotes a live
    /// preview; in Tree it closes the focused tile because a visible tile is, by
    /// definition, still a needed operation and would otherwise respawn.
    pub(super) fn stop_focused_operation(&mut self) {
        let Some(member) = self.focused_member() else {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                super::observability::Severity::Warn,
                "stop focused operation: no focused node",
            );
            return;
        };
        if self.workbench_active() {
            self.view.workbench.close_tile(member);
            if self.view.workbench.open_members().is_empty() {
                self.close_workbench();
            } else if self.view.focused_tile == Some(member) {
                self.view.focused_tile = self.view.workbench.open_members().first().copied();
            }
        }
        self.view.live_previews.remove(&member);
        self.shared.content.constellation.reap(member);
        self.shared.observability
            .record_actor("content", "stopped", Some(member.to_string()));
        self.view.request_redraw();
    }

    /// Keep the focused operation alive in the background. If the node is dormant,
    /// promote it to a live preview first so the actor exists to pin.
    pub(super) fn pin_focused_operation(&mut self) {
        let Some(member) = self.focused_member() else {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                super::observability::Severity::Warn,
                "pin focused operation: no focused node",
            );
            return;
        };
        if let Some(url) = self.current_focus_url() {
            self.ensure_content(&url);
        }
        self.view.live_previews.insert(member);
        let gid = self.view.focused_graph;
        let needed: Vec<_> = self.needed_members().into_iter().map(|m| (m, gid)).collect();
        self.shared.content.constellation.reconcile(&needed);
        if self.shared.content.constellation.set_background(member, true) {
            self.shared.observability
                .record_actor("content", "pinned", Some(member.to_string()));
        }
        self.view.request_redraw();
    }

    /// The set of graph members that should be active this frame: in Tree the open
    /// tiles, in Cartography just the focused node (if any). The constellation
    /// reconciles its actor pool to this.
    pub(super) fn needed_members(&self) -> Vec<GraphMemberId> {
        // The orrery and the workbench coexist, so the active set is the union: the
        // orrery's live-preview cards plus (when the workbench pane is open) every
        // open tile across every slot. A node showing in both counts once.
        let mut needed: Vec<GraphMemberId> = self.view.live_previews.iter().copied().collect();
        if self.workbench_open() {
            for member in self.view.workbench.open_members() {
                if !needed.contains(&member) {
                    needed.push(member);
                }
            }
        }
        needed
    }

    /// Double-click on the focused node toggles its **live preview**: promote the
    /// static "last visit" snapshot card to a live actor card, or demote it back.
    /// The live set drives [`needed_members`](Self::needed_members) in Cartography,
    /// so promoting spawns the actor next frame and demoting reaps it (its last
    /// scene is kept as the node's snapshot). (Card system P2/P3.)
    pub(super) fn toggle_live_preview(&mut self) {
        let Some(member) = self.focused_member() else {
            return;
        };
        if self.view.live_previews.remove(&member) {
            self.shared.content.constellation.reap(member); // demote: actor down, scene -> snapshot
        } else {
            self.view.live_previews.insert(member);
        }
        self.view.request_redraw();
    }

    /// The per-node activation state for the orrery's node coloring. A node with
    /// real fetched (`Ready`) content is `Open` (green) when a live actor is
    /// showing it, else `Closed` (red); everything else — a local / synthesized
    /// page, a blank (loading) one, or an errored one — is `Idle` (blue).
    pub(super) fn node_states(&self) -> HashMap<GraphMemberId, NodeState> {
        self.orrery
            .graph()
            .nodes()
            .map(|(_key, node)| {
                let state = match self.shared.content.pages.get(node.url()) {
                    Some(fetch::ContentState::Ready(_)) => {
                        if self.shared.content.constellation.is_active(node.id) {
                            NodeState::Open
                        } else {
                            NodeState::Closed
                        }
                    }
                    _ => NodeState::Idle,
                };
                (node.id, state)
            })
            .collect()
    }

    /// The per-node content silhouette for the orrery's node shaping, computed
    /// from each node's fetched content type (the same content map `node_states`
    /// reads). Only this-session-fetched nodes get an entry; unknown / unfetched
    /// nodes fall back to [`NodeShape::Square`] (the orrery's default), so a node
    /// takes its content shape as soon as it loads.
    pub(super) fn node_shapes(&self) -> HashMap<GraphMemberId, NodeShape> {
        self.orrery
            .graph()
            .nodes()
            .filter_map(|(_key, node)| match self.shared.content.pages.get(node.url()) {
                Some(fetch::ContentState::Ready(fetched)) => {
                    Some((node.id, content_shape(fetched.content_type.as_deref())))
                }
                _ => None,
            })
            .collect()
    }

    /// The focused node's graph member, if a node is focused (resolved URL → node
    /// UUID via the kernel node id).
    pub(super) fn focused_member(&self) -> Option<GraphMemberId> {
        let url = self.orrery.focused_url()?;
        self.orrery
            .graph()
            .get_node_by_url(url)
            .map(|(_, node)| node.id)
    }

    /// Load durably-cached content for `url` (page or subresource), or `None`.
    /// The fjall store's futures are ready, so `block_on` does not stall the UI.
    pub(super) fn load_cached(&mut self, url: &str) -> Option<content_store::StoredContent> {
        let store = self.shared.content.store.as_mut()?;
        pollster::block_on(content_store::load_content(store, url))
            .ok()
            .flatten()
    }

    /// Persist `body` (+ its content-type) for `url` to the durable content cache,
    /// so a reload need not re-fetch it. Best-effort; a write failure is logged.
    pub(super) fn save_cached(&mut self, url: &str, content_type: Option<String>, body: &[u8]) {
        let Some(store) = self.shared.content.store.as_mut() else {
            return;
        };
        let stored = content_store::StoredContent {
            content_type,
            body: body.to_vec(),
        };
        if let Err(err) = pollster::block_on(content_store::save_content(store, url, &stored)) {
            tracing::warn!(%err, url, "failed to cache content");
        }
    }

    /// Apply a pending workbench action the workbench root captured: switch the
    /// visible tab, close a tab (reaping its actor), or toggle its pin (the
    /// background-keep flag, which also exempts it from cap eviction).
    pub(super) fn drain_workbench_action(&mut self) {
        let Some(action) = self.view.workbench_runner.state().pending else {
            return;
        };
        self.view.workbench_runner.update(|s| s.pending = None);
        match action {
            WorkbenchAction::Activate(member) => {
                self.view.workbench.activate(member);
                self.view.focused_tile = Some(member);
            }
            WorkbenchAction::Close(member) => {
                self.view.workbench.close_tile(member);
                self.shared.content.constellation.reap(member);
                if self.view.workbench.open_members().is_empty() {
                    // Closing the last tile closes the workbench pane entirely (back
                    // to just the orrery), rather than leaving an empty pane.
                    // (Workbench-as-pane.)
                    self.close_workbench();
                } else if self.view.focused_tile == Some(member) {
                    self.view.focused_tile = self.view.workbench.open_members().first().copied();
                }
            }
            WorkbenchAction::TogglePin(member) => {
                let pinned = self.shared.content.constellation.is_background(member);
                self.shared.content.constellation.set_background(member, !pinned);
            }
            WorkbenchAction::NewTile => {
                // The "+" affordance: mint a fresh unlinked node and open it as a
                // tile, focused — a non-omnibar "new tile". Summon/tile the workbench
                // pane first (idempotent), matching every other tile-opening path, so
                // the new tile is actually shown regardless of the prior projection.
                self.open_workbench();
                let url = "mere://welcome";
                let member = self.orrery.open_member_as_new_node(None, url);
                self.view.workbench.open_tile(member);
                self.view.focused_tile = Some(member);
                self.ensure_content(url);
                self.save_session();
            }
        }
        self.view.request_redraw();
    }

    /// Execute a pending "connect to peer" request the chrome queued (S5.1): take
    /// the ticket the verb captured from the address bar and drive the sync actor.
    /// The chrome records the intent; this is the host executing it.
    pub(super) fn drain_pending_connect(&mut self) {
        let Some(ticket) = self.view.runner.state().pending_connect.clone() else {
            return;
        };
        self.view.runner.update(|c| {
            c.pending_connect = None;
        });
        if ticket.is_empty() {
            tracing::warn!("connect to peer: paste the peer's ticket in the address bar first");
            return;
        }
        // Route the verb to the sync actor; it runs the dial on its runtime and logs
        // the outcome (the actor boundary, so no synchronous result here).
        self.shared.sync_handle.command(sync::SyncCommand::Connect(ticket));
        self.view.request_redraw();
    }

    /// Execute a pending host action the palette queued: take it from the chrome
    /// and dispatch to the matching shell method. Returns a one-shot user-facing
    /// note for commands that want to report why they no-opped (the omnibar echoes
    /// it; other callers ignore the return). `None` means "nothing to say".
    pub(super) fn drain_pending_command(&mut self) -> Option<String> {
        let Some(cmd) = self.view.runner.state().pending_command else {
            return None;
        };
        self.view.runner.update(|c| c.pending_command = None);
        let mut note = None;
        match cmd {
            Command::ToggleWorkbench => self.toggle_workbench(),
            Command::DeleteNode => self.delete_focused_node(),
            Command::BackgroundNode => self.toggle_focus_background(),
            Command::HideSelectedEdge => {
                if self.orrery.hide_selected_edges() > 0 {
                    self.view.request_redraw();
                }
            }
            Command::ShowAllEdges => {
                if self.orrery.show_all_edges() > 0 {
                    self.view.request_redraw();
                }
            }
            Command::ToggleRoster => self.toggle_pane(PaneContent::Roster),
            Command::ToggleGloss => self.toggle_pane(PaneContent::Gloss),
            Command::ToggleApparatus => self.toggle_pane(PaneContent::Apparatus),
            Command::ToggleInspector => self.toggle_pane(PaneContent::Inspector),
            Command::ToggleSteward => self.toggle_pane(PaneContent::Steward),
            Command::RetryFocusedContent => self.retry_focused_content(),
            Command::StopFocusedOperation => self.stop_focused_operation(),
            Command::PinFocusedOperation => self.pin_focused_operation(),
            Command::ToggleCompatView => self.toggle_focus_compat(),
            Command::AssertEdge => {
                if self.orrery.assert_selected_relation(SemanticSubKind::UserGrouped) {
                    self.save_session();
                    self.view.request_redraw();
                } else {
                    note = Some("Select exactly two nodes to relate".to_string());
                }
            }
            Command::RetractEdge => {
                if self.orrery.retract_selected_relation() > 0 {
                    self.save_session();
                    self.view.request_redraw();
                } else {
                    note = Some("Select two nodes (or an edge) to unrelate".to_string());
                }
            }
            // History / connect / settings / comms verbs run in the chrome; never
            // queued here as host intents.
            Command::Back
            | Command::Forward
            | Command::Home
            | Command::ConnectPeer
            | Command::OpenSettings
            | Command::ToggleComms => {}
        }
        note
    }

    /// Evaluate a `>`-prefixed omnibar command expression through the privileged
    /// [`CommandShell`] and apply what it emits. The expression reads a read-only
    /// [`ShellContext`] snapshot and returns the [`Command`]s it called; each is
    /// run through the same chrome path a palette pick takes (so back / forward /
    /// pane toggles behave identically), then the result text (or the error) is
    /// echoed in the omnibar. The fourth driver of the one `Command` spine,
    /// alongside the palette, the agent harness, and accesskit actions. (Omnibar
    /// command shell, S3.)
    pub(super) fn submit_omnibar_command(&mut self, expr: &str) {
        let context = self.shell_context();
        let outcome = CommandShell::new().eval(expr, &context);
        // `pending_command` is a single slot, so each emitted command is applied
        // and drained before the next — the same per-interaction routine
        // `chrome_activate` runs.
        let mut note: Option<String> = None;
        for &cmd in &outcome.commands {
            self.view.runner.update(move |c| c.run_command_and_close(cmd));
            self.drain_pending_connect();
            if let Some(n) = self.drain_pending_command() {
                note = Some(n);
            }
            self.drain_comms_intent();
            self.drain_history_step();
            self.drain_physics_toggle();
            self.sync_settings();
            self.sync_orrery();
        }
        // A kind-qualified `relate("cites")` applies the chosen relation to the
        // selected pair directly (the 0-arg `relate()` rode the AssertEdge path
        // above with the default kind).
        if let Some(kind) = &outcome.relation_kind {
            if self.orrery.assert_selected_relation(relation_kind_from_str(kind)) {
                self.save_session();
                self.view.request_redraw();
            } else {
                note = Some("Select exactly two nodes to relate".to_string());
            }
        }
        let (severity, echo) = match &outcome.error {
            Some(err) => (Severity::Warn, format!("error: {err}")),
            None => (Severity::Info, outcome.text.clone()),
        };
        // An honest, inspectable record of every command-line eval (no placebo):
        // the expression, how many commands it ran, and the result / error.
        self.shared.observability.record_diagnostic(
            "meerkat.omnibar.command",
            severity,
            format!("{expr:?} -> {} command(s); {echo}", outcome.commands.len()),
        );
        // Reset the bar. Priority: a command's no-op note (e.g. "select two nodes
        // to relate") so a silent no-op explains itself; else a query result /
        // error; else the focused location. Either way the typed `>expr` is cleared
        // and the (now command-empty) suggestion dropdown closes.
        let shown = note
            .or(if echo.is_empty() { None } else { Some(echo) })
            .unwrap_or_else(|| self.current_focus_url().unwrap_or_default());
        self.view.runner.update(move |c| c.show_location(&shown));
        self.view.request_redraw();
    }

    /// A read-only snapshot of host state the command shell may query: the
    /// location, history + nav capability, the focused node, and every graph node
    /// URL (the cross-to-orrery reach). Built fresh per eval; nothing writes it.
    fn shell_context(&self) -> ShellContext {
        // The focused node's inspection rows (the same the Inspector pane shows),
        // surfaced to scripts as `inspect()`.
        let node = self
            .focused_member()
            .and_then(|member| self.orrery.graph().get_node_by_id(member))
            .map(|(_, node)| node);
        let state = node.and_then(|node| self.shared.content.pages.get(node.url()));
        let inspect = super::inspector::inspector_rows(node, state);

        let chrome = self.view.runner.state();
        ShellContext {
            current_url: self.current_focus_url().unwrap_or_default(),
            history: chrome.history.entries().to_vec(),
            can_back: chrome.toolbar.can_go_back,
            can_forward: chrome.toolbar.can_go_forward,
            focused_node: self.orrery.focused_url().map(str::to_string),
            nodes: self
                .orrery
                .graph()
                .nodes()
                .map(|(_, node)| node.url().to_string())
                .collect(),
            inspect,
        }
    }

    /// Run the chrome's pending comms request (P6c): take the recorded
    /// [`CommsIntent`] and route it to the comms actor as a `CommsCommand`. The
    /// chrome can't reach the actor, so it records the intent and the host drains
    /// it here (mirrors [`drain_pending_command`](Self::drain_pending_command)).
    pub(super) fn drain_comms_intent(&mut self) {
        let Some(intent) = self.view.runner.state().comms_intent.clone() else {
            return;
        };
        self.view.runner.update(|c| c.comms_intent = None);
        self.shared.observability
            .record_actor("comms", "started", Some(format!("{intent:?}")));
        match intent {
            CommsIntent::Refresh => {
                self.shared.comms_handle.command(comms_host::CommsCommand::Refresh);
            }
            CommsIntent::Open(id) => {
                self.shared.comms_handle
                    .command(comms_host::CommsCommand::Open(id));
            }
            CommsIntent::Send(draft) => {
                self.shared.comms_handle
                    .command(comms_host::CommsCommand::Send(draft));
            }
            CommsIntent::ConnectCabal(ticket) => {
                self.shared.comms_handle
                    .command(comms_host::CommsCommand::ConnectCabal(ticket));
            }
        }
    }

    /// Apply the chrome's current settings to the host: the active-tab cap to the
    /// actor pool. Called after a chrome interaction that could have changed them.
    /// Persists to the settings sidecar when the value actually changed (so an
    /// unrelated chrome click doesn't re-write the file).
    pub(super) fn sync_settings(&mut self) {
        let cap = self.view.runner.state().settings.tab_cap;
        self.shared.content.constellation.set_cap(cap);
        if cap != self.shared.presentation.saved_tab_cap {
            self.shared.presentation.saved_tab_cap = cap;
            self.persist_settings();
        }
    }

    /// Write the current settings to the session's `settings.json` sidecar. A
    /// failure is logged, not fatal (the shell runs without persistence).
    pub(super) fn persist_settings(&self) {
        let settings = PersistedSettings {
            tab_cap: self.shared.presentation.saved_tab_cap,
            theme_id: Some(self.shared.presentation.active_theme_id.clone()),
            shellbar_edge: self.shared.presentation.shellbar_edge,
        };
        if let Err(err) = settings_store::save_settings(&self.shared.session.mere_root, &settings) {
            tracing::warn!(%err, "failed to persist settings");
        }
    }

    // ── Frame tree (F1) ──────────────────────────────────────────────────────

    /// The active session's root graph id (the `GraphId` graph-bound leaves carry).
    /// Falls back to a fresh id if the active manifest is somehow missing. (MG5.)
    pub(super) fn active_graph_id(&self) -> GraphId {
        self.shared.session.manifests
            .get(self.shared.session.active_session_id)
            .map(|m| m.root_graph_id)
            .unwrap_or_default()
    }

    /// The `graph_id` a freshly-summoned leaf of `content` should carry: the active
    /// graph for graph-bound panes, a nil (unbound) id for window-chrome. Keeps the
    /// "graph-bound leaves in this window share the active graph" invariant instead of
    /// the old random-per-leaf `GraphId::default()`. (MG5.)
    fn leaf_graph_id(&self, content: &PaneContent) -> GraphId {
        if content.follows_active_graph() {
            self.active_graph_id()
        } else {
            GraphId::nil()
        }
    }

    /// The content band (below the toolbar, inside the shellbar carve) in window
    /// coords — the same band `render` lays the panes out in. It must match render's
    /// `band_after_shellbar`: the input hit-rects (pane leaves, dividers, the orrery
    /// origin) and the a11y bounds all derive from this, so any divergence shifts
    /// every content click off from what's drawn (a left-docked shellbar carves the
    /// band's left edge, so a band starting at x=0 would offset clicks by the strip
    /// width). (Shellbar F2.1.)
    pub(super) fn content_band(&self) -> [f32; 4] {
        let th = self.view.toolbar_h.max(FALLBACK_TOOLBAR_H) as f32;
        // A slim (leaf) window carves no shellbar, so the band is the whole area below
        // the toolbar; the carve must match render's band exactly. (MW3 step 4.)
        if self.view.kind.is_slim() {
            return [0.0, th, self.view.width as f32, self.view.height as f32];
        }
        super::shellbar::band_after_shellbar(
            self.shared.presentation.shellbar_edge,
            self.view.width as f32,
            self.view.height as f32,
            th,
        )
    }

    /// Map a window point to the orrery pane's local space (its rect origin at 0,0) —
    /// the coordinate space the orrery's pointer + camera operate in (it rasterizes
    /// into its own texture and `render` composites that at the orrery leaf's origin).
    /// Subtracts the *leaf* origin, so it stays correct under a shellbar carve or a
    /// frame split, not just the toolbar inset. (Cursor-offset fix.)
    pub(super) fn orrery_point(&self, x: f32, y: f32) -> (f32, f32) {
        let rect = self.orrery_leaf_rect();
        (x - rect[0], y - rect[1])
    }

    /// The laid-out content panes (leaf rects) for the current frame layout.
    pub(super) fn laid_leaves(&self) -> Vec<frame_view::LaidLeaf> {
        frame_view::leaf_rects(&self.view.frame_layout, self.content_band(), self.view.maximized_pane)
    }

    /// The orrery (graph) pane's screen rect; the whole band when no orrery leaf is
    /// laid out (e.g. another pane is maximized). The orrery is always present.
    pub(super) fn orrery_leaf_rect(&self) -> [f32; 4] {
        let band = self.content_band();
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Orrery))
            .map(|l| l.rect)
            .unwrap_or(band)
    }

    /// The tiled-workbench pane's screen rect, if the workbench pane is open.
    pub(super) fn workbench_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Workbench))
            .map(|l| l.rect)
    }

    /// Whether the tiled-workbench pane is open (a Workbench leaf exists).
    pub(super) fn workbench_open(&self) -> bool {
        self.pane_of_content(&PaneContent::Workbench).is_some()
    }

    /// Whether the workbench is the active content pane (open + last-interacted) —
    /// so navigation (omnibar / Ctrl+Enter / Back-Forward) targets its focused tile
    /// rather than the orrery's selected node. (Workbench-as-pane.)
    pub(super) fn workbench_active(&self) -> bool {
        self.view.active_content == super::ContentPane::Workbench && self.workbench_open()
    }

    /// Summon the workbench pane beside the orrery (Tree projection) and make it the
    /// active content pane. Idempotent — only summons when not already open.
    pub(super) fn open_workbench(&mut self) {
        if !self.workbench_open() {
            self.view.workbench.ensure_tiled();
            let id = PaneId(self.view.next_pane_id);
            self.view.next_pane_id += 1;
            let graph_id = self.leaf_graph_id(&PaneContent::Workbench);
            let leaf = PaneNode::Leaf {
                pane_id: id,
                content: PaneContent::Workbench,
                graph_id,
            };
            let anchor =
                frame_view::pane_path(&self.view.frame_layout, super::GRAPH_PANE).unwrap_or_default();
            if self
                .view
                .frame_layout
                .summon_leaf(&anchor, InsertSide::Right, leaf)
            {
                self.view.frame_layout.set_split_ratio(&anchor, 0.6);
            }
            self.view.maximized_pane = None;
            self.shared.observability
                .record_pane_toggle(&PaneContent::Workbench, true);
            self.shared.observability
                .record_frame_layout_changed("workbench opened");
        }
        self.view.active_content = super::ContentPane::Workbench;
    }

    /// Close the workbench pane: reap its tiles' actors, clear the tiles, drop the
    /// pane leaf, and hand focus back to the orrery.
    pub(super) fn close_workbench(&mut self) {
        for member in self.view.workbench.open_members() {
            self.shared.content.constellation.reap(member);
        }
        self.view.workbench.clear_tiles();
        if let Some(id) = self.pane_of_content(&PaneContent::Workbench) {
            if let Some(path) = frame_view::pane_path(&self.view.frame_layout, id) {
                self.view.frame_layout.close_leaf(&path);
            }
        }
        self.view.focused_tile = None;
        self.view.active_content = super::ContentPane::Orrery;
        self.view.maximized_pane = None;
        self.shared.observability
            .record_pane_toggle(&PaneContent::Workbench, false);
        self.shared.observability
            .record_frame_layout_changed("workbench closed");
    }

    /// The comms pane's screen rect, if the comms pane is open. (Comms pane.)
    pub(super) fn comms_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Comms))
            .map(|l| l.rect)
    }

    /// Keep a Comms frame leaf in sync with the chrome's comms-open state. The comms
    /// pane is chrome-rendered (its compose field + click handlers live in the
    /// chrome), so it stays there; this only reserves / drops a frame leaf so the
    /// other panes make room, and the render positions the chrome overlay into it.
    /// (Comms pane.)
    pub(super) fn sync_comms_pane(&mut self) {
        let open = self.view.runner.state().comms.is_open();
        match (open, self.pane_of_content(&PaneContent::Comms)) {
            (true, None) => {
                let id = PaneId(self.view.next_pane_id);
                self.view.next_pane_id += 1;
                let graph_id = self.leaf_graph_id(&PaneContent::Comms);
                let leaf = PaneNode::Leaf {
                    pane_id: id,
                    content: PaneContent::Comms,
                    graph_id,
                };
                let anchor = frame_view::pane_path(&self.view.frame_layout, super::GRAPH_PANE)
                    .unwrap_or_default();
                if self
                    .view
                    .frame_layout
                    .summon_leaf(&anchor, InsertSide::Right, leaf)
                {
                    self.view.frame_layout.set_split_ratio(&anchor, 0.66);
                }
                self.view.maximized_pane = None;
                self.shared.observability
                    .record_pane_toggle(&PaneContent::Comms, true);
                self.shared.observability
                    .record_frame_layout_changed("comms opened");
            }
            (false, Some(id)) => {
                if let Some(path) = frame_view::pane_path(&self.view.frame_layout, id) {
                    self.view.frame_layout.close_leaf(&path);
                }
                self.view.maximized_pane = None;
                self.shared.observability
                    .record_pane_toggle(&PaneContent::Comms, false);
                self.shared.observability
                    .record_frame_layout_changed("comms closed");
            }
            _ => {}
        }
    }

    /// The roster pane's screen rect, if the roster is open.
    pub(super) fn roster_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Roster))
            .map(|l| l.rect)
    }

    /// The gloss pane's screen rect, if open.
    pub(super) fn gloss_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Gloss))
            .map(|l| l.rect)
    }

    /// If window point `(x, y)` falls on the focused scrying tile, the member +
    /// **tile-local** `(x, y)` to forward into its WebView. (Scrying X2.)
    pub(super) fn scrying_at(&self, x: f32, y: f32) -> Option<(GraphMemberId, i32, i32)> {
        let (member, r) = self.view.scrying_rect?;
        (x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
            .then(|| (member, (x - r[0]) as i32, (y - r[1]) as i32))
    }

    /// The node whose gloss minimap square contains window point `(x, y)`, if any.
    pub(super) fn gloss_node_at(&self, x: f32, y: f32) -> Option<GraphMemberId> {
        self.view.gloss_node_rects
            .iter()
            .find(|(_, r)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
            .map(|(member, _)| *member)
    }

    /// The pane (leaf) under window point `(x, y)`, if any.
    pub(super) fn pane_at(&self, x: f32, y: f32) -> Option<PaneId> {
        self.laid_leaves()
            .into_iter()
            .find(|l| x >= l.rect[0] && x < l.rect[2] && y >= l.rect[1] && y < l.rect[3])
            .map(|l| l.pane_id)
    }

    /// The id of the open leaf whose content equals `content`, if any.
    pub(super) fn pane_of_content(&self, content: &PaneContent) -> Option<PaneId> {
        self.view.frame_layout
            .iter_leaves()
            .find(|(_, c, _)| **c == *content)
            .map(|(id, _, _)| id)
    }

    /// Toggle a sibling pane (roster / apparatus / …): close it if its content is
    /// open, else summon it as a right split beside the graph pane (the graph keeps
    /// the larger share). Anchoring at the graph leaf — not the root — lets a
    /// second pane nest rather than fail against a non-leaf root. A layout change
    /// clears any maximize. (Frame tree, F1.)
    pub(super) fn toggle_pane(&mut self, content: PaneContent) {
        let recorded_content = content.clone();
        let mut opened = false;
        if let Some(id) = self.pane_of_content(&content) {
            if let Some(path) = frame_view::pane_path(&self.view.frame_layout, id) {
                self.view.frame_layout.close_leaf(&path);
            }
        } else {
            let id = PaneId(self.view.next_pane_id);
            self.view.next_pane_id += 1;
            let graph_id = self.leaf_graph_id(&content);
            let leaf = PaneNode::Leaf {
                pane_id: id,
                content,
                graph_id,
            };
            let anchor = frame_view::pane_path(&self.view.frame_layout, GRAPH_PANE).unwrap_or_default();
            if self
                .view
                .frame_layout
                .summon_leaf(&anchor, InsertSide::Right, leaf)
            {
                self.view.frame_layout.set_split_ratio(&anchor, 0.7);
            }
            opened = true;
        }
        self.view.maximized_pane = None;
        self.shared.observability
            .record_pane_toggle(&recorded_content, opened);
        self.shared.observability.record_frame_layout_changed(format!(
            "{} {}",
            recorded_content.tag(),
            if opened { "opened" } else { "closed" }
        ));
        self.view.request_redraw();
    }

    /// Switch the active theme: re-resolve from the registry, rebuild the chrome
    /// CSS + tokens, drop the host-drawn caches so they re-rasterize with the new
    /// palette, persist the choice, and redraw. (Theme switcher; the orrery's own
    /// palette is themed in A2.)
    pub(super) fn set_theme(&mut self, theme_id: &str) {
        let resolution = self.shared.presentation.theme.set_active_theme(theme_id);
        self.shared.presentation.active_theme_id = resolution.resolved_id;
        self.shared.presentation.chrome_theme = resolution.tokens.chrome;
        self.shared.presentation.chrome_sheet = crate::chrome_sheet(&self.shared.presentation.chrome_theme);
        // Re-theme the orrery's backdrop + edges to match. (A2.)
        let (backdrop, edge) = crate::orrery_palette(&resolution.tokens);
        self.orrery.set_palette(backdrop, edge);
        self.view.window_controls_tex = None;
        self.view.divider_tex = None;
        self.persist_settings();
        self.shared.observability
            .record_theme_activated(&self.shared.presentation.active_theme_id);
        self.view.request_redraw();
    }

    /// The apparatus pane's screen rect, if open.
    pub(super) fn apparatus_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Apparatus))
            .map(|l| l.rect)
    }

    /// The registered themes as apparatus options (id + display name + active),
    /// from the known theme ids. (The registry doesn't list themes yet.)
    pub(super) fn theme_options(&self) -> Vec<apparatus::ThemeOption> {
        use register_theme::theme::{
            THEME_ID_DARK, THEME_ID_DEFAULT, THEME_ID_HIGH_CONTRAST, THEME_ID_LIGHT,
        };
        [
            THEME_ID_DEFAULT,
            THEME_ID_LIGHT,
            THEME_ID_DARK,
            THEME_ID_HIGH_CONTRAST,
        ]
        .iter()
        .map(|id| {
            let res = self.shared.presentation.theme.resolve_theme(Some(id));
            apparatus::ThemeOption {
                active: res.resolved_id == self.shared.presentation.active_theme_id,
                id: res.resolved_id,
                name: res.tokens.display_name,
            }
        })
        .collect()
    }

    /// Read-only system rows for the apparatus Overview section.
    pub(super) fn apparatus_system_rows(&self) -> Vec<(String, String)> {
        vec![
            (
                "Nodes".to_string(),
                self.orrery.graph().nodes().count().to_string(),
            ),
            (
                "Active actors".to_string(),
                self.shared.content.constellation.active_count().to_string(),
            ),
            ("Tab cap".to_string(), self.shared.presentation.saved_tab_cap.to_string()),
            ("Theme".to_string(), self.shared.presentation.active_theme_id.clone()),
            ("Uptime".to_string(), self.shared.observability.snapshot().uptime),
        ]
    }

    /// Refresh and snapshot the host observability cache for Apparatus.
    pub(super) fn apparatus_observability(&mut self) -> ObservabilitySnapshot {
        self.refresh_a11y_summary();
        self.shared.observability.snapshot()
    }

    /// Toggle maximize of the pane under the cursor (full-screen and back). With a
    /// single pane this is a no-op visually. (Frame tree, F1.)
    pub(super) fn toggle_maximize(&mut self) {
        if self.view.maximized_pane.is_some() {
            self.view.maximized_pane = None;
        } else if let Some(pane) = self.pane_at(self.view.cursor.0, self.view.cursor.1) {
            self.view.maximized_pane = Some(pane);
        }
        self.view.request_redraw();
    }

    /// The frame divider gutter under window point `(x, y)`, as its split path +
    /// the split's (parent) rect + axis — for starting a divider drag.
    pub(super) fn frame_divider_at(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(Vec<SplitChoice>, [f32; 4], SplitAxis)> {
        let band = self.content_band();
        frame_view::divider_rects(&self.view.frame_layout, band, self.view.maximized_pane)
            .into_iter()
            .find(|d| x >= d.rect[0] && x < d.rect[2] && y >= d.rect[1] && y < d.rect[3])
            .map(|d| (d.path, d.parent, d.axis))
    }

    /// Drive an in-progress frame-divider drag from the current cursor: map the
    /// pointer's position within the split's parent rect to a new ratio.
    pub(super) fn drag_frame_divider(&mut self) {
        let Some((path, parent, axis)) = self.view.frame_divider_drag.clone() else {
            return;
        };
        let (cx, cy) = self.view.cursor;
        let ratio = match axis {
            SplitAxis::Horizontal => {
                (cx - parent[0]) / (parent[2] - parent[0] - frame_view::DIVIDER).max(1.0)
            }
            SplitAxis::Vertical => {
                (cy - parent[1]) / (parent[3] - parent[1] - frame_view::DIVIDER).max(1.0)
            }
        };
        // `set_split_ratio` clamps to a sane minimum so a pane can't collapse.
        self.view.frame_layout.set_split_ratio(&path, ratio);
        self.view.request_redraw();
    }

    /// The roster rows: every graph node as a row (url as title, content type as
    /// subtitle, focused node marked selected). (Frame tree, F1 roster.)
    pub(super) fn roster_rows(&self) -> Vec<roster::RosterRow> {
        // Highlight by member id against the live selection set (not by focused_url,
        // which collapses to None for a multi-selection and aliases duplicate URLs
        // — both common now that Add node / New tile mint same-URL nodes).
        let selected_members: std::collections::HashSet<GraphMemberId> =
            self.orrery.selected_members().into_iter().collect();
        let graph = self.orrery.graph();
        let mut rows: Vec<roster::RosterRow> = self
            .orrery
            .graph()
            .nodes()
            .map(|(_key, node)| {
                let url = node.url().to_string();
                let title = if !node.title.is_empty() {
                    node.title.clone()
                } else if let Some(host) = &node.cached_host {
                    host.clone()
                } else {
                    url.clone()
                };
                let content_type = match self.shared.content.pages.get(&url) {
                    Some(fetch::ContentState::Ready(fetched)) => fetched.content_type.clone(),
                    _ => node.mime_hint.clone(),
                };
                let mut tags: Vec<String> = node.tags.iter().cloned().collect();
                tags.sort();
                let selected = selected_members.contains(&node.id);
                // Edge detail only for a sole focused node (not every row of a
                // multi-selection), keeping it O(n) once, never per selected row.
                let edges = if selected && selected_members.len() == 1 {
                    let node_key = graph.get_node_by_id(node.id).map(|(k, _)| k);
                    if let Some(key) = node_key {
                        graph
                            .relations()
                            .filter(|r| r.from == key || r.to == key)
                            .filter_map(|r| {
                                let (direction, other_key) = if r.from == key {
                                    (roster::EdgeDir::Out, r.to)
                                } else {
                                    (roster::EdgeDir::In, r.from)
                                };
                                let other = graph.get_node(other_key)?;
                                let other_title = if !other.title.is_empty() {
                                    other.title.clone()
                                } else {
                                    other.cached_host.clone().unwrap_or_else(|| other.url().to_string())
                                };
                                Some(roster::EdgeRow {
                                    direction,
                                    kind_label: relation_kind_label(r.kind).to_string(),
                                    other_title,
                                    other_url: other.url().to_string(),
                                    other_member: other.id,
                                })
                            })
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };
                roster::RosterRow {
                    member: node.id,
                    title,
                    url,
                    content_type,
                    tags,
                    edges,
                    selected,
                    section_header: None,
                }
            })
            .collect();
        // Sort by (content-type bucket, title) so nodes group by kind.
        rows.sort_by(|a, b| {
            let ba = content_bucket(a.content_type.as_deref());
            let bb = content_bucket(b.content_type.as_deref());
            ba.0.cmp(&bb.0)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        // Stamp section headers on the first row of each new bucket.
        let mut current: Option<u8> = None;
        for row in &mut rows {
            let (ord, label) = content_bucket(row.content_type.as_deref());
            if current != Some(ord) {
                current = Some(ord);
                row.section_header = Some(label.to_string());
            }
        }
        rows
    }

    /// Field-region rows for the roster: one per active field, its display name (the
    /// authoring name, else a short id) and hidden state. (Field regions — roster.)
    pub(super) fn roster_field_rows(&self) -> Vec<roster::FieldRow> {
        let mut out = Vec::new();
        for field in self.orrery.graph().fields() {
            if !field.is_active() {
                continue;
            }
            let id = field.id;
            let uuid = id.as_uuid().to_string();
            let name = field
                .name
                .clone()
                .unwrap_or_else(|| format!("Field {}", &uuid[..8.min(uuid.len())]));
            out.push(roster::FieldRow { id, name, hidden: !self.orrery.field_visible(id) });
        }
        // Deterministic order (graph.fields() is HashMap-unordered): by name, then id
        // — matching the node rows' explicit sort. (Field regions — roster.)
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.as_uuid().cmp(&b.id.as_uuid()))
        });
        out
    }

    pub(super) fn utility_pane_rows(&self, content: &PaneContent) -> Vec<(String, String)> {
        match content {
            PaneContent::Inspector => {
                let focused = self.focused_member();
                let node = focused
                    .and_then(|member| self.orrery.graph().get_node_by_id(member))
                    .map(|(_, node)| node);
                let state = node.and_then(|node| self.shared.content.pages.get(node.url()));
                super::inspector::inspector_rows(node, state)
            }
            PaneContent::Steward => self.steward_rows(),
            _ => Vec::new(),
        }
    }

    pub(super) fn steward_rows(&self) -> Vec<(String, String)> {
        let operations = self.shared.content.constellation.active_operations();
        let mut rows = vec![
            (
                "Active operations".to_string(),
                operations.len().to_string(),
            ),
            ("Tab cap".to_string(), self.shared.presentation.saved_tab_cap.to_string()),
            (
                "Live graphs".to_string(),
                format!("{} / {}", self.orrery_pool_count, super::MAX_POOLED_ORRERIES),
            ),
            (
                "Loading fetches".to_string(),
                self.fetch_state_count(1).to_string(),
            ),
            (
                "Failed fetches".to_string(),
                self.fetch_state_count(3).to_string(),
            ),
            ("Sync".to_string(), self.sync_summary()),
            (
                "Actions".to_string(),
                "retry.focused / stop.focused / pin.focused".to_string(),
            ),
        ];
        let focused = self.focused_member();
        rows.push((
            "Focused operation".to_string(),
            match focused {
                Some(member) if self.shared.content.constellation.is_active(member) => format!(
                    "active background={} recovering={}",
                    self.shared.content.constellation.is_background(member),
                    self.shared.content.constellation.is_recovering(member)
                ),
                Some(_) => "dormant".to_string(),
                None => "none".to_string(),
            },
        ));
        for operation in operations.into_iter().take(6) {
            rows.push((
                format!("Operation {}", short_member(operation.member)),
                format!(
                    "{} background={} recovering={} scene={} height={}",
                    operation.url.as_deref().unwrap_or("not shown yet"),
                    operation.background,
                    operation.recovering,
                    operation.scene_version,
                    operation.content_height
                ),
            ));
        }
        rows
    }

    fn fetch_state_count(&self, tag: u8) -> usize {
        self.shared.content.pages
            .values()
            .filter(|state| fetch::ContentState::tag(Some(*state)) == tag)
            .count()
    }

    fn sync_summary(&self) -> String {
        let indicator = &self.view.runner.state().sync;
        if indicator.active {
            format!(
                "{} syncing={} ops={}",
                indicator.label, indicator.syncing, indicator.ops
            )
        } else {
            "off".to_string()
        }
    }
}

// Session ops live on `Shell`, not `WindowCtx`: switching a session re-keys the
// orrery pool, and a `WindowCtx` holds exactly one orrery borrowed *out* of the
// pool, so it cannot insert or re-key entries. Per-window input handlers request
// these by pushing a [`ShellCommand`]; `Shell::apply` runs them after the ctx
// borrow ends (the same seam as spawn/close). WindowCtx-shaped sub-steps
// (save_session, the cache reset, thumbnails) re-enter through `self.ctx()`, which
// resolves the focused view primary-or-pending and bundles its pooled orrery.
// (Window composition P1, multi-graph.)
impl super::Shell {
    /// Mint a fresh, empty session and make it active (persisting the current one
    /// first). The new session opens on the welcome node. Returns its id. (MG3.)
    pub(super) fn create_session(&mut self) -> SessionId {
        self.ctx().save_session();
        let session_id = SessionId::new();
        let session_dir = self
            .shared
            .session
            .mere_root
            .join("sessions")
            .join(session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        let mut manifest = GraphSessionManifest::new(session_id, GraphId::new());
        manifest.storage_path = Some(session_dir.clone());
        self.shared.session.manifests.insert(manifest);
        let _ = self.shared.session.manifests.flush_dirty();
        self.load_active_session(session_id, session_dir, true);
        session_id
    }

    /// Switch the active session to `target`: persist the current one, then load the
    /// target's graph + camera + frame. No-op if already active or unknown. (MG2.)
    pub(super) fn switch_session(&mut self, target: SessionId) {
        if target == self.shared.session.active_session_id
            || self.shared.session.manifests.get(target).is_none()
        {
            return;
        }
        self.ctx().save_session();
        let session_dir = self
            .shared
            .session
            .mere_root
            .join("sessions")
            .join(target.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        self.load_active_session(target, session_dir, false);
    }

    /// Switch to the next (`forward`) or previous session in id order, wrapping.
    /// No-op with fewer than two sessions. (MG2.)
    pub(super) fn cycle_session(&mut self, forward: bool) {
        let mut ids: Vec<SessionId> =
            self.shared.session.manifests.iter().map(|(id, _)| id).collect();
        if ids.len() < 2 {
            return;
        }
        ids.sort_by_key(|id| *id.as_uuid());
        let Some(pos) = ids.iter().position(|id| *id == self.shared.session.active_session_id)
        else {
            return;
        };
        let next = if forward {
            (pos + 1) % ids.len()
        } else {
            (pos + ids.len() - 1) % ids.len()
        };
        self.switch_session(ids[next]);
    }

    /// Close (trash) the `target` session. Refuses to close the last one. If it was
    /// active, switches to the most-recently-updated survivor first. (MG3.)
    pub(super) fn close_session(&mut self, target: SessionId) {
        if self.shared.session.manifests.len() <= 1
            || self.shared.session.manifests.get(target).is_none()
        {
            return;
        }
        if target == self.shared.session.active_session_id {
            let survivor = self
                .shared
                .session
                .manifests
                .iter()
                .filter(|(id, _)| *id != target)
                .max_by_key(|(_, m)| m.updated_at)
                .map(|(id, _)| id);
            if let Some(next) = survivor {
                self.switch_session(next);
            }
        }
        if let Err(err) = self.shared.session.manifests.move_to_trash(target) {
            tracing::warn!(%err, "failed to trash the closed session");
        }
        self.focused_view_mut().renaming = None;
        self.ctx().refresh_session_thumbnails();
        self.focused_view_mut().request_redraw();
    }

    /// Make `id` (whose dir is `session_dir`) the active session: load its graph +
    /// camera + frame and reset the prior session's runtime caches. `fresh` marks a
    /// just-minted empty session. Re-keys the focused orrery to the target graph in
    /// the pool (the Shell-only step a WindowCtx cannot do); the rest runs on the
    /// re-entered ctx, which now resolves that re-keyed orrery. (MG2.)
    fn load_active_session(&mut self, id: SessionId, session_dir: std::path::PathBuf, fresh: bool) {
        // The target graph (empty for a fresh session, or a missing/corrupt file).
        let graph = if fresh {
            Graph::new()
        } else {
            session_graph_store::load(&session_dir.join(session_graph_store::GRAPH_FILE))
                .ok()
                .flatten()
                .unwrap_or_else(Graph::new)
        };
        let empty = graph.nodes().count() == 0;
        let target_graph = self
            .shared
            .session
            .manifests
            .get(id)
            .map(|m| m.root_graph_id)
            .unwrap_or_default();
        // Pool the target graph's orrery (Shell-level — a WindowCtx cannot insert
        // pool entries), keeping the *outgoing* graph's orrery live so two graphs
        // coexist. A graph shown for the first time mints a fresh orrery from its
        // loaded graph and offloads its own physics actor (like the seed did at
        // boot); switching back to a still-pooled graph reuses its live orrery —
        // its state was kept warm, so disk is not reloaded over it. The outgoing
        // graph is parked (its content actors reaped per-graph below); its orrery
        // stays in the pool.
        let old_gid = self.focused_view().focused_graph;
        let wake = self.physics_wake.clone();
        match self.orreries.entry(target_graph) {
            std::collections::hash_map::Entry::Occupied(_) => {
                // Switching back: the pooled orrery is authoritative. No reload.
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                let mut orrery = orrery::Orrery::with_graph(graph);
                // An empty session opens on the welcome node, like first launch.
                if empty {
                    orrery.visit("mere://welcome");
                }
                orrery.offload_physics(wake);
                slot.insert(orrery);
            }
        }
        self.focused_view_mut().focused_graph = target_graph;

        // Park the outgoing graph's physics (OQ2 park): a switched-away graph stays
        // warm in the pool, but stop it settling so it does not keep ticking and
        // waking the loop in the background — its actor then idles on its channel.
        // Switching back shows its settled positions; interaction resumes the settle.
        if old_gid != target_graph {
            if let Some(parked) = self.orreries.get_mut(&old_gid) {
                parked.park_physics();
            }
        }

        // Pool eviction (OQ2 unload): keep `target_graph` most-recent in the LRU,
        // then drop the stalest pooled orreries over the cap that no window is
        // focused on. Dropping an orrery ends its physics actor thread; its content
        // actors are reaped too. The graph was already saved when it was last
        // switched away from, so eviction loses no data — switching back reloads it.
        self.orrery_lru.retain(|g| *g != target_graph);
        self.orrery_lru.push(target_graph);
        if self.orreries.len() > super::MAX_POOLED_ORRERIES {
            let focused: std::collections::HashSet<GraphId> = self
                .windows
                .values()
                .map(|v| v.focused_graph)
                .chain(self.pending_view.iter().map(|v| v.focused_graph))
                .collect();
            while self.orreries.len() > super::MAX_POOLED_ORRERIES {
                let Some(stale) = self.orrery_lru.iter().copied().find(|g| !focused.contains(g))
                else {
                    break;
                };
                self.orrery_lru.retain(|g| *g != stale);
                self.orreries.remove(&stale); // drop → physics actor thread ends
                self.shared.content.constellation.reap_graph(stale);
            }
        }

        // The remainder runs on the focused window's ctx (now resolving the re-keyed
        // orrery): restore camera + focus, retag the graph-bound leaves, reset the
        // prior session's runtime caches, and swap identity.
        let restored_view =
            view_intent_store::load_view_intent(&session_dir, DEFAULT_FRAME, DEFAULT_PANE)
                .ok()
                .flatten();
        let mut ctx = self.ctx();
        match restored_view.as_ref().and_then(|v| v.camera.as_ref()) {
            Some(snapshot) => {
                ctx.orrery.set_camera(super::snapshot_to_camera(snapshot));
                ctx.view.centered = true;
            }
            None => ctx.view.centered = false,
        }
        if let Some(url) = restored_view.as_ref().and_then(|v| v.focus.as_deref()) {
            ctx.orrery.select_by_url(url);
        }
        ctx.view.frame_layout.retag_graph_bound(target_graph);
        ctx.shared.content.constellation.reap_graph(old_gid);
        ctx.view.scrying.clear();
        ctx.shared.content.compat_pins.clear();
        ctx.view.scrying_input_focus = None;
        ctx.view.scrying_rect = None;
        ctx.shared.content.pages.clear();
        ctx.view.live_previews.clear();
        ctx.view.tile_textures.clear();
        ctx.view.snapshot_textures.clear();
        ctx.view.scroll.clear();
        ctx.view.focused_tile = None;
        ctx.view.shown_location = None;
        ctx.view.renaming = None;
        ctx.view.workbench = platen::Workbench::new();
        ctx.view.maximized_pane = None;
        ctx.view.active_content = super::ContentPane::Orrery;
        ctx.shared.session.session_dir = session_dir;
        ctx.shared.session.active_session_id = id;
        ctx.view.content_location =
            ctx.orrery.focused_url().unwrap_or("mere://welcome").to_string();
        ctx.refresh_session_thumbnails();
        ctx.view.request_redraw();
    }
}

fn short_member(member: GraphMemberId) -> String {
    member.to_string().chars().take(8).collect()
}

/// A short switcher label for a session with no user-set display name: the first
/// non-intro node's cached host (else its title), or "New" for an empty /
/// welcome-only graph. (Host text path.)
fn derive_session_label(graph: &Graph) -> String {
    graph
        .nodes()
        .map(|(_, node)| node)
        .find(|node| node.url() != "mere://welcome")
        .and_then(|node| {
            node.cached_host
                .clone()
                .filter(|h| !h.trim().is_empty())
                .or_else(|| Some(node.title.clone()))
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "New".to_string())
}

/// Map a fetched content type to its orrery [`NodeShape`] (a first-cut vocabulary;
/// a theme / lens concern eventually). Feeds read as a circle, small-web menus /
/// directories as a rounded square, and documents (HTML / markdown / gemtext /
/// plain text / …) plus anything unrecognized as the default square.
fn content_shape(content_type: Option<&str>) -> NodeShape {
    let Some(ct) = content_type else {
        return NodeShape::Square;
    };
    let base = ct
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "application/rss+xml" | "application/atom+xml" | "application/feed+json" => {
            NodeShape::Circle
        }
        "application/gopher-menu"
        | "application/x-nex"
        | "application/x-guppy"
        | "text/x-finger" => NodeShape::Rounded,
        _ => NodeShape::Square,
    }
}

fn content_bucket(content_type: Option<&str>) -> (u8, &'static str) {
    match content_type {
        None => (3, "Unknown"),
        Some(ct) => match content_shape(Some(ct)) {
            NodeShape::Circle => (1, "Feeds"),
            NodeShape::Rounded => (2, "Menus"),
            NodeShape::Square => (0, "Documents"),
        },
    }
}

fn relation_kind_label(kind: RelationKind) -> &'static str {
    use ContainmentSubKind::*;
    use ProvenanceSubKind::*;
    use SemanticSubKind::*;
    match kind {
        RelationKind::Traversal => "Traversal",
        RelationKind::Semantic(Hyperlink) => "Hyperlink",
        RelationKind::Semantic(UserGrouped) => "Grouped",
        RelationKind::Semantic(AgentDerived) => "Agent",
        RelationKind::Semantic(Cites) => "Cites",
        RelationKind::Semantic(Quotes) => "Quotes",
        RelationKind::Semantic(Summarizes) => "Summarizes",
        RelationKind::Semantic(Elaborates) => "Elaborates",
        RelationKind::Semantic(ExampleOf) => "Example",
        RelationKind::Semantic(Supports) => "Supports",
        RelationKind::Semantic(Contradicts) => "Contradicts",
        RelationKind::Semantic(Questions) => "Questions",
        RelationKind::Semantic(SameEntityAs) => "Same As",
        RelationKind::Semantic(DuplicateOf) => "Duplicate",
        RelationKind::Semantic(CanonicalMirrorOf) => "Mirror",
        RelationKind::Semantic(DependsOn) => "Depends",
        RelationKind::Semantic(Blocks) => "Blocks",
        RelationKind::Semantic(NextStep) => "Next",
        RelationKind::Containment(UrlPath) => "Path",
        RelationKind::Containment(Domain) => "Domain",
        RelationKind::Containment(FileSystem) => "Filesystem",
        RelationKind::Containment(UserFolder) => "Folder",
        RelationKind::Containment(ClipSource) => "Clip",
        RelationKind::Containment(NotebookSection) => "Section",
        RelationKind::Containment(CollectionMember) => "Collection",
        RelationKind::Arrangement(_) => "Arrangement",
        RelationKind::Imported(_) => "Imported",
        RelationKind::Provenance(ClippedFrom) => "Clipped",
        RelationKind::Provenance(ExcerptedFrom) => "Excerpt",
        RelationKind::Provenance(SummarizedFrom) => "Summary",
        RelationKind::Provenance(TranslatedFrom) => "Translation",
        RelationKind::Provenance(RewrittenFrom) => "Rewritten",
        RelationKind::Provenance(GeneratedFrom) => "Generated",
        RelationKind::Provenance(ExtractedFrom) => "Extracted",
        RelationKind::Provenance(ImportedFromSource) => "Imported",
    }
}
