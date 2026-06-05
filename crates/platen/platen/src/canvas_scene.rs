/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Graph-to-canvas scene derivation.

use std::collections::HashSet;

use canvas_ir::packet::Color;
use canvas_ir::projection::{ProjectionMode, ViewDimension};
use canvas_ir::scene::{CanvasEdge, CanvasNode, CanvasSceneInput, SceneMode, ViewId};
use kernel::graph::{EdgeFamily, Graph, GraphViewId, NodeKey, RelationKind};

/// View-local relation-hide key. One stored edge between a node pair
/// can carry several typed relations; the key names the specific
/// `(from, to, kind)` tuple so a Hyperlink line and a Cites line on
/// the same pair hide independently.
pub type HiddenRelationKey = (NodeKey, NodeKey, RelationKind);

/// Canvas projection options for one graph view.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasSceneOptions {
    pub view_id: GraphViewId,
    pub scene_mode: SceneMode,
    pub dimension: ViewDimension,
    pub visible_nodes: Option<Vec<NodeKey>>,
    pub default_node_radius: f32,
    /// Relations the consuming view has hidden. View-local policy —
    /// graph truth is untouched; these `(from, to, kind)` tuples are
    /// simply dropped from the derived scene's edges. Per the
    /// 2026-05-11 relation-taxonomy plan §6.2.
    pub hidden_relations: HashSet<HiddenRelationKey>,
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
            dimension: ViewDimension::default(),
            visible_nodes: None,
            default_node_radius: 18.0,
            hidden_relations: HashSet::new(),
        }
    }
}

/// Build canvas scene input from a portable graph model.
pub fn build_canvas_scene_input(
    graph: &Graph,
    options: CanvasSceneOptions,
) -> CanvasSceneInput<NodeKey> {
    let visible_nodes = options
        .visible_nodes
        .as_ref()
        .map(|nodes| nodes.iter().copied().collect::<HashSet<_>>());
    let nodes = graph
        .nodes()
        .filter(|(key, _)| visible_nodes.as_ref().is_none_or(|mask| mask.contains(key)))
        .map(|(key, node)| CanvasNode {
            id: key,
            position: node.projected_position(),
            radius: options.default_node_radius,
            label: Some(node.title.clone()),
        })
        .collect();
    let edges = graph
        .relations()
        .filter(|edge| {
            visible_nodes
                .as_ref()
                .is_none_or(|mask| mask.contains(&edge.from) && mask.contains(&edge.to))
        })
        .filter(|edge| {
            !options
                .hidden_relations
                .contains(&(edge.from, edge.to, edge.kind))
        })
        .map(|edge| CanvasEdge {
            source: edge.from,
            target: edge.to,
            weight: 1.0,
            color: Some(family_color(edge.kind.family())),
            tag: Some(edge.kind.tag()),
        })
        .collect();

    CanvasSceneInput {
        view_id: graph_view_id_to_canvas(options.view_id),
        nodes,
        edges,
        scene_objects: Vec::new(),
        overlays: Vec::new(),
        scene_mode: options.scene_mode,
        projection: ProjectionMode::from_view_dimension(&options.dimension),
    }
}

/// Stroke colour for a relation family. Chosen so the six families
/// stay visually distinct on a dark canvas background (~`0x1c1c1c`),
/// with semantic — the most common — at the brightest end and the
/// derived/imported families muted further. Per the 2026-05-11
/// relation-taxonomy plan §6.3.
pub fn family_color(family: EdgeFamily) -> Color {
    match family {
        EdgeFamily::Semantic => Color::new(0.65, 0.72, 0.88, 0.85),
        EdgeFamily::Traversal => Color::new(0.85, 0.68, 0.42, 0.78),
        EdgeFamily::Containment => Color::new(0.48, 0.78, 0.72, 0.70),
        EdgeFamily::Arrangement => Color::new(0.72, 0.60, 0.88, 0.72),
        EdgeFamily::Imported => Color::new(0.80, 0.76, 0.46, 0.72),
        EdgeFamily::Provenance => Color::new(0.84, 0.60, 0.72, 0.72),
    }
}

pub fn graph_view_id_to_canvas(id: GraphViewId) -> ViewId {
    let uuid = id.as_uuid();
    let bytes = uuid.as_bytes();
    let lower = u64::from_le_bytes(bytes[8..16].try_into().expect("uuid lower bytes"));
    ViewId(lower)
}

#[cfg(test)]
mod tests {
    use kernel::geometry::PortablePoint;
    use kernel::graph::{EdgeAssertion, SemanticSubKind};
    use uuid::Uuid;

    use super::*;

    fn view_id(value: u128) -> GraphViewId {
        GraphViewId::from_uuid(Uuid::from_u128(value))
    }

    fn graph_with_edge() -> (Graph, NodeKey, NodeKey) {
        let mut graph = Graph::new();
        let first = graph.add_node(
            "https://example.test/1".into(),
            PortablePoint::new(1.0, 2.0),
        );
        let second = graph.add_node(
            "https://example.test/2".into(),
            PortablePoint::new(3.0, 4.0),
        );
        graph
            .assert_relation(
                first,
                second,
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Hyperlink,
                    label: None,
                    decay_progress: None,
                },
            )
            .unwrap();
        (graph, first, second)
    }

    #[test]
    fn builds_canvas_scene_from_graph() {
        let (graph, first, second) = graph_with_edge();
        let view_id = view_id(3);

        let scene = build_canvas_scene_input(&graph, CanvasSceneOptions::new(view_id));

        assert_eq!(scene.view_id, graph_view_id_to_canvas(view_id));
        assert_eq!(scene.nodes.len(), 2);
        assert_eq!(scene.edges.len(), 1);
        assert!(scene.nodes.iter().any(|node| node.id == first));
        assert!(scene.nodes.iter().any(|node| node.id == second));
    }

    #[test]
    fn visible_node_mask_filters_edges_too() {
        let (graph, first, _) = graph_with_edge();

        let scene = build_canvas_scene_input(
            &graph,
            CanvasSceneOptions {
                view_id: view_id(4),
                visible_nodes: Some(vec![first]),
                ..CanvasSceneOptions::default()
            },
        );

        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(scene.nodes[0].id, first);
        assert!(scene.edges.is_empty());
    }

    #[test]
    fn hidden_relations_drop_edge_without_touching_graph_truth() {
        let (graph, first, second) = graph_with_edge();
        let hidden: HashSet<HiddenRelationKey> = HashSet::from([(
            first,
            second,
            RelationKind::Semantic(SemanticSubKind::Hyperlink),
        )]);

        let scene = build_canvas_scene_input(
            &graph,
            CanvasSceneOptions {
                view_id: view_id(5),
                hidden_relations: hidden,
                ..CanvasSceneOptions::default()
            },
        );

        // The hidden relation is dropped from the derived scene...
        assert_eq!(scene.nodes.len(), 2);
        assert!(scene.edges.is_empty());
        // ...but graph truth is untouched — `relations()` still yields it.
        assert_eq!(graph.relations().count(), 1);
    }

    #[test]
    fn scene_edges_carry_family_color_and_relation_tag() {
        let (graph, _, _) = graph_with_edge();
        let scene = build_canvas_scene_input(&graph, CanvasSceneOptions::new(view_id(7)));
        assert_eq!(scene.edges.len(), 1);
        let edge = &scene.edges[0];
        // Hyperlink → Semantic family palette entry.
        let expected_color = family_color(EdgeFamily::Semantic);
        assert_eq!(edge.color, Some(expected_color));
        // Tag round-trips back to the relation it came from.
        let tag = edge.tag.expect("relation tag should be set");
        assert_eq!(
            RelationKind::from_tag(tag),
            Some(RelationKind::Semantic(SemanticSubKind::Hyperlink))
        );
    }

    #[test]
    fn non_matching_hidden_key_leaves_edge_visible() {
        let (graph, first, second) = graph_with_edge();
        // Same node pair, wrong kind — must not match the Hyperlink edge.
        let hidden: HashSet<HiddenRelationKey> = HashSet::from([(
            first,
            second,
            RelationKind::Semantic(SemanticSubKind::Cites),
        )]);

        let scene = build_canvas_scene_input(
            &graph,
            CanvasSceneOptions {
                view_id: view_id(6),
                hidden_relations: hidden,
                ..CanvasSceneOptions::default()
            },
        );

        assert_eq!(scene.edges.len(), 1);
    }
}
