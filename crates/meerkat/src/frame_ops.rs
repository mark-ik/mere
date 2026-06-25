/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Navigation, content, session, and chrome-drain operations for
//! [`Shell`](super::Shell). Factored from `main.rs` to keep files under the
//! workspace 600-LOC ceiling.

use forme::GraphMemberId;
use frame::{GraphId, InsertSide, PaneContent, PaneId, PaneNode};
use meerkat::Chrome;
use orrery::Orrery;
use session_runtime::{PersistedSettings, settings_store};

use super::observability::ObservabilitySnapshot;
use super::{GRAPH_PANE, WindowCtx, fetch, frame_view};

impl WindowCtx<'_> {
    /// Make the focused node's content available. A network address already in
    /// this session's content map is left as-is; otherwise a durable cache hit is
    /// shown without re-fetching (so a reload need not hit the network), and a
    /// miss marks it `Loading` and spawns a fetch.
    pub(super) fn ensure_content(&mut self, url: &str) {
        if !fetch::is_fetchable(url) || self.shared.content.pages.contains_key(url) {
            return;
        }
        if let Some(stored) = self.load_cached(url) {
            let fetched = super::fetched_from(stored);
            // A cached page skips the network `FetchUpdate::Page` favicon discovery,
            // so do it here when the node still has no favicon (a first-time cache
            // hit, or after a host change cleared the old one). (Favicon-on-tile.)
            let needs_favicon = self
                .orrery()
                .graph()
                .get_node_by_url(url)
                .is_none_or(|(_, node)| node.favicon_rgba.is_none());
            if needs_favicon {
                if let Some(icon_url) = crate::app_handler::favicon_url_for(url, &fetched.body) {
                    self.shared.content.fetch_handle.command(fetch::FetchCommand::Favicon {
                        owner_url: url.to_string(),
                        url: icon_url,
                    });
                }
            }
            self.shared.content.pages
                .insert(url.to_string(), fetch::ContentState::Ready(fetched));
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
        self.view.chrome_update(Chrome::close_suggestions);
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
                .orrery()
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
        let selected = self.orrery().selected_members();
        if selected.len() > 1 {
            return selected; // multi-select → the selection
        }
        match selected.first() {
            Some(&focus) => self
                .orrery()
                .connected_members(focus)
                .into_iter()
                .filter(|m| *m == focus || self.shared.content.constellation.is_active(*m))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Apply the chrome's current settings to the host: the active-tab cap to the
    /// actor pool. Called after a chrome interaction that could have changed them.
    /// Persists to the settings sidecar when the value actually changed (so an
    /// unrelated chrome click doesn't re-write the file).
    pub(super) fn sync_settings(&mut self) {
        let cap = self.view.chrome().settings.tab_cap;
        self.shared.content.constellation.set_cap(cap);
        if cap != self.shared.presentation.saved_tab_cap {
            self.shared.presentation.saved_tab_cap = cap;
            self.persist_settings();
        }
    }

    /// Write the current settings to the session's `settings.json` sidecar. A
    /// failure is logged, not fatal (the shell runs without persistence).
    pub(super) fn persist_settings(&self) {
        // Preserve the on-disk DocumentScript permission opinion (§11.4): the host
        // caches it nowhere — it is read on demand at attach and edited via the
        // settings lane, not reconstructed from runtime state here — so this save
        // path must not clobber it back to the default. (Follow-on #1.)
        let script_permissions = settings_store::load_settings(&self.shared.session.mere_root)
            .ok()
            .flatten()
            .unwrap_or_default()
            .script_permissions;
        let settings = PersistedSettings {
            tab_cap: self.shared.presentation.saved_tab_cap,
            theme_id: Some(self.shared.presentation.active_theme_id.clone()),
            shellbar_edge: self.shared.presentation.shellbar_edge,
            shellbar_hidden: self.shared.presentation.shellbar_hidden,
            physics_damping: self.shared.presentation.physics_damping,
            disabled_engines: self.shared.content.engine_activation.global_disabled_vec(),
            // The document typography as embedded JSON; `None` keeps the file
            // clean when it is still the built-in look. (Typography surface.)
            document_typography: (self.shared.presentation.document_sheet
                != document_canvas::DocumentStyleSheet::default())
            .then(|| serde_json::to_value(&self.shared.presentation.document_sheet).ok())
            .flatten(),
            script_permissions,
            crawl_scope: Some(self.shared.content.crawl.scope().as_key().to_string()),
            crawl_depth: Some(self.shared.content.crawl.max_depth()),
        };
        if let Err(err) = settings_store::save_settings(&self.shared.session.mere_root, &settings) {
            tracing::warn!(%err, "failed to persist settings");
        }
    }

    /// Toggle the shellbar's visibility on this window and persist the new state. The
    /// content band grows (hidden) or shrinks (shown), so the orrery recenters once,
    /// mirroring a shellbar move. When hidden, the strip can be right-clicked no more, so
    /// it is revealed again from the command palette / `>shellbar`. (Hide-shellbar.)
    pub(super) fn toggle_shellbar_visibility(&mut self) {
        self.shared.presentation.shellbar_hidden = !self.shared.presentation.shellbar_hidden;
        self.view.centered = false; // the content band changed size; recenter the orrery once
        self.persist_settings();
        self.view.request_redraw();
    }

    /// Persist the persona's curated context menu (command registry P4) to the persona settings
    /// store (`personas/<id>/settings/ui.json`). v0 uses the single default persona; v1 threads
    /// the active one. A failure is logged, not fatal.
    pub(super) fn persist_menu_actions(&self) {
        let settings = session_runtime::PersonaSettings {
            menu_actions: Some(self.shared.presentation.menu_actions.clone()),
            command_usage: self.shared.presentation.command_usage.clone(),
        };
        if let Err(err) = session_runtime::save_persona_settings(
            &self.shared.session.mere_root,
            self.shared.session.active_persona,
            &settings,
        ) {
            tracing::warn!(%err, "failed to persist persona menu settings");
        }
    }

    /// Record one invocation of registry command `id` — the frequency signal behind the context
    /// menu's auto-suggestions (command registry S3). Called at the command-invocation hook for
    /// both host commands and cataloged context actions; persists the updated counts.
    ///
    /// v1 persists on every invocation (the file is tiny); a debounce / write-on-idle is the
    /// refinement if the write rate ever matters.
    pub(super) fn record_command_usage(&mut self, id: &str) {
        *self.shared.presentation.command_usage.entry(id.to_string()).or_insert(0) += 1;
        self.persist_menu_actions();
    }

    /// Toggle a registry command's membership in the context menu (the `pelt/menu` page, command
    /// registry P4): present → removed, absent → appended. This is the "add any command / remove
    /// a gesture" edit; it persists the new list to the persona store.
    pub(super) fn toggle_menu_action(&mut self, id: &str) {
        let actions = &mut self.shared.presentation.menu_actions;
        if let Some(pos) = actions.iter().position(|a| a == id) {
            actions.remove(pos);
        } else {
            actions.push(id.to_string());
        }
        self.persist_menu_actions();
        self.view.request_redraw();
    }

    /// Move a context-menu command one place up or down in the order (the `pelt/menu` ▲ / ▼
    /// controls, command registry P4) by swapping it with its neighbor, then persist. A no-op at
    /// the ends or for an id not in the menu.
    pub(super) fn move_menu_action(&mut self, id: &str, up: bool) {
        let actions = &mut self.shared.presentation.menu_actions;
        let Some(pos) = actions.iter().position(|a| a == id) else {
            return;
        };
        let swap = if up {
            pos.checked_sub(1)
        } else if pos + 1 < actions.len() {
            Some(pos + 1)
        } else {
            None
        };
        if let Some(swap) = swap {
            actions.swap(pos, swap);
            self.persist_menu_actions();
            self.view.request_redraw();
        }
    }

    /// Move context-menu command `id` to where `target` sits in the order — the drag-reorder
    /// drop (command registry B2). Removes `id`, then inserts it at `target`'s slot ("drop before
    /// the target"), and persists. Where the ▲ / ▼ buttons swap neighbors one step at a time,
    /// this lands a command anywhere in a single drag. A no-op if either id isn't in the menu or
    /// they're already adjacent in place.
    pub(super) fn reorder_menu_action_to(&mut self, id: &str, target: &str) {
        let before = self.shared.presentation.menu_actions.clone();
        crate::list_pane::reorder_before(&mut self.shared.presentation.menu_actions, id, target);
        if self.shared.presentation.menu_actions != before {
            self.persist_menu_actions();
            self.view.request_redraw();
        }
    }

    /// Restore the context menu to the registry default order (command registry P4) and persist.
    pub(super) fn reset_menu_actions(&mut self) {
        self.shared.presentation.menu_actions =
            meerkat::command::DEFAULT_MENU_ACTIONS.iter().map(|s| s.to_string()).collect();
        self.persist_menu_actions();
        self.view.request_redraw();
    }

    /// The current "inertia" physics setting (linear damping), for the apparatus
    /// readout. (Physics settings.)
    pub(super) fn physics_damping(&self) -> f32 {
        self.shared.presentation.physics_damping
    }

    /// Adjust the "inertia" setting (linear damping) by `delta`, clamped to a sane
    /// range, apply it to **every** pooled orrery (the setting is global), persist
    /// it, and redraw so the apparatus readout updates. The apparatus −/+ buttons and
    /// the omnibar drive this. Lower damping keeps more drift after a settle; higher
    /// brings nodes to rest sooner. (Physics settings.)
    pub(super) fn adjust_physics_damping(&mut self, delta: f32) {
        let current = self.shared.presentation.physics_damping;
        let next = (current + delta).clamp(0.5, 8.0);
        if (next - current).abs() < f32::EPSILON {
            return;
        }
        self.shared.presentation.physics_damping = next;
        for orrery in self.orreries.values_mut() {
            orrery.set_physics_damping(next);
        }
        self.persist_settings();
        self.view.request_redraw();
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

    /// The `graph_id` a freshly-summoned leaf of `content` should carry: the
    /// **focused** pane's graph for graph-bound panes (so a pane summoned beside the
    /// one you're in shows your graph), a nil (unbound) id for window-chrome. Was the
    /// active session's graph; now keyed off `focused_graph` directly — equal today,
    /// but the pane-as-unit choice once focus can leave the active session. (MG5;
    /// Window composition — pane-as-unit.)
    fn leaf_graph_id(&self, content: &PaneContent) -> GraphId {
        if content.follows_active_graph() {
            self.view.focused_graph
        } else {
            GraphId::nil()
        }
    }

    /// The focused window's orrery, resolved from the pool by the window's tracked
    /// `focused_graph`. The read half of the bundled `self.orrery` field P1
    /// removed: P2 hands the ctx the whole pool so render / input can resolve any
    /// pane's orrery, and the window-focused-graph sites reach the focused one
    /// here. Resolution-identical to P1's bundling key. (Window composition P2.)
    pub(super) fn orrery(&self) -> &Orrery {
        self.orreries
            .get(&self.view.focused_graph)
            .expect("focused orrery is pooled")
    }

    /// The focused window's orrery, mutable — the write half of the removed
    /// `self.orrery` field. (Window composition P2.)
    pub(super) fn orrery_mut(&mut self) -> &mut Orrery {
        let gid = self.view.focused_graph;
        self.orreries
            .get_mut(&gid)
            .expect("focused orrery is pooled")
    }

    /// A specific pooled orrery by `graph_id`, mutable — the per-pane resolution a
    /// render / hit-test drives the orrery a pane resolves to with. Panics if the
    /// graph isn't pooled, which a laid-out leaf's graph always is. (Window
    /// composition P2.)
    pub(super) fn pane_orrery_mut(&mut self, graph_id: GraphId) -> &mut Orrery {
        self.orreries
            .get_mut(&graph_id)
            .expect("a laid-out pane's graph is pooled")
    }

    /// Read twin of [`pane_orrery_mut`](Self::pane_orrery_mut). (Window
    /// composition P2.)
    pub(super) fn pane_orrery(&self, graph_id: GraphId) -> &Orrery {
        self.orreries
            .get(&graph_id)
            .expect("a laid-out pane's graph is pooled")
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

    /// Keep a Comms frame leaf in sync with the chrome's comms-open state. The comms
    /// pane is chrome-rendered (its compose field + click handlers live in the
    /// chrome), so it stays there; this only reserves / drops a frame leaf so the
    /// other panes make room, and the render positions the chrome overlay into it.
    /// (Comms pane.)
    pub(super) fn sync_comms_pane(&mut self) {
        let open = self.view.chrome().comms.is_open();
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

    /// Read-only system rows for the apparatus Overview section.
    pub(super) fn apparatus_system_rows(&self) -> Vec<(String, String)> {
        vec![
            (
                "Nodes".to_string(),
                self.orrery().graph().nodes().count().to_string(),
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

    /// Whether more than one graph (Orrery) pane is open — gates the close-pane
    /// affordances (you can't close the last graph view). (Window composition —
    /// pane-as-unit.)
    pub(super) fn has_multiple_graph_panes(&self) -> bool {
        self.view
            .frame_layout
            .iter_leaves()
            .filter(|(_, c, _)| matches!(c, PaneContent::Orrery))
            .count()
            > 1
    }

    /// Close the focused graph (Orrery) pane when more than one is open — the
    /// dismiss for a second graph-pane (Ctrl+W, the `close_pane` command, the orrery
    /// context menu). A no-op with a single graph pane (never close the last graph
    /// view). Focus hands off to a surviving graph pane; the closed graph's orrery
    /// stays pooled (the LRU evicts it later). (Window composition — pane-as-unit.)
    pub(super) fn close_focused_graph_pane(&mut self) {
        let orrery_panes: Vec<(PaneId, GraphId)> = self
            .view
            .frame_layout
            .iter_leaves()
            .filter(|(_, c, _)| matches!(c, PaneContent::Orrery))
            .map(|(id, _, gid)| (id, gid))
            .collect();
        if orrery_panes.len() <= 1 {
            return;
        }
        // The focused graph pane (else the first one), and a survivor to focus next.
        let focused = self.view.focused_graph;
        let (target_pane, _) = orrery_panes
            .iter()
            .find(|(_, g)| *g == focused)
            .copied()
            .unwrap_or(orrery_panes[0]);
        if let Some(path) = frame_view::pane_path(&self.view.frame_layout, target_pane) {
            self.view.frame_layout.close_leaf(&path);
        }
        self.view.maximized_pane = None;
        if let Some((_, surviving)) = orrery_panes.iter().find(|(p, _)| *p != target_pane) {
            self.focus_pane_graph(*surviving);
        }
        self.shared
            .observability
            .record_frame_layout_changed("graph pane closed");
        self.view.request_redraw();
    }
}
