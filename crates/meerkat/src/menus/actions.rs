/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Menu actions: facets, pickers, orrery toggles, drains.

use super::*;

impl WindowCtx<'_> {
    /// Open the context node's facets settings tile (the `node:<id>` provider), at the
    /// info page — the menu's pointer to the per-node config (engine pin / representation)
    /// that used to be inlined here. Acts on the first selected member. (Settings lane P3.)
    pub(crate) fn open_node_facets(&mut self) {
        if let Some(&member) = self.view.context_set.first() {
            self.open_settings_tile(&format!("node:{member}/info"));
        }
    }


    /// The layout-strategy rows for the focused orrery pane: "Force-directed" (the
    /// gyre default) plus each wired cartography strategy ([`platen::ORRERY_LAYOUT_STRATEGIES`]),
    /// with the pane's current choice ✓-marked. A pane-level choice, so it rides the
    /// no-selection right-click beside "Add node". (Layout picker.)
    pub(crate) fn layout_picker_items(&self) -> Vec<ContextItem> {
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

    /// The hand-reachable semantic relation kinds the two-node relate picker offers, each
    /// paired with its menu label. Mirrors the curated set `relation_kind_from_str` accepts by
    /// word (the `relate("cites")` vocabulary); the menu asserts the kind directly. (Audit A3.)
    const RELATE_PICKER_KINDS: &'static [(SemanticSubKind, &'static str)] = &[
        (SemanticSubKind::Cites, "Cites"),
        (SemanticSubKind::Quotes, "Quotes"),
        (SemanticSubKind::Summarizes, "Summarizes"),
        (SemanticSubKind::Elaborates, "Elaborates"),
        (SemanticSubKind::ExampleOf, "Example of"),
        (SemanticSubKind::Supports, "Supports"),
        (SemanticSubKind::Contradicts, "Contradicts"),
        (SemanticSubKind::Questions, "Questions"),
        (SemanticSubKind::SameEntityAs, "Same entity as"),
        (SemanticSubKind::DuplicateOf, "Duplicate of"),
        (SemanticSubKind::Hyperlink, "Hyperlink"),
    ];

    /// The relation-kind rows for a two-node selection: one "Relate as <kind>" row per curated
    /// semantic relation, each asserting that kind on the pair. Gives the edge vocabulary a click
    /// path — previously only `relate("cites")` by typing reached it, so every drawn edge was an
    /// undifferentiated `UserGrouped`. The plain pinned "Relate" row still asserts `UserGrouped`.
    /// (Audit A3 — the relation-kind picker; the audit's top pick.)
    pub(crate) fn relate_picker_items(&self) -> Vec<ContextItem> {
        Self::RELATE_PICKER_KINDS
            .iter()
            .map(|&(kind, label)| {
                ContextItem::new(format!("Relate as {label}"), ContextAction::RelateAs(kind))
            })
            .collect()
    }

    /// Dismiss the context menu (an outside click / Escape), dropping its set and
    /// the add-node cursor anchor.
    pub(crate) fn close_context_menu(&mut self) {
        self.view.context_set.clear();
        self.view.context_origin = None;
        self.view.context_link = None;
        self.view.chrome_update(Chrome::close_context_menu);
        self.view.request_redraw();
    }

    /// Set the focused orrery's layout strategy (`""` reverts to force-directed); persisted
    /// per pane. Shared by the empty-canvas layout picker, the radial toggle, and the
    /// `pelt/orrery` settings page. (Settings lane P2b.)
    pub(crate) fn set_orrery_layout(&mut self, id: &str) {
        self.orrery_mut().set_layout_strategy((!id.is_empty()).then(|| id.to_string()));
        self.save_session();
        self.view.request_redraw();
    }

    /// Toggle the focused orrery's size-by-degree mode (persisted in the cartography
    /// sidecar). Shared by the selection menu and the `pelt/orrery` page. (Settings lane P2b.)
    pub(crate) fn toggle_orrery_size_by_degree(&mut self) {
        let on = self.orrery().size_by_degree();
        self.orrery_mut().set_size_by_degree(!on);
        self.view.request_redraw();
    }

    /// Toggle the focused orrery's size-by-importance mode (the graph-signals importance
    /// encoding). Shared by the selection menu and the `pelt/orrery` page. (Graph signals.)
    pub(crate) fn toggle_orrery_size_by_importance(&mut self) {
        let on = self.orrery().size_by_importance();
        self.orrery_mut().set_size_by_importance(!on);
        self.view.request_redraw();
    }

    /// Toggle the live workbench mirror: scope the focused orrery to the open tiles (on) or
    /// lift the lens (off); persisted per pane. Shared by the menu and the `pelt/orrery`
    /// page. (Settings lane P2b.)
    pub(crate) fn toggle_mirror_tiles(&mut self) {
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
    pub(crate) fn open_link_in_new_tab(&mut self, origin: GraphMemberId, url: String) {
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
    pub(crate) fn open_shellbar_menu_at(&mut self, x: f32, y: f32) {
        let current = self.shared.presentation.shellbar_edge;
        // The four edges fold under a "Move shellbar" submenu (the current edge ✓-marked); "Hide"
        // stays a top-level row. (Submenus.)
        let edges: Vec<ContextItem> = [
            (ShellbarEdge::Left, "Left"),
            (ShellbarEdge::Right, "Right"),
            (ShellbarEdge::Top, "Top"),
            (ShellbarEdge::Bottom, "Bottom"),
        ]
        .iter()
        .map(|&(edge, label)| {
            let label = if edge == current {
                format!("{label}  \u{2713}") // ✓ marks the current edge
            } else {
                label.to_string()
            };
            ContextItem::new(label, ContextAction::ShellbarMove(edge))
        })
        .collect();
        let items = vec![
            ContextItem::with_children("Move shellbar", edges),
            // Hide the shellbar (reveal it again from the palette / `>shellbar`). A hidden strip
            // can't be right-clicked, so this row only ever hides. (Hide-shellbar.)
            ContextItem::new("Hide shellbar", ContextAction::ShellbarToggleVisibility),
        ];
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
    pub(crate) fn drain_palette_context_action(&mut self) {
        let Some(action) = self.view.chrome().pending_palette_action else {
            return;
        };
        self.view.chrome_update(|c| c.pending_palette_action = None);
        self.view.context_set = self.selection_working_set();
        self.view.context_origin = None;
        self.view.chrome_update(move |c| c.pending_context = Some(action));
    }

    pub(crate) fn drain_pending_context(&mut self) {
        let Some(action) = self.view.chrome().pending_context else {
            return;
        };
        self.view.chrome_update(|c| c.pending_context = None);
        // A submenu-parent sentinel never picks an action (it expands its children via the
        // press-gate intercept / `open_submenu`); a stray dispatch is a harmless no-op, returned
        // before any audit so it emits no diagnostic noise. (Submenus.)
        if let ContextAction::OpenSubmenu = action {
            return;
        }
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
        // Tally cataloged context actions for the menu's frequency auto-suggest; skip the
        // parameterized / non-catalog ones (no stable registry id to rank). (Command registry S3.)
        if let Some(id) = meerkat::command::context_action_id(action) {
            self.record_command_usage(id);
        }
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
        // Pin / unpin a searched command to the curated menu, keeping the menu open: toggle its
        // membership (+ persist) and rebuild the rows so the pin state refreshes. (Searchable S2.)
        if let ContextAction::PinToMenu(id) = action {
            self.toggle_menu_action(id);
            self.rebuild_context_menu();
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
        // Shellbar hide: the right-click row hides the strip; reveal from `>shellbar`.
        if let ContextAction::ShellbarToggleVisibility = action {
            self.toggle_shellbar_visibility();
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
        if let ContextAction::ToggleSizeByImportance = action {
            self.toggle_orrery_size_by_importance();
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
        // Open the focused node as a persistent Linked graphlet in its own scoped window —
        // the manual Linked-graphlet consumers (component / neighborhood / link web). Each
        // maps to a (kind, selectors, chip) edge-projection; Shell-level (mint + open a
        // window), so it queues a command. (Graphlet wiring Phase 3 slice 2 / 2+.)
        let projection = match action {
            ContextAction::OpenComponentGraphlet => {
                Some((forme::GraphletKind::Component, Vec::new(), "component"))
            }
            ContextAction::OpenNeighborhoodGraphlet => Some((
                forme::GraphletKind::Ego { radius: 2 },
                Vec::new(),
                "neighborhood",
            )),
            ContextAction::OpenLinkWebGraphlet => Some((
                forme::GraphletKind::Component,
                vec!["Semantic".to_string()],
                "link web",
            )),
            _ => None,
        };
        if let Some((kind, selectors, chip)) = projection {
            if let Some(node) = self.orrery().focused_member() {
                self.commands.push(crate::ShellCommand::OpenLinkedGraphlet {
                    node,
                    from: self.view.focused_graph,
                    kind,
                    selectors,
                    chip,
                });
            }
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
        // Relate the two selected nodes as the picked semantic kind — like `Relate`, but
        // carries the chosen kind instead of the UserGrouped default. (Audit A3.)
        if let ContextAction::RelateAs(kind) = action {
            if self.orrery_mut().assert_selected_relation(kind) {
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
            self.commands.push(crate::ShellCommand::CreateSession);
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
        // Set the per-node face (the texture picker): override each context node's face. Applies
        // to the context set like the engine pins; the next frame's snapshot reads it through
        // `node_face`. Sets only the face — the body (collider) is untouched. The override is
        // held on the orrery (not yet persisted — a follow-up). (Node body & face — Face axis.)
        if let ContextAction::SetFace(id) = action {
            let face = match id {
                "sprite" => Face::Sprite,
                "bare" => Face::Bare,
                _ => Face::Favicon,
            };
            for member in std::mem::take(&mut self.view.context_set) {
                self.orrery_mut().set_node_face(member, face);
            }
            // The face override persists in the cartography sidecar; save it now. (Body & face.)
            self.save_session();
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
            | ContextAction::ShellbarToggleVisibility
            | ContextAction::Relate
            | ContextAction::RelateAs(_)
            | ContextAction::OpenSubmenu
            | ContextAction::AddNode
            | ContextAction::AddTile
            | ContextAction::AddSession
            | ContextAction::AddField
            | ContextAction::DeleteField
            | ContextAction::CloseGraphPane
            | ContextAction::PinEngine(_)
            | ContextAction::AutoEngine
            | ContextAction::SetFace(_)
            | ContextAction::AddTag
            | ContextAction::OpenLinkNewTab
            | ContextAction::CopyLink
            | ContextAction::SetLayoutStrategy(_)
            | ContextAction::ToggleSizeByDegree
            | ContextAction::ToggleSizeByImportance
            | ContextAction::ResizeNode
            | ContextAction::IsolateSelection
            | ContextAction::OpenComponentGraphlet
            | ContextAction::OpenNeighborhoodGraphlet
            | ContextAction::OpenLinkWebGraphlet
            | ContextAction::ShowAllNodes
            | ContextAction::MirrorTiles
            | ContextAction::OpenNodeFacets
            | ContextAction::RunCommand(_)
            | ContextAction::PinToMenu(_) => {
                unreachable!("handled above")
            }
        }
        self.view.request_redraw();
    }
}
