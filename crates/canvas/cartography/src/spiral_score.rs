//! Mere's graph-to-Scenograph Spiral adapter.
//!
//! The solver is portable (`scenomise::solve`). This module is deliberately
//! not: it reads graph recency, chooses the browser's LOD rungs, and maps
//! `NodeKey`s back into the local canvas projection. That is the boundary P3
//! exists to prove.

use std::collections::HashMap;

use kernel::geometry::{PortablePoint, PortableRect, PortableSize};
use kernel::graph::{Graph, NodeKey};
use sceno::{
    Arrangement, Footprint, Placement, Representation, Score, ScoreItem, Size2, SourceRef, Spiral,
};

use crate::scene_out::MERE_GRAPH_ADAPTER;
use crate::{PositionedEdge, PositionedNode, Projection, ProjectionMetadata};

/// The persisted score plus the ordinary Mere canvas projection it realizes.
#[derive(Clone, Debug, PartialEq)]
pub struct MereSpiralProjection {
    pub score: Score,
    pub projection: Projection,
}

/// Build and realize Mere's P3 pane-spiral score.
///
/// `extents` are the host's measured node faces. `recent_first` maps native
/// timestamps to a score ordinal; the portable score itself retains only that
/// deterministic ordinal, never a Mere timestamp.
pub fn project_spiral_score(
    graph: &Graph,
    extents: Option<&HashMap<NodeKey, (f32, f32)>>,
    focus: Option<NodeKey>,
    recent_first: bool,
) -> MereSpiralProjection {
    let mut ordered: Vec<NodeKey> = graph.nodes().map(|(key, _)| key).collect();
    if recent_first {
        ordered.sort_by_key(|key| {
            std::cmp::Reverse(
                graph
                    .get_node(*key)
                    .map(|node| node.last_visited)
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            )
        });
    }

    let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
    score.generation = graph.revision();
    for (ordinal, key) in ordered.iter().enumerate() {
        let node = graph
            .get_node(*key)
            .expect("node keys came from this graph");
        let extent = extents
            .and_then(|items| items.get(key).copied())
            .unwrap_or((0.0, 0.0));
        score.items.push(ScoreItem {
            source: SourceRef::new(MERE_GRAPH_ADAPTER, node.id.to_string()),
            ordinal: ordinal as u32,
            footprint: footprint_for(extent),
            representation: representation_for(extent, focus == Some(*key)),
            placement: Placement::Ordinal,
            layer: 0,
            visible: true,
        });
    }

    let scene = scenomise::solve(&score);
    let positions: HashMap<NodeKey, PortablePoint> = ordered
        .iter()
        .copied()
        .zip(scene.items.iter())
        .map(|(key, item)| {
            (
                key,
                PortablePoint::new(item.transform.translate.x, item.transform.translate.y),
            )
        })
        .collect();
    let nodes = ordered
        .iter()
        .filter_map(|key| {
            positions.get(key).copied().map(|position| PositionedNode {
                node: *key,
                position,
                radius: 0.0,
            })
        })
        .collect();
    let edges = graph
        .relations()
        .filter(|relation| relation.from != relation.to)
        .map(|relation| PositionedEdge {
            edge: None,
            from: relation.from,
            to: relation.to,
            path: vec![
                positions.get(&relation.from).copied().unwrap_or_default(),
                positions.get(&relation.to).copied().unwrap_or_default(),
            ],
            weight: 1.0,
        })
        .collect();
    let projection = Projection {
        nodes,
        edges,
        overlays: Vec::new(),
        minimap: None,
        content_bounds: PortableRect::new(
            PortablePoint::new(scene.bounds.origin.x, scene.bounds.origin.y),
            PortableSize::new(scene.bounds.size.w, scene.bounds.size.h),
        ),
        metadata: ProjectionMetadata {
            strategy_id: Some("phyllotaxis.default".to_string()),
            settled: true,
        },
    };
    MereSpiralProjection { score, projection }
}

fn footprint_for((w, h): (f32, f32)) -> Footprint {
    if w > 0.0 && h > 0.0 {
        Footprint::Rect {
            size: Size2::new(w, h),
        }
    } else {
        Footprint::Point
    }
}

/// Mere's local LOD policy. Focus is a view fact and wins over scale; the
/// source's live content stays an application concern, not a Scenograph type.
fn representation_for((w, h): (f32, f32), focused: bool) -> Representation {
    if focused {
        Representation::LivePane
    } else {
        match w.max(h) {
            side if side >= 72.0 => Representation::Card,
            side if side >= 48.0 => Representation::Snapshot,
            _ => Representation::Glyph,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
    use uuid::Uuid;

    #[test]
    fn recency_becomes_portable_order_and_selects_every_lod_rung() {
        let mut graph = Graph::new();
        let keys: Vec<_> = (1..=4)
            .map(|id| {
                add_node(
                    &mut graph,
                    Some(Uuid::from_u128(id)),
                    format!("fixture://{id}"),
                    PortablePoint::zero(),
                )
            })
            .collect();
        for (offset, key) in keys.iter().enumerate() {
            let node_id = graph.get_node(*key).unwrap().id;
            apply_graph_delta(
                &mut graph,
                GraphDelta::ReplayTouchNodeLastVisitedById {
                    node_id,
                    timestamp_ms: offset as u64,
                },
            );
        }
        let extents = HashMap::from([
            (keys[0], (36.0, 36.0)),
            (keys[1], (52.0, 52.0)),
            (keys[2], (80.0, 80.0)),
            (keys[3], (88.0, 88.0)),
        ]);
        let projected = project_spiral_score(&graph, Some(&extents), Some(keys[3]), true);
        assert_eq!(
            projected.score.items[0].source.id,
            Uuid::from_u128(4).to_string()
        );
        assert_eq!(
            projected.score.items[0].representation,
            Representation::LivePane
        );
        assert!(
            projected
                .score
                .items
                .iter()
                .any(|item| item.representation == Representation::Card)
        );
        assert!(
            projected
                .score
                .items
                .iter()
                .any(|item| item.representation == Representation::Snapshot)
        );
        assert!(
            projected
                .score
                .items
                .iter()
                .any(|item| item.representation == Representation::Glyph)
        );
        assert_eq!(projected.projection.nodes.len(), 4);
    }
}
