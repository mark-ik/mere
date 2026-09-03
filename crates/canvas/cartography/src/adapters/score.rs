// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The bridge every adapter crosses: graph in, [`sceno::Score`] out, then
//! [`scenomise`]'s scene back to a [`Projection`].
//!
//! This replaces `arrangements`' `run_static_layout_one_shot` and its "origin
//! trick" — build the scene with every node at the origin and `damping = 1.0`
//! so the returned *delta* equals the absolute target. That existed only
//! because `Layout::step` returned deltas and there was no other way to ask an
//! analytic layout for an answer in one call. `scenomise::solve` returns
//! absolute positions, so the trick has nothing left to work around.

use std::collections::HashMap;

use kernel::graph::NodeKey;
use sceno::{Arrangement, Footprint, Representation, Score, ScoreItem, Size2, SourceRef, Vec2};

use crate::projection::{PositionedEdge, PositionedNode, Projection, ProjectionMetadata};
use crate::request::{AxisValue, ProjectionRequest};
use crate::scene_out::MERE_GRAPH_ADAPTER;
use kernel::geometry::{PortablePoint, PortableRect, PortableSize};

/// What a producer computed and is disclosing to the solver.
///
/// Each map is keyed by node. A node absent from a map disclosed nothing, which
/// is distinct from disclosing a zero — every arrangement that reads these
/// treats the two differently.
#[derive(Default)]
pub struct Disclosures {
    pub axis: Option<HashMap<NodeKey, AxisValue>>,
    pub embedding: Option<HashMap<NodeKey, Vec2>>,
    pub weight: Option<HashMap<NodeKey, f32>>,
}

impl Disclosures {
    /// Read the axis values the caller threaded onto the request.
    pub fn from_intent(request: &ProjectionRequest<'_>) -> Self {
        Self {
            axis: request.intent.axis_values.clone(),
            ..Self::default()
        }
    }

    pub fn with_axis(mut self, axis: HashMap<NodeKey, AxisValue>) -> Self {
        self.axis = Some(axis);
        self
    }

    pub fn with_embedding(mut self, embedding: HashMap<NodeKey, Vec2>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    pub fn with_weight(mut self, weight: HashMap<NodeKey, f32>) -> Self {
        self.weight = Some(weight);
        self
    }
}

/// Cartography's axis vocabulary is deliberately its own; this is the one place
/// it meets sceno's.
fn axis_to_sceno(value: &AxisValue) -> sceno::AxisValue {
    match value {
        AxisValue::Numeric(at) => sceno::AxisValue::Numeric(*at),
        AxisValue::Categorical(tag) => sceno::AxisValue::Categorical(tag.clone()),
    }
}

/// Build a score over every node in the request's graph.
///
/// Ordinal follows graph enumeration order, which is what the analytic families
/// place by. The returned key vector is in score order so the caller can map the
/// solved scene back without re-deriving anything.
pub fn score_from_request(
    request: &ProjectionRequest<'_>,
    arrangement: Arrangement,
    disclosures: &Disclosures,
) -> (Score, Vec<NodeKey>) {
    let mut score = Score::new(arrangement);
    let mut keys = Vec::new();

    for (ordinal, (key, node)) in request.graph.nodes().enumerate() {
        let (width, height) = request
            .intent
            .extents
            .as_ref()
            .and_then(|extents| extents.get(&key))
            .copied()
            .unwrap_or((0.0, 0.0));
        score.items.push(ScoreItem {
            source: SourceRef::new(MERE_GRAPH_ADAPTER, node.id.to_string()),
            ordinal: ordinal as u32,
            footprint: if width > 0.0 && height > 0.0 {
                Footprint::Rect {
                    size: Size2::new(width, height),
                }
            } else {
                Footprint::Point
            },
            representation: Representation::Glyph,
            placement: sceno::Placement::Ordinal,
            layer: 0,
            visible: true,
            axis: disclosures
                .axis
                .as_ref()
                .and_then(|axis| axis.get(&key))
                .map(axis_to_sceno),
            embedding: disclosures
                .embedding
                .as_ref()
                .and_then(|embedding| embedding.get(&key))
                .copied(),
            weight: disclosures
                .weight
                .as_ref()
                .and_then(|weight| weight.get(&key))
                .copied(),
        });
        keys.push(key);
    }

    (score, keys)
}

/// Solve `score` and translate the resulting scene into a projection.
///
/// `keys` must be in score order, as [`score_from_request`] returns it. The
/// solver sorts by `(ordinal, index)` and the score's ordinals are the
/// enumeration order, so scene item `i` is `keys[i]`.
pub fn project_score(
    strategy_id: &str,
    request: &ProjectionRequest<'_>,
    score: &Score,
    keys: &[NodeKey],
) -> Projection {
    let scene = scenomise::solve(score);
    let positions: HashMap<NodeKey, PortablePoint> = scene
        .items
        .iter()
        .zip(keys)
        .map(|(item, key)| {
            (
                *key,
                PortablePoint::new(item.transform.translate.x, item.transform.translate.y),
            )
        })
        .collect();
    projection_from_positions(strategy_id, request, positions)
}

/// An empty projection that still names the strategy that produced it, so a
/// caller can tell "this strategy had nothing to place" from "no strategy ran".
pub fn empty_projection(strategy_id: &str) -> Projection {
    Projection {
        metadata: ProjectionMetadata {
            strategy_id: Some(strategy_id.to_string()),
            settled: true,
        },
        ..Projection::empty()
    }
}

/// Build a projection from absolute node positions, with edges from the
/// request's graph and bounds from the positions.
pub fn projection_from_positions(
    strategy_id: &str,
    request: &ProjectionRequest<'_>,
    positions: HashMap<NodeKey, PortablePoint>,
) -> Projection {
    let nodes: Vec<PositionedNode> = positions
        .iter()
        .map(|(key, position)| PositionedNode {
            node: *key,
            position: *position,
            radius: 0.0,
        })
        .collect();
    let edges = build_positioned_edges(request, &positions);
    Projection {
        nodes,
        edges,
        overlays: Vec::new(),
        minimap: None,
        content_bounds: bounds_of(&positions),
        metadata: ProjectionMetadata {
            strategy_id: Some(strategy_id.to_string()),
            settled: true,
        },
    }
}

/// Graph edges as positioned edges. Self-loops are dropped; no analytic family
/// renders them.
pub fn build_positioned_edges(
    request: &ProjectionRequest<'_>,
    positions: &HashMap<NodeKey, PortablePoint>,
) -> Vec<PositionedEdge> {
    request
        .graph
        .relations()
        .filter(|view| view.from != view.to)
        .map(|view| PositionedEdge {
            edge: None,
            from: view.from,
            to: view.to,
            path: vec![
                positions.get(&view.from).copied().unwrap_or_default(),
                positions.get(&view.to).copied().unwrap_or_default(),
            ],
            weight: 1.0,
        })
        .collect()
}

/// Axis-aligned bounding box around a set of positions; a zero rect when empty.
pub fn bounds_of(positions: &HashMap<NodeKey, PortablePoint>) -> PortableRect {
    let mut values = positions.values();
    let Some(first) = values.next() else {
        return PortableRect::zero();
    };
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first.x, first.y, first.x, first.y);
    for position in values {
        min_x = min_x.min(position.x);
        min_y = min_y.min(position.y);
        max_x = max_x.max(position.x);
        max_y = max_y.max(position.y);
    }
    PortableRect::new(
        PortablePoint::new(min_x, min_y),
        PortableSize::new((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
    )
}
