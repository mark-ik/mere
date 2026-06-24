/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Camera on the view: per-window viewport install / readback around a ctx pass.
//!
//! The pooled [`Orrery`](orrery::Orrery) is the *authority* (graph + physics + node
//! positions) and is shared across every window showing its graph. The
//! camera/viewport is *view* state and lives on the [`WindowView`] as one
//! [`Viewport`](orrery::Viewport) per shown graph (`WindowView::viewports`). This
//! module is the seam that keeps the two consistent: when a [`WindowCtx`] is built
//! for a window's render or input pass it installs that window's stored viewports
//! into the shared orreries, and when the ctx drops it reads the (pan / zoom /
//! orbit / inertia-mutated) viewports back.
//!
//! Bracketing the ctx *lifecycle* (not individual call sites) is what makes this
//! correct: every camera read (screen<->world hit-tests) and write (pan, wheel,
//! recenter, frame inertia, isometric) runs inside a ctx, and `Drop` runs on every
//! scope exit including the early returns scattered through `window_event`. So two
//! windows on one graph render and pan their own viewports over the *shared* node
//! positions instead of mirroring a single camera.

use crate::WindowCtx;
use frame::{GraphId, PaneContent};

impl WindowCtx<'_> {
    /// The graphs this window shows in an Orrery pane right now (the focused pane plus
    /// any side-by-side secondaries). The set whose per-pane viewports this ctx
    /// installs on build and reads back on drop.
    fn shown_orrery_graphs(&self) -> Vec<GraphId> {
        // `GraphId` is `Hash + Eq` (it keys the orrery pool) but not `Ord`, so dedup
        // through a set rather than sort+dedup. A window shows a given graph in at most
        // one Orrery pane in practice; the dedup is belt-and-braces.
        let mut seen = std::collections::HashSet::new();
        self.view
            .frame_layout
            .iter_leaves()
            .filter(|(_, content, _)| matches!(content, PaneContent::Orrery))
            .map(|(_, _, graph_id)| graph_id)
            .filter(|graph_id| seen.insert(*graph_id))
            .collect()
    }

    /// Install each shown pane's stored [`Viewport`](orrery::Viewport) into its pooled
    /// orrery before this ctx's render / input pass, so the orrery projects *this*
    /// window's camera, not whichever window touched the shared orrery last. The first
    /// time this window shows a graph it has no stored viewport, so it adopts the
    /// orrery's current framing (seeding from `orrery.viewport()`); this is also how a
    /// boot-restored camera (set straight on the pooled orrery) flows in. Paired with
    /// [`readback_viewports`](Self::readback_viewports) on drop.
    pub(super) fn install_viewports(&mut self) {
        for graph_id in self.shown_orrery_graphs() {
            let stored = self.view.viewports.get(&graph_id).copied();
            if let Some(orrery) = self.orreries.get_mut(&graph_id) {
                orrery.set_viewport(stored.unwrap_or_else(|| orrery.viewport()));
            }
        }
    }

    /// Read each shown pane's (pan / zoom / orbit / inertia-mutated) viewport back onto
    /// this window after the pass, so an input pan survives the next install and the
    /// other window on the same graph keeps its own viewport.
    fn readback_viewports(&mut self) {
        for graph_id in self.shown_orrery_graphs() {
            if let Some(orrery) = self.orreries.get(&graph_id) {
                let viewport = orrery.viewport();
                self.view.viewports.insert(graph_id, viewport);
            }
        }
    }
}

impl Drop for WindowCtx<'_> {
    /// Read this window's viewports back out of the shared orreries when the ctx ends.
    /// On `Drop` so it cannot be skipped by an early return inside a handler — every
    /// camera mutation made during the pass lands on this window's stored viewport.
    fn drop(&mut self) {
        self.readback_viewports();
    }
}
