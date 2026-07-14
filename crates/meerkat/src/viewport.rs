/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! View state on the view: per-window viewport **and selection** install / readback
//! around a ctx pass.
//!
//! The pooled [`Orrery`](mere::canvas::Canvas) is the *authority* (graph + physics + node
//! positions) and is shared across every window showing its graph. Two pieces of
//! *view* state live on the [`WindowView`] instead, one entry per shown graph: the
//! camera ([`Viewport`](mere::canvas::Viewport), `WindowView::viewports`) and the node
//! **selection** (and thus focus, `WindowView::selections`). This module is the seam
//! that keeps them consistent: when a [`WindowCtx`] is built for a window's render or
//! input pass it installs that window's stored viewports + selection into the shared
//! orreries, and when the ctx drops it reads the (pan / zoom / orbit / click / marquee
//! -mutated) state back.
//!
//! Bracketing the ctx *lifecycle* (not individual call sites) is what makes this
//! correct: every camera and selection read (screen<->world hit-tests, focus-driven
//! nav, branch lineage) and write (pan, wheel, click-select, marquee) runs inside a
//! ctx, and `Drop` runs on every scope exit including the early returns scattered
//! through `window_event`. So two windows on one graph pan and select independently
//! over the *shared* node positions instead of mirroring a single camera + selection.

use crate::WindowCtx;
use frisket::{GraphId, PaneContent};

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

    /// Install each shown pane's stored [`Viewport`](mere::canvas::Viewport) into its pooled
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

    /// Install each shown pane's stored node selection (member uuids) into its pooled
    /// orrery before this ctx's pass, the selection twin of
    /// [`install_viewports`](Self::install_viewports). The first time this window shows
    /// a graph it has no stored selection, so it adopts the orrery's current one (a
    /// boot-restored focus, or whatever a sibling window left) rather than wiping it.
    /// Paired with [`readback_selections`](Self::readback_selections) on drop.
    pub(super) fn install_selections(&mut self) {
        for graph_id in self.shown_orrery_graphs() {
            let stored = self.view.selections.get(&graph_id).cloned();
            if let (Some(members), Some(orrery)) = (stored, self.orreries.get_mut(&graph_id)) {
                orrery.set_selected_members(&members);
            }
        }
    }

    /// Read each shown pane's (click / marquee / clear-mutated) selection back onto this
    /// window after the pass, so a selection made here survives the next install and the
    /// other window on the same graph keeps its own. Member-keyed, so it survives an
    /// evict+reload of the orrery.
    fn readback_selections(&mut self) {
        for graph_id in self.shown_orrery_graphs() {
            if let Some(orrery) = self.orreries.get(&graph_id) {
                self.view
                    .selections
                    .insert(graph_id, orrery.selected_members());
            }
        }
    }

    /// If this is a tear-out **branch** window, scope its focused-graph orrery to the
    /// branch graphlet's live roster (so it shows only the branch, not the whole donor
    /// graph), saving the orrery's prior scope into `branch_scope_restore` for
    /// [`restore_scope`](Self::restore_scope) on drop. Only branch windows touch the
    /// scope; a leaf / the primary leaves it untouched, so the donor's own scope (e.g. an
    /// isolate-selection) survives the branch's transient override. Re-derived each pass,
    /// so the scope tracks the graphlet as it grows. (Graphlet wiring Phase 2 slice 3.)
    pub(super) fn install_scope(&mut self) {
        let Some(graphlet_id) = self.view.branch_graphlet else {
            return;
        };
        let graph = self.view.focused_graph;
        // A **Linked** graphlet re-derives its roster from the live graph each pass, so its
        // window tracks graph drift (a node added elsewhere joins the component live); a
        // **Branched** graphlet uses its accumulated roster. Re-deriving every pass is fine
        // at demo scale; a revision-cached derive is the refinement. (Phase 3 slice 2+ —
        // live drift.) `derive_members` is read-only, so this needs no `&mut graphlets`.
        let roster: Vec<uuid::Uuid> = {
            let Some(graphlet) = self
                .graphlets
                .get(&graph)
                .and_then(|idx| idx.get(graphlet_id))
            else {
                return;
            };
            match &graphlet.binding {
                mere::forme::GraphletBinding::Linked { spec } => match self.orreries.get(&graph) {
                    Some(o) => mere::graphlets::derive_members(o.graph(), spec),
                    None => return,
                },
                _ => graphlet.anchors.clone(),
            }
        };
        if roster.is_empty() {
            return; // nothing to scope to — leave the whole graph visible
        }
        if let Some(orrery) = self.orreries.get_mut(&graph) {
            self.branch_scope_restore = Some(orrery.scope_members());
            orrery.scope_to_members(roster);
        }
    }

    /// Restore the orrery's pre-branch scope saved by [`install_scope`](Self::install_scope),
    /// so a branch window's graphlet scope never leaks to the next window's pass.
    fn restore_scope(&mut self) {
        let Some(prior) = self.branch_scope_restore.take() else {
            return;
        };
        if let Some(orrery) = self.orreries.get_mut(&self.view.focused_graph) {
            match prior {
                Some(members) => orrery.scope_to_members(members),
                None => orrery.clear_scope(),
            }
        }
    }
}

impl Drop for WindowCtx<'_> {
    /// Read this window's viewports + selection back out of the shared orreries when the
    /// ctx ends. On `Drop` so it cannot be skipped by an early return inside a handler —
    /// every camera + selection mutation made during the pass lands on this window's
    /// stored state.
    fn drop(&mut self) {
        self.readback_viewports();
        self.readback_selections();
        self.restore_scope();
    }
}
