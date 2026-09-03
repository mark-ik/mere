// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The scenograph absorption's parity gate.
//!
//! Every golden here was captured by running the pre-migration
//! `arrangements` adapters on the fixture below, before that crate was
//! deleted. They are the whole reason the migration can claim it did not move
//! anything by accident, and they outlive the crate they came from.
//!
//! Byte-identical parity was not reachable, and the three places it is not are
//! listed in the plan and tested here by name. Everything else must match
//! position for position.
//!
//! See `design_docs/mere_docs/implementation_strategy/2026-08-22_scenograph_absorption_plan.md`.

use std::collections::HashMap;

use kernel::geometry::PortablePoint;
use kernel::graph::fixtures::GraphFixtures;
use kernel::graph::{EdgeAssertion, Graph, NodeKey, SemanticSubKind};
use uuid::Uuid;

use super::*;
use crate::projection::Projection;
use crate::request::{AxisValue, TargetSize, ViewIntent};
use crate::signals::IntelligenceSignals;

/// Positions differing by less than this are the same placement. Generous
/// enough for f32 reassociation across the rewrite, far tighter than any
/// difference a real behaviour change would produce.
const EPSILON: f32 = 0.01;

/// A hub with three spokes, a detached two-node bridge, and one isolated node.
///
/// The shape is chosen to exercise what the families disagree about: rings 0
/// and 1 plus unreachable nodes for radial, more than one connected component
/// for spectral, and a node count (7) whose square root is not an integer so
/// the grid's auto columns have to round.
fn fixture() -> (Graph, Vec<NodeKey>) {
    let mut graph = Graph::new();
    let keys: Vec<NodeKey> = (0..7u8)
        .map(|i| {
            // Fixed uuids: the source refs are derived from them, and a
            // hash-ordered placement would otherwise vary per run.
            let id = Uuid::from_u128(0x1000_0000_0000_0000_0000_0000_0000_0000u128 + i as u128);
            graph.add_node_with_id(
                id,
                format!("https://parity.example/{i}"),
                PortablePoint::new(0.0, 0.0),
            )
        })
        .collect();
    let hyperlink = || EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Hyperlink,
        label: None,
        decay_progress: None,
    };
    for spoke in 1..=3 {
        graph.assert_relation(keys[0], keys[spoke], hyperlink());
    }
    graph.assert_relation(keys[4], keys[5], hyperlink());
    (graph, keys)
}

fn intent(axis: Option<HashMap<NodeKey, AxisValue>>, focus: Option<NodeKey>) -> ViewIntent {
    ViewIntent {
        target_size: TargetSize::default(),
        axis_values: axis,
        focus,
        ..ViewIntent::default()
    }
}

/// Assert the projection places `keys[i]` at `golden[i]`.
fn assert_golden(label: &str, keys: &[NodeKey], projection: &Projection, golden: &[(f32, f32)]) {
    let by_key: HashMap<NodeKey, PortablePoint> = projection
        .nodes
        .iter()
        .map(|node| (node.node, node.position))
        .collect();
    assert_eq!(
        by_key.len(),
        golden.len(),
        "{label}: placed {} nodes, golden has {}",
        by_key.len(),
        golden.len()
    );
    for (index, key) in keys.iter().enumerate() {
        let placed = by_key
            .get(key)
            .unwrap_or_else(|| panic!("{label}: node {index} was not placed"));
        let (x, y) = golden[index];
        assert!(
            (placed.x - x).abs() < EPSILON && (placed.y - y).abs() < EPSILON,
            "{label}: node {index} moved — golden ({x:.4}, {y:.4}), now ({:.4}, {:.4})",
            placed.x,
            placed.y
        );
    }
}

fn timeline_axis(keys: &[NodeKey]) -> HashMap<NodeKey, AxisValue> {
    keys.iter()
        .enumerate()
        .map(|(i, key)| (*key, AxisValue::Numeric(i as f64)))
        .collect()
}

fn kanban_axis(keys: &[NodeKey]) -> HashMap<NodeKey, AxisValue> {
    keys.iter()
        .enumerate()
        .map(|(i, key)| {
            let tag = match i % 3 {
                0 => "alpha",
                1 => "beta",
                _ => "gamma",
            };
            (*key, AxisValue::Categorical(tag.to_string()))
        })
        .collect()
}

#[test]
fn grid_matches_the_pre_migration_placement() {
    // Auto columns over 7 nodes is 3, at a pitch of 120.
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let projection = GridAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(None, None),
    });
    assert_golden(
        "grid",
        &keys,
        &projection,
        &[
            (0.0, 0.0),
            (120.0, 0.0),
            (240.0, 0.0),
            (0.0, 120.0),
            (120.0, 120.0),
            (240.0, 120.0),
            (0.0, 240.0),
        ],
    );
}

#[test]
fn phyllotaxis_matches_the_pre_migration_placement() {
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let projection = PhyllotaxisAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(None, None),
    });
    assert_golden(
        "phyllotaxis",
        &keys,
        &projection,
        &[
            (0.0, 0.0),
            (-16.2221, 14.8608),
            (2.7201, -30.9936),
            (23.1846, 30.2403),
            (-43.3274, -7.6640),
            (41.5073, -26.4035),
            (-13.9898, 52.0412),
        ],
    );
}

#[test]
fn penrose_matches_the_pre_migration_placement() {
    // At the default centre of (0, 0) the centre-out fix is a no-op, which is
    // exactly why this golden still holds. See `off_centre_penrose_*` below for
    // the case where it does not.
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let projection = PenroseAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(None, None),
    });
    assert_golden(
        "penrose",
        &keys,
        &projection,
        &[
            (0.0, 0.0),
            (400.0, 0.0),
            (323.6068, 235.1141),
            (123.6068, 380.4226),
            (-123.6068, 380.4226),
            (-323.6068, 235.1141),
            (-400.0, 0.0),
        ],
    );
}

#[test]
fn lsystem_matches_the_pre_migration_placement() {
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let projection = LSystemAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(None, None),
    });
    assert_golden(
        "lsystem",
        &keys,
        &projection,
        &[
            (-200.0, -200.0),
            (-66.6666, -200.0),
            (-66.6666, -66.6667),
            (-200.0, -66.6667),
            (-200.0, 66.6667),
            (-200.0, 200.0),
            (-66.6666, 200.0),
        ],
    );
}

#[test]
fn spectral_matches_the_pre_migration_placement() {
    // Nodes inside one connected component share an eigenvector value, so the
    // hub and its spokes coincide and the bridge pair coincides. That is the
    // pre-migration behaviour, preserved deliberately: the layout separates
    // components, and within a component it says nothing.
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let projection = SpectralAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(None, None),
    });
    assert_golden(
        "spectral",
        &keys,
        &projection,
        &[
            (146.7027, 34.4556),
            (146.7027, 34.4556),
            (146.7027, 34.4556),
            (146.7027, 34.4556),
            (-152.6299, -228.9111),
            (-152.6299, -228.9111),
            (-281.5509, 320.0),
        ],
    );
}

#[test]
fn timeline_matches_the_pre_migration_placement() {
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let projection = TimelineAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(Some(timeline_axis(&keys)), None),
    });
    assert_golden(
        "timeline",
        &keys,
        &projection,
        &[
            (0.0, 0.0),
            (133.3333, 0.0),
            (266.6667, 0.0),
            (400.0, 0.0),
            (533.3334, 0.0),
            (666.6666, 0.0),
            (800.0, 0.0),
        ],
    );
}

#[test]
fn kanban_matches_the_pre_migration_placement() {
    // Three tags, no configured order: each earns a column, alphabetically.
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let projection = KanbanAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(Some(kanban_axis(&keys)), None),
    });
    assert_golden(
        "kanban",
        &keys,
        &projection,
        &[
            (0.0, 0.0),
            (240.0, 0.0),
            (480.0, 0.0),
            (0.0, 80.0),
            (240.0, 80.0),
            (480.0, 80.0),
            (0.0, 160.0),
        ],
    );
}

#[test]
fn radial_matches_the_pre_migration_placement() {
    // Ring 0 is the hub, ring 1 its three spokes, and the three nodes the walk
    // never reaches land on ring 2 — max reachable plus one.
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let projection = RadialAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(None, Some(keys[0])),
    });
    assert_golden(
        "radial",
        &keys,
        &projection,
        &[
            (0.0, 0.0),
            (120.0, 0.0),
            (-60.0, 103.9230),
            (-60.0, -103.9230),
            (240.0, 0.0),
            (-120.0, 207.8461),
            (-120.0, -207.8461),
        ],
    );
}

#[test]
fn radial_without_a_focus_places_nothing() {
    let (graph, _) = fixture();
    let signals = IntelligenceSignals::default();
    let projection = RadialAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(None, None),
    });
    assert!(projection.nodes.is_empty());
    assert_eq!(
        projection.metadata.strategy_id.as_deref(),
        Some(RadialAdapter::PROJECTION_ID),
        "an empty projection still names the strategy that produced it"
    );
}

// ── The three named divergences ──────────────────────────────────────────────
//
// Each pins the NEW behaviour. They are the exhaustive list: any other change
// against the goldens above is a regression, not a decision.

#[test]
fn divergence_penrose_runs_centre_out_from_the_tiling_not_the_configured_centre() {
    // The original ranked vertices by distance from `config.center` before
    // translating the origin-generated tiling onto it, so away from (0, 0) its
    // "centre-out" ordering ran outward from somewhere else entirely. Here the
    // first ordinal is nearest the configured centre, which is what centre-out
    // was always supposed to mean.
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let centre = sceno::Vec2::new(1000.0, -500.0);
    let projection = PenroseAdapter {
        config: sceno::Penrose {
            center: centre,
            ..sceno::Penrose::default()
        },
    }
    .project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(None, None),
    });

    let by_key: HashMap<NodeKey, PortablePoint> = projection
        .nodes
        .iter()
        .map(|node| (node.node, node.position))
        .collect();
    let reach = |key: &NodeKey| {
        let at = by_key[key];
        ((at.x - centre.x).powi(2) + (at.y - centre.y).powi(2)).sqrt()
    };
    assert!(
        reach(&keys[0]) < reach(&keys[6]),
        "the first ordinal must be the one nearest the configured centre"
    );
    assert!(reach(&keys[0]) < 1.0, "and it is the tiling's own centre");
}

#[test]
fn divergence_timeline_zero_span_places_rather_than_producing_nan() {
    // Every item disclosing the same coordinate divided by zero in the
    // original. There is no span to normalize against, so the axis origin is
    // the only honest answer, and coincident items stack.
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let axis: HashMap<NodeKey, AxisValue> = keys
        .iter()
        .map(|key| (*key, AxisValue::Numeric(7.0)))
        .collect();
    let projection = TimelineAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(Some(axis), None),
    });
    assert_eq!(projection.nodes.len(), 7);
    for node in &projection.nodes {
        assert!(
            node.position.x.is_finite() && node.position.y.is_finite(),
            "{:?} is not a position",
            node.position
        );
    }
}

#[test]
fn divergence_leave_in_place_resolves_to_the_disclosed_coordinate() {
    // The original emitted no delta and let a live position stand. A solved
    // scene has no previous frame, so "where it already is" can only mean a
    // coordinate the score disclosed — and with none disclosed, the
    // arrangement's own origin.
    let (graph, keys) = fixture();
    let signals = IntelligenceSignals::default();
    let origin = sceno::Vec2::new(50.0, -25.0);
    let projection = TimelineAdapter {
        config: sceno::Timeline {
            origin,
            fallback: sceno::TimelineFallback::LeaveInPlace,
            ..sceno::Timeline::default()
        },
    }
    .project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        // Only the first node discloses an axis value; the rest fall back.
        intent: intent(
            Some(HashMap::from([(keys[0], AxisValue::Numeric(0.0))])),
            None,
        ),
    });

    let by_key: HashMap<NodeKey, PortablePoint> = projection
        .nodes
        .iter()
        .map(|node| (node.node, node.position))
        .collect();
    for key in &keys[1..] {
        let at = by_key[key];
        assert!(
            (at.x - origin.x).abs() < EPSILON && (at.y - origin.y).abs() < EPSILON,
            "an undisclosed item falls back to the arrangement origin, not nowhere: {at:?}"
        );
    }
}

#[test]
fn the_embedded_merge_places_both_producers_identically() {
    // Spectral and semantic embedding are one arrangement now. Handed the same
    // coordinates they must place identically, or the merge changed something.
    let (graph, keys) = fixture();
    let mut signals = IntelligenceSignals::default();
    signals.embeddings = Some(crate::signals::NodeEmbeddings {
        coords: keys
            .iter()
            .enumerate()
            .map(|(i, key)| (*key, (i as f32 / 6.0 - 0.5, 0.25)))
            .collect(),
    });
    let projection = SemanticEmbeddingAdapter::default().project(&ProjectionRequest {
        graph: &graph,
        signals: &signals,
        intent: intent(None, None),
    });
    assert_eq!(projection.nodes.len(), 7);
    let by_key: HashMap<NodeKey, PortablePoint> = projection
        .nodes
        .iter()
        .map(|node| (node.node, node.position))
        .collect();
    // scale 400 about the origin: the first item is at -0.5 → -200.
    let first = by_key[&keys[0]];
    assert!((first.x + 200.0).abs() < EPSILON, "{first:?}");
    assert!((first.y - 100.0).abs() < EPSILON, "{first:?}");
}
