/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Navigation and location sync for a window: pointing the omnibar at the focused
//! node, navigating the focused node in place (or minting a new node on
//! Ctrl/Cmd-Enter), per-node back/forward history steps, reflecting nav state onto
//! the toolbar buttons, and draining the physics pause/play toggle. The drains run
//! each input pass; the syncs run each render. Factored out of `frame_ops.rs` to
//! keep files under the 600-LOC ceiling.

use forme::GraphMemberId;

use super::WindowCtx;

impl WindowCtx<'_> {
    /// The URL of whatever is in focus: in the tiled view the focused tile's node,
    /// in the orrery the focused node. `None` when nothing is focused.
    pub(super) fn current_focus_url(&self) -> Option<String> {
        if self.workbench_active() {
            let member = self.view.focused_tile?;
            self.orrery
                .graph()
                .get_node_by_id(member)
                .map(|(_, node)| node.url().to_string())
        } else {
            self.orrery.focused_url().map(str::to_string)
        }
    }

    /// Point the omnibar at the focused tile / node (the address bar follows focus),
    /// but only when that focus actually changed and the user isn't editing the
    /// omnibar (no chrome field holds the caret) — so it never clobbers typing.
    pub(super) fn sync_location(&mut self) {
        let url = self.current_focus_url();
        if url == self.view.shown_location {
            return;
        }
        self.view.shown_location = url.clone();
        if let (Some(url), None) = (url, self.view.runner.focus()) {
            self.view.runner.update(move |c| c.show_location(&url));
            self.view.request_redraw();
        }
    }

    /// Sync the graph to the chrome's current navigation target when it changes.
    ///
    /// Per-node navigation (the node-lineage model): a node is a browsing surface.
    /// If one is focused — the focused tile in Tree, the selected node in
    /// Cartography — the omnibar **navigates it in place** (`navigate_member`):
    /// the node's URL and within-node history advance, no new node is minted, so
    /// it stays right where it is in the workbench. Only when nothing is focused
    /// (empty workbench / no selection) does this mint a node (`visit`), the
    /// first-context case. Called after any input that can navigate (omnibar
    /// submit, suggestion / palette).
    pub(super) fn sync_orrery(&mut self) {
        let loc = self.view.runner.state().content_location().to_string();
        // Ctrl/Cmd-Enter: open the address as a *new node* linked from the focused
        // one. Handled before the change guard below, since duplicates are welcome
        // (the node-identity model) — opening the current page as a new node is a
        // valid branch, not a no-op.
        if self.view.runner.state().open_as_new_node {
            self.view.runner.update(|c| c.open_as_new_node = false);
            let origin = self.nav_target_member();
            let new_member = self.orrery.open_member_as_new_node(origin, &loc);
            // With the workbench active, tile the new node: stack it into the
            // focused tile's slot as the active tab (or a fresh slot when nothing is
            // focused), and focus it so the next navigation targets it. Otherwise
            // the orrery has selected it, so it shows as the focused-node card.
            if self.workbench_active() {
                let stacked = origin.is_some_and(|o| self.view.workbench.open_in_slot_of(new_member, o));
                if !stacked {
                    self.view.workbench.open_tile(new_member);
                }
                self.view.focused_tile = Some(new_member);
            } else {
                // Cartography: opening a node is deliberate — show it as a live
                // card straight away (passive focus shows only the snapshot).
                self.view.live_previews.insert(new_member);
            }
            self.ensure_content(&loc);
            self.view.content_location = loc;
            self.save_session();
            self.view.request_redraw();
            return;
        }
        if loc == self.view.content_location {
            return;
        }
        match self.nav_target_member() {
            Some(member) => {
                self.orrery.navigate_member(member, &loc);
                self.view.scroll.remove(&member); // a new page starts at the top
            }
            None => {
                self.orrery.visit(&loc);
            }
        }
        // Navigating is deliberate: with the orrery active, show the target as a
        // live card (passive focus shows only the snapshot). With the workbench
        // active the node already tiled, so no card.
        if !self.workbench_active() {
            if let Some(member) = self.focused_member() {
                self.view.live_previews.insert(member);
            }
        }
        self.ensure_content(&loc);
        self.view.content_location = loc;
        self.save_session();
        self.view.request_redraw();
    }

    /// The node per-node navigation acts on: the focused tile in Tree, the single
    /// selected node in Cartography. `None` when nothing is focused.
    fn nav_target_member(&self) -> Option<GraphMemberId> {
        if self.workbench_active() {
            self.view.focused_tile
        } else {
            self.orrery.focused_member()
        }
    }

    /// Apply a queued back/forward step to the **focused node's own** history:
    /// move its cursor (no new node, no fork), mirror the revealed page into the
    /// chrome (content target + omnibar) so the following `sync_orrery` sees no
    /// change and does not record a fresh visit, and refetch it. Drained each input
    /// pass, before `sync_orrery`.
    pub(super) fn drain_history_step(&mut self) {
        let Some(step) = self.view.runner.state().history_step else {
            return;
        };
        self.view.runner.update(|c| c.history_step = None);
        let Some(member) = self.nav_target_member() else {
            return;
        };
        let url = match step {
            meerkat::HistoryStep::Back => self.orrery.member_history_back(member),
            meerkat::HistoryStep::Forward => self.orrery.member_history_forward(member),
        };
        let Some(url) = url else {
            return;
        };
        self.view.scroll.remove(&member); // the revealed page starts at the top
        self.view.content_location = url.clone();
        self.ensure_content(&url);
        self.view.runner.update(|c| {
            c.content_location = url.clone();
            c.show_location(&url);
        });
        self.save_session();
        self.view.request_redraw();
    }

    /// Reflect the focused node's history onto the toolbar's back/forward
    /// enabled-state, so the buttons track whichever node is focused. Cheap; a
    /// no-op when unchanged. Called each render.
    pub(super) fn sync_nav_buttons(&mut self) {
        let (can_back, can_forward) = match self.nav_target_member() {
            Some(m) => (
                self.orrery.member_can_back(m),
                self.orrery.member_can_forward(m),
            ),
            None => (false, false),
        };
        let paused = self.orrery.physics_paused();
        let (cur_back, cur_forward, cur_paused) = {
            let c = self.view.runner.state();
            (c.toolbar.can_go_back, c.toolbar.can_go_forward, c.physics_paused)
        };
        if cur_back == can_back && cur_forward == can_forward && cur_paused == paused {
            return;
        }
        self.view.runner.update(move |c| {
            c.toolbar.can_go_back = can_back;
            c.toolbar.can_go_forward = can_forward;
            c.physics_paused = paused;
        });
        self.view.request_redraw();
    }

    /// Apply a pending pause/play toggle the toolbar button queued (the chrome can't
    /// reach the orrery). Mirrors `drain_history_step`'s shape. (Physics pause.)
    pub(super) fn drain_physics_toggle(&mut self) {
        let mut toggled = false;
        self.view.runner.update(|c| toggled = c.take_physics_toggle());
        if toggled {
            self.orrery.toggle_physics_paused();
            self.sync_nav_buttons();
            self.view.request_redraw();
        }
    }
}
