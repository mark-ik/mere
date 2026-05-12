/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tile + app-tree operations on `HostRoot`.
//!
//! Split out of [`crate::host_navigation`] to keep that file under
//! the 600-LOC ceiling and to group the workbench/tile/app-tree
//! work together. Sibling to [`crate::host_navigation`] (which keeps
//! the navigation-flow, history, chrome-cycling, and panel-summon
//! methods).

use gpui::Context;
use mere_frame::PaneId;
use mere_kernel::graph::Graph;

use crate::HostRoot;
use crate::demo::build_demo_application_tree;

// Note: `focus_tile_in` and `close_tile_in` sync the omnibar +
// toolbar to the *active tile's current URL* via
// `HostRoot::sync_omnibar_to_active_tile` (in host_navigation.rs),
// not to the tile's anchor URL — once within-tile navigation has
// happened, those can differ.

impl HostRoot {
    /// Switch the active workbench to the tile at `index`. Syncs
    /// the omnibar / toolbar to that tile's address.
    pub(crate) fn focus_tile(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(pane_id) = self.active_workbench else {
            return;
        };
        self.focus_tile_in(pane_id, index, cx);
    }

    /// Switch the workbench at `pane_id` to its tile at `index`.
    /// Used by tile clicks in any workbench (the click handler
    /// captures the pane_id explicitly).
    pub(crate) fn focus_tile_in(
        &mut self,
        pane_id: PaneId,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let node = self
            .panes
            .get_mut(&pane_id)
            .and_then(|s| s.as_workbench_mut())
            .and_then(|w| w.tiles.focus_index(index));
        if node.is_none() {
            return;
        }
        self.active_workbench = Some(pane_id);
        // Sync omnibar to the tile's *active history URL* (not the
        // anchor URL — they may differ once within-tile nav has
        // happened).
        self.sync_omnibar_to_active_tile(cx);
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Close the tile at `index` in the active workbench.
    pub(crate) fn close_tile(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(pane_id) = self.active_workbench else {
            return;
        };
        self.close_tile_in(pane_id, index, cx);
    }

    /// Close the tile at `index` in the workbench at `pane_id`.
    pub(crate) fn close_tile_in(
        &mut self,
        pane_id: PaneId,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(workbench) = self
            .panes
            .get_mut(&pane_id)
            .and_then(|s| s.as_workbench_mut())
        else {
            return;
        };
        workbench.tiles.close_index(index);
        // After close, the active tile may have shifted; sync the
        // omnibar + toolbar to whatever's active now (which might be
        // nothing, in which case both clear).
        self.sync_omnibar_to_active_tile(cx);
        self.rebuild_app_tree(cx);
        cx.notify();
    }

    /// Recompute `app_tree` from the current shared graph snapshot
    /// + per-window state. Centralised here so the borrow dance
    /// around `self.graph.read(cx)` lives in one place.
    pub(crate) fn rebuild_app_tree(&mut self, cx: &mut Context<Self>) {
        let active_doc = self
            .active_tiles()
            .and_then(|t| t.active_document())
            .cloned();
        let frame_layout = &self.frame_layout;
        let graph_snapshot: &Graph = self.graph.read(cx);
        self.app_tree =
            build_demo_application_tree(active_doc.as_ref(), graph_snapshot, frame_layout);
    }
}
