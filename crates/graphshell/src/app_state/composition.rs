/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pure workspace-to-canvas selectors.
//!
//! This module is the first portable subset of the donor `composition` seam. It
//! derives canvas-facing scene input from reducer-owned workspace state without
//! importing host adapters, renderer backends, or desktop widget types.
//!
//! This is deliberately not the owner of graph-aware layout policy. If this
//! code starts deciding tile placement, split topology, engine surfaces, or
//! render scheduling, that behavior belongs in `platen`, `verso-tile`, `inker`,
//! or host glue instead.

use graph_canvas::scene::CanvasSceneInput;
use graphshell_core::graph::{GraphViewId, NodeKey};
pub use platen::canvas_scene::{CanvasSceneOptions, graph_view_id_to_canvas};

use super::{FrameState, GraphWorkspace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositionError {
    MissingGraphView(GraphViewId),
}

pub type CompositionResult<T> = Result<T, CompositionError>;

/// Build canvas scene input for a reducer-owned graph view.
pub fn build_canvas_scene_input(
    workspace: &GraphWorkspace,
    options: CanvasSceneOptions,
) -> CompositionResult<CanvasSceneInput<NodeKey>> {
    if !workspace.views.graph_views.contains_key(&options.view_id) {
        return Err(CompositionError::MissingGraphView(options.view_id));
    }

    Ok(platen::canvas_scene::build_canvas_scene_input(
        &workspace.domain.graph,
        options,
    ))
}

/// Select the active frame for workbench-aware composition.
pub fn select_active_frame(workspace: &GraphWorkspace) -> Option<&FrameState> {
    platen::workbench::select_active_frame(
        &workspace.workbench.frames,
        workspace.workbench.active_frame.as_ref(),
    )
}

/// Select the active frame's root view for workbench-aware composition.
pub fn select_active_root_view(workspace: &GraphWorkspace) -> Option<GraphViewId> {
    platen::workbench::select_active_root_view(
        &workspace.workbench.frames,
        workspace.workbench.active_frame.as_ref(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use graph_canvas::projection::{ProjectionMode, ThreeDMode, ViewDimension, ZSource};
    use graph_canvas::scene::SceneMode;
    use graphshell_core::geometry::PortablePoint;
    use graphshell_core::graph::EdgeType;
    use uuid::Uuid;

    use super::*;
    use crate::app_state::FrameId;
    use crate::app_state::graph_runtime::{GraphRuntimeIntent, reduce_graph_runtime_intent};

    fn view_id(value: u128) -> GraphViewId {
        GraphViewId::from_uuid(Uuid::from_u128(value))
    }

    fn add_node(workspace: &mut GraphWorkspace, value: u128) -> NodeKey {
        reduce_graph_runtime_intent(
            workspace,
            GraphRuntimeIntent::AddNode {
                id: Uuid::from_u128(value),
                url: format!("https://example.test/{value}"),
                position: PortablePoint::new(value as f32, value as f32 + 1.0),
            },
        )
        .unwrap()
        .node
        .unwrap()
    }

    #[test]
    fn builds_canvas_scene_from_workspace_graph() {
        let mut workspace = GraphWorkspace::new();
        let first = add_node(&mut workspace, 1);
        let second = add_node(&mut workspace, 2);
        workspace
            .domain
            .graph
            .add_edge(first, second, EdgeType::Hyperlink, None)
            .unwrap();
        let view_id = view_id(3);
        workspace.views.ensure_view(view_id);

        let scene = build_canvas_scene_input(&workspace, CanvasSceneOptions::new(view_id)).unwrap();

        assert_eq!(scene.view_id, graph_view_id_to_canvas(view_id));
        assert_eq!(scene.nodes.len(), 2);
        assert_eq!(scene.edges.len(), 1);
        assert_eq!(scene.scene_mode, SceneMode::Browse);
        assert_eq!(scene.projection, ProjectionMode::TwoD);
    }

    #[test]
    fn visible_node_mask_filters_edges_too() {
        let mut workspace = GraphWorkspace::new();
        let first = add_node(&mut workspace, 10);
        let second = add_node(&mut workspace, 11);
        workspace
            .domain
            .graph
            .add_edge(first, second, EdgeType::Hyperlink, None)
            .unwrap();
        let view_id = view_id(12);
        workspace.views.ensure_view(view_id);

        let scene = build_canvas_scene_input(
            &workspace,
            CanvasSceneOptions {
                view_id,
                visible_nodes: Some(vec![first]),
                ..CanvasSceneOptions::default()
            },
        )
        .unwrap();

        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(scene.nodes[0].id, first);
        assert!(scene.edges.is_empty());
    }

    #[test]
    fn projection_options_map_to_canvas_scene() {
        let mut workspace = GraphWorkspace::new();
        add_node(&mut workspace, 20);
        let view_id = view_id(21);
        workspace.views.ensure_view(view_id);

        let scene = build_canvas_scene_input(
            &workspace,
            CanvasSceneOptions {
                view_id,
                scene_mode: SceneMode::Arrange,
                dimension: ViewDimension::ThreeD {
                    mode: ThreeDMode::TwoPointFive,
                    z_source: ZSource::Recency { max_depth: 8.0 },
                },
                ..CanvasSceneOptions::default()
            },
        )
        .unwrap();

        assert_eq!(scene.scene_mode, SceneMode::Arrange);
        assert!(matches!(
            scene.projection,
            ProjectionMode::TwoPointFive {
                z_source: ZSource::Recency { max_depth: 8.0 }
            }
        ));
    }

    #[test]
    fn missing_view_is_rejected_without_scene_derivation() {
        let workspace = GraphWorkspace::new();
        let missing = view_id(30);

        let error =
            build_canvas_scene_input(&workspace, CanvasSceneOptions::new(missing)).unwrap_err();

        assert_eq!(error, CompositionError::MissingGraphView(missing));
    }

    #[test]
    fn active_frame_selection_uses_workbench_state() {
        let frame = FrameState {
            id: FrameId::new("main"),
            label: "Main".to_string(),
            root_view: Some(view_id(40)),
            panes: Vec::new(),
        };
        let mut workspace = GraphWorkspace::new();
        workspace.workbench.frames = HashMap::from([(frame.id.clone(), frame.clone())]);
        workspace.workbench.active_frame = Some(frame.id.clone());

        let selected = select_active_frame(&workspace);

        assert_eq!(selected, Some(&frame));
    }

    #[test]
    fn active_root_view_selection_uses_workbench_state() {
        let frame = FrameState {
            id: FrameId::new("main"),
            label: "Main".to_string(),
            root_view: Some(view_id(41)),
            panes: Vec::new(),
        };
        let mut workspace = GraphWorkspace::new();
        workspace.workbench.frames = HashMap::from([(frame.id.clone(), frame.clone())]);
        workspace.workbench.active_frame = Some(frame.id.clone());

        let selected = select_active_root_view(&workspace);

        assert_eq!(selected, frame.root_view);
    }
}
