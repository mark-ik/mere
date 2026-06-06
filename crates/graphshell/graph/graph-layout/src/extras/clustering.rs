/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use canvas_ir::camera::CanvasViewport;
use canvas_ir::scene::CanvasSceneInput;

use crate::curves::{DegreeWeighting, ProximityFalloff, SimilarityCurve};
use crate::{Layout, LayoutExtras};
use super::{StatelessPassState, TargetPolicy};
use super::{advance_step_count, degrees_from_scene, emit_deltas, resolve_group_target};

// ── Semantic clustering ───────────────────────────────────────────────────────

/// Tuning for the semantic-clustering extras pass.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SemanticClusteringConfig {
    /// Scale factor applied to the pair force.
    pub strength: f32,
    /// Minimum similarity required for a pair to attract. Pairs below this
    /// threshold contribute no force.
    pub similarity_floor: f32,
    /// How similarity maps to attraction magnitude.
    pub similarity_curve: SimilarityCurve,
}

/// Semantically similar nodes pulled together.
///
/// Reads `LayoutExtras.semantic_similarity` for pairwise similarity scores
/// in `[0.0, 1.0]`. Pairs with similarity ≥ `similarity_floor` receive a
/// symmetric pull scaled by similarity × strength. Only pairs explicitly
/// present in the map are considered.
#[derive(Debug, Default)]
pub struct SemanticClustering {
    pub config: SemanticClusteringConfig,
}

impl SemanticClustering {
    pub fn new(config: SemanticClusteringConfig) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for SemanticClustering
where
    N: Clone + Eq + Hash,
{
    type State = StatelessPassState;

    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        _dt: f32,
        _viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        advance_step_count(state);
        if self.config.strength.abs() < 1e-6 || extras.semantic_similarity.is_empty() {
            return HashMap::new();
        }

        let position_by_id: HashMap<&N, Point2D<f32>> = scene
            .nodes
            .iter()
            .map(|node| (&node.id, node.position))
            .collect();

        let mut deltas: HashMap<N, Vector2D<f32>> = HashMap::new();
        for ((a, b), similarity) in &extras.semantic_similarity {
            if a == b || *similarity < self.config.similarity_floor {
                continue;
            }
            let (Some(pa), Some(pb)) = (position_by_id.get(a), position_by_id.get(b)) else {
                continue;
            };
            let delta = *pb - *pa;
            let weight = self.config.similarity_curve.evaluate(*similarity);
            let force = delta * weight * self.config.strength;
            *deltas.entry(a.clone()).or_insert_with(Vector2D::zero) += force;
            *deltas.entry(b.clone()).or_insert_with(Vector2D::zero) -= force;
        }

        emit_deltas(scene, deltas, extras)
    }
}

// ── Hub pull ──────────────────────────────────────────────────────────────────

/// Tuning for the hub-pull extras pass.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HubPullConfig {
    pub radius_px: f32,
    pub strength: f32,
    /// Minimum degree a node must reach to count as a hub.
    pub degree_floor: usize,
    /// Shape of the proximity falloff within `radius_px`.
    pub proximity_falloff: ProximityFalloff,
    /// How the hub's degree scales pull strength.
    pub hub_degree_weighting: DegreeWeighting,
}

impl Default for HubPullConfig {
    fn default() -> Self {
        Self {
            radius_px: 260.0,
            strength: 0.05,
            degree_floor: 3,
            proximity_falloff: ProximityFalloff::Linear,
            hub_degree_weighting: DegreeWeighting::Logarithmic,
        }
    }
}

/// Low-degree leaves pulled toward nearby high-degree hubs.
///
/// For each pair within `radius_px` where one endpoint's degree is
/// strictly higher than the other's and the hub meets `degree_floor`, pull
/// the lower-degree node toward the higher-degree one. Force scales with
/// proximity, `ln(1 + hub_degree)`, and the degree gap.
#[derive(Debug, Default)]
pub struct HubPull {
    pub config: HubPullConfig,
}

impl HubPull {
    pub fn new(config: HubPullConfig) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for HubPull
where
    N: Clone + Eq + Hash,
{
    type State = StatelessPassState;

    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        _dt: f32,
        _viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        advance_step_count(state);
        if scene.nodes.len() < 2 {
            return HashMap::new();
        }

        let degrees = degrees_from_scene(scene);
        let mut deltas: HashMap<N, Vector2D<f32>> = HashMap::new();

        for i in 0..scene.nodes.len() {
            for j in (i + 1)..scene.nodes.len() {
                let a = &scene.nodes[i];
                let b = &scene.nodes[j];
                let deg_a = *degrees.get(&a.id).unwrap_or(&0);
                let deg_b = *degrees.get(&b.id).unwrap_or(&0);
                if deg_a == deg_b {
                    continue;
                }

                let (hub_pos, hub_degree, leaf_id, leaf_pos, leaf_degree) = if deg_a > deg_b {
                    (a.position, deg_a, &b.id, b.position, deg_b)
                } else {
                    (b.position, deg_b, &a.id, a.position, deg_a)
                };

                if hub_degree < self.config.degree_floor {
                    continue;
                }

                let delta = hub_pos - leaf_pos;
                let distance = delta.length();
                if distance <= 1.0 || distance > self.config.radius_px {
                    continue;
                }

                let t = 1.0 - (distance / self.config.radius_px);
                let proximity = self.config.proximity_falloff.evaluate(t);
                let hub_weight = self.config.hub_degree_weighting.evaluate(hub_degree);
                let degree_gap = hub_degree.saturating_sub(leaf_degree).max(1) as f32;
                let pull = delta * proximity * hub_weight * degree_gap * self.config.strength;
                *deltas.entry(leaf_id.clone()).or_insert_with(Vector2D::zero) += pull;
            }
        }

        emit_deltas(scene, deltas, extras)
    }
}

// ── Frame affinity ────────────────────────────────────────────────────────────

/// A derived frame-affinity region passed in via [`LayoutExtras::frame_regions`].
///
/// **Definition lives in `canvas_ir::scene`** (since 2026-05-18 sibling-
/// crate move) because `canvas_ir::derive` also consumes this type as
/// host-provided scene-derivation input. Re-exported here for back-compat;
/// future code should import from `canvas_ir` directly.
pub use canvas_ir::scene::FrameRegion;

/// Tuning for the frame-affinity extras pass.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameAffinityConfig {
    /// Multiplied with each region's own `strength` for a final pull scale.
    pub global_strength: f32,
    /// Where within each region the members are pulled toward. `Centroid`
    /// matches legacy behavior; `FirstMember` creates anchor-driven
    /// formations; `Medoid` is more outlier-robust. `NamedAnchor` uses
    /// each region's `anchor` field as the target.
    pub target_policy: TargetPolicy,
    /// Minimum member count for a region to apply force. Regions with
    /// fewer members are skipped.
    pub min_members: u32,
}

impl Default for FrameAffinityConfig {
    fn default() -> Self {
        Self {
            global_strength: 1.0,
            target_policy: TargetPolicy::Centroid,
            min_members: 2,
        }
    }
}

/// Frame members pulled toward their frame's centroid.
///
/// Each region's centroid is computed from the scene's current member
/// positions. Pinned nodes skipped. Regions with zero resolvable members
/// contribute nothing.
#[derive(Debug, Default)]
pub struct FrameAffinity {
    pub config: FrameAffinityConfig,
}

impl FrameAffinity {
    pub fn new(config: FrameAffinityConfig) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for FrameAffinity
where
    N: Clone + Eq + Hash,
{
    type State = StatelessPassState;

    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        _dt: f32,
        _viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        advance_step_count(state);
        if extras.frame_regions.is_empty() || scene.nodes.is_empty() {
            return HashMap::new();
        }

        let position_by_id: HashMap<&N, Point2D<f32>> = scene
            .nodes
            .iter()
            .map(|node| (&node.id, node.position))
            .collect();

        let min_members = self.config.min_members.max(1) as usize;
        let mut deltas: HashMap<N, Vector2D<f32>> = HashMap::new();
        for region in &extras.frame_regions {
            if region.members.len() < min_members {
                continue;
            }
            // Build a slice of member references for target resolution.
            let member_refs: Vec<&N> = region.members.iter().collect();
            let target = resolve_group_target(
                &member_refs,
                &position_by_id,
                self.config.target_policy,
                Some(&region.anchor),
            );
            let region_strength = region.strength * self.config.global_strength;
            for member in &region.members {
                let Some(pos) = position_by_id.get(member) else {
                    continue;
                };
                let pull = (target - *pos) * region_strength;
                *deltas.entry(member.clone()).or_insert_with(Vector2D::zero) += pull;
            }
        }

        emit_deltas(scene, deltas, extras)
    }
}
