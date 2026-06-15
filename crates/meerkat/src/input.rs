/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mouse, keyboard, and palette input handlers for [`Shell`](super::Shell). Factored
//! from `main.rs` to keep files under the workspace 600-LOC ceiling.

use std::time::{Duration, Instant};

use forme::GraphMemberId;
use layout_dom_api::LayoutDom;
use meerkat::{Chrome, nav, submit_omnibar};
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
    FALLBACK_TOOLBAR_H, WindowCtx, class_bottom_in, first_tag, first_with_class, has_class,
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
                // anywhere else just dismisses it.
                if self.view.runner.state().context_menu.is_some() {
                    if button == MouseButton::Left {
                        self.chrome_click(x, y);
                    }
                    if self.view.runner.state().context_menu.is_some() {
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
                    if self.view.runner.focus().is_some() || !self.view.runner.state().suggest.is_empty() {
                        self.view.runner.set_focus(None);
                        self.view.runner.update(Chrome::close_suggestions);
                        self.view.request_redraw();
                    }
                    // A press on a frame divider starts a pane-resize drag. (F1.)
                    if button == MouseButton::Left {
                        if let Some((path, parent, axis)) = self.frame_divider_at(x, y) {
                            self.view.frame_divider_drag = Some((path, parent, axis));
                            return;
                        }
                    }
                    // The roster pane consumes the press: a left click focuses the
                    // clicked node's row (shared selection with the orrery). (F1.)
                    if let Some(rrect) = self.roster_leaf_rect() {
                        if x >= rrect[0] && x < rrect[2] && y >= rrect[1] && y < rrect[3] {
                            if button == MouseButton::Left {
                                // Route through the roster runner: hit-test its DOM,
                                // dispatch the click (the row handler queues a Select),
                                // then apply each queued selection. (P2 companion.)
                                let local = (x - rrect[0], y - rrect[1]);
                                if let Some(node) = self.view.roster_pane.hit_test(
                                    local.0,
                                    local.1,
                                    self.view.roster_scroll,
                                ) {
                                    self.view
                                        .roster_pane
                                        .dispatch_click(node, PointerClick::at(local));
                                    // Shift makes a roster click additive (build a
                                    // multi-selection); without it, the click replaces
                                    // the selection. The click event carries no
                                    // modifier, so the host decides from its live
                                    // modifier state (as the canvas does). This applies
                                    // to edge rows too: a plain edge click traverses to
                                    // the other endpoint, Shift+edge additively selects
                                    // it — Shift = additive everywhere in the roster.
                                    let additive = self.view.modifiers.shift;
                                    for intent in self.view.roster_pane.take_intents() {
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
                                                // Click a field row: center the canvas on it.
                                                if self.orrery_mut().center_on_field(id) {
                                                    self.view.request_redraw();
                                                }
                                            }
                                            crate::roster_view::RosterIntent::ToggleFieldVisibility(id) => {
                                                // The field row's hide/show toggle.
                                                self.orrery_mut().toggle_field_visible(id);
                                                self.view.request_redraw();
                                            }
                                            crate::roster_view::RosterIntent::AdjustFieldStrength(id, delta) => {
                                                // − / + the field's coupling strength,
                                                // clamped to a sane range. Strength is
                                                // graph truth, so persist on a change.
                                                // (Field regions — strength tuning.)
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
                            }
                            return;
                        }
                    }
                    // The apparatus pane consumes the press: a left click routes
                    // through the apparatus runner — hit-test its DOM, dispatch the
                    // click (a theme button queues its id), then switch to each
                    // drained theme id. (Apparatus; P2 companion.)
                    if let Some(arect) = self.apparatus_leaf_rect() {
                        if x >= arect[0] && x < arect[2] && y >= arect[1] && y < arect[3] {
                            if button == MouseButton::Left {
                                let local = (x - arect[0], y - arect[1]);
                                if let Some(node) =
                                    self.view.apparatus_pane.hit_test(local.0, local.1, self.view.apparatus_scroll)
                                {
                                    self.view
                                        .apparatus_pane
                                        .dispatch_click(node, PointerClick::at(local));
                                    // An apparatus button key routes by prefix: the
                                    // Physics −/+ step the damping; `engine:toggle:<id>`
                                    // flips an engine's activation; anything else is a
                                    // theme id. (Physics / engine-picker settings.)
                                    for key in self.view.apparatus_pane.take_activations() {
                                        match key.as_str() {
                                            "phys:damping:down" => self.adjust_physics_damping(-0.5),
                                            "phys:damping:up" => self.adjust_physics_damping(0.5),
                                            k if k.starts_with("engine:toggle:") => {
                                                self.toggle_engine(&k["engine:toggle:".len()..]);
                                            }
                                            _ => self.set_theme(&key),
                                        }
                                    }
                                }
                            }
                            return;
                        }
                    }
                    // The gloss pane consumes the press: a left click on a minimap
                    // node focuses it (shared selection with the orrery). (Gloss.)
                    if let Some(grect) = self.gloss_leaf_rect() {
                        if x >= grect[0] && x < grect[2] && y >= grect[1] && y < grect[3] {
                            if button == MouseButton::Left {
                                if let Some(member) = self.gloss_node_at(x, y) {
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
                    // The comms pane is chrome-rendered into its leaf, so route a
                    // press there to the chrome — its hit-test fires the comms close
                    // X, conversation rows, and compose-field focus. (Comms pane.)
                    if let Some(cr) = self.comms_leaf_rect() {
                        if x >= cr[0] && x < cr[2] && y >= cr[1] && y < cr[3] {
                            if button == MouseButton::Left {
                                self.chrome_click(x, y);
                            }
                            return;
                        }
                    }
                    // A press on a live card's close (X) button reaps that preview
                    // (its last scene is kept as the node's snapshot).
                    if button == MouseButton::Left {
                        if let Some(member) = self.close_button_at(x, y) {
                            self.view.live_previews.remove(&member);
                            self.shared.content.constellation.reap(member);
                            self.view.request_redraw();
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
                            // A press on a slot divider starts a resize (host-authority:
                            // the drag reweights the Workbench directly); otherwise it
                            // routes to the surface frame (tab activate / close).
                            if let Some(i) = self.surface_divider_at(x, y) {
                                self.view.divider_drag = Some((i, x, self.view.workbench.weights()));
                            } else {
                                self.workbench_surface_click(x, y);
                            }
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
                        if button == MouseButton::Right {
                            self.open_context_menu_at(x, y);
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
                if button == MouseButton::Left {
                    if let Some((base, href)) = self.card_link_at(x, y) {
                        let url = nav::resolve_href(&base, &href);
                        self.view.runner.update(|c| c.follow_link(url));
                        self.sync_orrery();
                        self.view.request_redraw();
                        return;
                    }
                }
                // The same inline-link follow for a workbench tile: a click on a link in
                // a tile's content navigates *that tile's member* in place — focus it so
                // the omnibar + `sync_orrery` target it (`nav_target_member` is the
                // focused tile in Tree), then follow the link the card path's way.
                if button == MouseButton::Left {
                    if let Some((member, base, href)) = self.tile_link_at(x, y) {
                        self.view.workbench.activate(member);
                        self.view.focused_tile = Some(member);
                        let url = nav::resolve_href(&base, &href);
                        self.view.runner.update(|c| c.follow_link(url));
                        self.sync_orrery();
                        self.view.request_redraw();
                        return;
                    }
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
                // A divider resize ends on release (tile-tree slot + frame pane).
                if button == MouseButton::Left {
                    self.view.divider_drag = None;
                    self.view.frame_divider_drag = None;
                }
                // Resolve a tab drag (tiled view): if the press moved past the slop and
                // released over a tile, drop by zone. The nearest edge within the outer
                // quarter splits — left/right makes a horizontal (Row) split, top/bottom
                // a vertical (Column) split — and the center stacks the tab into that
                // cell (reorder within, move across). Dropping on the dragged tab's own
                // cell edge splits it out of its stack. A release in place was a plain
                // click (the tab activated on press).
                if button == MouseButton::Left {
                    if let Some((member, (px, py))) = self.view.tab_drag.take() {
                        if (x - px).hypot(y - py) > 6.0 {
                            if let Some((target, [x0, y0, x1, y1])) = self.tile_at(x, y) {
                                let w = (x1 - x0).max(1.0);
                                let h = (y1 - y0).max(1.0);
                                let left = (x - x0) / w;
                                let right = (x1 - x) / w;
                                let top = (y - y0) / h;
                                let bottom = (y1 - y) / h;
                                let nearest = left.min(right).min(top).min(bottom);
                                let moved = if nearest > 0.25 {
                                    self.view.workbench.move_to_slot_of(member, target)
                                } else {
                                    let (axis, after) = if nearest == left {
                                        (pelt_core::tile::SplitAxis::Row, false)
                                    } else if nearest == right {
                                        (pelt_core::tile::SplitAxis::Row, true)
                                    } else if nearest == top {
                                        (pelt_core::tile::SplitAxis::Column, false)
                                    } else {
                                        (pelt_core::tile::SplitAxis::Column, true)
                                    };
                                    if target == member {
                                        self.view.workbench.split_out(member, axis, after)
                                    } else {
                                        self.view.workbench.split_beside_axis(member, target, axis, after)
                                    }
                                };
                                if moved {
                                    self.view.focused_tile = Some(member);
                                    self.view.request_redraw();
                                }
                            }
                        }
                    }
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
                        if over_card {
                            self.toggle_live_preview();
                        } else if !self.orrery().selected_members().is_empty() {
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

    /// The live card whose close (X) button contains window point `(x, y)`, if
    /// any — its composited rect from the last frame.
    fn close_button_at(&self, x: f32, y: f32) -> Option<GraphMemberId> {
        self.view.close_button_rects
            .iter()
            .find(|(_, r)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
            .map(|(m, _)| *m)
    }

    /// Hit-test the chrome root at `(x, y)` and dispatch the click (buttons +
    /// suggestion / palette rows). A row / backdrop click that closes the palette
    /// restores focus so the caret doesn't dangle on the removed field.
    pub(super) fn chrome_click(&mut self, x: f32, y: f32) {
        let offsets = ScrollOffsets::<NodeId>::default();
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
        let palette_was_open = self.view.runner.state().palette_open;
        self.view.runner.dispatch_click(node, PointerClick::at(at));
        self.drain_pending_connect();
        self.drain_pending_command();
        self.drain_comms_intent();
        self.drain_pending_context();
        self.drain_history_step();
        self.drain_physics_toggle();
        self.sync_settings();
        self.sync_orrery();
        if palette_was_open && !self.view.runner.state().palette_open {
            self.focus_after_palette_close();
        }
        self.view.request_redraw();
    }

    /// Route a left press in the workbench pane to the pelt tile surface (V6,
    /// host-authority): hit-test the surface frame at the pane-local point, dispatch the
    /// click (queuing a gesture), then apply each emitted [`TileEvent`] to the
    /// `Workbench` — the authority — keyed back to its member by the tile id's UUID low
    /// 64 bits. A tab activates / closes its member; drag + divider resize are a
    /// follow-on (the surface's pointer state machine). Re-projection happens on the
    /// next render (`Workbench::to_tile_tree`), so the surface stays a driven view.
    fn workbench_surface_click(&mut self, x: f32, y: f32) {
        let Some(wr) = self.workbench_leaf_rect() else { return };
        let ww = (wr[2] - wr[0]).round().max(1.0) as u32;
        let wh = (wr[3] - wr[1]).round().max(1.0) as u32;
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let events = {
            let Some(surface) = self.view.pelt_surface.as_mut() else { return };
            let Some(node) = surface.hit_test_frame(lx, ly, ww, wh) else { return };
            surface.dispatch_click(node, xilem_serval::PointerClick::at((lx, ly)));
            surface.take_events()
        };
        if events.is_empty() {
            return;
        }
        let members = self.view.workbench.open_members();
        let member_of = |id: pelt_core::tile::TileId| {
            members.iter().copied().find(|m| m.as_u128() as u64 == id.0)
        };
        for event in events {
            match event {
                pelt_core::tile::TileEvent::Activated(id) => {
                    if let Some(m) = member_of(id) {
                        self.view.workbench.activate(m);
                        self.view.focused_tile = Some(m);
                        // Remember the activated tab as a drag candidate (the press is
                        // kept in window coords, matching `tile_rects`). The release
                        // resolves it: a move/split when dragged onto another slot past
                        // the slop, else a plain click (the tab already activated here).
                        self.view.tab_drag = Some((m, (x, y)));
                    }
                },
                pelt_core::tile::TileEvent::Closed(id) => {
                    if let Some(m) = member_of(id) {
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
                },
                // DividerMoved is handled by the host divider-drag path
                // (`surface_divider_at` + `drag_divider`); the surface does not own
                // the resize gesture.
                _ => {},
            }
        }
        self.view.request_redraw();
    }

    /// The tile (member + window rect) under `(x, y)` — the drag drop target, from
    /// this frame's laid-out tile rects.
    pub(super) fn tile_at(&self, x: f32, y: f32) -> Option<(GraphMemberId, [f32; 4])> {
        self.view.tile_rects
            .iter()
            .find(|(_, r)| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
            .copied()
    }

    /// The slot-boundary index of a `.tile-divider` in the pelt surface frame under
    /// window `(x, y)`, or `None` — the surface counterpart to [`divider_at`]. The
    /// surface lays its frame out, marks each divider with `data-dindex` (the boundary
    /// index), and the boundary maps 1:1 to the Workbench's slots (the projection keeps
    /// slot order), so the resize ([`drag_divider`]) reweights the right pair.
    fn surface_divider_at(&self, x: f32, y: f32) -> Option<usize> {
        let wr = self.workbench_leaf_rect()?;
        let ww = (wr[2] - wr[0]).round().max(1.0) as u32;
        let wh = (wr[3] - wr[1]).round().max(1.0) as u32;
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let surface = self.view.pelt_surface.as_ref()?;
        let node = surface.hit_test_frame(lx, ly, ww, wh)?;
        let dom = surface.dom();
        let dom = dom.borrow();
        if !has_class(&dom, node, "tile-divider") {
            return None;
        }
        // Only the top-level split's dividers resize here (the back-compat `weights`
        // path): they carry an empty `data-divider` path. Nested-split dividers stay
        // inert until per-split resize lands, so a nested drag can't corrupt the
        // top-level fractions.
        let nested = dom
            .attributes(node)
            .find(|a| a.name.local.as_ref() == "data-divider")
            .is_some_and(|a| !a.value.is_empty());
        if nested {
            return None;
        }
        dom.attributes(node)
            .find(|a| a.name.local.as_ref() == "data-dindex")
            .and_then(|a| a.value.parse::<usize>().ok())
    }

    /// Resize on a divider drag: shift width between the two slots the divider sits
    /// between, by the cursor's offset from the press as a fraction of the band.
    pub(super) fn drag_divider(&mut self) {
        let Some((i, press_x, snapshot)) = self.view.divider_drag.clone() else {
            return;
        };
        if i + 1 >= snapshot.len() {
            return;
        }
        // The slots span the workbench leaf, so reweight against the leaf width.
        let band_w = self
            .workbench_leaf_rect()
            .map(|wr| (wr[2] - wr[0]).max(1.0))
            .unwrap_or(self.view.width.max(1) as f32);
        let sum: f32 = snapshot.iter().sum();
        let dw = (self.view.cursor.0 - press_x) / band_w * sum;
        let mut weights = snapshot;
        weights[i] = (weights[i] + dw).max(0.05);
        weights[i + 1] = (weights[i + 1] - dw).max(0.05);
        self.view.workbench.set_weights(&weights);
        self.view.request_redraw();
    }

    /// While a tab is being dragged (moved past the slop), the member of the tile
    /// under the pointer — the highlighted drop target. `None` otherwise.
    pub(super) fn drag_target_member(&self) -> Option<GraphMemberId> {
        let (_, (px, py)) = self.view.tab_drag?;
        let (cx, cy) = self.view.cursor;
        if (cx - px).hypot(cy - py) <= 6.0 {
            return None; // not dragging yet (still a click)
        }
        self.tile_at(cx, cy).map(|(m, _)| m)
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
        // An open context menu eats Escape to dismiss (other keys fall through).
        if self.view.runner.state().context_menu.is_some()
            && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
        {
            self.close_context_menu();
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
        // While the settings overlay is open, Escape closes it and other keys are
        // swallowed (clicks on its controls go through the chrome path).
        if self.view.runner.state().settings_open {
            if matches!(key, WinitKey::Named(WinitNamedKey::Escape)) {
                self.view.runner.update(Chrome::close_settings);
                self.view.request_redraw();
            }
            return;
        }
        // Command palette: Ctrl+P. Ctrl+K toggles the comms pane (freed up for it).
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("p"))
        {
            self.toggle_palette();
            return;
        }
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("k"))
        {
            self.view.runner.update(Chrome::toggle_comms);
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
        if self.view.runner.state().palette_open {
            self.on_palette_key(key);
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
        } else {
            // No chrome field holds the caret: graph-level keys act on the
            // selection in the orrery.
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
        let suggestions_open = !self.view.runner.state().suggest.is_empty();
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                // A `>`-prefixed expression with no highlighted suggestion is a
                // command, not an address: route it to the command shell instead of
                // navigating. (Omnibar command shell, S3.)
                let chrome = self.view.runner.state();
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
                self.view.runner.update(move |c| {
                    submit_omnibar(c);
                    c.open_as_new_node = as_new_node;
                });
                tracing::info!(
                    location = %self.view.runner.state().toolbar.editable.location,
                    as_new_node,
                    "omnibar submit"
                );
                self.sync_orrery();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowDown) if suggestions_open => {
                self.view.runner.update(|c| c.step_suggestion(1));
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) if suggestions_open => {
                self.view.runner.update(|c| c.step_suggestion(-1));
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
                    self.view.runner.update(Chrome::refresh_suggestions);
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Whether the omnibar has a ghost completion to accept. `at_end_only` gates
    /// the Right-arrow case to a caret at the buffer end (so a mid-text → is still
    /// a plain caret move); Tab passes `false` (it has no other omnibar meaning).
    fn omnibar_ghost_acceptable(&self, at_end_only: bool) -> bool {
        let omnibar = &self.view.runner.state().omnibar;
        !omnibar.ghost().is_empty()
            && (!at_end_only || omnibar.caret() == omnibar.text().chars().count())
    }

    /// Splice the omnibar's ghost completion into the buffer and recompute (which
    /// clears the now-complete ghost and refreshes suggestions).
    fn accept_omnibar_ghost(&mut self) {
        self.view.runner.update(|c| {
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
        let palette = self.view.runner.state().palette_open;
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
            let c = self.view.runner.state();
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
            let c = self.view.runner.state();
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
        self.view.runner.update(|c| {
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
        self.view.runner.update(|c| {
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
            self.view.runner.update(Chrome::sync_palette_query);
        } else {
            self.view.runner.update(Chrome::refresh_suggestions);
        }
        self.view.request_redraw();
    }

    /// Route a key to the open command palette: Enter runs the selection, Arrow
    /// Up/Down step it, Escape closes, anything else edits the query.
    pub(super) fn on_palette_key(&mut self, key: &WinitKey) {
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                self.view.runner.update(Chrome::run_palette_selection);
                self.drain_pending_connect();
                self.drain_pending_command();
                self.drain_comms_intent();
                self.drain_history_step();
                self.drain_physics_toggle();
                self.sync_orrery();
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.view.runner.update(Chrome::close_palette);
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowDown) => {
                self.view.runner.update(|c| c.step_palette(1));
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) => {
                self.view.runner.update(|c| c.step_palette(-1));
                self.view.request_redraw();
            }
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.view.modifiers) {
                    self.view.runner.dispatch_key(key_event);
                    self.view.runner.update(Chrome::sync_palette_query);
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Route a key to the focused comms compose field: Enter sends the draft,
    /// Escape blurs back to the omnibar, anything else edits the field — never
    /// touching the omnibar suggestions (the dropdown stays closed while chatting).
    pub(super) fn on_comms_key(&mut self, key: &WinitKey) {
        let composing_new = self.view.runner.state().comms.new_message_open();
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                // Enter sends: the compose-new form when it's open, else the reply.
                if composing_new {
                    self.view.runner.update(Chrome::send_new_message);
                } else {
                    self.view.runner.update(Chrome::send_comms);
                }
                self.drain_comms_intent();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                // Escape closes the compose-new form, else blurs to the omnibar.
                if composing_new {
                    self.view.runner.update(Chrome::close_new_message);
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
    pub(super) fn caret_field(&self, node: NodeId) -> &xilem_serval::TextInput {
        let focus = Some(node);
        let c = self.view.runner.state();
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
        self.view.runner.update(Chrome::toggle_palette);
        if self.view.runner.state().palette_open {
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

    /// The first `<input>` under the first element carrying CSS class `class`
    /// (the omnibar under `.toolbar`, the query field under `.palette`).
    pub(super) fn input_under_class(&self, class: &str) -> Option<NodeId> {
        let dom = self.view.dom.borrow();
        let container = first_with_class(&dom, dom.document(), class)?;
        first_tag(&dom, container, "input")
    }
}
