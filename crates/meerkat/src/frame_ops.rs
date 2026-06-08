/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Navigation, content, session, and chrome-drain operations for
//! [`App`](super::App). Factored from `main.rs` to keep files under the
//! workspace 600-LOC ceiling.

use std::collections::HashMap;

use forme::GraphMemberId;
use frame::{GraphId, InsertSide, PaneContent, PaneId, PaneNode, SplitAxis, SplitChoice};
use meerkat::command::Command;
use meerkat::{Chrome, CommsIntent, ContextAction, ContextItem};
use orrery::{NodeShape, NodeState};
use platen_view::WorkbenchAction;
use session_runtime::{
    content_store, session_graph_store, settings_store, view_intent_store, PersistedSettings,
    ViewIntent,
};

use super::{
    apparatus, comms_host, fetch, frame_view, roster, sync, App, DEFAULT_FRAME, DEFAULT_PANE,
    FALLBACK_TOOLBAR_H, GRAPH_PANE,
};

impl App {
    /// The URL of whatever is in focus: in the tiled view the focused tile's node,
    /// in the orrery the focused node. `None` when nothing is focused.
    pub(super) fn current_focus_url(&self) -> Option<String> {
        if self.workbench.is_tiled() {
            let member = self.focused_tile?;
            self.orrery.graph().get_node_by_id(member).map(|(_, node)| node.url().to_string())
        } else {
            self.orrery.focused_url().map(str::to_string)
        }
    }

    /// Point the omnibar at the focused tile / node (the address bar follows focus),
    /// but only when that focus actually changed and the user isn't editing the
    /// omnibar (no chrome field holds the caret) — so it never clobbers typing.
    pub(super) fn sync_location(&mut self) {
        let url = self.current_focus_url();
        if url == self.shown_location {
            return;
        }
        self.shown_location = url.clone();
        if let (Some(url), None) = (url, self.runner.focus()) {
            self.runner.update(move |c| c.show_location(&url));
            self.request_redraw();
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
        let loc = self.runner.state().content_location().to_string();
        // Ctrl/Cmd-Enter: open the address as a *new node* linked from the focused
        // one. Handled before the change guard below, since duplicates are welcome
        // (the node-identity model) — opening the current page as a new node is a
        // valid branch, not a no-op.
        if self.runner.state().open_as_new_node {
            self.runner.update(|c| c.open_as_new_node = false);
            let origin = self.nav_target_member();
            let new_member = self.orrery.open_member_as_new_node(origin, &loc);
            // In Tree, tile the new node: stack it into the focused tile's slot as
            // the active tab (or a fresh slot when nothing is focused), and focus
            // it so the next navigation targets it. In Cartography the orrery has
            // already selected it, so it shows as the focused-node card.
            if self.workbench.is_tiled() {
                let stacked = origin.is_some_and(|o| self.workbench.open_in_slot_of(new_member, o));
                if !stacked {
                    self.workbench.open_tile(new_member);
                }
                self.focused_tile = Some(new_member);
            } else {
                // Cartography: opening a node is deliberate — show it as a live
                // card straight away (passive focus shows only the snapshot).
                self.live_previews.insert(new_member);
            }
            self.ensure_content(&loc);
            self.content_location = loc;
            self.save_session();
            self.request_redraw();
            return;
        }
        if loc == self.content_location {
            return;
        }
        match self.nav_target_member() {
            Some(member) => {
                self.orrery.navigate_member(member, &loc);
                self.scroll.remove(&member); // a new page starts at the top
            },
            None => {
                self.orrery.visit(&loc);
            },
        }
        // Navigating is deliberate: show the target as a live card in Cartography
        // (passive focus shows only the snapshot).
        if !self.workbench.is_tiled() {
            if let Some(member) = self.focused_member() {
                self.live_previews.insert(member);
            }
        }
        self.ensure_content(&loc);
        self.content_location = loc;
        self.save_session();
        self.request_redraw();
    }

    /// The node per-node navigation acts on: the focused tile in Tree, the single
    /// selected node in Cartography. `None` when nothing is focused.
    fn nav_target_member(&self) -> Option<GraphMemberId> {
        if self.workbench.is_tiled() {
            self.focused_tile
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
        let Some(step) = self.runner.state().history_step else {
            return;
        };
        self.runner.update(|c| c.history_step = None);
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
        self.scroll.remove(&member); // the revealed page starts at the top
        self.content_location = url.clone();
        self.ensure_content(&url);
        self.runner.update(|c| {
            c.content_location = url.clone();
            c.show_location(&url);
        });
        self.save_session();
        self.request_redraw();
    }

    /// Reflect the focused node's history onto the toolbar's back/forward
    /// enabled-state, so the buttons track whichever node is focused. Cheap; a
    /// no-op when unchanged. Called each render.
    pub(super) fn sync_nav_buttons(&mut self) {
        let (can_back, can_forward) = match self.nav_target_member() {
            Some(m) => (self.orrery.member_can_back(m), self.orrery.member_can_forward(m)),
            None => (false, false),
        };
        let (cur_back, cur_forward) = {
            let c = self.runner.state();
            (c.toolbar.can_go_back, c.toolbar.can_go_forward)
        };
        if cur_back == can_back && cur_forward == can_forward {
            return;
        }
        self.runner.update(move |c| {
            c.toolbar.can_go_back = can_back;
            c.toolbar.can_go_forward = can_forward;
        });
        self.request_redraw();
    }

    /// Persist the session (graph + camera view-intent) under the session dir.
    /// Best-effort: a write failure is logged, not fatal. Called after each
    /// navigation and on window close.
    pub(super) fn save_session(&self) {
        let graph_file = self.session_dir.join(session_graph_store::GRAPH_FILE);
        if let Err(err) = session_graph_store::save(&graph_file, self.orrery.graph()) {
            tracing::warn!(%err, path = ?graph_file, "failed to persist the session graph");
        }
        let intent = ViewIntent {
            camera: Some(super::camera_to_snapshot(self.orrery.camera())),
            focus: self.orrery.focused_url().map(str::to_string),
            ..Default::default()
        };
        if let Err(err) = view_intent_store::save_view_intent(
            &self.session_dir,
            DEFAULT_FRAME,
            DEFAULT_PANE,
            &intent,
        ) {
            tracing::warn!(%err, dir = ?self.session_dir, "failed to persist the view intent");
        }
        // The content frame's pane layout (which panes are open + split ratios).
        if let Err(err) =
            session_runtime::frame_layout_store::save_frame_layout(&self.session_dir, &self.frame_layout)
        {
            tracing::warn!(%err, dir = ?self.session_dir, "failed to persist the frame layout");
        }
    }

    /// Make the focused node's content available. A network address already in
    /// this session's content map is left as-is; otherwise a durable cache hit is
    /// shown without re-fetching (so a reload need not hit the network), and a
    /// miss marks it `Loading` and spawns a fetch.
    pub(super) fn ensure_content(&mut self, url: &str) {
        if !fetch::is_fetchable(url) || self.content.contains_key(url) {
            return;
        }
        if let Some(stored) = self.load_cached(url) {
            self.content
                .insert(url.to_string(), fetch::ContentState::Ready(super::fetched_from(stored)));
            return;
        }
        self.content.insert(url.to_string(), fetch::ContentState::Loading);
        self.fetch_handle.command(fetch::FetchCommand::Page(url.to_string()));
    }

    /// Toggle between the orrery (Cartography) and the tiled workbench (Tree).
    /// Entering Tree seeds the open set from the focused node and its graph
    /// neighbors, so the tiled view reflects the node you toggled on; exiting
    /// clears it. The constellation reconciles its actors to the resulting needed
    /// set on the next frame — spawning the tiles, reaping what's no longer shown
    /// (background-flagged nodes excepted).
    pub(super) fn toggle_workbench(&mut self) {
        // Clear the omnibar suggestions dropdown so it doesn't hang over the tiles.
        self.runner.update(Chrome::close_suggestions);
        self.workbench.toggle_mode();
        self.workbench.clear_tiles();
        if self.workbench.is_tiled() {
            for member in self.selection_working_set() {
                self.workbench.open_tile(member);
            }
            // Focus the node the open was seeded from (the primary selection), so the
            // omnibar shows its URL; fall back to the first opened tile.
            self.focused_tile = self
                .orrery
                .selected_members()
                .first()
                .copied()
                .or_else(|| self.workbench.open_members().first().copied());
        }
        self.request_redraw();
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
                .filter(|m| *m == focus || self.constellation.is_active(*m))
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
            return;
        }
        let items = if set.len() == 1 {
            vec![ContextItem::new("Open tile", ContextAction::OpenSplits)]
        } else {
            vec![
                ContextItem::new("Open in splits", ContextAction::OpenSplits),
                ContextItem::new("Open in a stack", ContextAction::Stack),
            ]
        };
        self.context_set = set;
        self.runner.update(move |c| c.open_context_menu(x, y, items));
        self.request_redraw();
    }

    /// Dismiss the context menu (an outside click / Escape), dropping its set.
    pub(super) fn close_context_menu(&mut self) {
        self.context_set.clear();
        self.runner.update(Chrome::close_context_menu);
        self.request_redraw();
    }

    /// Run a pending context-menu action the chrome captured: open the menu's
    /// member set as splits or as one stack, switching into the tiled (Tree)
    /// projection first if needed.
    pub(super) fn drain_pending_context(&mut self) {
        let Some(action) = self.runner.state().pending_context else {
            return;
        };
        self.runner.update(|c| c.pending_context = None);
        let set = std::mem::take(&mut self.context_set);
        if set.is_empty() {
            return;
        }
        // These open tiles, so surface the tiled view (closing the suggestions
        // dropdown on the way in, like Ctrl+T does).
        if self.workbench.ensure_tiled() {
            self.runner.update(Chrome::close_suggestions);
        }
        match action {
            ContextAction::OpenSplits => {
                self.workbench.open_split(&set);
            },
            ContextAction::Stack => {
                self.workbench.open_stack(&set);
            },
        }
        self.request_redraw();
    }

    /// Delete the focused node from the graph and reap its activation (the actor
    /// winds down on drop). A no-op when zero or many nodes are focused. Deletion
    /// removes the node's data; deactivation just stops its actor — this does
    /// both, because the node itself is gone.
    pub(super) fn delete_focused_node(&mut self) {
        if let Some(member) = self.orrery.remove_focused() {
            self.live_previews.remove(&member);
            self.constellation.reap(member);
            self.save_session();
            self.request_redraw();
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
        let next = !self.constellation.is_background(member);
        if self.constellation.set_background(member, next) {
            tracing::info!(%member, background = next, "toggled node background");
            self.request_redraw();
        }
    }

    /// The set of graph members that should be active this frame: in Tree the open
    /// tiles, in Cartography just the focused node (if any). The constellation
    /// reconciles its actor pool to this.
    pub(super) fn needed_members(&self) -> Vec<GraphMemberId> {
        if self.workbench.is_tiled() {
            // Every tab across every slot stays warm, not just the visible ones, so
            // switching a stack's tab is instant (the actor already has its scene).
            self.workbench.open_members()
        } else {
            // Cartography: a node is active only when it has a live preview card.
            // Focusing alone shows the static "last visit" snapshot (no actor), so
            // the preview no longer activates the node. (Card system P2/P3.)
            self.live_previews.iter().copied().collect()
        }
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
        if self.live_previews.remove(&member) {
            self.constellation.reap(member); // demote: actor down, scene -> snapshot
        } else {
            self.live_previews.insert(member);
        }
        self.request_redraw();
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
                let state = match self.content.get(node.url()) {
                    Some(fetch::ContentState::Ready(_)) => {
                        if self.constellation.is_active(node.id) {
                            NodeState::Open
                        } else {
                            NodeState::Closed
                        }
                    },
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
            .filter_map(|(_key, node)| match self.content.get(node.url()) {
                Some(fetch::ContentState::Ready(fetched)) => {
                    Some((node.id, content_shape(fetched.content_type.as_deref())))
                },
                _ => None,
            })
            .collect()
    }

    /// The focused node's graph member, if a node is focused (resolved URL → node
    /// UUID via the kernel node id).
    pub(super) fn focused_member(&self) -> Option<GraphMemberId> {
        let url = self.orrery.focused_url()?;
        self.orrery.graph().get_node_by_url(url).map(|(_, node)| node.id)
    }

    /// Load durably-cached content for `url` (page or subresource), or `None`.
    /// The fjall store's futures are ready, so `block_on` does not stall the UI.
    pub(super) fn load_cached(&mut self, url: &str) -> Option<content_store::StoredContent> {
        let store = self.store.as_mut()?;
        pollster::block_on(content_store::load_content(store, url)).ok().flatten()
    }

    /// Persist `body` (+ its content-type) for `url` to the durable content cache,
    /// so a reload need not re-fetch it. Best-effort; a write failure is logged.
    pub(super) fn save_cached(&mut self, url: &str, content_type: Option<String>, body: &[u8]) {
        let Some(store) = self.store.as_mut() else {
            return;
        };
        let stored = content_store::StoredContent { content_type, body: body.to_vec() };
        if let Err(err) = pollster::block_on(content_store::save_content(store, url, &stored)) {
            tracing::warn!(%err, url, "failed to cache content");
        }
    }

    /// Apply a pending workbench action the workbench root captured: switch the
    /// visible tab, close a tab (reaping its actor), or toggle its pin (the
    /// background-keep flag, which also exempts it from cap eviction).
    pub(super) fn drain_workbench_action(&mut self) {
        let Some(action) = self.workbench_runner.state().pending else {
            return;
        };
        self.workbench_runner.update(|s| s.pending = None);
        match action {
            WorkbenchAction::Activate(member) => {
                self.workbench.activate(member);
                self.focused_tile = Some(member);
            },
            WorkbenchAction::Close(member) => {
                self.workbench.close_tile(member);
                self.constellation.reap(member);
                if self.workbench.open_members().is_empty() {
                    // Closing the last tile returns to the graph with nothing
                    // focused, so the node deactivates rather than stranding an
                    // empty workbench (where the Cartography preview would just
                    // re-activate it). (Card-system plan, Phase 1.)
                    if self.workbench.is_tiled() {
                        self.workbench.toggle_mode();
                    }
                    self.workbench.clear_tiles();
                    self.focused_tile = None;
                    self.orrery.clear_selection();
                } else if self.focused_tile == Some(member) {
                    self.focused_tile = self.workbench.open_members().first().copied();
                }
            },
            WorkbenchAction::TogglePin(member) => {
                let pinned = self.constellation.is_background(member);
                self.constellation.set_background(member, !pinned);
            },
        }
        self.request_redraw();
    }

    /// Execute a pending "connect to peer" request the chrome queued (S5.1): take
    /// the ticket the verb captured from the address bar and drive the sync actor.
    /// The chrome records the intent; this is the host executing it.
    pub(super) fn drain_pending_connect(&mut self) {
        let Some(ticket) = self.runner.state().pending_connect.clone() else {
            return;
        };
        self.runner.update(|c| {
            c.pending_connect = None;
        });
        if ticket.is_empty() {
            tracing::warn!("connect to peer: paste the peer's ticket in the address bar first");
            return;
        }
        // Route the verb to the sync actor; it runs the dial on its runtime and logs
        // the outcome (the actor boundary, so no synchronous result here).
        self.sync_handle.command(sync::SyncCommand::Connect(ticket));
        self.request_redraw();
    }

    /// Execute a pending host action the palette queued (toggle workbench / delete
    /// node / background a node): take it from the chrome and dispatch to the
    /// matching shell method.
    pub(super) fn drain_pending_command(&mut self) {
        let Some(cmd) = self.runner.state().pending_command else {
            return;
        };
        self.runner.update(|c| c.pending_command = None);
        match cmd {
            Command::ToggleWorkbench => self.toggle_workbench(),
            Command::DeleteNode => self.delete_focused_node(),
            Command::BackgroundNode => self.toggle_focus_background(),
            Command::HideSelectedEdge => {
                if self.orrery.hide_selected_edges() > 0 {
                    self.request_redraw();
                }
            },
            Command::ShowAllEdges => {
                if self.orrery.show_all_edges() > 0 {
                    self.request_redraw();
                }
            },
            // History / connect / settings / comms verbs run in the chrome; never
            // queued here as host intents.
            Command::Back
            | Command::Forward
            | Command::Home
            | Command::ConnectPeer
            | Command::OpenSettings
            | Command::ToggleComms => {},
        }
    }

    /// Run the chrome's pending comms request (P6c): take the recorded
    /// [`CommsIntent`] and route it to the comms actor as a `CommsCommand`. The
    /// chrome can't reach the actor, so it records the intent and the host drains
    /// it here (mirrors [`drain_pending_command`](Self::drain_pending_command)).
    pub(super) fn drain_comms_intent(&mut self) {
        let Some(intent) = self.runner.state().comms_intent.clone() else {
            return;
        };
        self.runner.update(|c| c.comms_intent = None);
        match intent {
            CommsIntent::Refresh => {
                self.comms_handle.command(comms_host::CommsCommand::Refresh);
            },
            CommsIntent::Open(id) => {
                self.comms_handle.command(comms_host::CommsCommand::Open(id));
            },
            CommsIntent::Send(draft) => {
                self.comms_handle.command(comms_host::CommsCommand::Send(draft));
            },
        }
    }

    /// Apply the chrome's current settings to the host: the active-tab cap to the
    /// actor pool. Called after a chrome interaction that could have changed them.
    /// Persists to the settings sidecar when the value actually changed (so an
    /// unrelated chrome click doesn't re-write the file).
    pub(super) fn sync_settings(&mut self) {
        let cap = self.runner.state().settings.tab_cap;
        self.constellation.set_cap(cap);
        if cap != self.saved_tab_cap {
            self.saved_tab_cap = cap;
            self.persist_settings();
        }
    }

    /// Write the current settings to the session's `settings.json` sidecar. A
    /// failure is logged, not fatal (the shell runs without persistence).
    pub(super) fn persist_settings(&self) {
        let settings = PersistedSettings {
            tab_cap: self.saved_tab_cap,
            theme_id: Some(self.active_theme_id.clone()),
        };
        if let Err(err) = settings_store::save_settings(&self.session_dir, &settings) {
            tracing::warn!(%err, "failed to persist settings");
        }
    }

    // ── Frame tree (F1) ──────────────────────────────────────────────────────

    /// The content band (below the toolbar) in window coords.
    pub(super) fn content_band(&self) -> [f32; 4] {
        let th = self.toolbar_h.max(FALLBACK_TOOLBAR_H) as f32;
        [0.0, th, self.width as f32, self.height as f32]
    }

    /// The laid-out content panes (leaf rects) for the current frame layout.
    pub(super) fn laid_leaves(&self) -> Vec<frame_view::LaidLeaf> {
        frame_view::leaf_rects(&self.frame_layout, self.content_band(), self.maximized_pane)
    }

    /// The graph pane's screen rect (orrery / tiled workbench); the whole band when
    /// no graph leaf is laid out (e.g. another pane is maximized).
    pub(super) fn graph_leaf_rect(&self) -> [f32; 4] {
        let band = self.content_band();
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Workbench))
            .map(|l| l.rect)
            .unwrap_or(band)
    }

    /// The roster pane's screen rect, if the roster is open.
    pub(super) fn roster_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Roster))
            .map(|l| l.rect)
    }

    /// The pane (leaf) under window point `(x, y)`, if any.
    pub(super) fn pane_at(&self, x: f32, y: f32) -> Option<PaneId> {
        self.laid_leaves()
            .into_iter()
            .find(|l| x >= l.rect[0] && x < l.rect[2] && y >= l.rect[1] && y < l.rect[3])
            .map(|l| l.pane_id)
    }

    /// The id of the open leaf whose content equals `content`, if any.
    fn pane_of_content(&self, content: &PaneContent) -> Option<PaneId> {
        self.frame_layout
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
        if let Some(id) = self.pane_of_content(&content) {
            if let Some(path) = frame_view::pane_path(&self.frame_layout, id) {
                self.frame_layout.close_leaf(&path);
            }
        } else {
            let id = PaneId(self.next_pane_id);
            self.next_pane_id += 1;
            let leaf = PaneNode::Leaf { pane_id: id, content, graph_id: GraphId::default() };
            let anchor =
                frame_view::pane_path(&self.frame_layout, GRAPH_PANE).unwrap_or_default();
            if self.frame_layout.summon_leaf(&anchor, InsertSide::Right, leaf) {
                self.frame_layout.set_split_ratio(&anchor, 0.7);
            }
        }
        self.maximized_pane = None;
        self.request_redraw();
    }

    /// Switch the active theme: re-resolve from the registry, rebuild the chrome
    /// CSS + tokens, drop the host-drawn caches so they re-rasterize with the new
    /// palette, persist the choice, and redraw. (Theme switcher; the orrery's own
    /// palette is themed in A2.)
    pub(super) fn set_theme(&mut self, theme_id: &str) {
        let resolution = self.theme.set_active_theme(theme_id);
        self.active_theme_id = resolution.resolved_id;
        self.chrome_theme = resolution.tokens.chrome;
        self.chrome_sheet = crate::chrome_sheet(&self.chrome_theme);
        // Re-theme the orrery's backdrop + edges to match. (A2.)
        let (backdrop, edge) = crate::orrery_palette(&resolution.tokens);
        self.orrery.set_palette(backdrop, edge);
        self.window_controls_tex = None;
        self.divider_tex = None;
        self.persist_settings();
        self.request_redraw();
    }

    /// The apparatus pane's screen rect, if open.
    pub(super) fn apparatus_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Apparatus))
            .map(|l| l.rect)
    }

    /// The theme id whose apparatus button contains window point `(x, y)`, if any.
    pub(super) fn apparatus_button_at(&self, x: f32, y: f32) -> Option<String> {
        self.apparatus_button_rects
            .iter()
            .find(|(_, r)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
            .map(|(id, _)| id.clone())
    }

    /// The registered themes as apparatus options (id + display name + active),
    /// from the known theme ids. (The registry doesn't list themes yet.)
    pub(super) fn theme_options(&self) -> Vec<apparatus::ThemeOption> {
        use register_theme::theme::{
            THEME_ID_DARK, THEME_ID_DEFAULT, THEME_ID_HIGH_CONTRAST, THEME_ID_LIGHT,
        };
        [THEME_ID_DEFAULT, THEME_ID_LIGHT, THEME_ID_DARK, THEME_ID_HIGH_CONTRAST]
            .iter()
            .map(|id| {
                let res = self.theme.resolve_theme(Some(id));
                apparatus::ThemeOption {
                    active: res.resolved_id == self.active_theme_id,
                    id: res.resolved_id,
                    name: res.tokens.display_name,
                }
            })
            .collect()
    }

    /// Read-only diagnostics for the apparatus System section.
    pub(super) fn apparatus_diagnostics(&self) -> Vec<(String, String)> {
        vec![
            ("Nodes".to_string(), self.orrery.graph().nodes().count().to_string()),
            ("Active actors".to_string(), self.constellation.active_count().to_string()),
            ("Tab cap".to_string(), self.saved_tab_cap.to_string()),
            ("Theme".to_string(), self.active_theme_id.clone()),
        ]
    }

    /// Toggle maximize of the pane under the cursor (full-screen and back). With a
    /// single pane this is a no-op visually. (Frame tree, F1.)
    pub(super) fn toggle_maximize(&mut self) {
        if self.maximized_pane.is_some() {
            self.maximized_pane = None;
        } else if let Some(pane) = self.pane_at(self.cursor.0, self.cursor.1) {
            self.maximized_pane = Some(pane);
        }
        self.request_redraw();
    }

    /// The frame divider gutter under window point `(x, y)`, as its split path +
    /// the split's (parent) rect + axis — for starting a divider drag.
    pub(super) fn frame_divider_at(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(Vec<SplitChoice>, [f32; 4], SplitAxis)> {
        let band = self.content_band();
        frame_view::divider_rects(&self.frame_layout, band, self.maximized_pane)
            .into_iter()
            .find(|d| x >= d.rect[0] && x < d.rect[2] && y >= d.rect[1] && y < d.rect[3])
            .map(|d| (d.path, d.parent, d.axis))
    }

    /// Drive an in-progress frame-divider drag from the current cursor: map the
    /// pointer's position within the split's parent rect to a new ratio.
    pub(super) fn drag_frame_divider(&mut self) {
        let Some((path, parent, axis)) = self.frame_divider_drag.clone() else {
            return;
        };
        let (cx, cy) = self.cursor;
        let ratio = match axis {
            SplitAxis::Horizontal => {
                (cx - parent[0]) / (parent[2] - parent[0] - frame_view::DIVIDER).max(1.0)
            },
            SplitAxis::Vertical => {
                (cy - parent[1]) / (parent[3] - parent[1] - frame_view::DIVIDER).max(1.0)
            },
        };
        // `set_split_ratio` clamps to a sane minimum so a pane can't collapse.
        self.frame_layout.set_split_ratio(&path, ratio);
        self.request_redraw();
    }

    /// The node whose roster row contains window point `(x, y)`, if any.
    pub(super) fn roster_row_at(&self, x: f32, y: f32) -> Option<GraphMemberId> {
        self.roster_row_rects
            .iter()
            .find(|(_, r)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
            .map(|(member, _)| *member)
    }

    /// The roster rows: every graph node as a row (url as title, content type as
    /// subtitle, focused node marked selected). (Frame tree, F1 roster.)
    pub(super) fn roster_rows(&self) -> Vec<roster::RosterRow> {
        let focused = self.orrery.focused_url();
        self.orrery
            .graph()
            .nodes()
            .map(|(_key, node)| {
                let url = node.url();
                let content_type = match self.content.get(url) {
                    Some(fetch::ContentState::Ready(fetched)) => fetched.content_type.clone(),
                    _ => None,
                };
                roster::RosterRow {
                    member: node.id,
                    title: url.to_string(),
                    subtitle: content_type.unwrap_or_else(|| "—".to_string()),
                    selected: focused == Some(url),
                }
            })
            .collect()
    }
}

/// Map a fetched content type to its orrery [`NodeShape`] (a first-cut vocabulary;
/// a theme / lens concern eventually). Feeds read as a circle, small-web menus /
/// directories as a rounded square, and documents (HTML / markdown / gemtext /
/// plain text / …) plus anything unrecognized as the default square.
fn content_shape(content_type: Option<&str>) -> NodeShape {
    let Some(ct) = content_type else { return NodeShape::Square };
    let base = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    match base.as_str() {
        "application/rss+xml" | "application/atom+xml" | "application/feed+json" => NodeShape::Circle,
        "application/gopher-menu" | "application/x-nex" | "application/x-guppy" | "text/x-finger" => {
            NodeShape::Rounded
        },
        _ => NodeShape::Square,
    }
}
