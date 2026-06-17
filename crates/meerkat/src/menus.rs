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
use session_runtime::ShellbarEdge;

use super::WindowCtx;

impl WindowCtx<'_> {
    /// Open the right-click context menu over the current selection's working set,
    /// at window `(x, y)`. A no-op when nothing is selected (no set to act on). A
    /// single-member set offers one "open tile"; a larger set offers splits vs a
    /// stack. The host remembers the set; the chrome renders the rows.
    pub(super) fn open_context_menu_at(&mut self, x: f32, y: f32) {
        // "Close graph view" is offered whenever a second graph-pane is open, so it
        // sits at the foot of the menu in either branch. (Pane-as-unit.)
        let multi_graph = self.has_multiple_graph_panes();
        let close_item =
            || ContextItem::new("Close graph view", ContextAction::CloseGraphPane);
        let set = self.selection_working_set();
        if set.is_empty() {
            // No selection (typically a right-click on empty canvas): offer "Add
            // node" at the cursor. Remember the content-band cursor point so AddNode
            // mints the node under it; leave context_set empty (no member set). A
            // node-hit-test for true spatial emptiness is a refinement.
            self.view.context_origin = Some(self.orrery_point(x, y));
            self.view.context_set.clear();
            let mut items = vec![
                ContextItem::new("Add node", ContextAction::AddNode),
                ContextItem::new("Add field", ContextAction::AddField),
            ];
            // "Show all" lifts an active scope lens (back to the whole graph). (Curated orrery.)
            if self.orrery().is_scoped() {
                items.push(ContextItem::new("Show all", ContextAction::ShowAllNodes));
            }
            // Mirror the workbench's open tiles into the orrery — the same arrangement as
            // both a tiled workbench and a spatial map, tracked live. A toggle (✓ when
            // on); offered when tiles are open or the mirror is already on so it can be
            // turned off. (Curated orrery — workbench mirror.)
            if self.view.mirror_tiles || !self.view.workbench.open_members().is_empty() {
                let label = if self.view.mirror_tiles {
                    "Mirror tiles  \u{2713}"
                } else {
                    "Mirror tiles"
                };
                items.push(ContextItem::new(label, ContextAction::MirrorTiles));
            }
            // Layout is a pane-level choice, so it rides the empty-canvas menu. (Layout picker.)
            items.extend(self.layout_picker_items());
            if multi_graph {
                items.push(close_item());
            }
            self.view.chrome_update(move |c| c.open_context_menu(x, y, items));
            self.view.request_redraw();
            return;
        }
        // A selection-based menu never mints at the cursor; clear any stale anchor.
        self.view.context_origin = None;
        let mut items = if set.len() == 1 {
            // Single node: "Open tile" plus the per-node engine picker ("Open in
            // <engine>"), so the user can flip this node's engine. (Phase 3.)
            let mut items = vec![ContextItem::new("Open tile", ContextAction::OpenSplits)];
            items.extend(self.engine_picker_items(set[0]));
            items.push(ContextItem::new("Add tag\u{2026}", ContextAction::AddTag));
            items
        } else {
            let mut items = vec![
                ContextItem::new("Open in splits", ContextAction::OpenSplits),
                ContextItem::new("Open in a stack", ContextAction::Stack),
            ];
            // Relate is pairwise — offer it only for exactly two selected nodes.
            if set.len() == 2 {
                items.push(ContextItem::new("Relate", ContextAction::Relate));
            }
            items.push(ContextItem::new("Add tag\u{2026}", ContextAction::AddTag));
            items
        };
        // Radial layout: centers the orrery on a single selected node (BFS rings
        // outward) and re-centers live as the selection moves. Focus-driven, so it
        // rides the selection menu rather than the empty-canvas layout picker, and only
        // for a single selection (a clean focus). A toggle — ✓ when active, picking it
        // again reverts to force-directed. (Layout picker — radial.)
        if self.orrery().focused_key().is_some() {
            let radial_on = self.orrery().layout_strategy() == Some("radial.default");
            items.push(ContextItem::new(
                if radial_on {
                    "Radial layout  \u{2713}"
                } else {
                    "Radial layout"
                },
                if radial_on {
                    ContextAction::SetLayoutStrategy("")
                } else {
                    ContextAction::SetLayoutStrategy("radial.default")
                },
            ));
        }
        // Isolate the selection into the orrery's scope lens (a curated subgraph). A
        // pane-level lens, offered on any selection. (Curated orrery.)
        items.push(ContextItem::new("Isolate", ContextAction::IsolateSelection));
        if multi_graph {
            items.push(close_item());
        }
        self.view.context_set = set;
        self.view.chrome_update(move |c| c.open_context_menu(x, y, items));
        self.view.request_redraw();
    }

    /// The engine-picker rows for `member`: "Auto (default engine)" plus each
    /// pickable web / surface engine that is available this session (present +
    /// active), with the node's current choice ✓-marked. Routing prefers the picked
    /// engine for the node. (engine-picker Phase 3.)
    ///
    /// The web/surface engines only render **web** content (http/https), so they are
    /// offered only for web nodes — picking "System WebView" for a `gemini://` node
    /// would route it to WebView2, which cannot load gemtext, and render blank. For a
    /// non-web node the picker is empty unless a (stale) pin needs clearing, in which
    /// case only "Auto" is offered to reset it.
    fn engine_picker_items(&self, member: GraphMemberId) -> Vec<ContextItem> {
        // The web-rendering alternatives a node can be flipped between. Smolweb
        // protocols route to one engine each, so they are not offered here; the
        // surface engines (system WebView, later weld / graft) are the compat path.
        const PICKABLE: &[(&str, &str)] = &[
            (inker::routing::ENGINE_SERVAL_WEB, "Serval (web)"),
            (inker::routing::ENGINE_SCRYING_WEB, "System WebView"),
        ];
        let pin = self.shared.content.engine_pins.get(&member).map(String::as_str);
        // Web nodes (http/https) can flip between the web engines; other schemes
        // cannot render through them. Scheme resolved from the node's url (owned, to
        // release the graph borrow).
        let scheme = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .and_then(|(_, n)| inker::routing::address_scheme(n.url()).map(str::to_string));
        let is_web = scheme
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https"));
        // Nothing to choose and no pin to clear → no picker rows for this node.
        if !is_web && pin.is_none() {
            return Vec::new();
        }
        let mark = |label: &str, on: bool| {
            if on {
                format!("{label}  \u{2713}") // ✓ marks the current choice
            } else {
                label.to_string()
            }
        };
        let mut items = vec![ContextItem::new(
            mark("Auto (default engine)", pin.is_none()),
            ContextAction::AutoEngine,
        )];
        if is_web {
            for &(id, name) in PICKABLE {
                if self.engine_available(id) {
                    items.push(ContextItem::new(
                        mark(&format!("Open in {name}"), pin == Some(id)),
                        ContextAction::PinEngine(id),
                    ));
                }
            }
        }
        items
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
    pub(super) fn drain_pending_context(&mut self) {
        let Some(action) = self.view.chrome().pending_context else {
            return;
        };
        self.view.chrome_update(|c| c.pending_context = None);
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
            self.orrery_mut().set_layout_strategy((!id.is_empty()).then(|| id.to_string()));
            self.save_session();
            self.view.request_redraw();
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
            self.view.mirror_tiles = !self.view.mirror_tiles;
            if self.view.mirror_tiles {
                let members = self.view.workbench.open_members();
                self.orrery_mut().scope_to_members(members);
            } else {
                self.orrery_mut().clear_scope();
            }
            self.save_session();
            self.view.request_redraw();
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
            | ContextAction::CloseGraphPane
            | ContextAction::PinEngine(_)
            | ContextAction::AutoEngine
            | ContextAction::AddTag
            | ContextAction::OpenLinkNewTab
            | ContextAction::CopyLink
            | ContextAction::SetLayoutStrategy(_)
            | ContextAction::IsolateSelection
            | ContextAction::ShowAllNodes
            | ContextAction::MirrorTiles => {
                unreachable!("handled above")
            }
        }
        self.view.request_redraw();
    }
}
