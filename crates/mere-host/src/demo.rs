/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Demo fixtures used by the v0 shell — markdown body, a tiny graph,
//! a fixed toolbar / frame layout, and the application-uxtree
//! composer that ties the per-domain projections together.
//!
//! Everything here is throwaway: when real intents + reducer state
//! land, these fixtures get replaced one by one with the runtime
//! equivalents.

use euclid::default::Point2D;
use inker::{Engine, EngineDocument, EngineInput};
use mere_frame::{FrameLayout, GraphId, PaneContent, PaneNode, SplitAxis};
use mere_graphshell::frame_model::ToolbarViewModel;
use mere_kernel::graph::{EdgeType, Graph, NodeKey};
use mere_kernel::pane::PaneId;
use nematic::MarkdownEngine;
use platen::{FrameId, ProjectedPane, WorkbenchProjection};
use uxtree::UxTree;

const DEMO_MARKDOWN: &str = r#"# mere-host v0

This window is opened by `mere-host::run()`. The host bootstraps gpui,
walks the active `FrameLayout`, and dispatches each pane content to
its dedicated renderer crate.

## Per-domain renderer crates

- `mere-host-workbench` — this pane (markdown / EngineDocument)
- `mere-host-orrery` — graph cards
- `mere-host-gloss` — outline of headings
- `mere-host-apparatus` — system inspector (tracing events + uxtree dump)
"#;

pub fn render_demo_document() -> EngineDocument {
    let engine = MarkdownEngine::new();
    let input = EngineInput::new("mere-host:demo", DEMO_MARKDOWN);
    engine
        .render(&input)
        .expect("MarkdownEngine should not fail on the demo doc")
}

pub fn render_demo_graph_state() -> Graph {
    let mut graph = Graph::new();
    let intro = graph.add_node_with_id(
        uuid::Uuid::from_u128(0xb1),
        "https://mere.test/intro".to_string(),
        Point2D::new(-80.0, -40.0),
    );
    let arch = graph.add_node_with_id(
        uuid::Uuid::from_u128(0xb2),
        "https://mere.test/architecture".to_string(),
        Point2D::new(80.0, -40.0),
    );
    let probes = graph.add_node_with_id(
        uuid::Uuid::from_u128(0xb3),
        "https://mere.test/probes".to_string(),
        Point2D::new(0.0, 80.0),
    );
    let _ = graph.add_edge(intro, arch, EdgeType::Hyperlink, None);
    let _ = graph.add_edge(arch, probes, EdgeType::Hyperlink, None);
    let _ = graph.add_edge(intro, probes, EdgeType::Hyperlink, None);
    graph
}

pub fn render_demo_toolbar() -> ToolbarViewModel {
    ToolbarViewModel {
        // Seed with the intro address so the omnibar mirrors what
        // the host loaded at startup. Enter on this string reloads
        // it; typing replaces it.
        location: "mere://intro".to_string(),
        location_dirty: false,
        location_submitted: false,
        load_status: None,
        status_text: Some("type a URL — file:// or mere:// in v0".to_string()),
        can_go_back: false,
        can_go_forward: true,
        active_pane_draft: None,
    }
}

/// Build the default frame layout for a window whose primary graph
/// is `graph_id`. Every leaf gets stamped with the same id today
/// (Phase 1B will let summoned panels target other graphs).
pub fn render_demo_frame_layout_for(graph_id: GraphId) -> FrameLayout {
    FrameLayout {
        id: mere_frame::FrameId::new("reading"),
        label: "Reading".to_string(),
        root: PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf {
                    pane_id: mere_frame::PaneId(1),
                    content: PaneContent::Workbench,
                    graph_id,
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: mere_frame::PaneId(2),
                    content: PaneContent::Gloss,
                    graph_id,
                }),
            }),
            second: Box::new(PaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf {
                    pane_id: mere_frame::PaneId(3),
                    content: PaneContent::Orrery,
                    graph_id,
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: mere_frame::PaneId(4),
                    content: PaneContent::Apparatus,
                    graph_id,
                }),
            }),
        },
    }
}

/// Project the frame layout, attaching each mere-domain subtree at
/// its corresponding leaf, then stitch the result under a single
/// application-root node.
pub fn build_demo_application_tree(
    document: Option<&EngineDocument>,
    graph: &Graph,
    layout: &FrameLayout,
) -> UxTree {
    let frame_tree = mere_frame::project_frame_with(layout, |content, _pane_id| match content {
        PaneContent::Workbench => Some(render_demo_workbench()),
        PaneContent::Orrery => Some(mere_orrery::project_graph(graph)),
        PaneContent::Gloss => document.map(mere_gloss::project_outline),
        PaneContent::Apparatus => Some(mere_apparatus::project_skeleton()),
        _ => None,
    });

    let mut app_root = accesskit::Node::new(accesskit::Role::Window);
    app_root.set_label("mere-host (demo)");
    uxtree::stitch("application", app_root, vec![frame_tree])
}

fn render_demo_workbench() -> UxTree {
    let projection = WorkbenchProjection {
        active_frame: Some(FrameId::new("frame-demo")),
        frame_label: Some("Reading".to_string()),
        root_view: None,
        panes: vec![
            ProjectedPane {
                pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(0xa1)),
                node: NodeKey::default(),
                surface_host: None,
                is_primary: true,
            },
            ProjectedPane {
                pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(0xa2)),
                node: NodeKey::default(),
                surface_host: None,
                is_primary: false,
            },
            ProjectedPane {
                pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(0xa3)),
                node: NodeKey::default(),
                surface_host: None,
                is_primary: false,
            },
        ],
    };
    mere_workbench::project_workbench(&projection)
}
