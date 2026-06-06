/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Physics extras — post-force-directed position adjustments.
//!
//! Each extras layout is a `Layout<N>` that reads the current scene plus
//! precomputed inputs from [`LayoutExtras`] and returns per-node position
//! deltas. They are designed to compose after `ForceDirected` or `BarnesHut`
//! by running in sequence, each seeing the positions updated by the previous
//! pass (the host applies deltas between calls).
//!
//! The extras operate directly on world-space positions and do **not** scale
//! by `dt` or `damping` — they are position-space nudges, not momentum-based
//! forces. This matches the legacy graphshell helper behavior where each
//! extras pass called `apply_position_deltas` unconditionally.
//!
//! Available extras:
//!
//! - [`DegreeRepulsion`] — high-degree nodes push their neighbors apart
//! - [`DomainClustering`] — same-domain nodes pulled to a shared centroid
//! - [`SemanticClustering`] — semantically similar nodes pulled together
//! - [`HubPull`] — low-degree leaves pulled toward nearby high-degree hubs
//! - [`FrameAffinity`] — frame members pulled toward their frame centroid
//!
//! All five share [`StatelessPassState`] — they accumulate only a step count
//! for diagnostics; no per-node state is persisted across steps.

use std::collections::HashMap;
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use super::curves::{DegreeWeighting, ProximityFalloff, SimilarityCurve};
use super::{Layout, LayoutExtras};
use canvas_ir::camera::CanvasViewport;
use canvas_ir::scene::CanvasSceneInput;

/// Shared persistent state for stateless extras passes.
///
/// Only tracks step count for diagnostics. Every extras pass uses this
/// single type so hosts don't have to carry one state per extras kind.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StatelessPassState {
    pub step_count: u64,
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn advance_step_count(state: &mut StatelessPassState) {
    state.step_count = state.step_count.saturating_add(1);
}

fn emit_deltas<N>(
    scene: &CanvasSceneInput<N>,
    deltas: HashMap<N, Vector2D<f32>>,
    extras: &LayoutExtras<N>,
) -> HashMap<N, Vector2D<f32>>
where
    N: Clone + Eq + Hash,
{
    let _ = scene;
    deltas
        .into_iter()
        .filter(|(key, d)| !extras.pinned.contains(key) && d.length() > f32::EPSILON)
        .collect()
}

fn degrees_from_scene<N>(scene: &CanvasSceneInput<N>) -> HashMap<&N, usize>
where
    N: Clone + Eq + Hash,
{
    let mut degrees: HashMap<&N, usize> = scene.nodes.iter().map(|node| (&node.id, 0)).collect();
    for edge in &scene.edges {
        if edge.source == edge.target {
            continue;
        }
        if let Some(count) = degrees.get_mut(&edge.source) {
            *count += 1;
        }
        if let Some(count) = degrees.get_mut(&edge.target) {
            *count += 1;
        }
    }
    degrees
}

// ── Degree repulsion ──────────────────────────────────────────────────────────

/// Tuning for the degree-repulsion extras pass.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DegreeRepulsionConfig {
    /// Falloff radius in world units. Node pairs further apart than this
    /// receive no push.
    pub radius_px: f32,
    /// Force coefficient. Scales the final push magnitude.
    pub strength: f32,
    /// Shape of the proximity falloff within `radius_px`.
    pub proximity_falloff: ProximityFalloff,
    /// How node degree scales the push magnitude.
    pub degree_weighting: DegreeWeighting,
    /// Minimum degree for a pair to qualify (maximum of the two degrees
    /// must reach this). Pairs below this are skipped entirely.
    pub min_degree: usize,
}

impl DegreeRepulsionConfig {
    pub const fn mild() -> Self {
        Self {
            radius_px: 220.0,
            strength: 4.0,
            proximity_falloff: ProximityFalloff::Linear,
            degree_weighting: DegreeWeighting::Logarithmic,
            min_degree: 2,
        }
    }

    pub const fn medium() -> Self {
        Self {
            radius_px: 220.0,
            strength: 8.0,
            proximity_falloff: ProximityFalloff::Linear,
            degree_weighting: DegreeWeighting::Logarithmic,
            min_degree: 2,
        }
    }
}

impl Default for DegreeRepulsionConfig {
    fn default() -> Self {
        Self::mild()
    }
}

impl<N> Default for DomainClusteringConfig<N>
where
    N: Clone + Eq + Hash,
{
    fn default() -> Self {
        Self {
            strength: 0.08,
            target_policy: TargetPolicy::default(),
            min_members: 2,
            anchor_by_group: HashMap::new(),
        }
    }
}

impl Default for SemanticClusteringConfig {
    fn default() -> Self {
        Self {
            strength: 0.1,
            similarity_floor: 0.5,
            similarity_curve: SimilarityCurve::Linear,
        }
    }
}

/// High-degree nodes push their neighbors apart.
///
/// For each pair within `config.radius_px` where at least one endpoint has
/// degree > 1, emit a symmetric push scaled by proximity and the natural log
/// of the higher degree.
#[derive(Debug, Default)]
pub struct DegreeRepulsion {
    pub config: DegreeRepulsionConfig,
}

impl DegreeRepulsion {
    pub fn new(config: DegreeRepulsionConfig) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for DegreeRepulsion
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
                let delta = b.position - a.position;
                let distance = delta.length();
                if distance <= 1.0 || distance > self.config.radius_px {
                    continue;
                }
                let deg_a = *degrees.get(&a.id).unwrap_or(&0);
                let deg_b = *degrees.get(&b.id).unwrap_or(&0);
                let max_degree = deg_a.max(deg_b);
                if max_degree < self.config.min_degree {
                    continue;
                }
                let t = 1.0 - (distance / self.config.radius_px);
                let proximity = self.config.proximity_falloff.evaluate(t);
                let degree_bonus = self.config.degree_weighting.evaluate(max_degree);
                let push = delta.normalize() * proximity * degree_bonus * self.config.strength;

                *deltas.entry(a.id.clone()).or_insert_with(Vector2D::zero) -= push;
                *deltas.entry(b.id.clone()).or_insert_with(Vector2D::zero) += push;
            }
        }

        emit_deltas(scene, deltas, extras)
    }
}

// ── Domain clustering ─────────────────────────────────────────────────────────

/// Where a group's pull-target sits.
///
/// Used by clustering passes (domain, frame-affinity) to decide where
/// members are pulled toward. The `NamedAnchor` variant reads a
/// per-group mapping from the layout's config (which picks a specific
/// member or external node as the per-group anchor, e.g., "the oldest
/// node of the example.com domain is the anchor").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetPolicy {
    /// Mean of member positions. Default.
    Centroid,
    /// Member whose position minimizes total distance to others
    /// (approximated as the member closest to the centroid). More
    /// robust to outliers than `Centroid`.
    Medoid,
    /// First member in scene-node order. Deterministic and cheap; useful
    /// when groups have a canonical "seed" member.
    FirstMember,
    /// Per-group anchor node specified in layout config. Falls back to
    /// `Centroid` if the config has no anchor entry for this group.
    NamedAnchor,
}

impl Default for TargetPolicy {
    fn default() -> Self {
        Self::Centroid
    }
}

/// Tuning for the domain-clustering extras pass.
///
/// Generic over the host's node-id type because the `NamedAnchor` target
/// policy maps domain strings to specific node ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainClusteringConfig<N>
where
    N: Clone + Eq + Hash,
{
    /// Scale factor on the pull-to-target delta.
    pub strength: f32,
    /// Where within each domain group the target sits.
    pub target_policy: TargetPolicy,
    /// Minimum group membership for the clustering to fire. Groups below
    /// this size are skipped entirely.
    pub min_members: u32,
    /// Per-domain anchor node ids used when `target_policy` is
    /// `NamedAnchor`. Maps domain string → anchor node id. Missing
    /// entries fall back to `Centroid`.
    #[serde(skip)]
    pub anchor_by_group: HashMap<String, N>,
}

/// Same-domain nodes pulled toward a shared centroid.
///
/// Groups nodes by `LayoutExtras.domain_by_node` and pulls each member
/// toward the centroid of its group. Nodes without a domain entry are not
/// affected.
#[derive(Debug)]
pub struct DomainClustering<N>
where
    N: Clone + Eq + Hash,
{
    pub config: DomainClusteringConfig<N>,
}

impl<N> Default for DomainClustering<N>
where
    N: Clone + Eq + Hash,
{
    fn default() -> Self {
        Self {
            config: DomainClusteringConfig::default(),
        }
    }
}

impl<N> DomainClustering<N>
where
    N: Clone + Eq + Hash,
{
    pub fn new(config: DomainClusteringConfig<N>) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for DomainClustering<N>
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
        if scene.nodes.is_empty() {
            return HashMap::new();
        }

        let position_by_id: HashMap<&N, Point2D<f32>> = scene
            .nodes
            .iter()
            .map(|node| (&node.id, node.position))
            .collect();

        let mut members_by_domain: HashMap<String, Vec<&N>> = HashMap::new();
        for node in &scene.nodes {
            if let Some(domain) = extras.domain_by_node.get(&node.id) {
                members_by_domain
                    .entry(domain.clone())
                    .or_default()
                    .push(&node.id);
            }
        }

        let min_members = self.config.min_members.max(2) as usize;
        let mut deltas: HashMap<N, Vector2D<f32>> = HashMap::new();
        for (domain, members) in members_by_domain {
            if members.len() < min_members {
                continue;
            }
            let target = resolve_group_target(
                &members,
                &position_by_id,
                self.config.target_policy,
                self.config.anchor_by_group.get(&domain),
            );
            for id in members {
                if let Some(pos) = position_by_id.get(id) {
                    let pull = (target - *pos) * self.config.strength;
                    *deltas.entry((*id).clone()).or_insert_with(Vector2D::zero) += pull;
                }
            }
        }

        emit_deltas(scene, deltas, extras)
    }
}

/// Resolve the target point for a group under a given policy.
fn resolve_group_target<N>(
    members: &[&N],
    position_by_id: &HashMap<&N, Point2D<f32>>,
    policy: TargetPolicy,
    named_anchor: Option<&N>,
) -> Point2D<f32>
where
    N: Clone + Eq + Hash,
{
    // Gather member positions once.
    let positions: Vec<Point2D<f32>> = members
        .iter()
        .filter_map(|id| position_by_id.get(id).copied())
        .collect();
    if positions.is_empty() {
        return Point2D::new(0.0, 0.0);
    }

    // Centroid computation is shared as a fallback target.
    let sum = positions
        .iter()
        .fold(Vector2D::<f32>::zero(), |acc, p| acc + p.to_vector());
    let centroid = (sum / positions.len() as f32).to_point();

    match policy {
        TargetPolicy::Centroid => centroid,
        TargetPolicy::FirstMember => positions[0],
        TargetPolicy::Medoid => {
            // Member closest to centroid.
            positions
                .iter()
                .copied()
                .min_by(|a, b| {
                    let da = (*a - centroid).square_length();
                    let db = (*b - centroid).square_length();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(centroid)
        }
        TargetPolicy::NamedAnchor => {
            if let Some(anchor) = named_anchor {
                if let Some(pos) = position_by_id.get(anchor) {
                    return *pos;
                }
            }
            centroid
        }
    }
}

pub mod clustering;
pub use clustering::*;
#[cfg(test)]
mod tests;
