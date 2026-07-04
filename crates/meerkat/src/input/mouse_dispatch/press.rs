/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mouse press handling (extracted from on_mouse_input).

use super::*;

impl WindowCtx<'_> {
    pub(crate) fn on_mouse_press(&mut self, button: MouseButton) {
        let orrery_button = match button {
            MouseButton::Left => Some(PointerButton::Left),
            MouseButton::Middle => Some(PointerButton::Middle),
            MouseButton::Right => Some(PointerButton::Right),
            _ => None,
        };
        let (x, y) = self.view.cursor;
        let th = self.current_toolbar_height() as f32;
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
            if let Some(ctl) = titlebar::control_at(
                x,
                y,
                self.view.width,
                th as u32,
                self.shared.presentation.ui_scale(),
            ) {
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
            if button == MouseButton::Left {
                if self.view.clip_picker == Some(member) {
                    let shown = self.finish_clip_pick(member, lx, ly);
                    self.view.chrome_update(move |c| c.show_location(&shown));
                    self.view.request_redraw();
                    return;
                } else if let Some(shown) = self.cancel_clip_picker() {
                    self.view.chrome_update(move |c| c.show_location(&shown));
                    self.view.request_redraw();
                    return;
                }
            }
            if let Some(btn) = scrying_btn(button) {
                self.view.scrying.forward_mouse(
                    member,
                    lx,
                    ly,
                    scrying_host::MousePress::Down(btn),
                );
                self.view.scrying.focus_tile(member);
                self.view.scrying_input_focus = Some(member);
                self.view.request_redraw();
            }
            return;
        }
        if button == MouseButton::Left {
            if let Some(shown) = self.cancel_clip_picker() {
                self.view.chrome_update(move |c| c.show_location(&shown));
                self.view.request_redraw();
                return;
            }
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
                None => {
                    measure_class_bottom(&dom, &sheet, self.view.width, self.view.height, "chrome")
                }
            };
            bottom.unwrap_or_else(|| self.current_toolbar_height())
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
            // Every folded pane (roster, the four list panes, comms, gloss) lives in the
            // one shell document, so a left press in any of them routes through the
            // single shell hit-test + dispatch (chrome_click); chrome_activate then
            // drains whatever the hit row/button queued (roster selections, apparatus
            // theme/engine/physics, trail recover, comms, gloss outline/recent/minimap
            // node selects). (Phase 1, step 3 — Y-band collapse; Scene-to-DOM migration
            // P3 folded gloss in.)
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
            // The shellbar strip: right-click opens the move menu. (Shellbar F2.2.)
            let sb = crate::shellbar::shellbar_rect(
                self.shared.presentation.shellbar_edge,
                self.view.width as f32,
                self.view.height as f32,
                th,
                self.shared.presentation.ui_scale(),
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
                    if let Some((id, _)) = self.view.session_row_rects.iter().find(|(_, r)| hit(r))
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
                    if let Some((id, _)) = self.view.session_row_rects.iter().find(|(_, r)| hit(r))
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
                    if self.try_begin_page_text_selection(x, y) {
                        return;
                    }
                    self.clear_page_text_selection();
                    // Drive the pelt shell's pointer state machine: it hit-tests
                    // the frame (divider / tab / close) at the pane-local point
                    // and emits gestures the Workbench applies. (Drag via TileEvents.)
                    self.workbench_pointer_down(x, y);
                }
            } else {
                // The orrery pane: right-click opens the context menu; a left
                // / middle press pans / selects / drags (unless it's over the
                // orrery's card, which owns its own clicks).
                self.focus_orrery_content();
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
                } else if button == MouseButton::Left
                    && self.try_open_connection_relation_card(x, y)
                {
                    // A relation dot in the connections swatch addresses the same Link Card as a
                    // Links-table row, preselected to that relation cell. (Swatch P4.)
                } else if button == MouseButton::Left && self.point_over_object_card(x, y) {
                    // The object card's widget buttons own this press: route it to the
                    // chrome so their `on_click` fires (queuing `object_card_keys`). It must
                    // not fall through to gyre, which would grab the node under the card and
                    // never reach the button. (Object card — the press-routing gate.)
                    self.chrome_click(x, y);
                } else if button == MouseButton::Left && self.try_begin_page_text_selection(x, y) {
                    return;
                } else if let Some(b) = orrery_button {
                    if button == MouseButton::Left {
                        self.clear_page_text_selection();
                    }
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
                        self.view.tear_out_drag = Some(crate::window_view::TearOutDrag {
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
}
