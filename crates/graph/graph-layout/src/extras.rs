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
use graph_canvas::camera::CanvasViewport;
use graph_canvas::scene::CanvasSceneInput;

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
/// **Definition lives in `graph_canvas::scene`** (since 2026-05-18 sibling-
/// crate move) because `graph_canvas::derive` also consumes this type as
/// host-provided scene-derivation input. Re-exported here for back-compat;
/// future code should import from `graph_canvas` directly.
pub use graph_canvas::scene::FrameRegion;

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

#[cfg(test)]
mod tests {
    use super::*;
    use graph_canvas::projection::ProjectionMode;
    use graph_canvas::scene::{CanvasEdge, CanvasNode, SceneMode, ViewId};
    use euclid::default::{Rect, Size2D};

    fn viewport() -> CanvasViewport {
        CanvasViewport {
            rect: Rect::new(Point2D::new(0.0, 0.0), Size2D::new(1000.0, 1000.0)),
            scale_factor: 1.0,
        }
    }

    fn scene(nodes: Vec<(u32, f32, f32)>, edges: Vec<(u32, u32)>) -> CanvasSceneInput<u32> {
        CanvasSceneInput {
            view_id: ViewId(0),
            nodes: nodes
                .into_iter()
                .map(|(id, x, y)| CanvasNode {
                    id,
                    position: Point2D::new(x, y),
                    radius: 16.0,
                    label: None,
                })
                .collect(),
            edges: edges
                .into_iter()
                .map(|(s, t)| CanvasEdge::untagged(s, t))
                .collect(),
            scene_objects: Vec::new(),
            overlays: Vec::new(),
            scene_mode: SceneMode::Browse,
            projection: ProjectionMode::default(),
        }
    }

    #[test]
    fn degree_repulsion_pushes_hub_neighbors_apart() {
        // Hub=0 with edges to 1,2,3. Neighbors 1 and 2 should be pushed apart.
        let input = scene(
            vec![(0, 0.0, 0.0), (1, -5.0, 0.0), (2, 5.0, 0.0), (3, 0.0, 20.0)],
            vec![(0, 1), (0, 2), (0, 3)],
        );
        let mut layout = DegreeRepulsion::new(DegreeRepulsionConfig::medium());
        let mut state = StatelessPassState::default();
        let deltas = layout.step(
            &input,
            &mut state,
            0.0,
            &viewport(),
            &LayoutExtras::default(),
        );
        // 1 (left) pushed further left; 2 (right) pushed further right.
        assert!(deltas[&1].x < 0.0);
        assert!(deltas[&2].x > 0.0);
    }

    #[test]
    fn domain_clustering_pulls_same_domain_members_together() {
        let input = scene(vec![(0, 0.0, 0.0), (1, 100.0, 0.0)], vec![]);
        let mut layout = DomainClustering::<u32>::new(DomainClusteringConfig {
            strength: 0.2,
            ..Default::default()
        });
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.domain_by_node.insert(0, "example.com".into());
        extras.domain_by_node.insert(1, "example.com".into());
        let deltas = layout.step(
            &input,
            &mut StatelessPassState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        assert!(deltas[&0].x > 0.0); // pulled right toward centroid at 50
        assert!(deltas[&1].x < 0.0); // pulled left toward centroid at 50
    }

    #[test]
    fn semantic_clustering_respects_similarity_floor() {
        let input = scene(vec![(0, 0.0, 0.0), (1, 100.0, 0.0)], vec![]);
        let mut layout = SemanticClustering::new(SemanticClusteringConfig {
            strength: 0.1,
            similarity_floor: 0.5,
            ..Default::default()
        });
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.semantic_similarity.insert((0, 1), 0.3); // below floor
        let deltas = layout.step(
            &input,
            &mut StatelessPassState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        assert!(deltas.is_empty());

        extras.semantic_similarity.insert((0, 1), 0.8); // above floor
        let deltas = layout.step(
            &input,
            &mut StatelessPassState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        assert!(deltas.contains_key(&0));
        assert!(deltas.contains_key(&1));
    }

    #[test]
    fn hub_pull_moves_leaf_toward_hub() {
        let input = scene(
            vec![
                (0, 0.0, 0.0),  // hub
                (1, 80.0, 0.0), // leaf (will be pulled left toward hub)
                (2, -20.0, 20.0),
                (3, 20.0, 20.0),
            ],
            vec![(0, 1), (0, 2), (0, 3)],
        );
        let mut layout = HubPull::new(HubPullConfig::default());
        let deltas = layout.step(
            &input,
            &mut StatelessPassState::default(),
            0.0,
            &viewport(),
            &LayoutExtras::default(),
        );
        assert!(deltas[&1].x < 0.0, "leaf should be pulled toward hub");
    }

    #[test]
    fn frame_affinity_pulls_members_to_centroid() {
        let input = scene(
            vec![(0, -50.0, 0.0), (1, 50.0, 0.0), (2, 0.0, 100.0)],
            vec![],
        );
        let mut layout = FrameAffinity::new(FrameAffinityConfig::default());
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.frame_regions.push(FrameRegion {
            anchor: 0,
            members: vec![0, 1, 2],
            strength: 0.5,
        });
        let deltas = layout.step(
            &input,
            &mut StatelessPassState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        // Centroid is (0, 33.33). Node 0 pulled right+up; node 1 pulled left+up; node 2 pulled down.
        assert!(deltas[&0].x > 0.0 && deltas[&0].y > 0.0);
        assert!(deltas[&1].x < 0.0 && deltas[&1].y > 0.0);
        assert!(deltas[&2].y < 0.0);
    }

    #[test]
    fn pinned_nodes_excluded_from_all_extras() {
        let input = scene(vec![(0, 0.0, 0.0), (1, 100.0, 0.0)], vec![]);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.pinned.insert(0);
        extras.domain_by_node.insert(0, "x".into());
        extras.domain_by_node.insert(1, "x".into());
        let mut layout = DomainClustering::<u32>::new(DomainClusteringConfig {
            strength: 0.2,
            ..Default::default()
        });
        let deltas = layout.step(
            &input,
            &mut StatelessPassState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        assert!(!deltas.contains_key(&0));
        assert!(deltas.contains_key(&1));
    }
}
