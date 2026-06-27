/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mouse-button routing: the region dispatcher (on_mouse_input). Kept whole pending runtime-verified pass extraction.

use super::*;

impl WindowCtx<'_> {
    /// Route a mouse button press/release by region. A left press in the chrome
    /// band (toolbar + any open dropdown) hit-tests + dispatches the chrome; any
    /// other press in the content band, and every release, goes to the orrery in
    /// content-band coordinates (its viewport top sits at the toolbar bottom).
    pub(crate) fn on_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        let orrery_button = match button {
            MouseButton::Left => Some(PointerButton::Left),
            MouseButton::Middle => Some(PointerButton::Middle),
            MouseButton::Right => Some(PointerButton::Right),
            _ => None,
        };
        let (x, y) = self.view.cursor;
        let th = self.toolbar_height() as f32;
        match state {
            ElementState::Pressed => {
                // The dedicated mouse back/forward thumb buttons step the focused
                // node's own history, the same intent as Alt+Left / Alt+Right. They
                // are never forwarded to a scrying tile, so they navigate everywhere.
                // (Browser flow.)
                if let Some(step) = match button {
                    MouseButton::Back => Some(HistoryStep::Back),
                    MouseButton::Forward => Some(HistoryStep::Forward),
                    _ => None,
                } {
                    self.view.chrome_update(|c| c.history_step = Some(step));
                    self.drain_history_step();
                    return;
                }
                // Borderless window: a left press on a window control acts on it; a
                // press near a window edge starts an OS resize drag. Both take
                // priority over the chrome / content (and any open menu).
                if button == MouseButton::Left {
                    if let Some(ctl) = titlebar::control_at(x, y, self.view.width, th as u32) {
                        self.window_control(ctl);
                        return;
                    }
                    if let Some(dir) = titlebar::resize_dir_at(x, y, self.view.width, self.view.height) {
                        // Manual resize: winit's drag_resize_window is inert on a
                        // frameless Windows window, so snapshot the rect + screen
                        // cursor and resize the window ourselves on each move.
                        if let Some(window) = self.view.window.as_ref() {
                            let outer = window
                                .outer_position()
                                .map(|p| (p.x, p.y))
                                .unwrap_or((0, 0));
                            let size = window.inner_size();
                            self.view.resize_drag = Some(crate::ResizeDrag {
                                dir,
                                start_outer: outer,
                                start_size: (size.width, size.height),
                                start_cursor_screen: (outer.0 as f32 + x, outer.1 as f32 + y),
                            });
                        }
                        return;
                    }
                }
                // A context menu swallows the next press: a left click on one of its
                // rows runs that action (the chrome closes the menu); a click
                // anywhere else just dismisses it. Exception: a pin toggle (the cursor
                // palette's search results) keeps the menu open so several can be pinned
                // in a row — the host rebuilds it in place. (Searchable context menu S2.)
                if self.view.chrome().context_menu.is_some() {
                    if button == MouseButton::Left {
                        // A press on a submenu-parent row expands its child panel instead of
                        // dismissing the menu — resolved by a dedicated hit-test, independent of
                        // the dispatch/drain timing the close decision below rides. (Nested submenus.)
                        if let Some(parent) = self.submenu_parent_at(x, y) {
                            // Toggle: clicking the already-open parent collapses it; otherwise
                            // expand (or switch to) it — mouse parity with ArrowLeft. (Submenus.)
                            let already_open = self
                                .view
                                .chrome()
                                .context_menu
                                .as_ref()
                                .and_then(|m| m.submenu.as_ref())
                                .is_some_and(|s| s.parent == parent);
                            self.view.chrome_update(move |c| {
                                if already_open {
                                    c.close_submenu();
                                } else {
                                    c.open_submenu(parent);
                                }
                            });
                            self.view.request_redraw();
                            return;
                        }
                        self.chrome_click(x, y);
                    }
                    let pinning = matches!(
                        self.view.chrome().pending_context,
                        Some(meerkat::ContextAction::PinToMenu(_))
                    );
                    if self.view.chrome().context_menu.is_some() && !pinning {
                        self.close_context_menu();
                    }
                    return;
                }
                // A press on the focused compatibility-view tile forwards into its
                // WebView and hands it the keyboard; a press anywhere else releases
                // that keyboard focus. (Scrying X2.)
                if let Some((member, lx, ly)) = self.scrying_at(x, y) {
                    if let Some(btn) = scrying_btn(button) {
                        self.view
                            .scrying
                            .forward_mouse(member, lx, ly, scrying_host::MousePress::Down(btn));
                        self.view.scrying.focus_tile(member);
                        self.view.scrying_input_focus = Some(member);
                        self.view.request_redraw();
                    }
                    return;
                }
                self.view.scrying_input_focus = None;
                // A right-click on a link (in a tile or card) opens the link context
                // menu (open in new tab / copy link) before any region routing, so it
                // works over either surface. (Browser link flow.)
                if button == MouseButton::Right && self.try_open_link_menu(x, y) {
                    return;
                }
                // A middle-click on a tile link opens it in a new background tab
                // directly; elsewhere middle stays the orrery's pan. (Browser flow.)
                if button == MouseButton::Middle && self.try_open_link_new_tab(x, y) {
                    return;
                }
                // The chrome's interactive area is the toolbar plus any open
                // dropdown (its `.chrome` border-box). A left press there dispatches
                // the chrome; below it (the content band) a right press opens the
                // context menu, anything else drives the orrery.
                let sheet = self.shared.presentation.chrome_sheet_refs();
                let chrome_h = {
                    let dom = self.view.dom.borrow();
                    // C4: read the chrome region off the render's retained layout
                    // (the session); only the first press before any render falls
                    // back to a stateless measure.
                    let bottom = match &self.view.chrome_session {
                        Some(s) => class_bottom_in(&dom, s.fragments(), "chrome"),
                        None => measure_class_bottom(
                            &dom,
                            &sheet,
                            self.view.width,
                            self.view.height,
                            "chrome",
                        ),
                    };
                    bottom.unwrap_or(self.view.toolbar_h.max(FALLBACK_TOOLBAR_H))
                };
                if y < th {
                    // The toolbar bar doubles as the titlebar: defer a left press to
                    // release so a press-and-drag moves the window (resolved in
                    // CursorMoved) while a press-and-release still clicks the button
                    // / focuses the omnibar (resolved on release).
                    if button == MouseButton::Left {
                        self.view.titlebar_press = Some((x, y));
                    }
                } else if y < chrome_h as f32 {
                    if button == MouseButton::Left {
                        self.chrome_click(x, y);
                    }
                } else {
                    // A press in the content band (canvas or workbench) is "clicking
                    // away" from the omnibar: blur the chrome caret and close the
                    // suggestion dropdown, so focus actually leaves the omnibar
                    // instead of the caret + dropdown lingering over the content.
                    if self.view.runner.focus().is_some() || !self.view.chrome().suggest.is_empty() {
                        self.view.runner.set_focus(None);
                        self.view.chrome_update(Chrome::close_suggestions);
                        self.view.request_redraw();
                    }
                    // A press on a frame divider starts a pane-resize drag. (F1.)
                    if button == MouseButton::Left {
                        if let Some((path, parent, axis)) = self.frame_divider_at(x, y) {
                            self.view.frame_divider_drag = Some((path, parent, axis));
                            return;
                        }
                    }
                    // Every folded pane (roster, the four list panes, comms) lives in the one
                    // shell document, so a left press in any of them routes through the single
                    // shell hit-test + dispatch (chrome_click); chrome_activate then drains
                    // whatever the hit row/button queued (roster selections, apparatus theme /
                    // engine / physics, trail recover, comms). The gloss is the one pane with
                    // bespoke handling (it focuses minimap nodes itself), so it stays its own
                    // branch below. (Phase 1, step 3 — Y-band collapse.)
                    // A settings tile's body lives in the shell document too (it paints over
                    // the workbench composite at the tile rect), so a press there routes to the
                    // same shell hit-test + dispatch as the folded panes. (Settings lane P1.)
                    if self.chrome_routed_leaf_at(x, y)
                        || self.settings_pane_at(x, y)
                        || self.knot_editor_pane_at(x, y)
                    {
                        if button == MouseButton::Left {
                            // A press near a node swatch's hull vertex begins a vertex drag
                            // (the shape editor); otherwise it's a normal pane interaction.
                            // (Swatch — node shape editor, Stage B.)
                            if self.try_begin_swatch_drag(x, y) {
                                return;
                            }
                            // A press on a reorderable row's drag grip begins a drag-reorder
                            // (the configurable menu list); otherwise it's a normal pane click.
                            // (Command registry B2.)
                            if self.try_begin_row_reorder(x, y) {
                                return;
                            }
                            self.chrome_click(x, y);
                        } else if button == MouseButton::Right {
                            // A right press on a swatch hull vertex removes it (the shape editor's
                            // delete gesture); elsewhere in a pane it does nothing. (Swatch — B3.)
                            self.try_remove_swatch_vertex(x, y);
                        }
                        return;
                    }
                    // The gloss pane consumes the press: a left click on a minimap
                    // node focuses it (shared selection with the orrery). (Gloss.)
                    if let Some(grect) = self.gloss_leaf_rect() {
                        if x >= grect[0] && x < grect[2] && y >= grect[1] && y < grect[3] {
                            if button == MouseButton::Left {
                                if let Some(member) =
                                    self.gloss_node_at(x, y).or_else(|| self.gloss_recent_at(x, y))
                                {
                                    if let Some(url) = self
                                        .orrery()
                                        .graph()
                                        .get_node_by_id(member)
                                        .map(|(_, n)| n.url().to_string())
                                    {
                                        self.orrery_mut().select_by_url(&url);
                                        self.view.request_redraw();
                                    }
                                }
                            }
                            return;
                        }
                    }
                    // The shellbar strip: right-click opens the move menu. (Shellbar F2.2.)
                    let sb = crate::shellbar::shellbar_rect(
                        self.shared.presentation.shellbar_edge,
                        self.view.width as f32,
                        self.view.height as f32,
                        th,
                    );
                    if x >= sb[0] && x < sb[2] && y >= sb[1] && y < sb[3] {
                        let hit = |r: &[f32; 4]| x >= r[0] && x < r[2] && y >= r[1] && y < r[3];
                        // The session switcher tiles (F2.3): a left press closes,
                        // switches, or mints a graph. Closes win over rows (the × sits
                        // inside its tile). (Multi-graph MG4.)
                        if button == MouseButton::Left {
                            if let Some((id, _)) =
                                self.view.session_close_rects.iter().find(|(_, r)| hit(r))
                            {
                                let id = *id;
                                self.commands.push(crate::ShellCommand::CloseSession(id));
                                return;
                            }
                            if self.view.session_add_rect.as_ref().is_some_and(hit) {
                                self.commands.push(crate::ShellCommand::CreateSession);
                                return;
                            }
                            if let Some((id, _)) =
                                self.view.session_row_rects.iter().find(|(_, r)| hit(r))
                            {
                                let id = *id;
                                // Shift+click opens that session's graph in a second
                                // Orrery pane beside the current one (per-pane render);
                                // a plain click switches the focused session. (Window
                                // composition P2 — second graph-pane.)
                                if self.view.modifiers.shift {
                                    self.commands.push(crate::ShellCommand::OpenGraphBeside(id));
                                } else {
                                    self.commands.push(crate::ShellCommand::SwitchSession(id));
                                }
                                return;
                            }
                            // Not a host-drawn session tile → a chrome-DOM control in
                            // the strip (the pane-toggle buttons). Commit an in-progress
                            // rename first (clicking away accepts it), then route to the
                            // chrome so its hit-test fires the button's `run_command`;
                            // without this the press falls to the strip's catch-all
                            // `return` below and the toggle buttons stay inert.
                            if self.view.renaming.is_some() {
                                self.commit_rename();
                            }
                            self.chrome_click(x, y);
                            return;
                        }
                        // A right press on a tile renames that session; elsewhere in the
                        // strip it opens the shellbar move menu. (Host text path.)
                        if button == MouseButton::Right {
                            if let Some((id, _)) =
                                self.view.session_row_rects.iter().find(|(_, r)| hit(r))
                            {
                                let id = *id;
                                self.start_rename(id);
                            } else {
                                self.open_shellbar_menu_at(x, y);
                            }
                        }
                        return;
                    }
                    // A press outside the shellbar while renaming commits the edit
                    // (clicking away accepts the new name). (Host text path.)
                    if self.view.renaming.is_some() {
                        self.commit_rename();
                    }
                    // Route by content pane (the orrery + the workbench coexist).
                    // A press in either makes it the active (nav-target) pane.
                    let in_workbench = self
                        .workbench_leaf_rect()
                        .is_some_and(|wr| x >= wr[0] && x < wr[2] && y >= wr[1] && y < wr[3]);
                    if in_workbench {
                        // The workbench root: a left press on a slot divider starts a
                        // resize; otherwise it routes to the root (tab switch / close
                        // / pin).
                        self.view.active_content = crate::ContentPane::Workbench;
                        if button == MouseButton::Left {
                            // Drive the pelt shell's pointer state machine: it hit-tests
                            // the frame (divider / tab / close) at the pane-local point
                            // and emits gestures the Workbench applies. (Drag via TileEvents.)
                            self.workbench_pointer_down(x, y);
                        }
                    } else {
                        // The orrery pane: right-click opens the context menu; a left
                        // / middle press pans / selects / drags (unless it's over the
                        // orrery's card, which owns its own clicks).
                        self.view.active_content = crate::ContentPane::Orrery;
                        // Focus-follows-click: a press on a graph-pane moves focus to
                        // it, so the context menu, selection, and pointer all act on
                        // *this* pane (the existing handlers resolve focused_graph).
                        // (Window composition — pane-as-unit; per-pane pointer input.)
                        if let Some((gid, _)) = self.orrery_pane_at(x, y) {
                            self.focus_pane_graph(gid);
                        }
                        // A left press on a node glyph routes to gyre like any other node
                        // press (no special-casing): gyre arms a drag on the node under the
                        // cursor, and its CLICK_SLOP splits a click (select the node) from a drag
                        // (move it) in `pointer_up`. The node is a physical object in the orrery;
                        // selection still shows on the glyph through `node_selected`. This is the
                        // node-as-object MVP model (a click will later open the node's content in
                        // pelt); it supersedes the cond-3 DOM-select-on-press, which forced the
                        // drag off the glyph onto the bare collider. (Node representation.)
                        if button == MouseButton::Right {
                            self.open_context_menu_at(x, y);
                        } else if button == MouseButton::Left && self.point_over_object_card(x, y) {
                            // The object card's widget buttons own this press: route it to the
                            // chrome so their `on_click` fires (queuing `node_card_keys`). It must
                            // not fall through to gyre, which would grab the node under the card and
                            // never reach the button. (Object card — the press-routing gate.)
                            self.chrome_click(x, y);
                        } else if let Some(b) = orrery_button {
                            let (ox, oy) = self.orrery_point(x, y);
                            // GA-1 (tear-out G1): a Shift-held left-press on a node arms a
                            // tear-out drag instead of the orrery's node-pin pick, so the
                            // modifier-drag never steals the pin. (Shift = branch, Ctrl+Shift
                            // = fork at release — slice 2; v0 spawns a leaf carrying the node.)
                            // Requires a node under the cursor; an empty Shift-press falls
                            // through to the orrery (marquee). (Tear-out gestures.)
                            let tear_node = (button == MouseButton::Left
                                && self.view.modifiers.shift
                                && !self.point_over_card(x, y))
                            .then(|| self.orrery().node_at_screen(ox, oy))
                            .flatten();
                            if let Some(node) = tear_node {
                                // Resolve the node's display label for the drag ghost (GA-5).
                                let label = {
                                    let graph = self.orrery().graph();
                                    graph
                                        .get_node_by_id(node)
                                        .map(|(key, _)| graph.node_display_label(key))
                                        .unwrap_or_default()
                                };
                                // The modifier fixes the operation at press (GA-1):
                                // Ctrl+Shift = fork, plain Shift = branch.
                                let op = if self.view.modifiers.ctrl {
                                    crate::window_view::TearOp::Fork
                                } else {
                                    crate::window_view::TearOp::Branch
                                };
                                self.view.tear_out_drag =
                                    Some(crate::window_view::TearOutDrag {
                                        node,
                                        source_graph: self.view.focused_graph,
                                        op,
                                        origin: (x, y),
                                    });
                                self.view.chrome_update(|c| c.tear_ghost = Some(label));
                                self.view.request_redraw();
                            } else if !self.point_over_card(x, y)
                                && self.orrery_mut().pointer_down(b, ox, oy)
                            {
                                self.view.request_redraw();
                            }
                        }
                    }
                }
            }
            ElementState::Released => {
                // A tear-out drag (G1): a Shift-drag of a node past the slop tears it out
                // into a new leaf window (queued; the registry op runs after the ctx borrow
                // ends). A non-moved press (a Shift-click that never dragged) just clears.
                // Handled first, like the other drags below. (Tear-out gestures.)
                if let Some(drag) = self.view.tear_out_drag.take() {
                    const TEAR_SLOP: f32 = 6.0;
                    let moved = (self.view.cursor.0 - drag.origin.0)
                        .hypot(self.view.cursor.1 - drag.origin.1)
                        > TEAR_SLOP;
                    if moved {
                        // Drop-target grammar (OQ-1): a drop on an orrery pane of a
                        // DIFFERENT graph is a cross-graph copy (G5); anything else (the
                        // source pane, chrome, off-window) tears into a new leaf window.
                        let dest = self
                            .orrery_pane_at(self.view.cursor.0, self.view.cursor.1)
                            .map(|(gid, _)| gid)
                            .filter(|&gid| gid != drag.source_graph);
                        let cmd = match dest {
                            // Cross-graph copy (G5): dropped on a different graph's pane.
                            Some(to) => crate::ShellCommand::CopyNodeAcross {
                                node: drag.node,
                                from: drag.source_graph,
                                to,
                            },
                            // Tear axis: the operation was fixed at press by the modifier.
                            // Fork mints an independent session + graph snapshot; branch
                            // mints a `Branched` graphlet in the donor's session (sharing
                            // its graph + nodes) and opens a window scoped to it.
                            None => match drag.op {
                                crate::window_view::TearOp::Fork => {
                                    crate::ShellCommand::ForkNode {
                                        node: drag.node,
                                        from: drag.source_graph,
                                    }
                                }
                                crate::window_view::TearOp::Branch => {
                                    crate::ShellCommand::BranchNode {
                                        node: drag.node,
                                        from: drag.source_graph,
                                    }
                                }
                            },
                        };
                        self.commands.push(cmd);
                    }
                    self.view.chrome_update(|c| c.tear_ghost = None);
                    self.view.request_redraw();
                    return;
                }
                // End an in-progress swatch vertex drag before any other release routing. The
                // collider was reshaped live on each move, so the release only clears the
                // gesture — but it must clear FIRST: a release that lands on a scrying tile (or
                // any other branch with its own early return) would otherwise leave the drag
                // armed, and the next no-button move would keep reshaping the hull. Any button
                // ends it (the drag is left-initiated and has no multi-button meaning).
                // (Swatch — node shape editor, Stage B.)
                if self.view.swatch_drag.take().is_some() {
                    // The hull is graph-truth geometry: persist the edit on release (the
                    // cartography sidecar), so a hitbox edit survives a reload even on a
                    // non-graceful exit. (Node body & face — the Body axis persists.)
                    self.save_session();
                    self.view.request_redraw();
                    return;
                }
                // End an in-progress row-reorder drag (the configurable menu list): drop the
                // grabbed row onto the row under the cursor (reposition + persist). Like the
                // swatch drag it clears first and consumes the release; a sub-threshold release
                // (a grip click that never moved) just clears. (Command registry B2.)
                if let Some(drag) = self.view.row_reorder_drag.take() {
                    if drag.moved {
                        if let Some(target) = drag.target {
                            if target != drag.id {
                                self.reorder_menu_action_to(&drag.id, &target);
                            }
                        }
                    }
                    self.view.request_redraw();
                    return;
                }
                // A release over the focused compatibility-view tile forwards into
                // its WebView (button-up to complete a click). (Scrying X2.)
                if let Some((member, lx, ly)) = self.scrying_at(x, y) {
                    if let Some(btn) = scrying_btn(button) {
                        self.view
                            .scrying
                            .forward_mouse(member, lx, ly, scrying_host::MousePress::Up(btn));
                        self.view.request_redraw();
                    }
                    return;
                }
                // Resolve a pending titlebar press that never became a window drag:
                // it was a click on the toolbar bar (a button / the omnibar). Run it
                // now, and skip the orrery / double-click paths — the press never
                // reached them. (Custom titlebar.)
                if button == MouseButton::Left {
                    if let Some((px, py)) = self.view.titlebar_press.take() {
                        self.chrome_click(px, py);
                        return;
                    }
                    // A manual window resize ends on release; skip the orrery /
                    // double-click paths (the press never reached them).
                    if self.view.resize_drag.take().is_some() {
                        return;
                    }
                }
                // A release over the focused card belongs to the card, not the
                // orrery — releasing on the card must not deselect the node (that
                // would break the card's double-click promote). Elsewhere the release
                // reaches the orrery, which acts only if it owns an in-progress pan /
                // drag / marquee; a click-release selects the node under the cursor.
                let over_card = self.point_over_card(x, y);
                // A left click on a link inside a content card follows it: map the
                // click into the card's content-local space (its rect origin + the
                // card's scroll), resolve the link URL (relative gemtext / markdown
                // links join the card's own URL), and navigate the focused node to
                // it — the omnibar's record-the-visit path. Consumes the release so
                // it doesn't fall through to the card's live-preview toggle.
                // (Inline-link nav; the document lane carries link regions today.)
                //
                // A workbench tab / divider gesture resolves through the pelt shell
                // wherever the release lands (a drop can end outside the pane). If the
                // shell consumed it (tab activate / close / drop / divider end) we're
                // done; otherwise the press was a tile-content click — fall through to
                // the link paths below. (Drag via pelt TileEvents.)
                if button == MouseButton::Left && self.view.workbench_gesture {
                    if self.workbench_pointer_up(x, y) {
                        return;
                    }
                }
                if button == MouseButton::Left {
                    if let Some((base, href)) = self.card_link_at(x, y) {
                        let url = nav::resolve_href(&base, &href);
                        self.view.chrome_update(|c| c.follow_link(url));
                        self.sync_orrery();
                        self.view.request_redraw();
                        return;
                    }
                }
                // A click on a link in a workbench tile: Ctrl+click opens it in a new
                // background tab (browser flow); a plain click navigates *that tile's
                // member* in place — focus it so the omnibar + `sync_orrery` target it
                // (`nav_target_member` is the focused tile in Tree), then follow the link.
                if button == MouseButton::Left {
                    if let Some((member, base, href)) = self.tile_link_at(x, y) {
                        let url = nav::resolve_href(&base, &href);
                        if self.view.modifiers.ctrl {
                            self.open_link_in_new_tab(member, url);
                        } else {
                            self.view.workbench.activate(member);
                            self.view.focused_tile = Some(member);
                            self.view.chrome_update(|c| c.follow_link(url));
                            self.sync_orrery();
                            self.view.request_redraw();
                        }
                        return;
                    }
                }
                // A click on a scripted-rung tile/card is interactive page content: it
                // routes to the live document (script listeners run) and is consumed,
                // exactly as a link click is — not passed through to the orrery. Other
                // lanes emit no scripted hit and fall through. (Render ladder phase 3.)
                #[cfg(feature = "scripted")]
                if button == MouseButton::Left
                    && (self.tile_scripted_click(x, y) || self.card_scripted_click(x, y))
                {
                    return;
                }
                if let Some(b) = orrery_button {
                    let (ox, oy) = self.orrery_point(x, y);
                    // A field move / resize ends here; its geometry is graph truth, so
                    // persist the session on release. (Field regions — move/resize.)
                    let was_field_drag = self.orrery().dragging_field();
                    if !over_card && self.orrery_mut().pointer_up(b, ox, oy) {
                        if was_field_drag {
                            self.save_session();
                        }
                        self.view.request_redraw();
                    }
                }
                // A frame-divider (host FrameLayout) resize ends on release. The pelt
                // tile-divider resize ended in the workbench-gesture path above.
                if button == MouseButton::Left {
                    self.view.frame_divider_drag = None;
                }
                // Double-click routing (orrery pane): on the focused card it toggles
                // the live preview (snapshot -> live actor, or back); on a node it
                // summons the workbench pane with that node + its active neighbors
                // (the contextual-staging gesture). Skip when the release is in the
                // workbench pane (tiles handle their own double-clicks).
                let released_in_workbench = self
                    .workbench_leaf_rect()
                    .is_some_and(|wr| x >= wr[0] && x < wr[2] && y >= wr[1] && y < wr[3]);
                if button == MouseButton::Left && !released_in_workbench {
                    let now = Instant::now();
                    let double = self.view.last_left_release.is_some_and(|(t, (lx, ly))| {
                        now.duration_since(t) < Duration::from_millis(400)
                            && (x - lx).hypot(y - ly) < 6.0
                    });
                    self.view.last_left_release = Some((now, (x, y)));
                    if double {
                        self.view.last_left_release = None; // don't chain a triple-click
                        // Double-click opens the node in pelt (a workbench tile), on a node
                        // or its snapshot card, replacing the retired promote-to-live-preview.
                        // (Node-rep P4.) But the object card's − / + are tier steps, not a
                        // node-open: a double-tap on + must step twice, never launch pelt.
                        // (Object card P0.)
                        if !self.point_over_object_card(x, y)
                            && (over_card || !self.orrery().selected_members().is_empty())
                        {
                            self.toggle_workbench();
                        }
                    }
                }
            }
        }
    }

}
