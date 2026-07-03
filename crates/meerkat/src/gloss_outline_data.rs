/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Data builder for the gloss outline lens: the host-enriched
//! [`GlossOutlineSnapshot`] `glossary` projects the graph into, with each node
//! row's NODE_SHEET state + selection supplied by a narrow per-window input
//! snapshot rather than live host reads scattered through the builder.
//! `glossary` itself stays graph-pure. (gloss-outline plan P1, P8 seam prep.)

use super::WindowCtx;
use gloss::GlossOutlineSnapshot;

impl WindowCtx<'_> {
    /// Project the focused graph into the gloss outline lens's snapshot for this
    /// frame: [`glossary::outline_rows`] for the URL-structure tree, each node row
    /// enriched with its member id + NODE_SHEET state/selection (the same
    /// `node_states` / `selected_members` the workbench tabs tint from), plus
    /// [`glossary::graph_metrics`] for the header readout. `available_height` is the
    /// outline rect's live pixel height (the caller's current `gloss_sections()`
    /// split), used only to cap the *view's* copy of the rows — `glossary::outline_rows`
    /// itself stays fully uncapped. (gloss-outline plan P1 / P2 dynamic caps.)
    pub(super) fn gloss_outline_snapshot(&self, available_height: f32) -> GlossOutlineSnapshot {
        let graph = self.orrery().graph();
        let input = self.pane_input_snapshot();
        gloss::build_outline_snapshot(
            graph,
            |member| (input.node_state(member), input.is_selected(member)),
            available_height,
        )
    }
}
