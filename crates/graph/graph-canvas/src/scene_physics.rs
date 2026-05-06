/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scene physics: node separation, viewport containment, region effects,
//! and simulation-feel motion profiles.
//!
//! These are pure functions that compute position deltas for a set of node
//! snapshots. The host applies the deltas to its own position storage.
//! No framework dependency, no mutation of graph truth.
//!
//! The motion profile and release-impulse system provide a lightweight
//! "physics feel" without a rigid-body engine. Full rigid-body simulation
//! belongs in host-side or sibling adapter crates.

use euclid::default::{Point2D, Rect, Vector2D};
use std::collections::HashMap;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::scene_region::{SceneRegion, SceneRegionEffect, SceneRegionId, SceneRegionShape};

/// Configuration for the scene physics pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenePhysicsConfig {
    /// Whether node-node separation is enabled.
    pub node_separation_enabled: bool,
    /// Whether viewport containment is enabled.
    pub viewport_containment_enabled: bool,
    /// Padding around nodes for collision detection.
    pub node_padding: f32,
    /// Scale factor for region effects.
    pub region_effect_scale: f32,
    /// Scale factor for containment response.
    pub containment_response_scale: f32,
    /// Number of separation passes per frame.
    pub separation_passes: u32,
    /// Maximum region-effect delta per pass.
    pub max_region_delta: f32,
}

impl Default for ScenePhysicsConfig {
    fn default() -> Self {
        Self {
            node_separation_enabled: false,
            viewport_containment_enabled: false,
            node_padding: 4.0,
            region_effect_scale: 1.0,
            containment_response_scale: 1.0,
            separation_passes: 3,
            max_region_delta: 18.0,
        }
    }
}

/// A snapshot of a node's spatial state for physics computation.
#[derive(Debug, Clone, Copy)]
pub struct NodeSnapshot<N> {
    pub id: N,
    pub position: Point2D<f32>,
    pub radius: f32,
    pub pinned: bool,
}

/// Compute node separation deltas using circle-circle collision detection.
///
/// Returns a map of node ID → position delta for nodes that overlap.
/// Uses iterative multi-pass resolution. Pinned nodes are immovable;
/// unpinned nodes share the overlap equally.
pub fn compute_node_separation<N: Clone + Eq + Hash>(
    nodes: &[NodeSnapshot<N>],
    padding: f32,
    passes: u32,
) -> HashMap<N, Vector2D<f32>> {
    if nodes.len() < 2 {
        return HashMap::new();
    }

    let mut positions: HashMap<N, Point2D<f32>> =
        nodes.iter().map(|n| (n.id.clone(), n.position)).collect();

    for _ in 0..passes {
        let mut changed = false;
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let id_a = &nodes[i].id;
                let id_b = &nodes[j].id;
                let pos_a = positions[id_a];
                let pos_b = positions[id_b];
                let radius_a = nodes[i].radius + padding;
                let radius_b = nodes[j].radius + padding;
                let min_distance = radius_a + radius_b;

                let delta = pos_b - pos_a;
                let distance = delta.length();

                if distance >= min_distance {
                    continue;
                }

                let normal = if distance > f32::EPSILON {
                    delta / distance
                } else {
                    Vector2D::new(1.0, 0.0)
                };
                let overlap = (min_distance - distance).max(0.0) + 0.5;
                if overlap <= f32::EPSILON {
                    continue;
                }

                let a_pinned = nodes[i].pinned;
                let b_pinned = nodes[j].pinned;
                if a_pinned && b_pinned {
                    continue;
                }

                if a_pinned {
                    positions.insert(id_b.clone(), pos_b + normal * overlap);
                } else if b_pinned {
                    positions.insert(id_a.clone(), pos_a - normal * overlap);
                } else {
                    let push = normal * (overlap * 0.5);
                    positions.insert(id_a.clone(), pos_a - push);
                    positions.insert(id_b.clone(), pos_b + push);
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    nodes
        .iter()
        .filter_map(|node| {
            let next = positions.get(&node.id)?;
            let delta = *next - node.position;
            (delta.length() > f32::EPSILON).then_some((node.id.clone(), delta))
        })
        .collect()
}

/// Compute viewport containment deltas.
///
/// Clamps nodes to stay within the given bounds rect, respecting padding.
/// Returns a map of node ID → position delta for nodes outside bounds.
pub fn compute_viewport_containment<N: Clone + Eq + Hash>(
    nodes: &[NodeSnapshot<N>],
    bounds: Rect<f32>,
    padding: f32,
    response_scale: f32,
) -> HashMap<N, Vector2D<f32>> {
    let mut deltas = HashMap::new();
    for node in nodes {
        if node.pinned {
            continue;
        }
        let inset = node.radius + padding;
        if bounds.size.width <= inset * 2.0 || bounds.size.height <= inset * 2.0 {
            continue;
        }
        let min_x = bounds.origin.x + inset;
        let max_x = bounds.origin.x + bounds.size.width - inset;
        let min_y = bounds.origin.y + inset;
        let max_y = bounds.origin.y + bounds.size.height - inset;
        let clamped = Point2D::new(
            node.position.x.clamp(min_x, max_x),
            node.position.y.clamp(min_y, max_y),
        );
        let delta = (clamped - node.position) * response_scale.max(0.0);
        if delta.length() > f32::EPSILON {
            deltas.insert(node.id.clone(), delta);
        }
    }
    deltas
}

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

// ── Scene events ───────────────────────────────────────────────────────────

/// An event emitted by the scene physics system.
///
/// This portable crate does not emit events by itself; hosts can detect region
/// enter/exit by comparing snapshots, or bridge events from a simulation
/// adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneEvent<N> {
    /// Two bodies began touching.
    ContactBegin { a: N, b: N },
    /// Two bodies stopped touching.
    ContactEnd { a: N, b: N },
    /// A node entered a region sensor.
    TriggerEnter { node: N, region: SceneRegionId },
    /// A node exited a region sensor.
    TriggerExit { node: N, region: SceneRegionId },
}

// ── Motion profile ─────────────────────────────────────────────────────────

/// Behavior preset for simulation-feel motion.
///
/// Each preset biases separation feel, containment response, and region
/// effect strength. The host maps these to a `SimulateMotionProfile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SimulateBehaviorPreset {
    /// Loose, gliding movement. Nodes coast longer after release.
    #[default]
    Float,
    /// Tight, snappy movement. Nodes settle quickly.
    Packed,
    /// Moderate coast with stronger region effects.
    Magnetic,
}

/// Motion parameters for release impulses.
///
/// When a node is released after dragging, its drag velocity can be captured as
/// a release impulse that decays over subsequent frames. This profile controls
/// the feel of that coasting behavior.
///
/// The host typically resolves this via a preset ([`SimulateBehaviorPreset`]
/// → [`SimulateMotionProfile::for_preset`]) but may also user-tune the
/// individual fields by setting a per-view or per-graph override.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SimulateMotionProfile {
    /// Scale factor applied to the captured drag impulse.
    pub release_impulse_scale: f32,
    /// Per-frame multiplicative decay applied to the impulse.
    pub release_decay: f32,
    /// Minimum impulse magnitude; below this the impulse is zeroed.
    pub min_impulse: f32,
}

impl Default for SimulateMotionProfile {
    fn default() -> Self {
        Self::for_preset(SimulateBehaviorPreset::default())
    }
}

impl SimulateMotionProfile {
    /// Get the canonical motion profile for a behavior preset.
    pub fn for_preset(preset: SimulateBehaviorPreset) -> Self {
        match preset {
            SimulateBehaviorPreset::Float => Self {
                release_impulse_scale: 1.15,
                release_decay: 0.84,
                min_impulse: 0.03,
            },
            SimulateBehaviorPreset::Packed => Self {
                release_impulse_scale: 0.45,
                release_decay: 0.45,
                min_impulse: 0.05,
            },
            SimulateBehaviorPreset::Magnetic => Self {
                release_impulse_scale: 0.7,
                release_decay: 0.62,
                min_impulse: 0.04,
            },
        }
    }
}

/// Compute one frame of release-impulse decay.
///
/// Takes a map of node ID → current impulse vector and returns the decayed
/// impulses for the next frame. Impulses below `min_impulse` are removed.
/// Also returns the position deltas to apply this frame.
///
/// The `remaining_frames` parameter controls a frame-based scale ramp
/// (nodes coast more at the start, less as the window closes). Set to the
/// remaining frames in the release window.
pub fn compute_release_impulse_frame<N: Clone + Eq + Hash>(
    impulses: &HashMap<N, Vector2D<f32>>,
    profile: &SimulateMotionProfile,
    remaining_frames: u32,
) -> (HashMap<N, Vector2D<f32>>, HashMap<N, Vector2D<f32>>) {
    if remaining_frames == 0 || impulses.is_empty() {
        return (HashMap::new(), HashMap::new());
    }

    let frame_scale = (remaining_frames as f32 / 10.0).clamp(0.1, 1.0);

    let deltas: HashMap<N, Vector2D<f32>> = impulses
        .iter()
        .filter_map(|(key, impulse)| {
            let delta = *impulse * frame_scale * profile.release_impulse_scale;
            (delta.square_length() > f32::EPSILON).then_some((key.clone(), delta))
        })
        .collect();

    let next_impulses: HashMap<N, Vector2D<f32>> = impulses
        .iter()
        .filter_map(|(key, impulse)| {
            let decayed = *impulse * profile.release_decay;
            (decayed.length() >= profile.min_impulse).then_some((key.clone(), decayed))
        })
        .collect();

    (deltas, next_impulses)
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

    fn snap_pinned(id: u32, x: f32, y: f32, radius: f32) -> NodeSnapshot<u32> {
        NodeSnapshot {
            id,
            position: Point2D::new(x, y),
            radius,
            pinned: true,
        }
    }

    // ── Node separation ─────────────────────────────────────────────────

    #[test]
    fn no_separation_when_far_apart() {
        let nodes = vec![snap(0, 0.0, 0.0, 10.0), snap(1, 100.0, 0.0, 10.0)];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        assert!(deltas.is_empty());
    }

    #[test]
    fn separation_pushes_overlapping_nodes() {
        let nodes = vec![snap(0, 0.0, 0.0, 10.0), snap(1, 15.0, 0.0, 10.0)];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        // Nodes overlap (distance 15 < 10+10+4+4=28).
        assert!(!deltas.is_empty());
        // Node 0 should move left, node 1 should move right.
        if let Some(d0) = deltas.get(&0) {
            assert!(d0.x < 0.0);
        }
        if let Some(d1) = deltas.get(&1) {
            assert!(d1.x > 0.0);
        }
    }

    #[test]
    fn pinned_node_is_immovable() {
        let nodes = vec![snap_pinned(0, 0.0, 0.0, 10.0), snap(1, 15.0, 0.0, 10.0)];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        assert!(deltas.get(&0).is_none());
        assert!(deltas.get(&1).is_some());
    }

    #[test]
    fn both_pinned_no_movement() {
        let nodes = vec![
            snap_pinned(0, 0.0, 0.0, 10.0),
            snap_pinned(1, 5.0, 0.0, 10.0),
        ];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        assert!(deltas.is_empty());
    }

    #[test]
    fn single_node_no_separation() {
        let nodes = vec![snap(0, 0.0, 0.0, 10.0)];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        assert!(deltas.is_empty());
    }

    // ── Viewport containment ────────────────────────────────────────────

    #[test]
    fn node_inside_bounds_no_delta() {
        let nodes = vec![snap(0, 100.0, 100.0, 10.0)];
        let bounds = Rect::new(
            Point2D::new(0.0, 0.0),
            euclid::default::Size2D::new(200.0, 200.0),
        );
        let deltas = compute_viewport_containment(&nodes, bounds, 4.0, 1.0);
        assert!(deltas.is_empty());
    }

    #[test]
    fn node_outside_bounds_gets_pushed_in() {
        let nodes = vec![snap(0, 250.0, 100.0, 10.0)];
        let bounds = Rect::new(
            Point2D::new(0.0, 0.0),
            euclid::default::Size2D::new(200.0, 200.0),
        );
        let deltas = compute_viewport_containment(&nodes, bounds, 4.0, 1.0);
        assert!(deltas.contains_key(&0));
        // Should push left.
        assert!(deltas[&0].x < 0.0);
    }

    #[test]
    fn pinned_node_not_contained() {
        let nodes = vec![snap_pinned(0, 250.0, 100.0, 10.0)];
        let bounds = Rect::new(
            Point2D::new(0.0, 0.0),
            euclid::default::Size2D::new(200.0, 200.0),
        );
        let deltas = compute_viewport_containment(&nodes, bounds, 4.0, 1.0);
        assert!(deltas.is_empty());
    }

    // ── Region effects ──────────────────────────────────────────────────

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
        // Should pull left toward center.
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
        // Should push right away from center.
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
        // Node at 105, circle radius 20 + padded_radius 14 = 34, distance 5 < 34.
        assert!(deltas.contains_key(&0));
        // Should push right (away from center).
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

    // ── Simulate motion profile ────────────────────────────────────────

    #[test]
    fn motion_profile_float_preset() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Float);
        assert!(profile.release_impulse_scale > 1.0);
        assert!(profile.release_decay > 0.7);
    }

    #[test]
    fn motion_profile_packed_preset() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Packed);
        assert!(profile.release_impulse_scale < 1.0);
        assert!(profile.release_decay < 0.5);
    }

    #[test]
    fn motion_profile_magnetic_preset() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Magnetic);
        // Magnetic sits between float and packed.
        let float = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Float);
        let packed = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Packed);
        assert!(profile.release_impulse_scale < float.release_impulse_scale);
        assert!(profile.release_impulse_scale > packed.release_impulse_scale);
    }

    #[test]
    fn release_impulse_decays_over_frames() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Float);
        let mut impulses = HashMap::new();
        impulses.insert(0u32, Vector2D::new(10.0, 0.0));

        let (deltas, next) = compute_release_impulse_frame(&impulses, &profile, 5);
        assert!(deltas.contains_key(&0));
        assert!(next.contains_key(&0));
        // Next impulse should be smaller than original.
        assert!(next[&0].length() < impulses[&0].length());
    }

    #[test]
    fn release_impulse_zeroes_when_small() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Packed);
        let mut impulses = HashMap::new();
        impulses.insert(0u32, Vector2D::new(0.01, 0.0));

        let (_deltas, next) = compute_release_impulse_frame(&impulses, &profile, 5);
        // Impulse is below min_impulse after decay, should be removed.
        assert!(next.is_empty());
    }

    #[test]
    fn release_impulse_empty_when_zero_frames() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Float);
        let mut impulses = HashMap::new();
        impulses.insert(0u32, Vector2D::new(10.0, 0.0));

        let (deltas, next) = compute_release_impulse_frame(&impulses, &profile, 0);
        assert!(deltas.is_empty());
        assert!(next.is_empty());
    }

    // ── Scene events ───────────────────────────────────────────────────

    #[test]
    fn scene_event_serde_roundtrip() {
        let events: Vec<SceneEvent<u32>> = vec![
            SceneEvent::ContactBegin { a: 0, b: 1 },
            SceneEvent::ContactEnd { a: 0, b: 1 },
            SceneEvent::TriggerEnter {
                node: 0,
                region: SceneRegionId(42),
            },
            SceneEvent::TriggerExit {
                node: 1,
                region: SceneRegionId(99),
            },
        ];
        let json = serde_json::to_string(&events).unwrap();
        let back: Vec<SceneEvent<u32>> = serde_json::from_str(&json).unwrap();
        assert_eq!(events, back);
    }

    #[test]
    fn behavior_preset_default_is_float() {
        assert_eq!(
            SimulateBehaviorPreset::default(),
            SimulateBehaviorPreset::Float
        );
    }
}
