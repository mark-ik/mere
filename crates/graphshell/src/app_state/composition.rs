/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pure workspace composition selectors.
//!
//! This module is the first portable subset of the donor `composition` seam. It
//! derives canvas-facing scene input from reducer-owned workspace state without
//! importing host adapters, renderer backends, or desktop widget types.

use std::collections::HashSet;

use graph_canvas::projection::{ProjectionMode, ViewDimension};
use graph_canvas::scene::{CanvasEdge, CanvasNode, CanvasSceneInput, SceneMode, ViewId};
use graphshell_core::graph::{GraphViewId, NodeKey};

use super::GraphWorkspace;

/// Canvas projection options for one graph view.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasSceneOptions {
    pub view_id: GraphViewId,
    pub scene_mode: SceneMode,
    pub dimension: ViewDimension,
    pub visible_nodes: Option<Vec<NodeKey>>,
    pub default_node_radius: f32,
}

impl CanvasSceneOptions {
    pub fn new(view_id: GraphViewId) -> Self {
        Self {
            view_id,
            ..Self::default()
        }
    }
}

impl Default for CanvasSceneOptions {
    fn default() -> Self {
        Self {
            view_id: GraphViewId::from_uuid(uuid::Uuid::nil()),
            scene_mode: SceneMode::Browse,
            dimension: ViewDimension::TwoD,
            visible_nodes: None,
            default_node_radius: 18.0,
        }
    }
}

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

    let visible_nodes = options
        .visible_nodes
        .as_ref()
        .map(|nodes| nodes.iter().copied().collect::<HashSet<_>>());
    let nodes = workspace
        .domain
        .graph
        .nodes()
        .filter(|(key, _)| visible_nodes.as_ref().is_none_or(|mask| mask.contains(key)))
        .map(|(key, node)| CanvasNode {
            id: key,
            position: node.projected_position(),
            radius: options.default_node_radius,
            label: Some(node.title.clone()),
        })
        .collect();
    let edges = workspace
        .domain
        .graph
        .edges()
        .filter(|edge| {
            visible_nodes
                .as_ref()
                .is_none_or(|mask| mask.contains(&edge.from) && mask.contains(&edge.to))
        })
        .map(|edge| CanvasEdge {
            source: edge.from,
            target: edge.to,
            weight: 1.0,
        })
        .collect();

    Ok(CanvasSceneInput {
        view_id: graph_view_id_to_canvas(options.view_id),
        nodes,
        edges,
        scene_objects: Vec::new(),
        overlays: Vec::new(),
        scene_mode: options.scene_mode,
        projection: ProjectionMode::from_view_dimension(&options.dimension),
    })
}

pub fn graph_view_id_to_canvas(id: GraphViewId) -> ViewId {
    let uuid = id.as_uuid();
    let bytes = uuid.as_bytes();
    let lower = u64::from_le_bytes(bytes[8..16].try_into().expect("uuid lower bytes"));
    ViewId(lower)
}

#[cfg(test)]
mod tests {
    use graph_canvas::projection::{ThreeDMode, ZSource};
    use graphshell_core::geometry::PortablePoint;
    use graphshell_core::graph::EdgeType;
    use uuid::Uuid;

    use super::*;
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
}
