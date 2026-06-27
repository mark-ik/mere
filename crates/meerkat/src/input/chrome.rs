/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome (toolbar / shell) click + activation routing and intent draining.

use super::*;

impl WindowCtx<'_> {
    /// Apply a window-control press (borderless titlebar). Minimize / maximize act
    /// on the window directly; close defers to the event handler via `pending_exit`
    /// (input has no event-loop handle), which saves the session and exits.
    pub(crate) fn window_control(&mut self, ctl: WindowControl) {
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
    pub(crate) fn chrome_routed_leaf_at(&self, x: f32, y: f32) -> bool {
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
    pub(crate) fn settings_pane_at(&self, x: f32, y: f32) -> bool {
        self.view
            .settings_rects
            .iter()
            .any(|(_, r)| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
    }

    /// Whether `(x, y)` is inside the open knot-editor pane — a press there routes to the shell
    /// hit-test (its × close button + source field) rather than the orrery underneath. The pane
    /// is `position: absolute`, so it sits outside `.chrome`'s normal-flow height; without this a
    /// press below the toolbar fails the `y < chrome_h` band and falls through to the graph,
    /// leaving the editor unclickable (no close, no focus). (Knot editor.)
    pub(crate) fn knot_editor_pane_at(&self, x: f32, y: f32) -> bool {
        if !self.view.chrome().knot_editor_open {
            return false;
        }
        let dom = self.view.dom.borrow();
        let Some(session) = self.view.chrome_session.as_ref() else { return false };
        crate::first_with_class(&dom, dom.document(), "knot-editor-pane")
            .and_then(|node| session.fragments().rect_of(node))
            .is_some_and(|r| {
                x >= r.location.x
                    && x < r.location.x + r.size.width
                    && y >= r.location.y
                    && y < r.location.y + r.size.height
            })
    }

    /// Hit-test the chrome root at `(x, y)` and dispatch the click (buttons +
    /// suggestion / palette rows). A row / backdrop click that closes the palette
    /// restores focus so the caret doesn't dangle on the removed field.
    pub(crate) fn chrome_click(&mut self, x: f32, y: f32) {
        // The retained chrome session folds its own `element_scroll` (the panes' wheel
        // offsets) into hit-testing via `merged_scroll`, so the host no longer mirrors a
        // per-pane offset map here. The stateless fallback (pre-first-render) has no scroll
        // yet, so empty is correct there too. (Host-scroll P2.)
        let scroll = ScrollOffsets::<NodeId>::default();
        let sheet = self.shared.presentation.chrome_sheet_refs();
        let hit = {
            let dom = self.view.dom.borrow();
            // C4: hit-test against the render's retained chrome layout (the
            // session); fall back to a stateless probe only before the first render.
            match &self.view.chrome_session {
                Some(s) => s.hit_test(&dom, x, y, &scroll),
                None => hit_test_node(&dom, &sheet, self.view.width, self.view.height, x, y, &scroll),
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
    pub(crate) fn chrome_activate(&mut self, node: NodeId, at: (f32, f32)) {
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
    pub(crate) fn drain_chrome_intents(&mut self) {
        self.drain_pending_connect();
        self.drain_pending_command();
        self.drain_comms_intent();
        self.drain_session_intent();
        self.drain_palette_context_action();
        self.drain_pending_context();
        // A context-menu row can carry a `RunCommand`, which queues a pending command
        // *inside* `drain_pending_context` above — after the first `drain_pending_command`
        // already ran this cycle. Flush once more so a clicked menu command (e.g. crawl /
        // crawl_stop) fires this interaction rather than lagging to the next. No-op when
        // nothing was queued. (The keyboard menu path already orders context before command.)
        self.drain_pending_command();
        self.drain_history_step();
        self.drain_physics_toggle();
        self.drain_roster_intents();
        self.drain_list_pane_activations();
        self.drain_object_card();
        self.sync_settings();
        self.sync_orrery();
    }

}
