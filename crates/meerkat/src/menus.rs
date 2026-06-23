/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Context and shellbar menus for a window: opening the right-click context menu
//! over the selection working set (or Add node / Add field on empty canvas),
//! opening the shellbar move menu, dismissing the menu, and draining the captured
//! menu action (open as splits / stack, relate, add node / tile / field / session,
//! move the shellbar). The chrome renders the rows; the host owns the set and runs
//! the action. Factored out of `frame_ops.rs` to keep files under the 600-LOC
//! ceiling.

use forme::GraphMemberId;
use kernel::graph::SemanticSubKind;
use meerkat::{Chrome, ContextAction, ContextItem};
use orrery::Representation;
use session_runtime::ShellbarEdge;

use super::WindowCtx;
use super::observability::Severity;

impl WindowCtx<'_> {
    /// Open the right-click context menu over the current selection's working set,
    /// at window `(x, y)`. A no-op when nothing is selected (no set to act on). A
    /// single-member set offers one "open tile"; a larger set offers splits vs a
    /// stack. The host remembers the set; the chrome renders the rows.
    pub(super) fn open_context_menu_at(&mut self, x: f32, y: f32) {
        let set = self.selection_working_set();
        // Empty-canvas menus mint at the cursor: record the anchor (so "Add node" lands under
        // it) and any field box under the point (so "Delete field" can target it). A selection
        // menu never mints at the cursor, so it just clears the stale anchor.
        if set.is_empty() {
            let (ofx, ofy) = self.orrery_point(x, y);
            self.view.context_origin = Some((ofx, ofy));
            self.view.context_field = self.orrery().field_at_screen(ofx, ofy);
        } else {
            self.view.context_origin = None;
        }
        // The working member set drives applicability; set it before building so the curated rows
        // resolve against it. The menu opens with an empty query (curated rows; typing searches).
        self.view.context_set = set;
        let items = self.build_curated_menu_items();
        self.view.chrome_update(move |c| c.open_context_menu(x, y, items));
        self.view.request_redraw();
    }

    /// The curated context-menu rows (the cursor palette's zero-query view): the persona's pinned
    /// commands resolved against the working set, plus the dynamic rows (delete field + layout
    /// submenu on the empty canvas, the single-focus radial toggle, the close-pane foot). Reads the
    /// working set from `context_set`, so callers set it first. (Command registry P4 / S1.)
    fn build_curated_menu_items(&self) -> Vec<ContextItem> {
        let len = self.view.context_set.len();
        // 1. The persona-curated command list: each configured registry id, resolved + applicability-
        //    filtered for this selection. One ordered list drives the canvas / single / multi menus.
        let mut items = Vec::new();
        for id in self.menu_actions() {
            if let Some(item) = self.resolve_menu_item(&id, len) {
                items.push(item);
            }
        }
        // 2. Dynamic / parameterized rows that aren't flat catalog gestures.
        if len == 0 {
            if self.view.context_field.is_some() {
                items.push(ContextItem::new("Delete field", ContextAction::DeleteField));
            }
            items.extend(self.layout_picker_items());
        } else if self.orrery().focused_key().is_some() {
            // Radial centers the orrery on the focused node (BFS rings) and re-centers live; a
            // focus-driven toggle (✓ when active), so it rides the selection menu. (Layout — radial.)
            let radial_on = self.orrery().layout_strategy() == Some("radial.default");
            items.push(ContextItem::new(
                if radial_on { "Radial layout  \u{2713}" } else { "Radial layout" },
                if radial_on {
                    ContextAction::SetLayoutStrategy("")
                } else {
                    ContextAction::SetLayoutStrategy("radial.default")
                },
            ));
        }
        if self.has_multiple_graph_panes() {
            items.push(ContextItem::new("Close graph view", ContextAction::CloseGraphPane));
        }
        items
    }

    /// The search results for a non-empty menu query (the cursor palette): every registry command +
    /// context action matching `query`, mapped to runnable rows. Searching is intentful, so results
    /// are not applicability-filtered; a searched command runs in the current context (a graceful
    /// no-op if it does not apply). (Searchable context menu S1.)
    fn search_menu_items(&self, query: &str) -> Vec<ContextItem> {
        use meerkat::command::{PaletteItem, context_action_id, context_action_palette_label};
        let pinned = |id: &str| self.shared.presentation.menu_actions.iter().any(|a| a == id);
        meerkat::command::palette_items(query)
            .into_iter()
            .filter_map(|item| match item {
                PaletteItem::Command(cmd) => Some(ContextItem::searchable(
                    cmd.label(),
                    ContextAction::RunCommand(cmd.verb()),
                    cmd.verb(),
                    pinned(cmd.verb()),
                )),
                // Catalog context actions always have an id (they came from the catalog).
                PaletteItem::Context(action) => context_action_id(action).map(|id| {
                    ContextItem::searchable(
                        context_action_palette_label(action).unwrap_or_default(),
                        action,
                        id,
                        pinned(id),
                    )
                }),
            })
            .collect()
    }

    /// Rebuild the open menu's rows from its current query: the curated rows when empty, the search
    /// results otherwise. Resets the highlight (the list changed). Called on each query edit.
    /// (Searchable context menu S1.)
    pub(super) fn rebuild_context_menu(&mut self) {
        let Some(query) = self.view.chrome().context_menu.as_ref().map(|m| m.query.clone()) else {
            return;
        };
        let items = if query.trim().is_empty() {
            self.build_curated_menu_items()
        } else {
            self.search_menu_items(&query)
        };
        self.view.chrome_update(move |c| {
            if let Some(menu) = &mut c.context_menu {
                menu.items = items;
                menu.selected = None;
            }
        });
        self.view.request_redraw();
    }

    /// The persona-curated context-menu command list (command registry P4): the user's configured
    /// order, or the registry default ([`DEFAULT_MENU_ACTIONS`](meerkat::command::DEFAULT_MENU_ACTIONS))
    /// when unset. Owned (cloned) so the build loop can borrow `self` to resolve each id.
    fn menu_actions(&self) -> Vec<String> {
        self.shared.presentation.menu_actions.clone()
    }

    /// Resolve a configured registry `id` to a context-menu row for a selection of `len` members,
    /// or `None` if it doesn't apply here (wrong selection shape, an unmet dynamic condition, or an
    /// unknown id). Handles both the menu's native context actions and global commands (carried via
    /// [`ContextAction::RunCommand`]). (Command registry P4.)
    fn resolve_menu_item(&self, id: &str, len: usize) -> Option<ContextItem> {
        if !meerkat::command::registry_scope(id)?.applies(len) {
            return None;
        }
        if let Some(action) = meerkat::command::context_action_from_id(id) {
            return self.resolve_context_action_item(action, len);
        }
        // A global command added to the menu: run it by verb.
        let cmd = meerkat::command::Command::from_id(id)?;
        Some(ContextItem::new(cmd.label(), ContextAction::RunCommand(cmd.verb())))
    }

    /// Build the row for a native context action at this selection: its count-adapted, ✓-marked
    /// label, or `None` when a dynamic condition isn't met (Relate needs exactly two, Show all a
    /// scope lens, Mirror tiles open). (Command registry P4.)
    fn resolve_context_action_item(&self, action: ContextAction, len: usize) -> Option<ContextItem> {
        use ContextAction::*;
        let item = match action {
            OpenSplits => {
                ContextItem::new(if len == 1 { "Open tile" } else { "Open in splits" }, OpenSplits)
            }
            Stack => ContextItem::new("Open in a stack", Stack),
            AddTag => ContextItem::new("Add tag\u{2026}", AddTag),
            ResizeNode => ContextItem::new("Resize", ResizeNode),
            OpenNodeFacets => ContextItem::new("Node settings\u{2026}", OpenNodeFacets),
            IsolateSelection => ContextItem::new("Isolate", IsolateSelection),
            ToggleSizeByDegree => {
                let on = self.orrery().size_by_degree();
                ContextItem::new(
                    if on { "Size by degree  \u{2713}" } else { "Size by degree" },
                    ToggleSizeByDegree,
                )
            }
            AddNode => ContextItem::new("Add node", AddNode),
            AddField => ContextItem::new("Add field", AddField),
            AddTile => ContextItem::new("Add tile", AddTile),
            AddSession => ContextItem::new("Add graph session", AddSession),
            ShowAllNodes => {
                if !self.orrery().is_scoped() {
                    return None; // only when a scope lens is active
                }
                ContextItem::new("Show all", ShowAllNodes)
            }
            MirrorTiles => {
                if !self.view.mirror_tiles && self.view.workbench.open_members().is_empty() {
                    return None; // only with tiles open (or already mirroring, to turn off)
                }
                ContextItem::new(
                    if self.view.mirror_tiles { "Mirror tiles  \u{2713}" } else { "Mirror tiles" },
                    MirrorTiles,
                )
            }
            _ => return None,
        };
        Some(item)
    }

    /// Open the context node's facets settings tile (the `node:<id>` provider), at the
    /// info page — the menu's pointer to the per-node config (engine pin / representation)
    /// that used to be inlined here. Acts on the first selected member. (Settings lane P3.)
    fn open_node_facets(&mut self) {
        if let Some(&member) = self.view.context_set.first() {
            self.open_settings_tile(&format!("node:{member}/info"));
        }
    }


    /// The layout-strategy rows for the focused orrery pane: "Force-directed" (the
    /// gyre default) plus each wired cartography strategy ([`platen::ORRERY_LAYOUT_STRATEGIES`]),
    /// with the pane's current choice ✓-marked. A pane-level choice, so it rides the
    /// no-selection right-click beside "Add node". (Layout picker.)
    fn layout_picker_items(&self) -> Vec<ContextItem> {
        let mark = |label: &str, on: bool| {
            if on {
                format!("{label}  \u{2713}") // ✓ marks the active layout
            } else {
                label.to_string()
            }
        };
        let active = self.orrery().layout_strategy();
        let mut items = vec![ContextItem::new(
            mark("Force-directed", active.is_none()),
            ContextAction::SetLayoutStrategy(""),
        )];
        for &(id, label) in platen::ORRERY_LAYOUT_STRATEGIES {
            items.push(ContextItem::new(
                mark(label, active == Some(id)),
                ContextAction::SetLayoutStrategy(id),
            ));
        }
        items
    }

    /// Dismiss the context menu (an outside click / Escape), dropping its set and
    /// the add-node cursor anchor.
    pub(super) fn close_context_menu(&mut self) {
        self.view.context_set.clear();
        self.view.context_origin = None;
        self.view.context_link = None;
        self.view.chrome_update(Chrome::close_context_menu);
        self.view.request_redraw();
    }

    /// Set the focused orrery's layout strategy (`""` reverts to force-directed); persisted
    /// per pane. Shared by the empty-canvas layout picker, the radial toggle, and the
    /// `pelt/orrery` settings page. (Settings lane P2b.)
    pub(super) fn set_orrery_layout(&mut self, id: &str) {
        self.orrery_mut().set_layout_strategy((!id.is_empty()).then(|| id.to_string()));
        self.save_session();
        self.view.request_redraw();
    }

    /// Toggle the focused orrery's size-by-degree mode (persisted in the cartography
    /// sidecar). Shared by the selection menu and the `pelt/orrery` page. (Settings lane P2b.)
    pub(super) fn toggle_orrery_size_by_degree(&mut self) {
        let on = self.orrery().size_by_degree();
        self.orrery_mut().set_size_by_degree(!on);
        self.view.request_redraw();
    }

    /// Toggle the live workbench mirror: scope the focused orrery to the open tiles (on) or
    /// lift the lens (off); persisted per pane. Shared by the menu and the `pelt/orrery`
    /// page. (Settings lane P2b.)
    pub(super) fn toggle_mirror_tiles(&mut self) {
        self.view.mirror_tiles = !self.view.mirror_tiles;
        if self.view.mirror_tiles {
            let members = self.view.workbench.open_members();
            self.orrery_mut().scope_to_members(members);
        } else {
            self.orrery_mut().clear_scope();
        }
        self.save_session();
        self.view.request_redraw();
    }

    /// Open `url` (a clicked link) as a new tab linked from `origin`. When `origin`
    /// is itself a tile, the new tab stacks into its slot in the **background** — the
    /// source tab stays active, the browser convention — so middle / Ctrl / right-click
    /// "open in new tab" don't yank you off the page you clicked from. When `origin` is
    /// not a tile (a card in Cartography, no tab context), it promotes to a focused new
    /// tile in the workbench. (Browser link flow.)
    pub(super) fn open_link_in_new_tab(&mut self, origin: GraphMemberId, url: String) {
        let new_member = self.orrery_mut().open_member_as_new_node(Some(origin), &url);
        if self.view.workbench.open_in_slot_of(new_member, origin) {
            // Stacked into the source's slot (which activates it); re-activate the
            // source so the new tab sits in the background.
            self.view.workbench.activate(origin);
        } else {
            // No tab context (card / Cartography): promote to a focused new tile.
            self.open_workbench();
            self.view.workbench.open_tile(new_member);
            self.view.focused_tile = Some(new_member);
        }
        self.ensure_content(&url);
        self.save_session();
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
        self.view.chrome_update(move |c| c.open_context_menu(x, y, items));
        self.view.request_redraw();
    }

    /// Run a pending context-menu action the chrome captured: open the menu's
    /// member set as splits or as one stack, switching into the tiled (Tree)
    /// projection first if needed.
    /// Apply a context action invoked from the **command palette**: the palette has no menu
    /// working set, so seed `context_set` from the live selection, clear the cursor anchor,
    /// and move the queued action into `pending_context` for [`drain_pending_context`]
    /// (called right after this) to run over the selection. (Command registry P2.)
    pub(super) fn drain_palette_context_action(&mut self) {
        let Some(action) = self.view.chrome().pending_palette_action else {
            return;
        };
        self.view.chrome_update(|c| c.pending_palette_action = None);
        self.view.context_set = self.selection_working_set();
        self.view.context_origin = None;
        self.view.chrome_update(move |c| c.pending_context = Some(action));
    }

    pub(super) fn drain_pending_context(&mut self) {
        let Some(action) = self.view.chrome().pending_context else {
            return;
        };
        self.view.chrome_update(|c| c.pending_context = None);
        // Audit the context action by its registry id (the Debug name for the not-yet-cataloged
        // parameterized ones), through the same spine as host commands. (Command registry P3.)
        let invoked_id = meerkat::command::context_action_id(action)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{action:?}").to_ascii_lowercase());
        self.shared.observability.record_diagnostic(
            "meerkat.command.invoked",
            Severity::Info,
            invoked_id,
        );
        // Open the node's facets settings tile (the per-node config's new home). No
        // member-set mutation — return before the orrery-tile logic below. (Settings lane P3.)
        if let ContextAction::OpenNodeFacets = action {
            self.open_node_facets();
            self.view.request_redraw();
            return;
        }
        // Run a global command carried into the menu by verb (command registry P4): enqueue it
        // as a chrome command intent; the per-frame command drain executes it. No member set.
        if let ContextAction::RunCommand(verb) = action {
            if let Some(cmd) = meerkat::command::Command::from_id(verb) {
                self.view.chrome_update(move |c| c.run_command_intent(cmd));
            }
            self.view.request_redraw();
            return;
        }
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
        // Set the focused orrery pane's layout strategy (the layout picker). An empty
        // id reverts to force-directed (gyre); persisted per pane via view-intent on
        // save_session. No member set — return before the orrery-tile logic below.
        if let ContextAction::SetLayoutStrategy(id) = action {
            self.set_orrery_layout(id);
            return;
        }
        // Toggle the focused orrery pane's size-by-degree mode (a scene presentation choice;
        // it + per-node overrides persist in the cartography sidecar). No member set — return
        // before the orrery-tile logic. (P0 resize.)
        if let ContextAction::ToggleSizeByDegree = action {
            self.toggle_orrery_size_by_degree();
            return;
        }
        // Summon the object card for the single focused node — it renders in the focus slot
        // in place of the snapshot preview until the focus moves off it. No member set —
        // return before the orrery-tile logic. (Object card — P0.)
        if let ContextAction::ResizeNode = action {
            self.view.object_card = self.focused_member();
            self.view.request_redraw();
            return;
        }
        // Delete the right-clicked field (retire it; the kernel keeps the definition). The
        // target was stored when the menu opened. No member set. (Field regions — delete.)
        if let ContextAction::DeleteField = action {
            if let Some(fid) = self.view.context_field.take() {
                self.orrery_mut().delete_field(fid);
                self.save_session();
                self.view.request_redraw();
            }
            return;
        }
        // Isolate the selection into the orrery's scope lens (a curated subgraph), or
        // lift it. A transient lens (keyed by NodeKey, not persisted). No member set.
        // (Curated orrery.)
        if let ContextAction::IsolateSelection = action {
            self.view.mirror_tiles = false; // a snapshot lens; don't let the live mirror override it
            self.orrery_mut().isolate_selection();
            self.view.request_redraw();
            return;
        }
        if let ContextAction::ShowAllNodes = action {
            self.view.mirror_tiles = false; // lift the live mirror too
            self.orrery_mut().clear_scope();
            self.view.request_redraw();
            return;
        }
        // Toggle the live workbench mirror. When on, the render loop re-scopes the
        // orrery to the open tiles each frame; turning it off lifts the lens. Persisted
        // per pane via view-intent (like the layout strategy), so the mode survives a
        // reload. No member set. (Curated orrery — workbench mirror.)
        if let ContextAction::MirrorTiles = action {
            self.toggle_mirror_tiles();
            return;
        }
        // Relate the two selected nodes — no tile / member-set logic, like the
        // shellbar move above.
        if let ContextAction::Relate = action {
            if self.orrery_mut().assert_selected_relation(SemanticSubKind::UserGrouped) {
                self.save_session();
            }
            self.view.request_redraw();
            return;
        }
        // Begin tagging the selected node(s): open the host tag prompt. The
        // selection is the target; commit inserts the typed tag on each. (Add-tag.)
        if let ContextAction::AddTag = action {
            self.start_tag();
            return;
        }
        // Add a fresh node. From the empty-space right-click it lands at the saved
        // cursor anchor; from the add-pill (no anchor) it mints at the default
        // position. No member set.
        if let ContextAction::AddNode = action {
            let url = "mere://welcome";
            match self.view.context_origin.take() {
                Some(origin) => {
                    let _ = self.orrery_mut().add_node_at(origin, url);
                }
                None => {
                    let _ = self.orrery_mut().open_member_as_new_node(None, url);
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
            let member = self.orrery_mut().open_member_as_new_node(None, url);
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
            let _ = self.orrery_mut().add_field_at(anchor);
            self.save_session();
            self.view.request_redraw();
            return;
        }
        // Close the focused graph pane — a pane op, no member set. (Pane-as-unit.)
        if let ContextAction::CloseGraphPane = action {
            self.close_focused_graph_pane();
            return;
        }
        // Engine picker: pin (or clear) the engine for the context node(s) without
        // opening tiles. The change re-routes the node next frame; the render's
        // `retain` reaps any now-unused scrying producer. (engine-picker Phase 3.)
        if let ContextAction::PinEngine(id) = action {
            for member in std::mem::take(&mut self.view.context_set) {
                self.shared.content.engine_pins.insert(member, id.to_string());
            }
            self.view.request_redraw();
            return;
        }
        if let ContextAction::AutoEngine = action {
            for member in std::mem::take(&mut self.view.context_set) {
                self.shared.content.engine_pins.remove(&member);
            }
            self.view.request_redraw();
            return;
        }
        // Set the per-node representation (the form picker): override each context node's
        // presentation form. Applies to the context set like the engine pins; the next
        // frame's snapshot reads the override through `node_representation`. The override
        // is held on the orrery (not yet persisted — a follow-up). (Node representation P1.)
        if let ContextAction::SetRepresentation(id) = action {
            let rep = match id {
                "shape" => Representation::Shape,
                _ => Representation::Tile,
            };
            for member in std::mem::take(&mut self.view.context_set) {
                self.orrery_mut().set_node_representation(member, rep);
            }
            self.view.request_redraw();
            return;
        }
        // Browser link flow: open the right-clicked link as a new tab, or copy it.
        // Reads `context_link` (source member + resolved url); no node set involved.
        if let ContextAction::OpenLinkNewTab = action {
            if let Some((origin, url)) = self.view.context_link.take() {
                self.open_link_in_new_tab(origin, url);
            }
            return;
        }
        if let ContextAction::CopyLink = action {
            if let Some((_, url)) = self.view.context_link.take() {
                if let Some(cb) = self.clipboard.as_mut() {
                    let _ = cb.set_text(url);
                }
            }
            return;
        }
        let set = std::mem::take(&mut self.view.context_set);
        if set.is_empty() {
            return;
        }
        // These open tiles, so summon the workbench pane (closing the suggestions
        // dropdown on the way in, like Ctrl+T does).
        if !self.workbench_open() {
            self.view.chrome_update(Chrome::close_suggestions);
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
            | ContextAction::AddField
            | ContextAction::DeleteField
            | ContextAction::CloseGraphPane
            | ContextAction::PinEngine(_)
            | ContextAction::AutoEngine
            | ContextAction::SetRepresentation(_)
            | ContextAction::AddTag
            | ContextAction::OpenLinkNewTab
            | ContextAction::CopyLink
            | ContextAction::SetLayoutStrategy(_)
            | ContextAction::ToggleSizeByDegree
            | ContextAction::ResizeNode
            | ContextAction::IsolateSelection
            | ContextAction::ShowAllNodes
            | ContextAction::MirrorTiles
            | ContextAction::OpenNodeFacets
            | ContextAction::RunCommand(_) => {
                unreachable!("handled above")
            }
        }
        self.view.request_redraw();
    }
}
