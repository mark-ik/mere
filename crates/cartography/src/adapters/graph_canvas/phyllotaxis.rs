/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! [`LayoutStrategy`] adapter for
//! [`graph_canvas::layout::Phyllotaxis`].
//!
//! Phyllotaxis is *analytic* — for a given node ordering, the
//! formula `angle_n = n × 137.508°`, `radius_n = scale × √n` produces
//! the same target positions in one call. No iteration, no state.
//!
//! That maps cleanly onto cartography's [`LayoutStrategy`] (one-shot)
//! rather than [`crate::StreamingLayoutStrategy`] (iterative): the
//! adapter just computes target positions from node ordinals and
//! emits them as a [`Projection`] with absolute positions.
//!
//! The existing `graph_canvas::layout::Phyllotaxis` uses an iterative
//! `Layout::step()` that returns *deltas* toward target positions —
//! to support eased-in motion via the shared `StaticLayoutState`'s
//! damping. The cartography adapter bypasses that machinery and emits
//! absolute targets directly; canvases that want eased motion
//! interpolate between two successive projections on their own.

use std::collections::HashMap;

use euclid::default::Point2D;
use graph_canvas::layout::{
    Phyllotaxis, PhyllotaxisConfig, PhyllotaxisRadiusCurve, SpiralOrientation,
};
use mere_kernel::graph::NodeKey;

use super::shared::{bounds_of, build_positioned_edges};
use crate::projection::{PositionedNode, Projection, ProjectionMetadata};
use crate::request::ProjectionRequest;
use crate::strategy::LayoutStrategy;

/// Cartography-side adapter for [`graph_canvas::layout::Phyllotaxis`].
///
/// Holds a [`PhyllotaxisConfig`] — center, scale, divergence angle,
/// radius curve, spiral orientation. The default config (golden-angle
/// divergence + square-root radius curve + outward orientation)
/// produces classic sunflower / phyllotaxis packing.
#[derive(Debug, Default, Clone, Copy)]
pub struct PhyllotaxisAdapter {
    pub config: PhyllotaxisConfig,
}

impl PhyllotaxisAdapter {
    pub const PROJECTION_ID: &'static str = "phyllotaxis.default";

    pub fn new(config: PhyllotaxisConfig) -> Self {
        Self { config }
    }
}

impl From<Phyllotaxis> for PhyllotaxisAdapter {
    fn from(p: Phyllotaxis) -> Self {
        Self { config: p.config }
    }
}

impl LayoutStrategy for PhyllotaxisAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        // Snapshot the graph's node order into a deterministic Vec.
        // Iteration order is `Graph::nodes()`'s order (petgraph
        // NodeIndex order); analytic strategies are deterministic in
        // that order.
        let ordered_keys: Vec<NodeKey> = request.graph.nodes().map(|(key, _)| key).collect();
        let n = ordered_keys.len();
        if n == 0 {
            return Projection {
                metadata: ProjectionMetadata {
                    strategy_id: Some(self.projection_id().to_string()),
                    settled: true,
                },
                ..Projection::empty()
            };
        }

        let mut positions: HashMap<NodeKey, Point2D<f32>> = HashMap::with_capacity(n);
        for (idx, key) in ordered_keys.iter().enumerate() {
            let ordinal = match self.config.orientation {
                SpiralOrientation::Outward => idx,
                SpiralOrientation::Inward => n - 1 - idx,
            };
            let radius =
                radius_from_ordinal(self.config.radius_curve, self.config.scale, ordinal);
            let angle = ordinal as f32 * self.config.angle_radians;
            let pos = Point2D::new(
                self.config.center.x + radius * angle.cos(),
                self.config.center.y + radius * angle.sin(),
            );
            positions.insert(*key, pos);
        }

        let nodes: Vec<PositionedNode> = positions
            .iter()
            .map(|(key, pos)| PositionedNode {
                node: *key,
                position: *pos,
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
                strategy_id: Some(self.projection_id().to_string()),
                // Analytic strategies produce a final projection in
                // one call. `settled = true` always.
                settled: true,
            },
        }
    }
}

fn radius_from_ordinal(curve: PhyllotaxisRadiusCurve, scale: f32, ordinal: usize) -> f32 {
    let n = ordinal as f32;
    scale
        * match curve {
            PhyllotaxisRadiusCurve::SquareRoot => n.sqrt(),
            PhyllotaxisRadiusCurve::Linear => n,
            PhyllotaxisRadiusCurve::Quadratic => n * n,
            PhyllotaxisRadiusCurve::Logarithmic => (1.0 + n).ln(),
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::ViewIntent;
    use crate::signals::IntelligenceSignals;
    use graph_canvas::layout::angles;
    use mere_kernel::geometry::PortablePoint;
    use mere_kernel::graph::Graph;
    use uuid::Uuid;

    fn small_graph() -> (Graph, Vec<NodeKey>) {
        let mut graph = Graph::new();
        let keys: Vec<NodeKey> = (0u128..5)
            .map(|i| {
                graph.add_node_with_id(
                    Uuid::from_u128(i + 1),
                    format!("test://n{i}"),
                    PortablePoint::new(0.0, 0.0),
                )
            })
            .collect();
        (graph, keys)
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = PhyllotaxisAdapter::default();
        assert_eq!(adapter.projection_id(), "phyllotaxis.default");
    }

    #[test]
    fn empty_graph_produces_empty_projection_marked_settled() {
        let graph = Graph::new();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = PhyllotaxisAdapter::default();
        let projection = adapter.project(&request);
        assert!(projection.nodes.is_empty());
        assert!(projection.metadata.settled);
    }

    #[test]
    fn project_places_every_node_with_unique_position() {
        let (graph, keys) = small_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = PhyllotaxisAdapter::default();
        let projection = adapter.project(&request);

        assert_eq!(projection.nodes.len(), keys.len());
        assert!(projection.metadata.settled);
        assert_eq!(
            projection.metadata.strategy_id.as_deref(),
            Some("phyllotaxis.default")
        );

        // Positions should all be distinct — golden-angle phyllotaxis
        // never revisits the same point.
        let mut positions: Vec<Point2D<f32>> =
            projection.nodes.iter().map(|n| n.position).collect();
        positions.sort_by(|a, b| {
            a.x.partial_cmp(&b.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
        });
        positions.dedup();
        assert_eq!(positions.len(), keys.len(), "positions should be distinct");
    }

    #[test]
    fn project_is_deterministic_for_same_graph() {
        let (graph, _) = small_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = PhyllotaxisAdapter::default();

        let p1 = adapter.project(&request);
        let p2 = adapter.project(&request);

        // Collect per-node position maps (order in `nodes` is HashMap
        // iteration order — compare by NodeKey).
        let map1: HashMap<NodeKey, Point2D<f32>> =
            p1.nodes.iter().map(|n| (n.node, n.position)).collect();
        let map2: HashMap<NodeKey, Point2D<f32>> =
            p2.nodes.iter().map(|n| (n.node, n.position)).collect();
        assert_eq!(map1, map2);
    }

    #[test]
    fn outward_and_inward_orientation_place_first_and_last_at_center() {
        let (graph, _) = small_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };

        // Outward: first node (idx=0) gets ordinal=0 → at center
        // (radius=0, since sqrt(0)=0).
        let outward = PhyllotaxisAdapter {
            config: PhyllotaxisConfig {
                center: Point2D::new(0.0, 0.0),
                scale: 22.0,
                angle_radians: angles::GOLDEN,
                radius_curve: PhyllotaxisRadiusCurve::SquareRoot,
                orientation: SpiralOrientation::Outward,
            },
        };
        let outward_proj = outward.project(&request);
        let first_key = graph.nodes().next().unwrap().0;
        let outward_first_pos = outward_proj
            .nodes
            .iter()
            .find(|n| n.node == first_key)
            .unwrap()
            .position;
        assert!(outward_first_pos.x.abs() < 0.001);
        assert!(outward_first_pos.y.abs() < 0.001);

        // Inward: last node (idx=n-1) gets ordinal=0 → at center.
        let inward = PhyllotaxisAdapter {
            config: PhyllotaxisConfig {
                center: Point2D::new(0.0, 0.0),
                scale: 22.0,
                angle_radians: angles::GOLDEN,
                radius_curve: PhyllotaxisRadiusCurve::SquareRoot,
                orientation: SpiralOrientation::Inward,
            },
        };
        let inward_proj = inward.project(&request);
        let last_key = graph.nodes().last().unwrap().0;
        let inward_last_pos = inward_proj
            .nodes
            .iter()
            .find(|n| n.node == last_key)
            .unwrap()
            .position;
        assert!(inward_last_pos.x.abs() < 0.001);
        assert!(inward_last_pos.y.abs() < 0.001);
    }

    #[test]
    fn linear_radius_curve_grows_proportional_to_ordinal() {
        // With Linear curve and scale=10, ordinal n → radius 10n.
        // At ordinal 0 the position is at center; at ordinal 1 the
        // position is exactly 10 units from center.
        let (graph, _) = small_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = PhyllotaxisAdapter {
            config: PhyllotaxisConfig {
                center: Point2D::new(0.0, 0.0),
                scale: 10.0,
                angle_radians: angles::GOLDEN,
                radius_curve: PhyllotaxisRadiusCurve::Linear,
                orientation: SpiralOrientation::Outward,
            },
        };
        let projection = adapter.project(&request);

        let first_key = graph.nodes().next().unwrap().0;
        let second_key = graph.nodes().nth(1).unwrap().0;
        let first_pos = projection
            .nodes
            .iter()
            .find(|n| n.node == first_key)
            .unwrap()
            .position;
        let second_pos = projection
            .nodes
            .iter()
            .find(|n| n.node == second_key)
            .unwrap()
            .position;
        assert!(first_pos.to_vector().length() < 0.001);
        assert!((second_pos.to_vector().length() - 10.0).abs() < 0.001);
    }

    #[test]
    fn from_existing_phyllotaxis_preserves_config() {
        let phyl = Phyllotaxis::new(PhyllotaxisConfig {
            center: Point2D::new(50.0, 50.0),
            scale: 7.0,
            angle_radians: angles::GOLDEN,
            radius_curve: PhyllotaxisRadiusCurve::Logarithmic,
            orientation: SpiralOrientation::Inward,
        });
        let adapter = PhyllotaxisAdapter::from(phyl);
        assert_eq!(adapter.config.center.x, 50.0);
        assert_eq!(adapter.config.scale, 7.0);
        assert_eq!(adapter.config.orientation, SpiralOrientation::Inward);
    }

    #[test]
    fn graph_truth_positions_are_never_mutated() {
        let (graph, keys) = small_graph();
        let original = graph.get_node(keys[0]).unwrap().projected_position();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = PhyllotaxisAdapter::default();
        // Project repeatedly — analytic strategy should produce same
        // result and never touch graph state.
        for _ in 0..5 {
            adapter.project(&request);
        }
        let after = graph.get_node(keys[0]).unwrap().projected_position();
        assert_eq!(original, after);
    }
}
