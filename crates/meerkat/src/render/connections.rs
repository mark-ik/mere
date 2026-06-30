/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Compute the connections swatch's `FocusCard` from the live multi-selection (swatch-primitive
//! plan P2, scope=Selection): gather the selected nodes' positions and their inter-edges from the
//! orrery's kernel graph, normalize them via [`swatch::connections_spec_from`], and place the card
//! in the focus-card slot. The pure spec-building lives in `swatch.rs`; this is the host extraction
//! glue (the untestable orrery / graph reads), kept out of the near-full `setup.rs`.

use kernel::graph::{EdgeFamily, RelationKind, RelationSelector};

use crate::swatch::connections_spec_from;
use crate::window_view::{FocusCard, FocusCardKind};

/// The connections swatch card's size (px) in the focus-card slot.
const CONN_W: u32 = 240;
const CONN_H: u32 = 240;

impl crate::WindowCtx<'_> {
    /// Build the connections-swatch focus card for the current multi-selection, or `None` when
    /// fewer than two selected nodes resolve to graph positions. The selected nodes' kernel
    /// positions feed the layout (cropped from the live arrangement for now; the cartography
    /// re-layout is P2b), the inter-edges colour by relation family, and the card anchors at the
    /// focused node. (Swatch primitive — P2.)
    pub(super) fn compute_connections_card(&self, orrery_rect: [f32; 4]) -> Option<FocusCard> {
        let members = self.orrery().selected_members();
        if members.len() < 2 {
            return None;
        }
        let graph = self.orrery().graph();
        // The selected nodes' kernel positions, keyed for the inter-edge filter below. A member
        // that no longer resolves to a node or a position is skipped.
        let mut keys_to_id = std::collections::HashMap::new();
        let mut positions: Vec<(uuid::Uuid, f32, f32)> = Vec::new();
        for &m in &members {
            if let Some((key, _)) = graph.get_node_by_id(m) {
                if let Some(p) = self.orrery().node_position(key) {
                    keys_to_id.insert(key, m);
                    positions.push((m, p.x, p.y));
                }
            }
        }
        if positions.len() < 2 {
            return None;
        }
        // Inter-edges: one entry per selected relation cell. The swatch fans cells that share the
        // same endpoints, and its DOM carries the relation kind for Link Card routing. Hidden
        // relation cells stay hidden here too, matching the canvas edge pass.
        let mut edges: Vec<(uuid::Uuid, uuid::Uuid, kernel::graph::RelationKind)> = Vec::new();
        for r in graph.relations() {
            let (Some(&a), Some(&b)) = (keys_to_id.get(&r.from), keys_to_id.get(&r.to)) else {
                continue;
            };
            if a == b {
                continue;
            }
            if !self.orrery().relation_between_members_hidden(
                a,
                b,
                selector_for_relation_kind(r.kind),
            ) {
                edges.push((a, b, r.kind));
            }
        }
        // Classify the selection's induced subgraph for the chip strip: the dominant shape plus its
        // best runners-up (fit-ranked, top 3). (Swatch primitive — P3, the strip.)
        let shape_chips: Vec<String> =
            crate::graphlet_classifier::classify_selection(graph, &members)
                .iter()
                .take(3)
                .filter(|s| s.fit >= 0.4)
                .map(|s| s.label.clone())
                .collect();
        let mut spec = connections_spec_from(positions, edges);
        spec.shape_chips = shape_chips;
        // Anchor at the focused node (one of the selection); the pane centre is the fallback.
        let local = [
            0.0,
            0.0,
            orrery_rect[2] - orrery_rect[0],
            orrery_rect[3] - orrery_rect[1],
        ];
        let (nx, ny) = self
            .orrery()
            .focused_node_screen()
            .unwrap_or((local[2] * 0.5, local[3] * 0.5));
        let (x0, y0, x1, y1, _, _) =
            crate::card::anchored_card_rect(nx, ny, CONN_W, CONN_H, local)?;
        Some(FocusCard {
            rect: [x0, y0, x1, y1],
            kind: FocusCardKind::Connections { spec },
        })
    }
}

fn selector_for_relation_kind(kind: RelationKind) -> RelationSelector {
    match kind {
        RelationKind::Semantic(sub) => RelationSelector::Semantic(sub),
        RelationKind::Traversal => RelationSelector::Family(EdgeFamily::Traversal),
        RelationKind::Containment(sub) => RelationSelector::Containment(sub),
        RelationKind::Arrangement(sub) => RelationSelector::Arrangement(sub),
        RelationKind::Imported(sub) => RelationSelector::Imported(sub),
        RelationKind::Provenance(sub) => RelationSelector::Provenance(sub),
    }
}
