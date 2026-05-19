/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scene region effect pass: attractor / repulsor / dampener / wall.
//!
//! For each node, sums deltas from each visible scene region whose effect
//! and shape touch the node. The Phase 6 follow-up (per the field-algebra
//! plan) re-grounds these as lowerings of the field algebra; today they
//! ship as bespoke math kept stable for behavior parity.

use euclid::default::{Point2D, Vector2D};
use std::collections::HashMap;
use std::hash::Hash;

use crate::scene_region::{SceneRegion, SceneRegionEffect, SceneRegionShape};

use super::NodeSnapshot;

/// Compute region effect deltas for all nodes.
///
/// Applies attractor/repulsor/dampener/wall effects from scene regions.
/// Returns a map of node ID → position delta.
pub fn compute_region_effects<N: Clone + Eq + Hash>(
    nodes: &[NodeSnapshot<N>],
    regions: &[SceneRegion],
    padding: f32,
    effect_scale: f32,
    containment_response_scale: f32,
    max_delta: f32,
) -> HashMap<N, Vector2D<f32>> {
    let mut deltas = HashMap::new();
    for node in nodes {
        if node.pinned {
            continue;
        }
        let mut total_delta = Vector2D::new(0.0f32, 0.0);
        for region in regions {
            if !region.visible {
                continue;
            }
            total_delta += region_delta_for_node(
                region,
                node.position,
                node.radius + padding,
                containment_response_scale,
            ) * effect_scale;
        }
        if total_delta.length() > max_delta {
            total_delta = total_delta.normalize() * max_delta;
        }
        if total_delta.length() > f32::EPSILON {
            deltas.insert(node.id.clone(), total_delta);
        }
    }
    deltas
}

/// Compute the position delta a region exerts on a single node.
fn region_delta_for_node(
    region: &SceneRegion,
    position: Point2D<f32>,
    padded_radius: f32,
    containment_response_scale: f32,
) -> Vector2D<f32> {
    match region.effect {
        SceneRegionEffect::Attractor { strength } => {
            if !region.shape.contains(position) {
                return Vector2D::zero();
            }
            let center = region.shape.center();
            (center - position) * strength
        }
        SceneRegionEffect::Repulsor { strength } => {
            if !region.shape.contains(position) {
                return Vector2D::zero();
            }
            let center = region.shape.center();
            let away = position - center;
            if away.length() <= f32::EPSILON {
                Vector2D::new(strength.max(1.0), 0.0)
            } else {
                away.normalize() * strength
            }
        }
        SceneRegionEffect::Dampener { factor } => {
            if !region.shape.contains(position) {
                return Vector2D::zero();
            }
            let center = region.shape.center();
            (center - position) * -(factor.abs() * 0.1)
        }
        SceneRegionEffect::Wall => {
            wall_pushout_delta(&region.shape, position, padded_radius) * containment_response_scale
        }
    }
}

/// Compute the pushout delta for a wall region.
fn wall_pushout_delta(
    shape: &SceneRegionShape,
    position: Point2D<f32>,
    padded_radius: f32,
) -> Vector2D<f32> {
    match *shape {
        SceneRegionShape::Circle { center, radius } => {
            let delta = position - center;
            let distance = delta.length();
            let min_distance = radius + padded_radius;
            if distance >= min_distance {
                return Vector2D::zero();
            }
            let normal = if distance > f32::EPSILON {
                delta / distance
            } else {
                Vector2D::new(1.0, 0.0)
            };
            normal * (min_distance - distance)
        }
        SceneRegionShape::Rect { rect } => {
            let expanded = rect.inflate(padded_radius, padded_radius);
            if !expanded.contains(position) {
                return Vector2D::zero();
            }
            let left = position.x - rect.origin.x;
            let right = (rect.origin.x + rect.size.width) - position.x;
            let top = position.y - rect.origin.y;
            let bottom = (rect.origin.y + rect.size.height) - position.y;
            let min_side = left.min(right).min(top).min(bottom);
            if (min_side - left).abs() <= f32::EPSILON {
                Vector2D::new(-(left + padded_radius), 0.0)
            } else if (min_side - right).abs() <= f32::EPSILON {
                Vector2D::new(right + padded_radius, 0.0)
            } else if (min_side - top).abs() <= f32::EPSILON {
                Vector2D::new(0.0, -(top + padded_radius))
            } else {
                Vector2D::new(0.0, bottom + padded_radius)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_region::SceneRegionId;

    fn snap(id: u32, x: f32, y: f32, radius: f32) -> NodeSnapshot<u32> {
        NodeSnapshot {
            id,
            position: Point2D::new(x, y),
            radius,
            pinned: false,
        }
    }

    #[test]
    fn attractor_pulls_toward_center() {
        let nodes = vec![snap(0, 120.0, 100.0, 10.0)];
        let regions = vec![SceneRegion::circle(
            SceneRegionId(1),
            Point2D::new(100.0, 100.0),
            50.0,
            SceneRegionEffect::Attractor { strength: 0.5 },
        )];
        let deltas = compute_region_effects(&nodes, &regions, 0.0, 1.0, 1.0, 18.0);
        assert!(deltas.contains_key(&0));
        assert!(deltas[&0].x < 0.0);
    }

    #[test]
    fn repulsor_pushes_away_from_center() {
        let nodes = vec![snap(0, 110.0, 100.0, 10.0)];
        let regions = vec![SceneRegion::circle(
            SceneRegionId(1),
            Point2D::new(100.0, 100.0),
            50.0,
            SceneRegionEffect::Repulsor { strength: 2.0 },
        )];
        let deltas = compute_region_effects(&nodes, &regions, 0.0, 1.0, 1.0, 18.0);
        assert!(deltas.contains_key(&0));
        assert!(deltas[&0].x > 0.0);
    }

    #[test]
    fn node_outside_region_unaffected_by_attractor() {
        let nodes = vec![snap(0, 200.0, 200.0, 10.0)];
        let regions = vec![SceneRegion::circle(
            SceneRegionId(1),
            Point2D::new(100.0, 100.0),
            30.0,
            SceneRegionEffect::Attractor { strength: 0.5 },
        )];
        let deltas = compute_region_effects(&nodes, &regions, 0.0, 1.0, 1.0, 18.0);
        assert!(deltas.is_empty());
    }

    #[test]
    fn invisible_region_has_no_effect() {
        let nodes = vec![snap(0, 100.0, 100.0, 10.0)];
        let mut region = SceneRegion::circle(
            SceneRegionId(1),
            Point2D::new(100.0, 100.0),
            50.0,
            SceneRegionEffect::Attractor { strength: 5.0 },
        );
        region.visible = false;
        let deltas = compute_region_effects(&nodes, &[region], 0.0, 1.0, 1.0, 18.0);
        assert!(deltas.is_empty());
    }

    #[test]
    fn wall_pushes_nearby_node_out() {
        let nodes = vec![snap(0, 105.0, 100.0, 10.0)];
        let regions = vec![SceneRegion::circle(
            SceneRegionId(1),
            Point2D::new(100.0, 100.0),
            20.0,
            SceneRegionEffect::Wall,
        )];
        let deltas = compute_region_effects(&nodes, &regions, 4.0, 1.0, 1.0, 50.0);
        assert!(deltas.contains_key(&0));
        assert!(deltas[&0].x > 0.0);
    }

    #[test]
    fn delta_clamped_to_max() {
        let nodes = vec![snap(0, 100.0, 100.0, 10.0)];
        let regions = vec![SceneRegion::circle(
            SceneRegionId(1),
            Point2D::new(100.0, 100.0),
            50.0,
            SceneRegionEffect::Repulsor { strength: 100.0 },
        )];
        let deltas = compute_region_effects(&nodes, &regions, 0.0, 1.0, 1.0, 5.0);
        if let Some(d) = deltas.get(&0) {
            assert!(d.length() <= 5.0 + 0.01);
        }
    }
}
