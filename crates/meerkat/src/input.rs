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
use platen_view::{WORKBENCH_SHEET, WorkbenchAction};
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
                                    for intent in self.view.roster_pane.take_intents() {
                                        match intent {
                                            crate::roster_view::RosterIntent::Select(member) => {
                                                if let Some(url) = self
                                                    .orrery
                                                    .graph()
                                                    .get_node_by_id(member)
                                                    .map(|(_, n)| n.url().to_string())
                                                {
                                                    self.orrery.select_by_url(&url);
                                                    self.view.request_redraw();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            return;
                        }
                    }
                    // The apparatus pane consumes the press: a left click on a
                    // theme button switches the theme. (Apparatus.)
                    if let Some(arect) = self.apparatus_leaf_rect() {
                        if x >= arect[0] && x < arect[2] && y >= arect[1] && y < arect[3] {
                            if button == MouseButton::Left {
                                if let Some(theme_id) = self.apparatus_button_at(x, y) {
                                    self.set_theme(&theme_id);
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
                                        .orrery
                                        .graph()
                                        .get_node_by_id(member)
                                        .map(|(_, n)| n.url().to_string())
                                    {
                                        self.orrery.select_by_url(&url);
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
                                self.commands.push(super::ShellCommand::SwitchSession(id));
                                return;
                            }
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
                            if let Some(i) = self.divider_at(x, y) {
                                self.view.divider_drag = Some((i, x, self.view.workbench.weights()));
                            } else {
                                self.workbench_click(x, y);
                            }
                        }
                    } else {
                        // The orrery pane: right-click opens the context menu; a left
                        // / middle press pans / selects / drags (unless it's over the
                        // orrery's card, which owns its own clicks).
                        self.view.active_content = super::ContentPane::Orrery;
                        if button == MouseButton::Right {
                            self.open_context_menu_at(x, y);
                        } else if let Some(b) = orrery_button {
                            let (ox, oy) = self.orrery_point(x, y);
                            if !self.point_over_card(x, y) && self.orrery.pointer_down(b, ox, oy) {
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
                if let Some(b) = orrery_button {
                    let (ox, oy) = self.orrery_point(x, y);
                    if !over_card && self.orrery.pointer_up(b, ox, oy) {
                        self.view.request_redraw();
                    }
                }
                // A divider resize ends on release (tile-tree slot + frame pane).
                if button == MouseButton::Left {
                    self.view.divider_drag = None;
                    self.view.frame_divider_drag = None;
                }
                // Resolve a tab drag (tiled view): if the press moved past the slop
                // and released over a tile, drop by zone — the outer quarter on
                // either side splits the tab out to a new slot there, the center
                // moves / stacks it into that slot (reorder within, move across). A
                // release in place was a plain click (the tab activated on press).
                if button == MouseButton::Left {
                    if let Some((member, (px, py))) = self.view.tab_drag.take() {
                        if (x - px).hypot(y - py) > 6.0 {
                            if let Some((target, [x0, _, x1, _])) = self.tile_at(x, y) {
                                let edge = (x1 - x0).max(1.0) * 0.25;
                                let moved = if x < x0 + edge {
                                    self.view.workbench.split_beside(member, target, false)
                                } else if x > x1 - edge {
                                    self.view.workbench.split_beside(member, target, true)
                                } else {
                                    self.view.workbench.move_to_slot_of(member, target)
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
                        } else if !self.orrery.selected_members().is_empty() {
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
        self.sync_settings();
        self.sync_orrery();
        if palette_was_open && !self.view.runner.state().palette_open {
            self.focus_after_palette_close();
        }
        self.view.request_redraw();
    }

    /// Hit-test the workbench root at window `(x, y)` (content-band coords) and
    /// dispatch the click — a tab switch, a close, or a pin toggle. The action is
    /// captured in the workbench scene and drained immediately onto the model.
    pub(super) fn workbench_click(&mut self, x: f32, y: f32) {
        // The workbench DOM is laid out at the workbench leaf size + composited at
        // its origin, so hit-test in leaf-local coordinates. (Workbench-as-pane.)
        let Some(wr) = self.workbench_leaf_rect() else {
            return;
        };
        let ww = (wr[2] - wr[0]).round().max(1.0) as u32;
        let wh = (wr[3] - wr[1]).round().max(1.0) as u32;
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let offsets = ScrollOffsets::<NodeId>::default();
        let hit = {
            let dom = self.view.workbench_dom.borrow();
            // C5: hit-test the workbench off its render's retained layout (session);
            // stateless fallback only before the workbench first renders.
            match &self.view.workbench_session {
                Some(s) => s.hit_test(&dom, lx, ly, &offsets),
                None => hit_test_node(&dom, WORKBENCH_SHEET, ww, wh, lx, ly, &offsets),
            }
        };
        if let Some(node) = hit {
            self.view.workbench_runner
                .dispatch_click(node, PointerClick::at((lx, ly)));
            // A tab activated → remember it as a drag candidate (resolved on
            // release: a move when dragged onto another slot, else a plain click).
            // The press is kept in window coords (tile_rects are window-space).
            if let Some(WorkbenchAction::Activate(member)) = self.view.workbench_runner.state().pending {
                self.view.tab_drag = Some((member, (x, y)));
            }
            self.drain_workbench_action();
            self.view.request_redraw();
        }
    }

    /// The tile (member + window rect) under `(x, y)` — the drag drop target, from
    /// this frame's laid-out tile rects.
    pub(super) fn tile_at(&self, x: f32, y: f32) -> Option<(GraphMemberId, [f32; 4])> {
        self.view.tile_rects
            .iter()
            .find(|(_, r)| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
            .copied()
    }

    /// If `(x, y)` is over a divider (the gutter between two slots), its left-slot
    /// index — the start of a resize drag.
    pub(super) fn divider_at(&self, x: f32, y: f32) -> Option<usize> {
        let wr = self.workbench_leaf_rect()?;
        let ww = (wr[2] - wr[0]).round().max(1.0) as u32;
        let wh = (wr[3] - wr[1]).round().max(1.0) as u32;
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let offsets = ScrollOffsets::<NodeId>::default();
        let dom = self.view.workbench_dom.borrow();
        // C5: divider probe off the workbench session's retained layout.
        let node = (match &self.view.workbench_session {
            Some(s) => s.hit_test(&dom, lx, ly, &offsets),
            None => hit_test_node(&dom, WORKBENCH_SHEET, ww, wh, lx, ly, &offsets),
        })?;
        if !has_class(&dom, node, "wb-divider") {
            return None;
        }
        dom.attributes(node)
            .find(|a| a.name.local.as_ref() == "data-divider")
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
        // F2 renames the active session (the switcher's keyboard rename affordance;
        // right-clicking a tile renames that one). (Host text path.)
        if matches!(key, WinitKey::Named(WinitNamedKey::F2)) {
            self.start_rename(self.shared.session.active_session_id);
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
            WinitKey::Named(WinitNamedKey::Escape) if suggestions_open => {
                self.view.runner.update(Chrome::close_suggestions);
                self.view.request_redraw();
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
