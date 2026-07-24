// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sceno scene output path — cartography as mere's graph adapter.
//!
//! [`scene_from_projection`] lowers a [`Projection`] into a portable
//! [`sceno::Scene`]: positioned nodes become placed instances with real
//! footprints, edges become routed relations, cluster halos become regions.
//! This is the projection-proofs P2 consumption slice: the first place mere
//! speaks the scenograph contract. The caller supplies the *stable* source
//! id per node (mere's durable ref is the node Uuid, not the session-local
//! `NodeKey`) and the measured extent per node ("the representation
//! measures content; the projection places it").
//!
//! Overlay migration per the scene-contract note
//! (`scenograph:design_docs/2026-07-22_scene_contract_note.md`):
//! `ClusterHalo` → [`sceno::Region`]; per-edge weight rides
//! [`sceno::RoutedRelation::weight`] (the projection's edges already carry
//! it); `ImportanceScale` folds into the instance transform; `ActivityHeat`
//! and `BridgeEmphasis` ride [`sceno::ProjectedItem::channels`] as `"heat"`
//! and `"bridge"`. The overlay vocabulary is now fully migrated: emphasis
//! travels inside the scene so a viewer without access to mere's graph can
//! still shade what it renders.

use std::collections::HashMap;

use kernel::graph::NodeKey;
use sceno::{
    Footprint, ProjectedItem, Region, Representation, RoutedRelation, Scene, Size2, SourceRef,
    Transform2, Vec2,
};

use crate::overlay::Overlay;
use crate::projection::Projection;

/// The adapter lane name mere's graph scenes carry in their
/// [`sceno::SourceRef`]s.
pub const MERE_GRAPH_ADAPTER: &str = "mere.graph";

/// Channel name carrying `Overlay::ActivityHeat` intensity.
pub const HEAT_CHANNEL: &str = "heat";

/// Channel name carrying `Overlay::BridgeEmphasis` weight.
pub const BRIDGE_CHANNEL: &str = "bridge";

/// Lower `projection` into a [`sceno::Scene`].
///
/// - `id_of` supplies each node's stable source id (the node Uuid string;
///   never the session-local `NodeKey`).
/// - `extent_of` supplies the measured `(w, h)` per node; `None` falls back
///   to the projection's `radius` (a [`Footprint::Circle`]) and then to
///   [`Footprint::Point`].
///
/// Instances land in scene order = projection node order; relations and
/// regions reference them by that instance identity.
pub fn scene_from_projection(
    projection: &Projection,
    id_of: impl Fn(NodeKey) -> String,
    extent_of: impl Fn(NodeKey) -> Option<(f32, f32)>,
) -> Scene {
    let mut scene = Scene::new();
    let mut instance_of: HashMap<NodeKey, sceno::InstanceId> = HashMap::new();

    // Importance folds into the instance transform's uniform scale; the
    // per-node emphases ride the item's open channel map.
    let mut importance: HashMap<NodeKey, f32> = HashMap::new();
    let mut channels: HashMap<NodeKey, Vec<(String, f32)>> = HashMap::new();
    for overlay in &projection.overlays {
        match overlay {
            Overlay::ImportanceScale { node, factor } => {
                importance.insert(*node, *factor);
            }
            Overlay::ActivityHeat { node, intensity } => {
                channels
                    .entry(*node)
                    .or_default()
                    .push((HEAT_CHANNEL.to_string(), *intensity));
            }
            Overlay::BridgeEmphasis { node, weight } => {
                channels
                    .entry(*node)
                    .or_default()
                    .push((BRIDGE_CHANNEL.to_string(), *weight));
            }
            _ => {}
        }
    }

    for positioned in &projection.nodes {
        let source = scene.intern_source(SourceRef::new(
            MERE_GRAPH_ADAPTER,
            id_of(positioned.node),
        ));
        let footprint = match extent_of(positioned.node) {
            Some((w, h)) => Footprint::Rect {
                size: Size2::new(w, h),
            },
            None if positioned.radius > 0.0 => Footprint::Circle {
                radius: positioned.radius,
            },
            None => Footprint::Point,
        };
        let scale = importance.get(&positioned.node).copied().unwrap_or(1.0);
        let item = ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2 {
                translate: Vec2::new(positioned.position.x, positioned.position.y),
                scale,
                rotate: 0.0,
            },
            footprint,
            representation: Representation::Glyph,
            layer: 0,
            visible: true,
            hit: None,
            channels: channels.get(&positioned.node).cloned().unwrap_or_default(),
        };
        instance_of.insert(positioned.node, sceno::InstanceId(scene.items.len() as u32));
        scene.items.push(item);
    }

    for edge in &projection.edges {
        let (Some(&from), Some(&to)) = (instance_of.get(&edge.from), instance_of.get(&edge.to))
        else {
            continue; // an edge whose endpoint was filtered out of the projection
        };
        scene.relations.push(RoutedRelation {
            from,
            to,
            space: Scene::WORLD,
            points: edge.path.iter().map(|p| Vec2::new(p.x, p.y)).collect(),
            kind: None,
            weight: Some(edge.weight),
        });
    }

    for overlay in &projection.overlays {
        if let Overlay::ClusterHalo {
            members,
            label,
            confidence,
            ..
        } = overlay
        {
            let members: Vec<_> = members
                .iter()
                .filter_map(|n| instance_of.get(n).copied())
                .collect();
            if !members.is_empty() {
                scene.regions.push(Region {
                    members,
                    space: Scene::WORLD,
                    contour: None,
                    label: label.clone(),
                    confidence: *confidence,
                });
            }
        }
    }

    scene.bounds = sceno::Rect::new(
        Vec2::new(
            projection.content_bounds.origin.x,
            projection.content_bounds.origin.y,
        ),
        Size2::new(
            projection.content_bounds.size.width,
            projection.content_bounds.size.height,
        ),
    );
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::{PositionedEdge, PositionedNode};
    use kernel::geometry::PortablePoint;

    fn projection_two_nodes() -> Projection {
        Projection {
            nodes: vec![
                PositionedNode {
                    node: NodeKey::new(0),
                    position: PortablePoint::new(0.0, 0.0),
                    radius: 12.0,
                },
                PositionedNode {
                    node: NodeKey::new(1),
                    position: PortablePoint::new(100.0, 0.0),
                    radius: 0.0,
                },
            ],
            edges: vec![PositionedEdge {
                edge: None,
                from: NodeKey::new(0),
                to: NodeKey::new(1),
                path: vec![PortablePoint::new(0.0, 0.0), PortablePoint::new(100.0, 0.0)],
                weight: 2.0,
            }],
            ..Projection::empty()
        }
    }

    #[test]
    fn nodes_lower_to_items_with_measured_footprints() {
        let scene = scene_from_projection(
            &projection_two_nodes(),
            |k| format!("uuid:{}", k.index()),
            |k| (k.index() == 0).then_some((40.0, 40.0)),
        );
        assert_eq!(scene.items.len(), 2);
        // Node 0: measured -> Rect. Node 1: unmeasured, radius 0 -> Point.
        assert_eq!(
            scene.items[0].footprint,
            Footprint::Rect {
                size: Size2::new(40.0, 40.0)
            }
        );
        assert_eq!(scene.items[1].footprint, Footprint::Point);
        assert_eq!(scene.sources[0].adapter, MERE_GRAPH_ADAPTER);
        assert_eq!(scene.sources[0].id, "uuid:0");
    }

    #[test]
    fn unmeasured_radius_falls_back_to_circle() {
        let scene = scene_from_projection(&projection_two_nodes(), |k| k.index().to_string(), |_| {
            None
        });
        assert_eq!(scene.items[0].footprint, Footprint::Circle { radius: 12.0 });
    }

    #[test]
    fn edges_lower_to_relations_with_weight_and_path() {
        let scene = scene_from_projection(&projection_two_nodes(), |k| k.index().to_string(), |_| {
            None
        });
        assert_eq!(scene.relations.len(), 1);
        let r = &scene.relations[0];
        assert_eq!(r.weight, Some(2.0));
        assert_eq!(r.points.len(), 2);
        assert_eq!((r.from, r.to), (sceno::InstanceId(0), sceno::InstanceId(1)));
    }

    #[test]
    fn cluster_halo_lowers_to_region() {
        let mut projection = projection_two_nodes();
        projection.overlays.push(Overlay::ClusterHalo {
            cluster_id: "c1".into(),
            members: vec![NodeKey::new(0), NodeKey::new(1)],
            label: Some("Work".into()),
            confidence: 0.8,
        });
        let scene = scene_from_projection(&projection, |k| k.index().to_string(), |_| None);
        assert_eq!(scene.regions.len(), 1);
        assert_eq!(scene.regions[0].members.len(), 2);
        assert_eq!(scene.regions[0].label.as_deref(), Some("Work"));
    }

    #[test]
    fn heat_and_bridge_lower_to_channels() {
        let mut projection = projection_two_nodes();
        projection.overlays.push(Overlay::ActivityHeat {
            node: NodeKey::new(0),
            intensity: 0.75,
        });
        projection.overlays.push(Overlay::BridgeEmphasis {
            node: NodeKey::new(0),
            weight: 0.4,
        });
        let scene = scene_from_projection(&projection, |k| k.index().to_string(), |_| None);
        let mut channels = scene.items[0].channels.clone();
        channels.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            channels,
            vec![
                (BRIDGE_CHANNEL.to_string(), 0.4),
                (HEAT_CHANNEL.to_string(), 0.75)
            ]
        );
    }

    #[test]
    fn an_unemphasized_node_carries_no_channels() {
        let mut projection = projection_two_nodes();
        projection.overlays.push(Overlay::ActivityHeat {
            node: NodeKey::new(0),
            intensity: 0.75,
        });
        let scene = scene_from_projection(&projection, |k| k.index().to_string(), |_| None);
        assert!(
            scene.items[1].channels.is_empty(),
            "emphasis is per node, not scene-wide"
        );
    }

    #[test]
    fn importance_scale_folds_into_transform() {
        let mut projection = projection_two_nodes();
        projection.overlays.push(Overlay::ImportanceScale {
            node: NodeKey::new(1),
            factor: 1.5,
        });
        let scene = scene_from_projection(&projection, |k| k.index().to_string(), |_| None);
        assert_eq!(scene.items[1].transform.scale, 1.5);
        assert_eq!(scene.items[0].transform.scale, 1.0);
    }
}
