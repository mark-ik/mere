/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Navigation and location sync for a window: pointing the omnibar at the focused
//! node, navigating the focused node in place (or minting a new node on
//! Ctrl/Cmd-Enter), per-node back/forward history steps, reflecting nav state onto
//! the toolbar buttons, and draining the physics pause/play toggle. The drains run
//! each input pass; the syncs run each render. Factored out of `frame_ops.rs` to
//! keep files under the 600-LOC ceiling.

use eidetic::browsing::TraceTransition;
use forme::GraphMemberId;

use super::WindowCtx;

impl WindowCtx<'_> {
    /// The URL of whatever is in focus: in the tiled view the focused tile's node,
    /// in the orrery the focused node. `None` when nothing is focused.
    pub(super) fn current_focus_url(&self) -> Option<String> {
        if self.workbench_active() {
            let member = self.view.focused_tile?;
            self.orrery()
                .graph()
                .get_node_by_id(member)
                .map(|(_, node)| node.url().to_string())
        } else {
            self.orrery().focused_url().map(str::to_string)
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
            self.view.chrome_update(move |c| c.show_location(&url));
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
        let loc = self.view.chrome().content_location().to_string();
        // Ctrl/Cmd-Enter: open the address as a *new node* linked from the focused
        // one. Handled before the change guard below, since duplicates are welcome
        // (the node-identity model) — opening the current page as a new node is a
        // valid branch, not a no-op.
        if self.view.chrome().open_as_new_node {
            self.view.chrome_update(|c| c.open_as_new_node = false);
            let origin = self.nav_target_member();
            let new_member = self.orrery_mut().open_member_as_new_node(origin, &loc);
            // With the workbench active, tile the new node: stack it into the
            // focused tile's slot as the active tab (or a fresh slot when nothing is
            // focused), and focus it so the next navigation targets it. Otherwise
            // the orrery has selected it, so it shows as the focused-node card.
            if self.workbench_active() {
                let stacked =
                    origin.is_some_and(|o| self.view.workbench.open_in_slot_of(new_member, o));
                if !stacked {
                    self.view.workbench.open_tile(new_member);
                }
                self.view.focused_tile = Some(new_member);
            }
            // Cartography: the new node is focused and shows its snapshot; opening it
            // live is the pelt path (double-click). (Node-rep P4.)
            self.ensure_content(&loc);
            // Record the navigation while `content_location` still holds the page
            // we came from (live trail recorder, C1).
            self.record_browse_nav(&loc, TraceTransition::UrlTyped);
            self.view.content_location = loc;
            // The freshly-opened node joins a branch window's lineage too (slice 2).
            self.record_branch_nav(new_member);
            self.save_session();
            self.view.request_redraw();
            return;
        }
        if loc == self.view.content_location {
            return;
        }
        let navigated = match self.nav_target_member() {
            Some(member) => {
                self.orrery_mut().navigate_member(member, &loc);
                self.view.scroll.remove(&member); // a new page starts at the top
                Some(member)
            }
            None => {
                let key = self.orrery_mut().visit(&loc);
                self.orrery().graph().get_node(key).map(|n| n.id)
            }
        };
        // Navigating focuses the target; the orrery shows its snapshot. Opening it live
        // is the pelt path (double-click a node or its card). (Node-rep P4.)
        self.ensure_content(&loc);
        // A knot note is a reading tile: open it as a workbench tile on navigate so
        // its content actually shows (the reframe — a note is a content tile). Web
        // content keeps its existing focus-card / tile flow; only the local note opens
        // a tile on navigate. (Slice 2 — open the note's reading tile.)
        if loc.starts_with("knot://") {
            if let Some(member) = navigated {
                self.open_workbench();
                self.view.workbench.open_tile(member);
                self.view.focused_tile = Some(member);
            }
        }
        // Record the navigation while `content_location` still holds the page we
        // came from (live trail recorder, C1).
        self.record_browse_nav(&loc, TraceTransition::UrlTyped);
        self.view.content_location = loc;
        // A branch window's navigation grows its graphlet's lineage (Phase 2 slice 2).
        if let Some(member) = navigated {
            self.record_branch_nav(member);
        }
        self.save_session();
        self.view.request_redraw();
    }

    /// If this is a tear-out **branch** window, the just-navigated `member` joins the
    /// branch graphlet's roster (its lineage diverges from the donor while kernel nodes
    /// stay shared, brief §4.2). Queued as a [`ShellCommand`](super::ShellCommand) so the
    /// Shell-side graphlet pool + persistence run after the ctx borrow ends; the Shell
    /// dedups, so revisiting a node is a cheap no-op. A leaf / the primary carries no
    /// `branch_graphlet`, so this is a no-op there. (Tear-out gestures G3.)
    fn record_branch_nav(&mut self, member: forme::GraphMemberId) {
        if let Some(graphlet) = self.view.branch_graphlet {
            self.commands.push(super::ShellCommand::RecordBranchMember {
                graph: self.view.focused_graph,
                graphlet,
                node: member,
            });
        }
    }

    /// The node per-node navigation acts on: the focused tile in Tree, the single
    /// selected node in Cartography. `None` when nothing is focused. Also the
    /// compatibility-view target — `>compat_view` pins, and the renderer scries,
    /// whatever this points at, so the pin and the surface agree in both modes.
    pub(super) fn nav_target_member(&self) -> Option<GraphMemberId> {
        if self.workbench_active() {
            self.view.focused_tile
        } else {
            self.orrery().focused_member()
        }
    }

    /// Apply a queued back/forward step to the **focused node's own** history:
    /// move its cursor (no new node, no fork), mirror the revealed page into the
    /// chrome (content target + omnibar) so the following `sync_orrery` sees no
    /// change and does not record a fresh visit, and refetch it. Drained each input
    /// pass, before `sync_orrery`.
    pub(super) fn drain_history_step(&mut self) {
        let Some(step) = self.view.chrome().history_step else {
            return;
        };
        self.view.chrome_update(|c| c.history_step = None);
        // Map the step to a trace transition before `step` is consumed below (C1).
        let transition = match &step {
            meerkat::HistoryStep::Back => TraceTransition::Back,
            meerkat::HistoryStep::Forward => TraceTransition::Forward,
        };
        let Some(member) = self.nav_target_member() else {
            return;
        };
        let url = match step {
            meerkat::HistoryStep::Back => self.orrery_mut().member_history_back(member),
            meerkat::HistoryStep::Forward => self.orrery_mut().member_history_forward(member),
        };
        let Some(url) = url else {
            return;
        };
        self.view.scroll.remove(&member); // the revealed page starts at the top
        // Record the back/forward navigation while `content_location` still holds
        // the page we came from (live trail recorder, C1).
        self.record_browse_nav(&url, transition);
        self.view.content_location = url.clone();
        self.ensure_content(&url);
        self.view.chrome_update(|c| {
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
                self.orrery().member_can_back(m),
                self.orrery().member_can_forward(m),
            ),
            None => (false, false),
        };
        let paused = self.orrery().physics_paused();
        let (cur_back, cur_forward, cur_paused) = {
            let c = self.view.chrome();
            (
                c.toolbar.can_go_back,
                c.toolbar.can_go_forward,
                c.physics_paused,
            )
        };
        if cur_back == can_back && cur_forward == can_forward && cur_paused == paused {
            return;
        }
        self.view.chrome_update(move |c| {
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
        self.view
            .chrome_update(|c| toggled = c.take_physics_toggle());
        if toggled {
            self.orrery_mut().toggle_physics_paused();
            self.sync_nav_buttons();
            self.view.request_redraw();
        }
    }
}
