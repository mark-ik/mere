/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mouse, keyboard, and palette input handlers for [`Shell`](super::Shell). Factored
//! from `main.rs` to keep files under the workspace 600-LOC ceiling.

use std::time::{Duration, Instant};

use forme::GraphMemberId;
use layout_dom_api::LayoutDom;
use meerkat::{Chrome, ContextAction, ContextItem, HistoryStep, nav, submit_omnibar};
use orrery::PointerButton;
use crate::serval_render::hit_test_node;
use serval_layout::ScrollOffsets;
use serval_scripted_dom::NodeId;
use serval_winit_host::key_event_from_winit;
use winit::event::{ElementState, MouseButton};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use xilem_serval::PointerClick;

use frame::PaneContent;

use super::titlebar::{self, WindowControl};
use super::{
    FALLBACK_TOOLBAR_H, WindowCtx, class_bottom_in, first_tag, first_with_class,
    measure_class_bottom, scrying_host,
};

/// Map a winit mouse button to the scrying host's button vocabulary. (Scrying X2.)
fn scrying_btn(button: MouseButton) -> Option<scrying_host::MouseBtn> {
    match button {
        MouseButton::Left => Some(scrying_host::MouseBtn::Left),
        MouseButton::Right => Some(scrying_host::MouseBtn::Right),
        MouseButton::Middle => Some(scrying_host::MouseBtn::Middle),
        _ => None,
    }
}

/// The (active, else first) tile id under the `TileTree` node addressed by `path` (a
/// chain of split-child indices). Resolves a tab-bar drop's target stack back to a
/// member for the Workbench mutation. (Drag via pelt TileEvents.)
fn member_at_path(
    tree: &pelt_core::tile::TileTree,
    path: &[usize],
) -> Option<pelt_core::tile::TileId> {
    use pelt_core::tile::TileTree;
    match (tree, path.split_first()) {
        (TileTree::Stack(s), _) => {
            s.tabs.get(s.active).or_else(|| s.tabs.first()).map(|t| t.id)
        }
        (TileTree::Split { children, .. }, Some((i, rest))) => {
            member_at_path(&children.get(*i)?.tree, rest)
        }
        (TileTree::Split { .. }, None) => None,
    }
}

/// Parse a `<i>:<count>` slider cell into the fraction `i/count` — the position a
/// click on cell `i` of a `count`-cell track maps to. `None` on a malformed key.
fn slider_cell_fraction(s: &str) -> Option<f64> {
    let (i, count) = s.split_once(':')?;
    let i: f64 = i.parse().ok()?;
    let count: f64 = count.parse().ok()?;
    (count > 0.0).then_some(i / count)
}

/// The index `i` of the hull edge (`hull[i]` → `hull[i+1]`, wrapping the last to the first)
/// nearest to point `(px, py)`, if its distance is within `tol`. Used to insert a new corner
/// where the user clicks a hull edge. (Swatch — add vertex, B3.)
fn nearest_hull_edge(hull: &[(f32, f32)], px: f32, py: f32, tol: f32) -> Option<usize> {
    let n = hull.len();
    (0..n)
        .map(|i| {
            let (ax, ay) = hull[i];
            let (bx, by) = hull[(i + 1) % n];
            (i, point_segment_dist(px, py, ax, ay, bx, by))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|&(_, d)| d <= tol)
        .map(|(i, _)| i)
}

/// Distance from point `(px, py)` to the segment `(ax, ay)`–`(bx, by)`. (Swatch — add vertex.)
fn point_segment_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

impl WindowCtx<'_> {
    /// Route a mouse button press/release by region. A left press in the chrome
    /// band (toolbar + any open dropdown) hit-tests + dispatches the chrome; any
    /// other press in the content band, and every release, goes to the orrery in
    /// content-band coordinates (its viewport top sits at the toolbar bottom).
    pub(super) fn on_mouse_input(&mut self, state: ElementState, button: MouseButton) {
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
                            self.view.resize_drag = Some(super::ResizeDrag {
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
                    if self.chrome_routed_leaf_at(x, y) || self.settings_pane_at(x, y) {
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
                    let sb = super::shellbar::shellbar_rect(
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
                                self.commands.push(super::ShellCommand::CloseSession(id));
                                return;
                            }
                            if self.view.session_add_rect.as_ref().is_some_and(hit) {
                                self.commands.push(super::ShellCommand::CreateSession);
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
                                    self.commands.push(super::ShellCommand::OpenGraphBeside(id));
                                } else {
                                    self.commands.push(super::ShellCommand::SwitchSession(id));
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
                        self.view.active_content = super::ContentPane::Workbench;
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
                        self.view.active_content = super::ContentPane::Orrery;
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
                            if !self.point_over_card(x, y) && self.orrery_mut().pointer_down(b, ox, oy) {
                                self.view.request_redraw();
                            }
                        }
                    }
                }
            }
            ElementState::Released => {
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

    /// Whether window point `(x, y)` is over a composited content card (its rect
    /// from the last frame). Clicks / scroll over the card route to the card, not
    /// the orrery beneath it.
    fn point_over_card(&self, x: f32, y: f32) -> bool {
        self.view.content_rects
            .iter()
            .any(|(_, r)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
    }

    /// The `(card URL, link href)` under window point `(x, y)`, if it lands on a link
    /// in a composited content card. Maps the point into the card's content-local
    /// space (its rect origin + the card's scroll) and queries the actor's link map;
    /// the base is the card member's own URL, for resolving relative links. `None`
    /// when the point is over no card link (the caller keeps its normal click).
    /// (Inline-link nav.)
    fn card_link_at(&self, x: f32, y: f32) -> Option<(String, String)> {
        for (member, r) in &self.view.content_rects {
            if x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3] {
                let scroll = self.view.scroll.get(member).copied().unwrap_or(0.0);
                let lx = x - r[0];
                let ly = (y - r[1]) + scroll;
                if let Some(href) = self.shared.content.constellation.link_at(*member, lx, ly) {
                    let href = href.to_string();
                    let base = self
                        .orrery()
                        .graph()
                        .get_node_by_id(*member)
                        .map(|(_, n)| n.url().to_string())
                        .unwrap_or_default();
                    return Some((base, href));
                }
            }
        }
        None
    }

    /// The `(tile member, member URL, link href)` under window `(x, y)`, if it lands on
    /// a link in a workbench tile's composited content. The tile counterpart to
    /// [`card_link_at`](Self::card_link_at): the tiles composite at `tile_rects` (window
    /// axis-aligned, no orrery camera), so the same content-local mapping (rect origin +
    /// the tile's scroll) and the same actor link map resolve it. Returns the member too,
    /// since a tile click navigates *that* tile rather than the focused card.
    fn tile_link_at(&self, x: f32, y: f32) -> Option<(GraphMemberId, String, String)> {
        for (member, r) in &self.view.tile_rects {
            if x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3] {
                let scroll = self.view.scroll.get(member).copied().unwrap_or(0.0);
                let lx = x - r[0];
                let ly = (y - r[1]) + scroll;
                if let Some(href) = self.shared.content.constellation.link_at(*member, lx, ly) {
                    let href = href.to_string();
                    let base = self
                        .orrery()
                        .graph()
                        .get_node_by_id(*member)
                        .map(|(_, n)| n.url().to_string())
                        .unwrap_or_default();
                    return Some((*member, base, href));
                }
            }
        }
        None
    }

    /// If window point `(x, y)` is over a composited content **card** on the scripted
    /// rung, forward the click to its live document (its listeners run, the DOM may
    /// mutate, the tile re-renders) and report it consumed. The card's scroll is *not*
    /// added: the scripted document owns its scroll internally and the dispatch
    /// re-applies it, so the host passes the viewport-local point. `false` when the
    /// point is over no scripted card. (Render ladder phase 3.)
    #[cfg(feature = "scripted")]
    fn card_scripted_click(&self, x: f32, y: f32) -> bool {
        for (member, r) in &self.view.content_rects {
            if x >= r[0]
                && x <= r[2]
                && y >= r[1]
                && y <= r[3]
                && self.shared.content.constellation.is_scripted(*member)
            {
                self.shared.content.constellation.click_scripted(*member, x - r[0], y - r[1]);
                return true;
            }
        }
        false
    }

    /// The tile counterpart to [`card_scripted_click`](Self::card_scripted_click): a
    /// click on a scripted workbench tile focuses that tile and forwards the click to
    /// its live document. `false` when the point is over no scripted tile. (Phase 3.)
    #[cfg(feature = "scripted")]
    fn tile_scripted_click(&mut self, x: f32, y: f32) -> bool {
        let hit = self.view.tile_rects.iter().find_map(|(member, r)| {
            (x >= r[0]
                && x <= r[2]
                && y >= r[1]
                && y <= r[3]
                && self.shared.content.constellation.is_scripted(*member))
            .then(|| (*member, x - r[0], y - r[1]))
        });
        let Some((member, lx, ly)) = hit else {
            return false;
        };
        self.view.workbench.activate(member);
        self.view.focused_tile = Some(member);
        self.shared.content.constellation.click_scripted(member, lx, ly);
        self.view.request_redraw();
        true
    }

    /// A right-click on a link in a tile or card opens the link context menu — open
    /// in new tab, copy link — instead of navigating in place. Resolves the link and
    /// its source member, stashes them in `context_link`, opens the menu, and returns
    /// whether a link was actually hit. (Browser link flow.)
    fn try_open_link_menu(&mut self, x: f32, y: f32) -> bool {
        let (origin, url) = if let Some((member, base, href)) = self.tile_link_at(x, y) {
            (member, nav::resolve_href(&base, &href))
        } else if let (Some(focused), Some((base, href))) =
            (self.focused_member(), self.card_link_at(x, y))
        {
            (focused, nav::resolve_href(&base, &href))
        } else {
            return false;
        };
        self.view.context_link = Some((origin, url));
        let items = vec![
            ContextItem::new("Open link in new tab", ContextAction::OpenLinkNewTab),
            ContextItem::new("Copy link address", ContextAction::CopyLink),
        ];
        self.view.chrome_update(move |c| c.open_context_menu(x, y, items));
        self.view.request_redraw();
        true
    }

    /// Whether `(x, y)` is over a clickable link in a tile or card — drives the hover
    /// (hand) cursor so links are discoverable. (Browser link flow.)
    pub(super) fn over_link(&self, x: f32, y: f32) -> bool {
        self.tile_link_at(x, y).is_some() || self.card_link_at(x, y).is_some()
    }

    /// Middle / Ctrl-click on a tile link opens it in a new background tab directly
    /// (no menu). Tile-only: a card link uses the right-click menu, and this keeps the
    /// orrery's own middle-drag pan free. Returns whether a tile link was hit.
    /// (Browser link flow.)
    fn try_open_link_new_tab(&mut self, x: f32, y: f32) -> bool {
        if let Some((member, base, href)) = self.tile_link_at(x, y) {
            let url = nav::resolve_href(&base, &href);
            self.open_link_in_new_tab(member, url);
            true
        } else {
            false
        }
    }

    /// Apply a window-control press (borderless titlebar). Minimize / maximize act
    /// on the window directly; close defers to the event handler via `pending_exit`
    /// (input has no event-loop handle), which saves the session and exits.
    fn window_control(&mut self, ctl: WindowControl) {
        match ctl {
            WindowControl::Minimize => {
                if let Some(window) = self.view.window.as_ref() {
                    window.set_minimized(true);
                }
            }
            WindowControl::Maximize => {
                if let Some(window) = self.view.window.as_ref() {
                    let maximized = window.is_maximized();
                    window.set_maximized(!maximized);
                }
            }
            WindowControl::Close => self.view.pending_exit = true,
        }
    }

    /// Whether `(x, y)` falls in a folded pane that routes its clicks through the shell
    /// hit-test (`chrome_click`): the roster, the four list panes, and comms — every pane in
    /// the shell document except the gloss (which focuses minimap nodes itself). The single
    /// content-band check that replaced the per-pane rect branches. (Phase 1, step 3.)
    fn chrome_routed_leaf_at(&self, x: f32, y: f32) -> bool {
        self.laid_leaves().iter().any(|leaf| {
            matches!(
                leaf.content,
                PaneContent::Roster
                    | PaneContent::Apparatus
                    | PaneContent::Steward
                    | PaneContent::Inspector
                    | PaneContent::Trail
                    | PaneContent::Alembic
                    | PaneContent::Comms
            ) && x >= leaf.rect[0]
                && x < leaf.rect[2]
                && y >= leaf.rect[1]
                && y < leaf.rect[3]
        })
    }

    /// Whether `(x, y)` is inside an open settings tile's body rect — a press there routes to
    /// the shell document (the settings pane's spine + controls) rather than the workbench
    /// surface underneath it. (Settings lane P1.)
    fn settings_pane_at(&self, x: f32, y: f32) -> bool {
        self.view
            .settings_rects
            .iter()
            .any(|(_, r)| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
    }

    /// Hit-test the chrome root at `(x, y)` and dispatch the click (buttons +
    /// suggestion / palette rows). A row / backdrop click that closes the palette
    /// restores focus so the caret doesn't dangle on the removed field.
    pub(super) fn chrome_click(&mut self, x: f32, y: f32) {
        let mut offsets = ScrollOffsets::<NodeId>::default();
        // The roster + the four folded list panes scroll their own inner containers; mirror
        // the render's scroll offsets so the hit-test lands on the visible row. (Phase 1.)
        {
            let dom = self.view.dom.borrow();
            let root = dom.document();
            for (class, scroll) in [
                ("roster", self.view.roster_scroll),
                ("apparatus", self.view.apparatus_scroll),
                ("steward", self.view.steward_scroll),
                ("inspector", self.view.inspector_scroll),
                ("trail", self.view.trail_scroll),
                ("settings-pane-body", self.view.settings_scroll),
            ] {
                if let Some(node) = crate::first_with_class(&dom, root, class) {
                    offsets.insert(node, (0.0, scroll));
                }
            }
        }
        let sheet = self.shared.presentation.chrome_sheet_refs();
        let hit = {
            let dom = self.view.dom.borrow();
            // C4: hit-test against the render's retained chrome layout (the
            // session); fall back to a stateless probe only before the first render.
            match &self.view.chrome_session {
                Some(s) => s.hit_test(&dom, x, y, &offsets),
                None => hit_test_node(&dom, &sheet, self.view.width, self.view.height, x, y, &offsets),
            }
        };
        if let Some(node) = hit {
            self.chrome_activate(node, (x, y));
        }
    }

    /// Dispatch a click to chrome `node` and drain every intent its handlers may
    /// have queued — the shared tail of a pointer [`chrome_click`](Self::chrome_click)
    /// (which hit-tests to find the node first) and an a11y-driven activation (which
    /// already knows the node from its route, G2.4). `at` is element-local, so the
    /// a11y path passes `(0.0, 0.0)` (the node's own origin) — a valid synthetic
    /// activation point, ignored by the chrome's position-agnostic button handlers.
    pub(super) fn chrome_activate(&mut self, node: NodeId, at: (f32, f32)) {
        let palette_was_open = self.view.chrome().palette_open;
        self.view.runner.dispatch_click(node, PointerClick::at(at));
        self.drain_chrome_intents();
        if palette_was_open && !self.view.chrome().palette_open {
            self.focus_after_palette_close();
        }
        self.view.request_redraw();
    }

    /// Drain + apply every intent the chrome and folded-pane handlers may have queued, then
    /// sync the derived state. The shared tail of a pointer activation
    /// ([`chrome_activate`](Self::chrome_activate)) and a keyboard activation (Enter/Space
    /// routed through `dispatch_key` in [`on_key_pressed`](Self::on_key_pressed)), so focus +
    /// Enter fires the same effect a click does. (Phase 1, step 3c.)
    fn drain_chrome_intents(&mut self) {
        self.drain_pending_connect();
        self.drain_pending_command();
        self.drain_comms_intent();
        self.drain_palette_context_action();
        self.drain_pending_context();
        self.drain_history_step();
        self.drain_physics_toggle();
        self.drain_roster_intents();
        self.drain_list_pane_activations();
        self.drain_object_card();
        self.sync_settings();
        self.sync_orrery();
    }

    /// Reconcile the object card: drop it once the focus moves off its member (a new or
    /// cleared selection), and dispatch the activation keys its widget controls queued.
    /// (Object card — P1.)
    fn drain_object_card(&mut self) {
        let keys = self.view.take_node_card_keys();
        if self.view.object_card.is_some() && self.view.object_card != self.focused_member() {
            self.view.object_card = None;
            self.view.request_redraw();
        }
        if let Some(member) = self.view.object_card {
            if !keys.is_empty() {
                let mut face_changed = false;
                for key in keys {
                    match key.as_str() {
                        "size:up" => {
                            self.orrery_mut().step_node_size_tier(member, 1);
                        }
                        "size:down" => {
                            self.orrery_mut().step_node_size_tier(member, -1);
                        }
                        "face:favicon" => {
                            self.orrery_mut().set_node_face(member, orrery::Face::Favicon);
                            face_changed = true;
                        }
                        "face:bare" => {
                            self.orrery_mut().set_node_face(member, orrery::Face::Bare);
                            face_changed = true;
                        }
                        _ => {}
                    }
                }
                // The face override persists in the cartography sidecar; save it now. (Body & face.)
                if face_changed {
                    self.save_session();
                }
                self.view.request_redraw();
            }
        }
    }

    /// Whether `(x, y)` lands on the **object card** specifically (not a node-card), so the
    /// double-click-to-open-in-pelt gesture can skip it — its − / + are tier steps, and a
    /// double-tap on + must step twice, never launch the node in pelt. (Object card P0.)
    fn point_over_object_card(&self, x: f32, y: f32) -> bool {
        let Some(session) = self.view.chrome_session.as_ref() else { return false };
        let dom = self.view.dom.borrow();
        let offsets = ScrollOffsets::<NodeId>::default();
        let Some(mut node) = session.hit_test(&dom, x, y, &offsets) else { return false };
        loop {
            if crate::has_class(&dom, node, "object-card") {
                return true;
            }
            match dom.parent(node) {
                Some(parent) => node = parent,
                None => return false,
            }
        }
    }

    /// Try to begin a swatch edit from a left press at `(x, y)`: grab the nearest hull vertex if
    /// the press is on one, else **insert** a new vertex where it lands on a hull edge and grab
    /// that — so the swatch is a full body designer (drag to move, click an edge to add a
    /// corner). Returns `true` if a drag started (the caller consumes the press), else `false`
    /// (it falls through to the normal pane click). The swatch is DOM in the chrome document, so
    /// this reuses the object-card press-gate pattern (hit-test the chrome session, walk up to
    /// the `node-swatch` container). (Swatch — node shape editor, Stage B / B3.)
    fn try_begin_swatch_drag(&mut self, x: f32, y: f32) -> bool {
        let Some((subject, hull, origin, edge, nx, ny)) = self.swatch_geometry_at(x, y) else {
            return false;
        };
        let grab = 16.0 / edge;
        let (vi, vdist2) = hull
            .iter()
            .enumerate()
            .map(|(i, &(vx, vy))| (i, (vx - nx).powi(2) + (vy - ny).powi(2)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("hull has >= 3 vertices");
        // On a vertex: drag it.
        if vdist2 <= grab * grab {
            self.view.swatch_drag =
                Some(crate::window_view::SwatchDrag { subject, vertex: vi, origin, edge });
            self.view.request_redraw();
            return true;
        }
        // Not on a vertex but inside the swatch near an edge: insert a corner there and drag it.
        if nx.abs() <= 0.5 && ny.abs() <= 0.5 {
            if let Some(after) = nearest_hull_edge(&hull, nx, ny, 14.0 / edge) {
                let insert_at = after + 1;
                let mut new_hull = hull;
                new_hull.insert(insert_at, (nx.clamp(-0.5, 0.5), ny.clamp(-0.5, 0.5)));
                self.orrery_mut().set_node_sprite_hull(subject, new_hull);
                self.view.swatch_drag = Some(crate::window_view::SwatchDrag {
                    subject,
                    vertex: insert_at,
                    origin,
                    edge,
                });
                self.view.request_redraw();
                return true;
            }
        }
        false
    }

    /// Remove the hull vertex under a right press at `(x, y)`, if the press lands on one and the
    /// hull stays a polygon (>= 3 vertices). Returns whether a vertex was removed. (Swatch — B3.)
    fn try_remove_swatch_vertex(&mut self, x: f32, y: f32) -> bool {
        // Never reshape the hull from a right-click while a left-drag owns it: removing a vertex
        // would shift the dragged vertex's index out from under the in-flight gesture. (Swatch.)
        if self.view.swatch_drag.is_some() {
            return false;
        }
        let Some((subject, hull, _origin, edge, nx, ny)) = self.swatch_geometry_at(x, y) else {
            return false;
        };
        if hull.len() <= 3 {
            return false;
        }
        let grab = 16.0 / edge;
        let (vi, vdist2) = hull
            .iter()
            .enumerate()
            .map(|(i, &(vx, vy))| (i, (vx - nx).powi(2) + (vy - ny).powi(2)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("hull has >= 3 vertices");
        if vdist2 > grab * grab {
            return false;
        }
        let mut new_hull = hull;
        new_hull.remove(vi);
        self.orrery_mut().set_node_sprite_hull(subject, new_hull);
        // Persist the hull edit (graph-truth geometry) immediately. (Node body & face.)
        self.save_session();
        self.view.request_redraw();
        true
    }

    /// Resolve a press at `(x, y)` to a node swatch and the press in its normalized face space:
    /// the subject node, its current hull (cloned), the container's painted top-left (window px),
    /// the edge length, and the normalized press point `(nx, ny)` in `[-0.5, 0.5]`. `None` when
    /// the press is not over an editable swatch. (Swatch — Stage B / B3.)
    fn swatch_geometry_at(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(uuid::Uuid, Vec<(f32, f32)>, (f32, f32), f32, f32, f32)> {
        let session = self.view.chrome_session.as_ref()?;
        let dom = self.view.dom.borrow();
        // Mirror the render / chrome_click scroll offsets so the hit-test lands on the visible
        // swatch (it sits inside the scrolled settings-pane-body).
        let mut offsets = ScrollOffsets::<NodeId>::default();
        let root = dom.document();
        for (class, scroll) in [
            ("roster", self.view.roster_scroll),
            ("apparatus", self.view.apparatus_scroll),
            ("steward", self.view.steward_scroll),
            ("inspector", self.view.inspector_scroll),
            ("trail", self.view.trail_scroll),
            ("settings-pane-body", self.view.settings_scroll),
        ] {
            if let Some(node) = crate::first_with_class(&dom, root, class) {
                offsets.insert(node, (0.0, scroll));
            }
        }
        // Walk up from the hit node to the `node-swatch` container.
        let mut node = session.hit_test(&dom, x, y, &offsets)?;
        let container = loop {
            if crate::has_class(&dom, node, "node-swatch") {
                break node;
            }
            node = dom.parent(node)?;
        };
        // Whose hull this swatch edits (the container's `data-subject`), and its current hull.
        let subject: uuid::Uuid = dom
            .attributes(container)
            .find(|a| a.name.local.as_ref() == "data-subject")
            .and_then(|a| a.value.parse().ok())?;
        let key = self.orrery().graph().get_node_by_id(subject).map(|(k, _)| k)?;
        let hull = self.orrery().node_sprite_hull(key)?.to_vec();
        if hull.len() < 3 {
            return None;
        }
        // The container's painted top-left: its absolute layout origin (Taffy locations are
        // parent-relative, so sum the chain to the root) minus the body's scroll offset.
        let frags = session.fragments();
        let mut abs = (0.0_f32, 0.0_f32);
        let mut n = container;
        loop {
            if let Some(l) = frags.rect_of(n) {
                abs.0 += l.location.x;
                abs.1 += l.location.y;
            }
            match dom.parent(n) {
                Some(p) => n = p,
                None => break,
            }
        }
        let edge = crate::swatch::swatch_edge_px();
        // `settings_scroll` is a single shared value (one active settings pane), matching the
        // host's existing model — `chrome_click` keys the same scroll onto the first
        // settings-pane-body. A per-pane scroll is a later refinement.
        let origin = (abs.0, abs.1 - self.view.settings_scroll);
        let nx = (x - origin.0) / edge - 0.5;
        let ny = (y - origin.1) / edge - 0.5;
        Some((subject, hull, origin, edge, nx, ny))
    }

    /// Advance an in-progress swatch vertex drag: map the cursor into the swatch's normalized
    /// face space and rewrite the dragged hull vertex, which rebuilds the node's collider live
    /// (`set_node_sprite_hull` pushes geometry). (Swatch — Stage B.)
    pub(super) fn drag_swatch_vertex(&mut self, x: f32, y: f32) {
        let Some(drag) = self.view.swatch_drag else { return };
        let nx = ((x - drag.origin.0) / drag.edge - 0.5).clamp(-0.5, 0.5);
        let ny = ((y - drag.origin.1) / drag.edge - 0.5).clamp(-0.5, 0.5);
        // Resolve the drag's target, requiring the vertex index to still be in range. If the
        // node or hull vanished, or a hull edit shortened it under the gesture, cancel the drag
        // rather than leave it armed in a zombie state. (Swatch — drag self-heal.)
        let target = self
            .orrery()
            .graph()
            .get_node_by_id(drag.subject)
            .map(|(k, _)| k)
            .and_then(|key| self.orrery().node_sprite_hull(key).map(<[(f32, f32)]>::to_vec))
            .filter(|hull| drag.vertex < hull.len());
        let Some(mut hull) = target else {
            self.view.swatch_drag = None;
            return;
        };
        hull[drag.vertex] = (nx, ny);
        self.orrery_mut().set_node_sprite_hull(drag.subject, hull);
        self.view.request_redraw();
    }

    /// The shell document's vertical scroll offsets, keyed by each scrolling pane container, so a
    /// hit-test lands on the visible row of a scrolled pane. Mirrors the offsets
    /// [`chrome_click`](Self::chrome_click) builds for its dispatch. (Row reorder B2 helper.)
    fn shell_scroll_offsets(
        &self,
        dom: &serval_scripted_dom::ScriptedDom,
    ) -> ScrollOffsets<NodeId> {
        let mut offsets = ScrollOffsets::<NodeId>::default();
        let root = dom.document();
        for (class, scroll) in [
            ("roster", self.view.roster_scroll),
            ("apparatus", self.view.apparatus_scroll),
            ("steward", self.view.steward_scroll),
            ("inspector", self.view.inspector_scroll),
            ("trail", self.view.trail_scroll),
            ("settings-pane-body", self.view.settings_scroll),
        ] {
            if let Some(node) = crate::first_with_class(dom, root, class) {
                offsets.insert(node, (0.0, scroll));
            }
        }
        offsets
    }

    /// Try to begin a row-reorder drag from a left press at `(x, y)`: if the press landed on a
    /// reorderable row's **drag grip** (`app-reorder-grip`), arm the drag with that row's
    /// `data-reorder-id` and return `true` (the caller consumes the press); otherwise `false`
    /// (it falls through to the normal pane click, so the label / ▲ / ▼ controls still work).
    /// Serval has no native DOM pointer-drag, so the host drives it from the cursor — the swatch
    /// editor's "handle press → drag → mutate" pattern, generalized to a list row. (Command
    /// registry B2 — drag reorder.)
    fn try_begin_row_reorder(&mut self, x: f32, y: f32) -> bool {
        let Some(id) = self.row_reorder_grip_at(x, y) else {
            return false;
        };
        self.view.row_reorder_drag = Some(crate::window_view::RowReorderDrag {
            id: id.clone(),
            origin: (x, y),
            moved: false,
            target: Some(id),
        });
        self.view.request_redraw();
        true
    }

    /// The `data-reorder-id` of the reorderable row whose **drag grip** is under `(x, y)`, if any.
    /// Walks up the chrome hit chain: a grip (`app-reorder-grip`) must be in the chain, and the
    /// row above it carries the id. `None` when the press is not on a grip. (Row reorder B2.)
    fn row_reorder_grip_at(&self, x: f32, y: f32) -> Option<String> {
        let session = self.view.chrome_session.as_ref()?;
        let dom = self.view.dom.borrow();
        let offsets = self.shell_scroll_offsets(&dom);
        let mut node = session.hit_test(&dom, x, y, &offsets)?;
        let mut on_grip = false;
        loop {
            if crate::has_class(&dom, node, "app-reorder-grip") {
                on_grip = true;
            }
            if on_grip {
                if let Some(id) = dom
                    .attributes(node)
                    .find(|a| a.name.local.as_ref() == "data-reorder-id")
                    .map(|a| a.value.to_string())
                {
                    return Some(id);
                }
            }
            node = dom.parent(node)?;
        }
    }

    /// The `data-reorder-id` of the reorderable row under `(x, y)`, if any — the drop target
    /// while a row-reorder drag is in flight. Walks up to the first element carrying the id.
    /// (Row reorder B2.)
    fn row_reorder_id_at(&self, x: f32, y: f32) -> Option<String> {
        let session = self.view.chrome_session.as_ref()?;
        let dom = self.view.dom.borrow();
        let offsets = self.shell_scroll_offsets(&dom);
        let mut node = session.hit_test(&dom, x, y, &offsets)?;
        loop {
            if let Some(id) = dom
                .attributes(node)
                .find(|a| a.name.local.as_ref() == "data-reorder-id")
                .map(|a| a.value.to_string())
            {
                return Some(id);
            }
            node = dom.parent(node)?;
        }
    }

    /// Advance an in-progress row-reorder drag: update the drop target to the row under the
    /// cursor and arm the movement flag once the pointer leaves the press point. The view dims
    /// the dragged row and marks the drop target on the next frame. (Command registry B2.)
    pub(super) fn drag_row_reorder(&mut self, x: f32, y: f32) {
        let target = self.row_reorder_id_at(x, y);
        let Some(drag) = self.view.row_reorder_drag.as_mut() else {
            return;
        };
        if !drag.moved && (x - drag.origin.0).hypot(y - drag.origin.1) > 4.0 {
            drag.moved = true;
        }
        drag.target = target;
        self.view.request_redraw();
    }

    /// Route an orrery-area wheel through the document: dispatch it to the orrery pane element
    /// (the runner resolves the wheel target by walking ancestors from the hit node, so a wheel
    /// over a card or the scene resolves to the orrery element), whose `on_wheel` queues the
    /// delta; then drain it into gyre's pan / Ctrl-zoom. Returns whether gyre consumed it. The
    /// host complement of the orrery pane element's `on_wheel`. (cond 5 input bridge.)
    pub(super) fn orrery_wheel_through_document(
        &mut self,
        gid: frame::GraphId,
        cx: f32,
        cy: f32,
        dx: f32,
        dy: f32,
    ) -> bool {
        let target = {
            let dom = self.view.dom.borrow();
            let offsets = ScrollOffsets::<NodeId>::default();
            self.view
                .chrome_session
                .as_ref()
                .and_then(|s| s.hit_test(&dom, cx, cy, &offsets))
                .and_then(|hit| self.view.runner.wheel_target(hit))
        };
        if let Some(node) = target {
            // The orrery element's handler reads only `delta`; `local` / `size` are unused here.
            let event =
                xilem_serval::WheelEvent { delta: (dx, dy), local: (0.0, 0.0), size: (0.0, 0.0) };
            self.view.runner.dispatch_wheel(node, event);
        }
        match self.view.take_orrery_wheel() {
            Some((wdx, wdy)) => self.pane_orrery_mut(gid).wheel(wdx, wdy),
            None => false,
        }
    }

    /// Apply the activation keys the folded list panes' button handlers queued. The
    /// apparatus + trail panes are in the shell document, so their button clicks arrive
    /// through `chrome_click` -> `chrome_activate`; this drains + routes them (apparatus:
    /// theme / engine / physics; trail: recover a removed node). The display-only inspector
    /// + steward panes queue nothing. Replaces the old per-pane branches in
    /// `on_mouse_input`. (Phase 1, step 2.)
    pub(super) fn drain_list_pane_activations(&mut self) {
        use crate::window_view::ShellListPane::{Alembic, Apparatus, Trail};
        for key in self.view.take_list_pane_activations(Apparatus) {
            self.apply_pelt_activation(&key);
        }
        for key in self.view.take_list_pane_activations(Trail) {
            if let Some(id) = key.strip_prefix("recover:") {
                self.recover_deleted_node(id);
            }
        }
        // Alembic: a clicked engram row thaws that engram into an Orrery pane beside. Pooling a
        // fresh graph re-keys the orrery pool, which a WindowCtx can't do (it holds one orrery
        // borrowed out), so queue it as a ShellCommand the Shell drains — like OpenGraphBeside. (B2.)
        for key in self.view.take_list_pane_activations(Alembic) {
            if let Some(id) = key.strip_prefix("engram:open:") {
                self.commands
                    .push(super::ShellCommand::OpenEngramBeside(id.to_string()));
            }
        }
        // The settings tiles' `pelt/*` pages carry the same control keys as the apparatus, so
        // a page control drives the host identically; a spine entry navigates the tile's node
        // to the chosen page (`navigate_member` retargets its url, which the next frame's
        // settings dispatch re-resolves). (Settings lane P1.)
        for (_member, key) in self.view.take_settings_pane_keys() {
            // `node:<id>` facets controls carry the subject node in the key; the rest are the
            // pelt pages' shared keys (theme / engine toggle / physics / orrery). (Registry P3.)
            if let Some(facet) = key.strip_prefix("nodefacet:") {
                self.apply_node_facet_key(facet);
            } else {
                self.apply_pelt_activation(&key);
            }
        }
        for (member, url) in self.view.take_settings_pane_nav() {
            self.orrery_mut().navigate_member(member, &url);
            self.view.request_redraw();
        }
    }

    /// Apply a `pelt` settings activation key (a theme id, `engine:toggle:<id>`, or a
    /// `phys:damping:*` step). Shared by the apparatus pane and the settings lane's `pelt/*`
    /// pages, so a control drives the host the same wherever it is shown. (Settings lane P1.)
    fn apply_pelt_activation(&mut self, key: &str) {
        match key {
            "phys:damping:down" => self.adjust_physics_damping(-0.5),
            "phys:damping:up" => self.adjust_physics_damping(0.5),
            // The active-tab cap (migrated from the settings overlay): edit the chrome cap;
            // the per-frame `sync_settings` applies it to the actor pool + persists. (P2.)
            "tiles:cap:down" => self.view.chrome_update(Chrome::dec_tab_cap),
            "tiles:cap:up" => self.view.chrome_update(Chrome::inc_tab_cap),
            // The `pelt/orrery` scene toggles, driving the same methods the context menu does
            // (so the page and the menu stay one source of truth). (Settings lane P2b.)
            "orrery:sizebydegree" => self.toggle_orrery_size_by_degree(),
            "orrery:sizebyimportance" => self.toggle_orrery_size_by_importance(),
            "orrery:importance:degree" => {
                self.orrery_mut().set_importance_metric(orrery::ImportanceMetric::Degree);
                self.view.request_redraw();
            }
            "orrery:importance:betweenness" => {
                self.orrery_mut().set_importance_metric(orrery::ImportanceMetric::Betweenness);
                self.view.request_redraw();
            }
            "orrery:mirror" => self.toggle_mirror_tiles(),
            k if k.starts_with("orrery:layout:") => {
                self.set_orrery_layout(&k["orrery:layout:".len()..]);
            }
            k if k.starts_with("engine:toggle:") => {
                self.toggle_engine(&k["engine:toggle:".len()..]);
            }
            // DocumentScript capability cyclers (the `pelt/scripts` page): cycle
            // log/document/net through default → Allow → Prompt → Deny. (Tail 3.)
            k if k.starts_with("script:cap:") => {
                self.set_script_cap(&k["script:cap:".len()..]);
            }
            // Theme editor (T5): fork / remove / mode-toggle / per-seed HSL nudge.
            // These must precede the theme-id fallback so they aren't read as ids.
            "theme:fork" => self.fork_active_theme(),
            "theme:remove" => self.remove_active_user_theme(),
            "theme:mode" => self.toggle_active_theme_mode(),
            k if k.starts_with("theme:harmony:") => {
                self.set_active_harmony(&k["theme:harmony:".len()..]);
            }
            k if k.starts_with("theme:seed:") => {
                self.adjust_seed_from_key(&k["theme:seed:".len()..]);
            }
            // Document typography (the `pelt/reading` page): text size / line
            // spacing sliders, link-arrows toggle, font choice, reset.
            k if k.starts_with("doc:") => {
                self.apply_doc_style_key(&k["doc:".len()..]);
            }
            // The persona-configurable context menu (`pelt/menu`): add / remove a command, move it
            // up / down in the order, or reset to the registry default. Persists to the persona
            // store. (Command registry P4.)
            "menu:reset" => self.reset_menu_actions(),
            k if k.starts_with("menu:move:") => {
                let rest = &k["menu:move:".len()..];
                if let Some((id, dir)) = rest.rsplit_once(':') {
                    self.move_menu_action(id, dir == "up");
                }
            }
            k if k.starts_with("menu:toggle:") => {
                self.toggle_menu_action(&k["menu:toggle:".len()..]);
            }
            // The `pelt/scene` page: load / clear a physics backdrop scene, a whirlpool / fountain /
            // liquid / ambient effect, or flip the graph's tangibility, by flat name. (Scene settings.)
            k if k.starts_with("scene:") => {
                self.load_named_scene(&k["scene:".len()..]);
            }
            _ => self.set_theme(key),
        }
    }

    /// Route a `doc:*` typography key from the `pelt/reading` page: `size` /
    /// `linespacing` carry a `<i>:<count>` slider cell (fraction `i/count`);
    /// `bodyfont` / `monofont` carry a family name; `arrows` / `reset` are bare.
    fn apply_doc_style_key(&mut self, rest: &str) {
        let (head, tail) = rest.split_once(':').unwrap_or((rest, ""));
        match head {
            "size" => {
                if let Some(f) = slider_cell_fraction(tail) {
                    self.set_doc_text_size(f);
                }
            }
            "linespacing" => {
                if let Some(f) = slider_cell_fraction(tail) {
                    self.set_doc_line_spacing(f);
                }
            }
            "arrows" => self.toggle_doc_link_arrows(),
            "bodyfont" => self.set_doc_body_font(tail),
            "monofont" => self.set_doc_mono_font(tail),
            "reset" => self.reset_doc_style(),
            _ => {}
        }
    }

    /// Parse a `theme:seed:<seed>:<h|s|l>:<i>:<count>` slider-cell key and set
    /// that channel to the fraction `i/count` on the active user theme.
    fn adjust_seed_from_key(&mut self, rest: &str) {
        let parts: Vec<&str> = rest.split(':').collect();
        let [seed, channel, i, count] = parts[..] else {
            return;
        };
        let (Ok(i), Ok(count)) = (i.parse::<f64>(), count.parse::<f64>()) else {
            return;
        };
        if count <= 0.0 {
            return;
        }
        let ch = channel.chars().next().unwrap_or(' ');
        self.set_active_seed_channel(seed, ch, i / count);
    }

    /// Apply the roster-row intents the shell runner's dispatch queued. The roster is
    /// folded into the shell document, so its row clicks arrive through `chrome_click` ->
    /// `chrome_activate`; this drains + applies them (Shift = additive selection).
    /// Replaces the old roster-pane branch in `on_mouse_input`. (Phase 1.)
    pub(super) fn drain_roster_intents(&mut self) {
        let intents = self.view.take_roster_intents();
        if intents.is_empty() {
            return;
        }
        let additive = self.view.modifiers.shift;
        for intent in intents {
            match intent {
                crate::roster_view::RosterIntent::Select(member) => {
                    if additive {
                        self.orrery_mut().toggle_select_member(member);
                        self.view.request_redraw();
                    } else if let Some(url) = self
                        .orrery()
                        .graph()
                        .get_node_by_id(member)
                        .map(|(_, n)| n.url().to_string())
                    {
                        self.orrery_mut().select_by_url(&url);
                        self.view.request_redraw();
                    }
                }
                crate::roster_view::RosterIntent::SelectField(id) => {
                    if self.orrery_mut().center_on_field(id) {
                        self.view.request_redraw();
                    }
                }
                crate::roster_view::RosterIntent::ToggleFieldVisibility(id) => {
                    self.orrery_mut().toggle_field_visible(id);
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::AdjustFieldStrength(id, delta) => {
                    if let Some(current) = self.orrery().field_strength(id) {
                        let next = (current + delta).clamp(1000.0, 20000.0);
                        if (next - current).abs() > f32::EPSILON
                            && self.orrery_mut().set_field_strength(id, next)
                        {
                            self.save_session();
                            self.view.request_redraw();
                        }
                    }
                }
            }
        }
    }

    /// Route a left press in the workbench pane into the host-authoritative pelt shell:
    /// set its cursor (pane-local), press, and apply each emitted gesture to the
    /// `Workbench` — the authority. Marks a gesture in flight so subsequent moves feed
    /// the shell (a drag continues past the pane edge). The shell does the frame
    /// hit-testing (tab / divider / close) internally; tile-content link clicks were
    /// already resolved earlier (`tile_link_at`). Re-projection is on the next render
    /// (`Workbench::to_tile_tree`), so the shell stays a driven view. (Drag via TileEvents.)
    pub(super) fn workbench_pointer_down(&mut self, x: f32, y: f32) {
        let Some(wr) = self.workbench_leaf_rect() else { return };
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let events = {
            let Some(shell) = self.view.pelt_shell.as_mut() else { return };
            shell.pointer_move(lx, ly);
            shell.pointer_down();
            shell.take_events()
        };
        self.view.workbench_gesture = true;
        for event in events {
            self.apply_tile_event(event);
        }
        self.view.request_redraw();
    }

    /// Feed a pointer move to the shell while a workbench gesture is in flight (advances
    /// a divider resize / tab drag), applying any emitted gesture. (Drag via TileEvents.)
    pub(super) fn workbench_pointer_move(&mut self, x: f32, y: f32) {
        let Some(wr) = self.workbench_leaf_rect() else { return };
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let events = {
            let Some(shell) = self.view.pelt_shell.as_mut() else { return };
            shell.pointer_move(lx, ly);
            shell.take_events()
        };
        for event in events {
            self.apply_tile_event(event);
        }
        self.view.request_redraw();
    }

    /// End a workbench gesture: release the shell (resolving a tab drop / activate),
    /// apply the emitted gesture, and clear the in-flight flag. Returns whether the
    /// shell consumed the release (it emitted a gesture); `false` means the press was a
    /// tile-content click that should fall through to the link/card paths. (Drag via
    /// TileEvents.)
    pub(super) fn workbench_pointer_up(&mut self, x: f32, y: f32) -> bool {
        self.view.workbench_gesture = false;
        let Some(wr) = self.workbench_leaf_rect() else { return false };
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let events = {
            let Some(shell) = self.view.pelt_shell.as_mut() else { return false };
            shell.pointer_move(lx, ly);
            shell.pointer_up();
            shell.take_events()
        };
        let consumed = !events.is_empty();
        for event in events {
            self.apply_tile_event(event);
        }
        self.view.request_redraw();
        consumed
    }

    /// The workbench member a pelt [`TileId`](pelt_core::tile::TileId) addresses, keyed
    /// by the UUID's low 64 bits (the encoding `Workbench::to_tile_tree` mints in
    /// `render.rs`). `None` if no open member matches.
    pub(super) fn tile_member(
        &self,
        id: pelt_core::tile::TileId,
    ) -> Option<GraphMemberId> {
        self.view
            .workbench
            .open_members()
            .iter()
            .copied()
            .find(|m| m.as_u128() as u64 == id.0)
    }

    /// Apply one pelt-surface [`TileEvent`](pelt_core::tile::TileEvent) to the
    /// `Workbench` (the single tiling authority). The one seam every tile gesture
    /// funnels through: a tab activates / closes its member today; drag + divider
    /// resize fill the remaining arms once the surface's pointer state machine drives
    /// them (B3). `press` is the window-space press point for the host's transitional
    /// tab-drag candidate (a click sets it; a surface-emitted gesture passes `None`).
    /// Re-projection happens on the next render (`Workbench::to_tile_tree`), so the
    /// surface stays a driven view. (Tile-event seam.)
    pub(super) fn apply_tile_event(&mut self, event: pelt_core::tile::TileEvent) {
        use pelt_core::tile::{DropTarget, Edge, SplitAxis, TileEvent};
        match event {
            TileEvent::Activated(id) => {
                if let Some(m) = self.tile_member(id) {
                    self.view.workbench.activate(m);
                    self.view.focused_tile = Some(m);
                }
            }
            TileEvent::Closed(id) => {
                if let Some(m) = self.tile_member(id) {
                    self.view.workbench.close_tile(m);
                    self.shared.content.constellation.reap(m);
                    if self.view.workbench.open_members().is_empty() {
                        // Closing the last tile closes the workbench pane entirely
                        // (back to just the orrery). (Workbench-as-pane.)
                        self.close_workbench();
                    } else if self.view.focused_tile == Some(m) {
                        self.view.focused_tile =
                            self.view.workbench.open_members().first().copied();
                    }
                }
            }
            // A tab dropped past the slop: onto a tab bar (Stack) it merges into that
            // stack; onto another tile's content (Edge) it splits that pane on the
            // dropped edge. The drag itself + the drop-zone resolution live in the pelt
            // shell; here each resolved DropTarget maps to a Workbench mutation.
            TileEvent::Dragged { tile, to } => {
                let Some(dragged) = self.tile_member(tile) else {
                    return;
                };
                match to {
                    DropTarget::Stack { stack, .. } => {
                        // Any member in the target stack identifies the slot; insertion
                        // index is appended for now (DropTarget::Stack{index} is a
                        // follow-on for precise reorder).
                        let target_id = self
                            .view
                            .pelt_shell
                            .as_ref()
                            .and_then(|sh| member_at_path(sh.tree(), &stack.0));
                        let target = target_id.and_then(|tid| self.tile_member(tid));
                        if let Some(target) = target {
                            if self.view.workbench.move_to_slot_of(dragged, target) {
                                self.view.focused_tile = Some(dragged);
                            }
                        }
                    }
                    DropTarget::Edge { tile: target_id, edge } => {
                        let Some(target) = self.tile_member(target_id) else {
                            return;
                        };
                        let (axis, after) = match edge {
                            Edge::Left => (SplitAxis::Row, false),
                            Edge::Right => (SplitAxis::Row, true),
                            Edge::Top => (SplitAxis::Column, false),
                            Edge::Bottom => (SplitAxis::Column, true),
                        };
                        let moved = if target == dragged {
                            self.view.workbench.split_out(dragged, axis, after)
                        } else {
                            self.view.workbench.split_beside_axis(dragged, target, axis, after)
                        };
                        if moved {
                            self.view.focused_tile = Some(dragged);
                        }
                    }
                }
            }
            // A divider drag: reweight the addressed split. Path-addressed, so nested
            // splits resize too (the host divider path only reached the top level).
            TileEvent::DividerMoved { split, fractions } => {
                self.view.workbench.set_split_fractions(&split.0, &fractions);
            }
        }
    }

    /// Handle a pressed key. First the global chords (Ctrl+P palette, Ctrl+K comms,
    /// Ctrl+T workbench, …) and clipboard shortcuts; then the key routes to whichever
    /// field owns the caret — the palette query ([`on_palette_key`](Self::on_palette_key)),
    /// the comms compose box ([`on_comms_key`](Self::on_comms_key)), or the omnibar
    /// ([`on_omnibar_key`](Self::on_omnibar_key)) — each handler scoped to its own
    /// field. A key with no focused field is ignored.
    pub(super) fn on_key_pressed(&mut self, key: &WinitKey) {
        // Renaming a session captures the keyboard: type into the switcher label,
        // Enter commits, Escape cancels, Backspace deletes. (Host text path.)
        if self.view.renaming.is_some() {
            match key {
                WinitKey::Named(WinitNamedKey::Enter) => self.commit_rename(),
                WinitKey::Named(WinitNamedKey::Escape) => self.cancel_rename(),
                WinitKey::Named(WinitNamedKey::Backspace) => self.rename_backspace(),
                WinitKey::Character(s)
                    if !self.view.modifiers.ctrl
                        && !self.view.modifiers.meta
                        && !s.chars().any(char::is_control) =>
                {
                    self.rename_push(s.as_str());
                }
                _ => {}
            }
            return;
        }
        // Tagging a node captures the keyboard like renaming: type the tag, Enter
        // commits it onto the selection, Escape cancels, Backspace deletes. (Add-tag.)
        if self.view.tagging.is_some() {
            match key {
                WinitKey::Named(WinitNamedKey::Enter) => self.commit_tag(),
                WinitKey::Named(WinitNamedKey::Escape) => self.cancel_tag(),
                WinitKey::Named(WinitNamedKey::Backspace) => self.tag_backspace(),
                WinitKey::Character(s)
                    if !self.view.modifiers.ctrl
                        && !self.view.modifiers.meta
                        && !s.chars().any(char::is_control) =>
                {
                    self.tag_push(s.as_str());
                }
                _ => {}
            }
            return;
        }
        // An open context menu eats Escape to dismiss (other keys fall through).
        if self.view.chrome().context_menu.is_some()
            && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
        {
            self.close_context_menu();
            return;
        }
        // Escape closes an open object card (after a menu, which eats Escape first). A
        // click-away already closes it: an empty-canvas click clears the selection, and the
        // card drops once focus leaves its member. (Object card — explicit close.)
        if self.view.object_card.is_some()
            && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
        {
            self.view.object_card = None;
            self.view.request_redraw();
            return;
        }
        // F2 renames the focused pane's session (the switcher's keyboard rename
        // affordance; right-clicking a tile renames that one). (Host text path;
        // pane-as-unit — the focused pane's session, not a global active one.)
        if matches!(key, WinitKey::Named(WinitNamedKey::F2)) {
            if let Some((id, _)) = self.session_for_graph(self.view.focused_graph) {
                self.start_rename(id);
            }
            return;
        }
        // Ctrl+L focuses the address bar and selects its whole contents (browser
        // convention) so a new URL can be typed to replace the shown one, without the
        // mouse.
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("l"))
        {
            let omnibar = self.input_under_class("toolbar");
            self.view.runner.set_focus(omnibar);
            self.view.chrome_update(|c| c.omnibar.select_all());
            self.view.request_redraw();
            return;
        }
        // F5 reloads the focused node's page — re-fetch, bypassing the durable cache.
        if matches!(key, WinitKey::Named(WinitNamedKey::F5)) {
            self.retry_focused_content();
            return;
        }
        // Alt+Left / Alt+Right step the focused node's own history (browser
        // back/forward). Mirror the toolbar buttons: record the intent on the chrome,
        // then drain it this pass so the revealed page loads at once. A no-op when
        // nothing is focused or the node is at a history end.
        if self.view.modifiers.alt
            && matches!(key, WinitKey::Named(WinitNamedKey::ArrowLeft))
        {
            self.view.chrome_update(|c| c.history_step = Some(HistoryStep::Back));
            self.drain_history_step();
            return;
        }
        if self.view.modifiers.alt
            && matches!(key, WinitKey::Named(WinitNamedKey::ArrowRight))
        {
            self.view.chrome_update(|c| c.history_step = Some(HistoryStep::Forward));
            self.drain_history_step();
            return;
        }
        // Command palette: Ctrl+P. Ctrl+K toggles the comms pane (freed up for it).
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("p"))
        {
            self.toggle_palette();
            return;
        }
        // Ctrl+F opens / closes the find-in-page bar (HTML/serval lane).
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("f"))
        {
            self.toggle_find();
            return;
        }
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("k"))
        {
            self.view.chrome_update(Chrome::toggle_comms);
            self.drain_comms_intent();
            self.view.request_redraw();
            return;
        }
        // Ctrl+T toggles the tiled workbench (Tree projection) and the orrery
        // (Cartography projection) of the same graph.
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("t"))
        {
            self.toggle_workbench();
            return;
        }
        // Ctrl+B flags the focused node to keep working in the background (its
        // actor outlives the view); Ctrl+Backspace deletes the focused node from
        // the graph. Both are modifier-gated so they don't collide with omnibar
        // editing, and intercepted here before the keystroke reaches the field.
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("b"))
        {
            self.toggle_focus_background();
            return;
        }
        // Ctrl+R summons / closes the roster pane (the graph's node list); Ctrl+,
        // the apparatus pane (settings + system); both split beside the graph pane.
        // Ctrl+M maximizes the pane under the cursor to full-screen and back.
        // (Frame tree, F1 / apparatus.)
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("r"))
        {
            self.toggle_pane(PaneContent::Roster);
            return;
        }
        if self.view.modifiers.ctrl && matches!(key, WinitKey::Character(s) if s.as_str() == ",") {
            self.toggle_pane(PaneContent::Apparatus);
            return;
        }
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("g"))
        {
            self.toggle_pane(PaneContent::Gloss);
            return;
        }
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("m"))
        {
            self.toggle_maximize();
            return;
        }
        // Ctrl+W closes the focused graph pane when a second graph-pane is open (the
        // dismiss for an open-beside graph). A no-op with a single graph view, so it
        // never closes your last graph. (Window composition — pane-as-unit.)
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("w"))
        {
            self.close_focused_graph_pane();
            return;
        }
        // Cmd/Ctrl+Shift+N opens a new OS window over the same shared session (a
        // second view). A per-window handler can't create a window itself (no event
        // loop, no registry access), so it queues a `SpawnWindow` the shell applies in
        // `about_to_wait`. Checked before the unshifted Ctrl+N (new session) so the
        // shift distinguishes the two. (Multi-window MW3.)
        if (self.view.modifiers.ctrl || self.view.modifiers.meta)
            && self.view.modifiers.shift
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("n"))
        {
            self.commands.push(super::ShellCommand::SpawnWindow);
            return;
        }
        // Ctrl+N mints a new graph session and switches to it; Ctrl+PageDown /
        // Ctrl+PageUp cycle through the open sessions (the interim keyboard switch
        // until the F2.3 shellbar switcher). (Multi-graph MG2 / MG3.)
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("n"))
        {
            self.commands.push(super::ShellCommand::CreateSession);
            return;
        }
        if self.view.modifiers.ctrl && matches!(key, WinitKey::Named(WinitNamedKey::PageDown)) {
            self.commands.push(super::ShellCommand::CycleSession(true));
            return;
        }
        if self.view.modifiers.ctrl && matches!(key, WinitKey::Named(WinitNamedKey::PageUp)) {
            self.commands.push(super::ShellCommand::CycleSession(false));
            return;
        }
        if self.view.modifiers.ctrl && matches!(key, WinitKey::Named(WinitNamedKey::Backspace)) {
            self.delete_focused_node();
            return;
        }
        if self.handle_clipboard_shortcut(key) {
            return;
        }
        if self.view.chrome().palette_open {
            self.on_palette_key(key);
            return;
        }
        // An open context menu owns the keyboard (arrow nav + Enter/Escape), like the palette.
        if self.view.chrome().context_menu.is_some() {
            self.on_context_menu_key(key);
            return;
        }
        if self.view.chrome().find_open {
            self.on_find_key(key);
            return;
        }
        // Route the key to whichever field owns the caret. Each focusable field has
        // its own handler, invoked only while it holds focus — so no field's logic
        // (the omnibar's suggestion refresh, its Enter-submits) leaks onto another.
        // A key with no focused field is ignored: the chrome owns the keyboard; the
        // orrery / workbench are pointer-driven.
        if self.comms_input_focused() {
            self.on_comms_key(key);
        } else if self.omnibar_focused() {
            self.on_omnibar_key(key);
        } else if self.view.runner.focus().is_some()
            || matches!(key, WinitKey::Named(WinitNamedKey::Tab))
        {
            // A non-field focusable holds focus (a pane button/row reached via Tab), or Tab
            // starts the traversal from nothing: route to the runner, which applies the
            // keyboard defaults (Tab/Shift+Tab traversal, Enter/Space activation) over the
            // focusable set, so focus moves on through the chrome and folded panes and the
            // focused control activates. (Phase 1, step 3c.)
            if let Some(key_event) = key_event_from_winit(key, self.view.modifiers) {
                self.view.runner.dispatch_key(key_event);
                // Enter/Space synthesizes a click on the focused control, queuing its intent
                // the same way a pointer click does, so drain + apply it. (Phase 1, step 3c.)
                self.drain_chrome_intents();
                self.view.request_redraw();
            }
        } else {
            // No chrome field or focusable element holds focus: graph-level keys act on
            // the orrery selection.
            self.on_graph_key(key);
        }
    }

    /// Keys handled when no chrome field is focused (the graph has the keyboard).
    /// `Delete` / `Backspace` removes the selection: a selected edge's relation is
    /// retracted; otherwise the focused node is deleted (the same as the legacy
    /// Ctrl+Backspace, now reachable with a bare `Delete`).
    fn on_graph_key(&mut self, key: &WinitKey) {
        if matches!(
            key,
            WinitKey::Named(WinitNamedKey::Delete) | WinitKey::Named(WinitNamedKey::Backspace)
        ) {
            if self.orrery().has_selected_edges() {
                if self.orrery_mut().retract_selected_relation() > 0 {
                    self.save_session();
                    self.view.request_redraw();
                }
            } else if self.focused_member().is_some() {
                self.delete_focused_node();
            }
        }
        // Space pauses / resumes the layout physics, so you can freeze the graph
        // mid-settle (or let a field's pull keep running). (Physics pause.)
        if matches!(key, WinitKey::Named(WinitNamedKey::Space)) {
            self.orrery_mut().toggle_physics_paused();
            self.view.request_redraw();
        }
    }

    /// Route a key to the focused omnibar: Enter submits (Ctrl/Cmd-Enter opens the
    /// address as a *new* node, a browsing surface linked from the focused one),
    /// Arrow Up/Down + Escape drive the suggestions dropdown, anything else edits
    /// the address and regenerates suggestions. Called only while the omnibar holds
    /// the caret, so it refreshes unconditionally — the source of the old "any
    /// focused field is the omnibar" bug, now scoped to the omnibar's own handler.
    fn on_omnibar_key(&mut self, key: &WinitKey) {
        let suggestions_open = !self.view.chrome().suggest.is_empty();
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                // A `>`-prefixed expression with no highlighted suggestion is a
                // command, not an address: route it to the command shell instead of
                // navigating. (Omnibar command shell, S3.)
                let chrome = self.view.chrome();
                let command_expr = if chrome.suggest_active.is_none() {
                    match nav::classify(chrome.omnibar.text()) {
                        nav::NavTarget::Command(expr) => Some(expr),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(expr) = command_expr {
                    self.submit_omnibar_command(&expr);
                    return;
                }
                let as_new_node = self.view.modifiers.ctrl || self.view.modifiers.meta;
                self.view.chrome_update(move |c| {
                    submit_omnibar(c);
                    c.open_as_new_node = as_new_node;
                });
                tracing::info!(
                    location = %self.view.chrome().toolbar.editable.location,
                    as_new_node,
                    "omnibar submit"
                );
                self.sync_orrery();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowDown) if suggestions_open => {
                self.view.chrome_update(|c| c.step_suggestion(1));
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) if suggestions_open => {
                self.view.chrome_update(|c| c.step_suggestion(-1));
                self.view.request_redraw();
            }
            // Accept the inline ghost completion. Right arrow only at the buffer
            // end (otherwise it is an ordinary caret move); Tab whenever a ghost is
            // present. Either splices `>ros` + "ter" → `>roster`; Enter still
            // evaluates only what is in the buffer, never the ghost.
            WinitKey::Named(WinitNamedKey::ArrowRight)
                if self.omnibar_ghost_acceptable(true) =>
            {
                self.accept_omnibar_ghost();
            }
            WinitKey::Named(WinitNamedKey::Tab) if self.omnibar_ghost_acceptable(false) => {
                self.accept_omnibar_ghost();
            }
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.view.modifiers) {
                    self.view.runner.dispatch_key(key_event);
                    self.view.chrome_update(Chrome::refresh_suggestions);
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Whether the omnibar has a ghost completion to accept. `at_end_only` gates
    /// the Right-arrow case to a caret at the buffer end (so a mid-text → is still
    /// a plain caret move); Tab passes `false` (it has no other omnibar meaning).
    fn omnibar_ghost_acceptable(&self, at_end_only: bool) -> bool {
        let omnibar = &self.view.chrome().omnibar;
        !omnibar.ghost().is_empty()
            && (!at_end_only || omnibar.caret() == omnibar.text().chars().count())
    }

    /// Splice the omnibar's ghost completion into the buffer and recompute (which
    /// clears the now-complete ghost and refreshes suggestions).
    fn accept_omnibar_ghost(&mut self) {
        self.view.chrome_update(|c| {
            c.omnibar.accept_ghost();
            c.refresh_suggestions();
        });
        self.view.request_redraw();
    }

    /// Ctrl/Cmd + C / X / V on the active text editor — the command-palette query
    /// when the palette is open, else the omnibar (only while it holds the caret).
    /// Returns `true` if it consumed the key, so the shortcut never also lands as a
    /// typed character. No-op without the modifier, an editor, or a clipboard.
    fn handle_clipboard_shortcut(&mut self, key: &WinitKey) -> bool {
        if !(self.view.modifiers.ctrl || self.view.modifiers.meta) {
            return false;
        }
        let WinitKey::Character(s) = key else {
            return false;
        };
        let palette = self.view.chrome().palette_open;
        // The clipboard shortcuts act on the palette query or the omnibar; when
        // another field (the comms compose box) holds the caret, let the key fall
        // through to that field's handler rather than editing the omnibar.
        if !palette && !self.omnibar_focused() {
            return false;
        }
        match s.as_str() {
            "c" => self.clipboard_copy(palette),
            "x" => self.clipboard_cut(palette),
            "v" => self.clipboard_paste(palette),
            _ => return false,
        }
        true
    }

    /// Copy the active editor's selection to the system clipboard.
    fn clipboard_copy(&mut self, palette: bool) {
        let text = {
            let c = self.view.chrome();
            if palette {
                c.palette_input.selected_text()
            } else {
                c.omnibar.selected_text()
            }
            .to_string()
        };
        if text.is_empty() {
            return;
        }
        if let Some(cb) = self.clipboard.as_mut() {
            let _ = cb.set_text(text);
        }
    }

    /// Cut: copy the selection, then delete it. No-op without a selection.
    fn clipboard_cut(&mut self, palette: bool) {
        let has = {
            let c = self.view.chrome();
            if palette {
                c.palette_input.has_selection()
            } else {
                c.omnibar.has_selection()
            }
        };
        if !has {
            return;
        }
        self.clipboard_copy(palette);
        self.view.chrome_update(|c| {
            if palette {
                c.palette_input.backspace();
            } else {
                c.omnibar.backspace();
            }
        });
        self.after_field_edit(palette);
    }

    /// Paste: insert the clipboard text at the caret, replacing any selection.
    fn clipboard_paste(&mut self, palette: bool) {
        let Some(text) = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.view.chrome_update(|c| {
            if palette {
                c.palette_input.insert_str(&text);
            } else {
                c.omnibar.insert_str(&text);
            }
        });
        self.after_field_edit(palette);
    }

    /// Mirror an edit of the active field back into its session state, exactly as a
    /// typed keystroke does (palette query sync, or omnibar suggestions), then redraw.
    fn after_field_edit(&mut self, palette: bool) {
        if palette {
            self.view.chrome_update(Chrome::sync_palette_query);
        } else {
            self.view.chrome_update(Chrome::refresh_suggestions);
        }
        self.view.request_redraw();
    }

    /// Route a key to the open command palette: Enter runs the selection, Arrow
    /// Up/Down step it, Escape closes, anything else edits the query.
    pub(super) fn on_palette_key(&mut self, key: &WinitKey) {
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                self.view.chrome_update(Chrome::run_palette_selection);
                self.drain_pending_connect();
                self.drain_pending_command();
                self.drain_comms_intent();
                // A palette-invoked context action applies to the live selection (registry P2):
                // seed `context_set` from it, then drain the context action — the same pair
                // `drain_chrome_intents` runs for the click path.
                self.drain_palette_context_action();
                self.drain_pending_context();
                self.drain_history_step();
                self.drain_physics_toggle();
                self.sync_orrery();
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.view.chrome_update(Chrome::close_palette);
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowDown) => {
                self.view.chrome_update(|c| c.step_palette(1));
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) => {
                self.view.chrome_update(|c| c.step_palette(-1));
                self.view.request_redraw();
            }
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.view.modifiers) {
                    self.view.runner.dispatch_key(key_event);
                    self.view.chrome_update(Chrome::sync_palette_query);
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Keyboard navigation for the open context menu, mirroring the palette: Down / Up move the
    /// highlight (wrapping), Enter runs the highlighted row (or the first when none is highlighted),
    /// Escape closes. Other keys are swallowed so they don't leak to the canvas while the menu is
    /// up. The chosen action is drained immediately, the same pair the click path runs. (Context-
    /// menu keyboard nav.)
    pub(super) fn on_context_menu_key(&mut self, key: &WinitKey) {
        match key {
            WinitKey::Named(WinitNamedKey::ArrowDown) => {
                self.view.chrome_update(|c| c.step_context_menu(1));
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) => {
                self.view.chrome_update(|c| c.step_context_menu(-1));
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Enter) => {
                self.view.chrome_update(Chrome::run_context_selection);
                self.drain_pending_context();
                self.drain_pending_command();
                self.sync_orrery();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.view.chrome_update(Chrome::close_context_menu);
                self.view.request_redraw();
            }
            // Typing searches the menu (the cursor palette): edit its query buffer and rebuild the
            // rows. Backspace deletes; Space (a named key) inserts a space. (Searchable context menu S1.)
            WinitKey::Named(WinitNamedKey::Backspace) => {
                self.view.chrome_update(|c| {
                    if let Some(menu) = &mut c.context_menu {
                        menu.query.pop();
                    }
                });
                self.rebuild_context_menu();
            }
            WinitKey::Named(WinitNamedKey::Space) => {
                self.view.chrome_update(|c| {
                    if let Some(menu) = &mut c.context_menu {
                        menu.query.push(' ');
                    }
                });
                self.rebuild_context_menu();
            }
            WinitKey::Character(s) => {
                let s = s.to_string();
                self.view.chrome_update(move |c| {
                    if let Some(menu) = &mut c.context_menu {
                        menu.query.push_str(&s);
                    }
                });
                self.rebuild_context_menu();
            }
            _ => {}
        }
    }

    /// Route a key to the focused comms compose field: Enter sends the draft,
    /// Escape blurs back to the omnibar, anything else edits the field — never
    /// touching the omnibar suggestions (the dropdown stays closed while chatting).
    pub(super) fn on_comms_key(&mut self, key: &WinitKey) {
        let composing_new = self.view.chrome().comms.new_message_open();
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                // Enter sends: the compose-new form when it's open, else the reply.
                if composing_new {
                    self.view.chrome_update(Chrome::send_new_message);
                } else {
                    self.view.chrome_update(Chrome::send_comms);
                }
                self.drain_comms_intent();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                // Escape closes the compose-new form, else blurs to the omnibar.
                if composing_new {
                    self.view.chrome_update(Chrome::close_new_message);
                }
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.view.modifiers) {
                    self.view.runner.dispatch_key(key_event);
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Whether any comms-pane `<input>` (the reply compose box, or the compose-new
    /// recipient / body) currently holds the keyboard caret.
    fn comms_input_focused(&self) -> bool {
        let focus = self.view.runner.focus();
        focus.is_some()
            && [
                self.input_under_class("comms-compose"),
                self.input_under_class("comms-new-to"),
                self.input_under_class("comms-new-body"),
            ]
            .contains(&focus)
    }

    /// Whether the omnibar `<input>` currently holds the keyboard caret.
    fn omnibar_focused(&self) -> bool {
        let focus = self.view.runner.focus();
        focus.is_some() && focus == self.input_under_class("toolbar")
    }

    /// The text field whose caret the host paints, by the focused DOM `node`: a
    /// comms compose / new-message field, the palette query, else the omnibar.
    /// Whether `node` is a text-editable field (an `<input>`: the omnibar, palette query, find
    /// bar, comms fields). The caret + selection overlay paints only for these; a focused button
    /// or orrery card (focusable since Phase 1 / 2a) rings but shows no editing caret. (Phase 2a.)
    pub(super) fn is_text_input(&self, node: NodeId) -> bool {
        let dom = self.view.dom.borrow();
        dom.element_name(node).map(|q| q.local.as_ref()) == Some("input")
    }

    pub(super) fn caret_field(&self, node: NodeId) -> &xilem_serval::TextInput {
        let focus = Some(node);
        let c = self.view.chrome();
        if focus == self.input_under_class("comms-new-to") {
            &c.comms_new_to
        } else if focus == self.input_under_class("comms-new-body") {
            &c.comms_new_body
        } else if focus == self.input_under_class("comms-compose") {
            &c.comms_draft
        } else if c.palette_open {
            &c.palette_input
        } else {
            &c.omnibar
        }
    }

    /// Toggle the palette and move focus to match: into the palette query when
    /// it opens, back to the omnibar when it closes.
    pub(super) fn toggle_palette(&mut self) {
        self.view.chrome_update(Chrome::toggle_palette);
        if self.view.chrome().palette_open {
            if let Some(node) = self.input_under_class("palette") {
                self.view.runner.set_focus(Some(node));
            }
        } else {
            self.focus_after_palette_close();
        }
        self.view.request_redraw();
    }

    /// Restore focus to the omnibar after the palette closes (so keyboard use
    /// continues there).
    pub(super) fn focus_after_palette_close(&mut self) {
        let omnibar = self.input_under_class("toolbar");
        self.view.runner.set_focus(omnibar);
    }

    /// Toggle the find bar and move focus to match: into the find query when it
    /// opens, back to the omnibar when it closes (clearing the actor's matches).
    pub(super) fn toggle_find(&mut self) {
        self.view.chrome_update(Chrome::toggle_find);
        if self.view.chrome().find_open {
            if let Some(node) = self.input_under_class("find-bar") {
                self.view.runner.set_focus(Some(node));
            }
        } else {
            self.clear_find_matches();
            self.focus_after_palette_close();
        }
        self.view.request_redraw();
    }

    /// Route a key to the open find bar: Enter = next match, Shift+Enter = prev,
    /// Escape closes, anything else edits the query and re-runs the search.
    pub(super) fn on_find_key(&mut self, key: &WinitKey) {
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                let delta = if self.view.modifiers.shift { -1 } else { 1 };
                self.step_find_match(delta);
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.view.chrome_update(Chrome::close_find);
                self.clear_find_matches();
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.view.modifiers) {
                    self.view.runner.dispatch_key(key_event);
                    self.submit_find_query();
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Push the edited find query to the content actor for the focused node, and
    /// reset the active match to the first. (The actor dedups and clears on empty.)
    fn submit_find_query(&mut self) {
        let query = self.view.chrome().find_input.text().to_string();
        self.recompute_find(&query);
        self.view.chrome_update(|c| c.find_active = 0);
    }

    /// Clear the find matches (an empty query / bar closed), so highlights vanish.
    fn clear_find_matches(&mut self) {
        self.clear_find();
    }

    /// Cycle the active match by `delta` (wrapping within the live count) and, if the
    /// newly-active match falls outside the focused card / tile's visible band, scroll
    /// it into view (placed ~20% down). The highlight overlay then tints it. (Find S2.)
    fn step_find_match(&mut self, delta: isize) {
        let Some(member) = self.focused_member() else {
            return;
        };
        let matches = self.find_matches_for(member);
        let count = matches.len();
        if count == 0 {
            return;
        }
        let cur = self.view.chrome().find_active.min(count - 1) as isize;
        let next = (cur + delta).rem_euclid(count as isize) as usize;
        // The active match's vertical extent (full-document px), for the auto-scroll.
        let (match_top, match_bot) = matches[next]
            .iter()
            .fold((f32::MAX, f32::MIN), |(t, b), r| (t.min(r[1]), b.max(r[3])));
        self.view.chrome_update(move |c| c.find_active = next);
        if let Some((vis_h, content_h)) = self.find_member_viewport(member) {
            let scroll = self.view.scroll.get(&member).copied().unwrap_or(0.0);
            let out_of_view = match_top < scroll || match_bot > scroll + vis_h;
            if out_of_view && match_top.is_finite() {
                let target =
                    (match_top - vis_h * 0.2).clamp(0.0, (content_h - vis_h).max(0.0));
                self.view.scroll.insert(member, target);
            }
        }
        self.view.request_redraw();
    }

    /// The focused find surface's `(visible_height, content_height)` in px — the live
    /// card or workbench tile showing `member`, 1:1 so the dest height is the visible
    /// document window. Drives the find auto-scroll. (Find S2.)
    fn find_member_viewport(&self, member: GraphMemberId) -> Option<(f32, f32)> {
        let rect = self
            .view
            .content_rects
            .iter()
            .chain(self.view.tile_rects.iter())
            .find(|(m, _)| *m == member)
            .map(|(_, r)| *r)?;
        let vis_h = (rect[3] - rect[1]).max(1.0);
        let content_h =
            (self.shared.content.constellation.content_height(member) as f32).max(vis_h);
        Some((vis_h, content_h))
    }

    /// The first `<input>` under the first element carrying CSS class `class`
    /// (the omnibar under `.toolbar`, the query field under `.palette`).
    pub(super) fn input_under_class(&self, class: &str) -> Option<NodeId> {
        let dom = self.view.dom.borrow();
        let container = first_with_class(&dom, dom.document(), class)?;
        first_tag(&dom, container, "input")
    }
}

#[cfg(test)]
mod tests {
    use super::{nearest_hull_edge, point_segment_dist};

    #[test]
    fn point_segment_dist_handles_interior_and_endpoints() {
        // A point above the middle of a horizontal segment: distance is the vertical gap.
        assert!((point_segment_dist(0.5, 0.3, 0.0, 0.0, 1.0, 0.0) - 0.3).abs() < 1e-5);
        // Past the segment's end: distance is to the nearer endpoint, not the infinite line.
        assert!((point_segment_dist(2.0, 0.0, 0.0, 0.0, 1.0, 0.0) - 1.0).abs() < 1e-5);
        // A degenerate (zero-length) segment is just the distance to the point.
        assert!((point_segment_dist(0.0, 1.0, 0.0, 0.0, 0.0, 0.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn nearest_hull_edge_picks_the_clicked_edge_within_tolerance() {
        // A unit square hull (CCW): edges bottom(0), right(1), top(2), left(3, wraps 3->0).
        let hull = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        // A click just outside the bottom edge picks edge 0.
        assert_eq!(nearest_hull_edge(&hull, 0.5, -0.05, 0.1), Some(0));
        // A click near the right edge picks edge 1.
        assert_eq!(nearest_hull_edge(&hull, 1.05, 0.5, 0.1), Some(1));
        // The wrapping left edge (last vertex -> first) is index 3.
        assert_eq!(nearest_hull_edge(&hull, -0.05, 0.5, 0.1), Some(3));
        // A click far from every edge (well inside) is beyond tolerance: no edge.
        assert_eq!(nearest_hull_edge(&hull, 0.5, 0.5, 0.1), None);
    }
}
