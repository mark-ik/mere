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

use crate::representation::{RepresentationState, default_graph_representation_registry};
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
    project_spiral_score_for_view(graph, extents, focus, recent_first, 1.0, None)
}

/// Build and realize Mere's P3 pane-spiral score for one declared view.
///
/// Representation conditions stay in Cartography's host registry. The score
/// records only the selected rung. `previous` supplies prior selections for
/// hysteresis; it never changes source identity, order, placement, or geometry.
pub fn project_spiral_score_for_view(
    graph: &Graph,
    extents: Option<&HashMap<NodeKey, (f32, f32)>>,
    focus: Option<NodeKey>,
    recent_first: bool,
    zoom_level: f32,
    previous: Option<&Score>,
) -> MereSpiralProjection {
    let mut ordered: Vec<NodeKey> = graph.nodes().map(|(key, _)| key).collect();
    if recent_first {
        ordered.sort_by_key(|key| {
            let node = graph
                .get_node(*key)
                .expect("node keys came from this graph");
            (
                std::cmp::Reverse(
                    graph
                        .node_last_visited(*key)
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                ),
                // Visit facets persist millisecond timestamps, so nodes created
                // in one tick commonly tie. Stable identity makes that score
                // order portable instead of inheriting local graph iteration.
                std::cmp::Reverse(node.id),
            )
        });
    }

    let recency = normalized_recency(graph, &ordered);
    let registry = default_graph_representation_registry();
    let previous: HashMap<&str, &Representation> = previous
        .into_iter()
        .flat_map(|score| score.items.iter())
        .filter(|item| item.source.adapter == MERE_GRAPH_ADAPTER)
        .map(|item| (item.source.id.as_str(), &item.representation))
        .collect();
    let zoom_level = if zoom_level.is_finite() && zoom_level > 0.0 {
        zoom_level
    } else {
        1.0
    };
    let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
    score.generation = graph.revision();
    for (ordinal, key) in ordered.iter().enumerate() {
        let node = graph
            .get_node(*key)
            .expect("node keys came from this graph");
        let extent = extents
            .and_then(|items| items.get(key).copied())
            .unwrap_or((0.0, 0.0));
        let source = SourceRef::new(MERE_GRAPH_ADAPTER, node.id.to_string());
        let profile = registry.resolve_classes(node.tags.iter().map(String::as_str));
        let state = RepresentationState {
            screen_width: extent.0 * zoom_level,
            screen_height: extent.1 * zoom_level,
            zoom_level,
            recency: recency.get(key).copied().unwrap_or(0.0),
            focused: focus == Some(*key),
        };
        score.items.push(ScoreItem {
            representation: profile
                .ladder
                .select(state, previous.get(source.id.as_str()).copied()),
            source,
            ordinal: ordinal as u32,
            footprint: footprint_for(extent),
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

fn normalized_recency(graph: &Graph, keys: &[NodeKey]) -> HashMap<NodeKey, f32> {
    let times: Vec<_> = keys
        .iter()
        .copied()
        .map(|key| {
            let seconds = graph
                .node_last_visited(key)
                .and_then(|time| time.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs_f64())
                .unwrap_or(0.0);
            (key, seconds)
        })
        .collect();
    let minimum = times
        .iter()
        .map(|(_, time)| *time)
        .fold(f64::INFINITY, f64::min);
    let maximum = times
        .iter()
        .map(|(_, time)| *time)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = maximum - minimum;
    times
        .into_iter()
        .map(|(key, time)| {
            let value = if span.is_finite() && span > f64::EPSILON {
                ((time - minimum) / span) as f32
            } else {
                1.0
            };
            (key, value)
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
    use uuid::Uuid;

    #[test]
    fn recency_becomes_portable_order_and_selects_declared_lod_rungs() {
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
                .any(|item| item.representation == Representation::Glyph)
        );
        assert_eq!(projected.projection.nodes.len(), 4);
    }

    #[test]
    fn one_graph_at_two_zooms_selects_different_rungs_without_moving_items() {
        let mut graph = Graph::new();
        let newest = add_node(
            &mut graph,
            Some(Uuid::from_u128(1)),
            "fixture://newest".to_string(),
            PortablePoint::zero(),
        );
        let oldest = add_node(
            &mut graph,
            Some(Uuid::from_u128(2)),
            "fixture://oldest".to_string(),
            PortablePoint::zero(),
        );
        for (key, timestamp_ms) in [(oldest, 1), (newest, 2)] {
            let node_id = graph.get_node(key).unwrap().id;
            apply_graph_delta(
                &mut graph,
                GraphDelta::ReplayTouchNodeLastVisitedById {
                    node_id,
                    timestamp_ms,
                },
            );
        }
        let extents = HashMap::from([(newest, (64.0, 64.0)), (oldest, (64.0, 64.0))]);

        let near = project_spiral_score_for_view(&graph, Some(&extents), None, true, 1.0, None);
        let far = project_spiral_score_for_view(&graph, Some(&extents), None, true, 0.5, None);

        assert_eq!(near.score.items[0].representation, Representation::Card);
        assert_eq!(far.score.items[0].representation, Representation::Glyph);
        assert_eq!(near.projection, far.projection);
        assert_eq!(near.score.items[0].source, far.score.items[0].source);
        assert_eq!(near.score.items[0].ordinal, far.score.items[0].ordinal);
    }

    #[test]
    fn an_unmeasured_item_does_not_claim_a_card() {
        let mut graph = Graph::new();
        add_node(
            &mut graph,
            Some(Uuid::from_u128(1)),
            "fixture://one".to_string(),
            PortablePoint::zero(),
        );

        let projected = project_spiral_score_for_view(&graph, None, None, true, 2.0, None);
        assert_eq!(
            projected.score.items[0].representation,
            Representation::Glyph
        );
        assert_eq!(projected.score.items[0].footprint, Footprint::Point);
    }

    #[test]
    fn prior_score_supplies_hysteresis_and_focus_stays_live() {
        let mut graph = Graph::new();
        let key = add_node(
            &mut graph,
            Some(Uuid::from_u128(1)),
            "fixture://one".to_string(),
            PortablePoint::zero(),
        );
        let extents = HashMap::from([(key, (64.0, 64.0))]);
        let card = project_spiral_score_for_view(&graph, Some(&extents), None, true, 1.0, None);
        let retained = project_spiral_score_for_view(
            &graph,
            Some(&extents),
            None,
            true,
            0.95,
            Some(&card.score),
        );
        let released = project_spiral_score_for_view(
            &graph,
            Some(&extents),
            None,
            true,
            0.89,
            Some(&retained.score),
        );
        let focused = project_spiral_score_for_view(
            &graph,
            Some(&extents),
            Some(key),
            true,
            0.2,
            Some(&released.score),
        );

        assert_eq!(retained.score.items[0].representation, Representation::Card);
        assert_eq!(
            released.score.items[0].representation,
            Representation::Glyph
        );
        assert_eq!(
            focused.score.items[0].representation,
            Representation::LivePane
        );
    }

    #[test]
    fn equal_recency_uses_stable_identity_order() {
        let mut graph = Graph::new();
        let keys: Vec<_> = (1..=3)
            .map(|id| {
                add_node(
                    &mut graph,
                    Some(Uuid::from_u128(id)),
                    format!("fixture://{id}"),
                    PortablePoint::zero(),
                )
            })
            .collect();
        for key in keys {
            let node_id = graph.get_node(key).unwrap().id;
            apply_graph_delta(
                &mut graph,
                GraphDelta::ReplayTouchNodeLastVisitedById {
                    node_id,
                    timestamp_ms: 7,
                },
            );
        }

        let projected = project_spiral_score(&graph, None, None, true);
        let ids: Vec<_> = projected
            .score
            .items
            .iter()
            .map(|item| item.source.id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                Uuid::from_u128(3).to_string(),
                Uuid::from_u128(2).to_string(),
                Uuid::from_u128(1).to_string(),
            ]
        );
    }
}
