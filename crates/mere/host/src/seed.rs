// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Host startup seeding — frametree layout, demo graph + documents
//! for the v0 orrery. Replace each seed with real state as
//! session-persistence + embed + real engines wire in.

use inker::{DocumentProvenance, DocumentTrustState, EngineDocument};
use frame::{FrameId, FrameLayout, GraphId, PaneContent, PaneId, PaneNode, SplitAxis};
use kernel::geometry::PortablePoint;
use kernel::graph::Graph;

/// Synthetic [`EngineDocument`] for tile seeding. The v0 host
/// doesn't have real engines wired; documents stand in as
/// placeholder content the workbench can show when its renderer
/// materialises.
pub fn fake_document(url: &str) -> EngineDocument {
    EngineDocument {
        address: url.to_string(),
        title: None,
        content_type: "text/html".to_string(),
        lang: None,
        provenance: DocumentProvenance::default(),
        trust: DocumentTrustState::default(),
        diagnostics: Vec::new(),
        blocks: Vec::new(),
    }
}

/// v0 seed frametree: a horizontal split with the workbench on the
/// left (60%), and a vertical split on the right holding an orrery
/// (top 60%) and an apparatus (bottom 40%). Replace with persisted
/// frame state once `view_intent_store` covers layout serialization.
///
/// ```text
///   ┌──────────┬─────────┐
///   │          │ orrery  │
///   │ workbench├─────────┤
///   │          │apparatus│
///   └──────────┴─────────┘
/// ```
pub fn build_seed_layout(graph_id: GraphId) -> FrameLayout {
    FrameLayout {
        id: FrameId::new("host-frame"),
        label: "Mere".to_string(),
        root: PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.6,
            first: Box::new(PaneNode::Leaf {
                pane_id: PaneId(1),
                content: PaneContent::Workbench,
                graph_id,
            }),
            second: Box::new(PaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio: 0.6,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(2),
                    content: PaneContent::Orrery,
                    graph_id,
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(3),
                    content: PaneContent::Apparatus,
                    graph_id,
                }),
            }),
        },
    }
}

/// Small seed graph for the orrery to project — six nodes the
/// v0 host renders via cartography's Grid strategy. Replace with
/// the session-loaded graph once persistence covers it; edges
/// land once a real relation graph drops in (the kernel's edge
/// API is `assert_relation`, not raw add-edge, and seed-shaped
/// relation assertions aren't worth wiring at this stage).
pub fn build_seed_graph() -> Graph {
    let mut graph = Graph::new();
    for i in 0..6 {
        graph.add_node(format!("seed://node/{i}"), PortablePoint::new(0.0, 0.0));
    }
    graph
}
