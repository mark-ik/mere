/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Data builder for the gloss outline lens: the host-enriched [`GlossOutlineSnapshot`]
//! `glossary` projects the graph into, with each node row's NODE_SHEET state +
//! selection resolved against this frame's orrery (never reproduced by `glossary`,
//! which stays graph-pure). (gloss-outline plan P1.)

use std::collections::HashSet;

use forme::GraphMemberId;

use super::WindowCtx;
use crate::gloss_outline_view::{
    GlossOutlineNode, GlossOutlineRow, GlossOutlineSnapshot, cap_outline_rows,
};

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
        let states = self.node_states();
        let selected: HashSet<GraphMemberId> =
            self.orrery().selected_members().into_iter().collect();
        let rows = glossary::outline_rows(graph)
            .into_iter()
            .map(|row| GlossOutlineRow {
                depth: row.depth,
                label: row.label,
                node: row.url.and_then(|url| {
                    let (_, found) = graph.get_node_by_url(&url)?;
                    let member = found.id;
                    Some(GlossOutlineNode {
                        member,
                        url,
                        state: states
                            .get(&member)
                            .copied()
                            .unwrap_or(orrery::NodeState::Idle),
                        selected: selected.contains(&member),
                    })
                }),
            })
            .collect();
        GlossOutlineSnapshot {
            rows: cap_outline_rows(rows, available_height),
            metrics: glossary::graph_metrics(graph),
        }
    }
}
