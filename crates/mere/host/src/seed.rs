// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Default-workspace content — the graph, frame layout, and
//! placeholder tiles a brand-new session starts with.
//!
//! This is **not** generic demo scaffolding bolted onto the boot path:
//! it's the canonical "what a fresh session contains" content. The boot
//! path ([`crate::setup::build_runtime_state`]) calls these only when a
//! session has no persisted workspace to restore (see the session-first
//! restore-or-seed boundary there). As real authoring + persistence
//! land, the *graph* is already restored from disk
//! (`session_graph_store`); the frame layout + placeholder tiles seed
//! each boot until layout/tile persistence lands (Phase D of the host
//! roadmap).

use inker::{DocumentProvenance, DocumentTrustState, EngineDocument};
use frame::{FrameId, FrameLayout, GraphId, PaneContent, PaneId, PaneNode, SplitAxis};
use host_substrate::HostApp;
use kernel::geometry::PortablePoint;
use kernel::graph::Graph;

/// Synthetic [`EngineDocument`] for tile seeding. The v0 host doesn't
/// have real engines wired; documents stand in as placeholder content
/// the workbench can show when its renderer materialises.
fn fake_document(url: &str) -> EngineDocument {
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

/// The frame layout a fresh session opens with: a horizontal split with
/// the workbench on the left (60%), and a vertical split on the right
/// holding an orrery (top 60%) and an apparatus (bottom 40%).
///
/// ```text
///   ┌──────────┬─────────┐
///   │          │ orrery  │
///   │ workbench├─────────┤
///   │          │apparatus│
///   └──────────┴─────────┘
/// ```
///
/// Layout persistence (restoring this from the session manifest rather
/// than seeding it) is Phase D of the host roadmap.
pub fn default_workspace_layout(graph_id: GraphId) -> FrameLayout {
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

/// The graph a fresh session starts with — six nodes the orrery
/// projects via its cartography strategy. Persisted to the session's
/// `graph.json` on first boot and restored thereafter
/// (`session_graph_store`); edges land once a real relation graph
/// drops in (the kernel's edge API is `assert_relation`, not raw
/// add-edge, and seed-shaped relation assertions aren't worth wiring
/// at this stage).
pub fn default_workspace_graph() -> Graph {
    let mut graph = Graph::new();
    for i in 0..6 {
        graph.add_node(format!("seed://node/{i}"), PortablePoint::new(0.0, 0.0));
    }
    graph
}

/// Open placeholder document tiles in the host's tile manager — the
/// content the workbench renderer will show once it materialises. Not
/// persisted (Phase D); seeded each boot as UI scaffolding.
pub fn open_placeholder_tiles(host_app: &mut HostApp) {
    for (i, url) in [
        "https://a.example",
        "https://b.example",
        "https://c.example",
        "https://d.example",
    ]
    .iter()
    .enumerate()
    {
        host_app.tiles.open_or_focus(
            petgraph::graph::NodeIndex::new(i),
            url.to_string(),
            fake_document(url),
        );
    }
}
