/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Semantic-similarity-driven layouts.
//!
//! Two distinct layouts that consume different inputs:
//!
//! - [`SemanticEmbedding`] — consumes host-precomputed 2D coordinates
//!   from `LayoutExtras::embedding_by_node`. The projection work (UMAP,
//!   t-SNE, PCA, whatever the host's ML pipeline runs) happens *outside*
//!   graph-canvas. This layout just snaps nodes to their precomputed
//!   targets, scaled and positioned per config.
//!
//! - [`SemanticEdgeWeight`] — consumes host-precomputed pairwise
//!   similarity from `LayoutExtras::semantic_similarity`. Runs an
//!   iterative force-based projection inline: repulsion keeps nodes
//!   apart, semantic-weighted attraction pulls similar pairs together.
//!   Below research-grade embedding quality, but no ML pipeline
//!   required. Named distinctly so users don't confuse it with real
//!   UMAP/t-SNE.

use std::collections::HashMap;
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use super::curves::Falloff;
use super::{Layout, LayoutExtras};
use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;

// ── SemanticEmbedding (precomputed) ──────────────────────────────────────────

/// Fallback strategy for nodes without a precomputed embedding coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingFallback {
    /// Nodes without an embedding keep their current position.
    LeaveInPlace,
    /// Place unembedded nodes at `origin`.
    CollapseToOrigin,
    /// Place unembedded nodes on a deterministic ring outside the main
    /// embedded cluster, positions derived from a stable hash of the id.
    RingOutside,
}

impl Default for EmbeddingFallback {
    fn default() -> Self {
        Self::LeaveInPlace
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticEmbeddingConfig {
    pub origin: Point2D<f32>,
    /// World-unit scale applied to the host's embedding coordinates.
    /// Hosts typically pass coords in `[-1, 1]` or `[0, 1]`.
    pub scale: f32,
    /// Rotation applied to the scaled embedding, in radians.
    pub rotation: f32,
    pub fallback: EmbeddingFallback,
}

impl Default for SemanticEmbeddingConfig {
    fn default() -> Self {
        Self {
            origin: Point2D::new(0.0, 0.0),
            scale: 400.0,
            rotation: 0.0,
            fallback: EmbeddingFallback::default(),
        }
    }
}

/// Places nodes at host-precomputed embedding coordinates. The host
/// runs UMAP / t-SNE / PCA / any projection and supplies the 2D result
/// via `LayoutExtras::embedding_by_node`.
#[derive(Debug, Default)]
pub struct SemanticEmbedding {
    pub config: SemanticEmbeddingConfig,
}

impl SemanticEmbedding {
    pub fn new(config: SemanticEmbeddingConfig) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for SemanticEmbedding
where
    N: Clone + Eq + Hash,
{
    type State = super::StatelessPassState;

    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        _dt: f32,
        _viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        state.step_count = state.step_count.saturating_add(1);
        if scene.nodes.is_empty() {
            return HashMap::new();
        }

        let (cos_r, sin_r) = (self.config.rotation.cos(), self.config.rotation.sin());
        let mut deltas = HashMap::with_capacity(scene.nodes.len());
        for node in &scene.nodes {
            if extras.pinned.contains(&node.id) {
                continue;
            }
            let target = match extras.embedding_by_node.get(&node.id) {
                Some(embed) => {
                    let sx = embed.x * self.config.scale;
                    let sy = embed.y * self.config.scale;
                    Point2D::new(
                        self.config.origin.x + sx * cos_r - sy * sin_r,
                        self.config.origin.y + sx * sin_r + sy * cos_r,
                    )
                }
                None => match self.config.fallback {
                    EmbeddingFallback::LeaveInPlace => continue,
                    EmbeddingFallback::CollapseToOrigin => self.config.origin,
                    EmbeddingFallback::RingOutside => {
                        fallback_ring_position(&node.id, &self.config)
                    }
                },
            };
            let delta = target - node.position;
            if delta.length() > f32::EPSILON {
                deltas.insert(node.id.clone(), delta);
            }
        }
        deltas
    }
}

fn fallback_ring_position<N: Hash>(id: &N, config: &SemanticEmbeddingConfig) -> Point2D<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    let hash = hasher.finish();
    // 0.0..TAU
    let angle = (hash as f32 / u64::MAX as f32) * std::f32::consts::TAU;
    // Ring radius is 1.5× the embedding scale, so fallback nodes ring
    // outside the main cluster.
    let radius = config.scale * 1.5;
    Point2D::new(
        config.origin.x + radius * angle.cos(),
        config.origin.y + radius * angle.sin(),
    )
}

// ── SemanticEdgeWeight (iterative) ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticEdgeWeightConfig {
    /// Pairs with similarity below this threshold contribute no attraction.
    pub similarity_floor: f32,
    /// Coefficient on the pairwise attraction force.
    pub attraction_strength: f32,
    /// Coefficient on the pairwise repulsion force (uniform across all
    /// pairs, like FR's repulsion).
    pub repulsion_strength: f32,
    /// Damping multiplier on per-step displacement.
    pub damping: f32,
    /// Simulation timestep.
    pub dt: f32,
    /// Hard cap on per-step displacement magnitude.
    pub max_step: f32,
    /// Minimum distance for force computation; prevents singularity.
    pub epsilon: f32,
    /// If true, graph edges also contribute uniform-weight attraction
    /// (in addition to similarity-weighted pulls). If false, only
    /// similarity drives attraction — pure semantic projection.
    pub include_graph_edges: bool,
    /// Edge-based attraction coefficient (only used when
    /// `include_graph_edges = true`).
    pub edge_strength: f32,
    /// Center-gravity coefficient. Zero disables.
    pub gravity_strength: f32,
    /// Shape of the pairwise repulsion vs distance.
    pub repulsion_falloff: Falloff,
    /// Shape of the center-gravity vs distance.
    pub gravity_falloff: Falloff,
    /// Gate for the whole simulation.
    pub is_running: bool,
}

impl Default for SemanticEdgeWeightConfig {
    fn default() -> Self {
        Self {
            similarity_floor: 0.2,
            attraction_strength: 1.0,
            repulsion_strength: 1.0,
            damping: 0.3,
            dt: 0.05,
            max_step: 10.0,
            epsilon: 1e-3,
            include_graph_edges: false,
            edge_strength: 0.3,
            gravity_strength: 0.2,
            repulsion_falloff: Falloff::Inverse,
            gravity_falloff: Falloff::Linear,
            is_running: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemanticEdgeWeightState {
    pub step_count: u64,
    #[serde(skip)]
    pub last_avg_displacement: Option<f32>,
}

/// Iterative force-based projection driven by pairwise semantic
/// similarity.
///
/// Algorithmically adjacent to FR, but each edge's attraction
/// coefficient is `similarity(a, b) × attraction_strength` instead of
/// a uniform constant. Similarity is read from
/// `LayoutExtras::semantic_similarity`. No ML pipeline required.
///
/// **Important**: this is *not* real UMAP or t-SNE. Quality is below
/// research-grade embedding; use `SemanticEmbedding` (with a proper
/// embedding pipeline) when projection quality matters. This layout
/// exists as a pipeline-free fallback.
#[derive(Debug)]
pub struct SemanticEdgeWeight {
    pub config: SemanticEdgeWeightConfig,
    scratch: Vec<Vector2D<f32>>,
}

impl Default for SemanticEdgeWeight {
    fn default() -> Self {
        Self {
            config: SemanticEdgeWeightConfig::default(),
            scratch: Vec::new(),
        }
    }
}

impl SemanticEdgeWeight {
    pub fn new(config: SemanticEdgeWeightConfig) -> Self {
        Self {
            config,
            scratch: Vec::new(),
        }
    }
}

impl<N> Layout<N> for SemanticEdgeWeight
where
    N: Clone + Eq + Hash,
{
    type State = SemanticEdgeWeightState;

    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        dt_override: f32,
        viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        state.step_count = state.step_count.saturating_add(1);
        if !self.config.is_running || scene.nodes.len() < 2 {
            return HashMap::new();
        }

        let n = scene.nodes.len();
        let dt = if dt_override > 0.0 {
            dt_override
        } else {
            self.config.dt
        };

        // Compute k for repulsion/attraction scale, analogous to FR.
        let area = (viewport.rect.size.width * viewport.rect.size.height).max(1.0);
        let k = (area / n as f32).sqrt();
        if !k.is_finite() {
            return HashMap::new();
        }

        let positions: Vec<Point2D<f32>> = scene.nodes.iter().map(|node| node.position).collect();
        let index_by_id: HashMap<&N, usize> = scene
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (&node.id, i))
            .collect();

        if self.scratch.len() == n {
            for v in self.scratch.iter_mut() {
                *v = Vector2D::zero();
            }
        } else {
            self.scratch.clear();
            self.scratch.resize(n, Vector2D::zero());
        }

        // Uniform repulsion (every pair). Shape driven by `repulsion_falloff`.
        for i in 0..n {
            for j in (i + 1)..n {
                let delta = positions[i] - positions[j];
                let distance = delta.length().max(self.config.epsilon);
                let force = self.config.repulsion_strength
                    * (k * k)
                    * self.config.repulsion_falloff.evaluate(distance);
                let dir = delta / distance;
                self.scratch[i] += dir * force;
                self.scratch[j] -= dir * force;
            }
        }

        // Similarity-weighted attraction.
        for ((a, b), similarity) in &extras.semantic_similarity {
            if a == b || *similarity < self.config.similarity_floor {
                continue;
            }
            let (Some(&i), Some(&j)) = (index_by_id.get(a), index_by_id.get(b)) else {
                continue;
            };
            if i == j {
                continue;
            }
            let delta = positions[j] - positions[i];
            let distance = delta.length().max(self.config.epsilon);
            let force = *similarity * self.config.attraction_strength * (distance * distance) / k;
            let dir = delta / distance;
            self.scratch[i] += dir * force;
            self.scratch[j] -= dir * force;
        }

        // Optional graph-edge attraction (uniform, on top of similarity).
        if self.config.include_graph_edges {
            for edge in &scene.edges {
                let (Some(&i), Some(&j)) =
                    (index_by_id.get(&edge.source), index_by_id.get(&edge.target))
                else {
                    continue;
                };
                if i == j {
                    continue;
                }
                let delta = positions[j] - positions[i];
                let distance = delta.length().max(self.config.epsilon);
                let force = self.config.edge_strength * (distance * distance) / k;
                let dir = delta / distance;
                self.scratch[i] += dir * force;
                self.scratch[j] -= dir * force;
            }
        }

        // Center gravity. Shape driven by `gravity_falloff`.
        if self.config.gravity_strength > 0.0 {
            let center = Point2D::new(
                viewport.rect.origin.x + viewport.rect.size.width * 0.5,
                viewport.rect.origin.y + viewport.rect.size.height * 0.5,
            );
            for i in 0..n {
                let to_center = center - positions[i];
                let distance = to_center.length().max(self.config.epsilon);
                let direction = to_center / distance;
                let magnitude =
                    self.config.gravity_strength * self.config.gravity_falloff.evaluate(distance);
                self.scratch[i] += direction * magnitude;
            }
        }

        // Apply displacement with damping + max_step cap. Pinned nodes
        // contribute forces to others but do not move.
        let mut deltas = HashMap::with_capacity(n);
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for (i, node) in scene.nodes.iter().enumerate() {
            if extras.pinned.contains(&node.id) {
                continue;
            }
            let mut step = self.scratch[i] * dt * self.config.damping;
            let len = step.length();
            if len > self.config.max_step {
                step = step.normalize() * self.config.max_step;
            }
            let new_pos = positions[i] + step;
            if !new_pos.x.is_finite() || !new_pos.y.is_finite() {
                continue;
            }
            let clamped = len.min(self.config.max_step);
            if clamped > f32::EPSILON {
                deltas.insert(node.id.clone(), step);
                sum += clamped;
                count += 1;
            }
        }
        state.last_avg_displacement = (count > 0).then_some(sum / count as f32);
        deltas
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::default::{Rect, Size2D};
    use crate::scene::{CanvasEdge, CanvasNode};

    fn viewport() -> CanvasViewport {
        CanvasViewport {
            rect: Rect::new(Point2D::new(0.0, 0.0), Size2D::new(1000.0, 1000.0)),
            scale_factor: 1.0,
        }
    }

    fn scene(nodes: Vec<(u32, f32, f32)>, edges: Vec<(u32, u32)>) -> CanvasSceneInput<u32> {
        CanvasSceneInput {
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
        }
    }

    #[test]
    fn semantic_embedding_snaps_to_precomputed_coords() {
        let mut layout = SemanticEmbedding::new(SemanticEmbeddingConfig {
            origin: Point2D::new(0.0, 0.0),
            scale: 100.0,
            rotation: 0.0,
            fallback: EmbeddingFallback::LeaveInPlace,
        });
        let mut state = super::super::StatelessPassState::default();
        let input = scene(vec![(0, 0.0, 0.0), (1, 0.0, 0.0)], vec![]);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.embedding_by_node.insert(0, Point2D::new(0.5, 0.5));
        extras.embedding_by_node.insert(1, Point2D::new(-0.5, -0.5));
        let deltas = layout.step(&input, &mut state, 0.0, &viewport(), &extras);
        assert_eq!(deltas[&0], Vector2D::new(50.0, 50.0));
        assert_eq!(deltas[&1], Vector2D::new(-50.0, -50.0));
    }

    #[test]
    fn semantic_embedding_fallback_leave_in_place() {
        let mut layout = SemanticEmbedding::new(SemanticEmbeddingConfig {
            fallback: EmbeddingFallback::LeaveInPlace,
            ..Default::default()
        });
        let input = scene(vec![(0, 100.0, 100.0)], vec![]);
        let deltas = layout.step(
            &input,
            &mut super::super::StatelessPassState::default(),
            0.0,
            &viewport(),
            &LayoutExtras::default(),
        );
        assert!(deltas.is_empty());
    }

    #[test]
    fn semantic_embedding_fallback_collapse_to_origin() {
        let mut layout = SemanticEmbedding::new(SemanticEmbeddingConfig {
            origin: Point2D::new(5.0, 5.0),
            fallback: EmbeddingFallback::CollapseToOrigin,
            ..Default::default()
        });
        let input = scene(vec![(0, 100.0, 100.0)], vec![]);
        let deltas = layout.step(
            &input,
            &mut super::super::StatelessPassState::default(),
            0.0,
            &viewport(),
            &LayoutExtras::default(),
        );
        assert_eq!(deltas[&0], Vector2D::new(-95.0, -95.0));
    }

    #[test]
    fn semantic_edge_weight_pulls_similar_pairs_together() {
        let mut layout = SemanticEdgeWeight::new(SemanticEdgeWeightConfig {
            repulsion_strength: 0.1,
            attraction_strength: 5.0,
            gravity_strength: 0.0,
            similarity_floor: 0.1,
            ..Default::default()
        });
        let mut state = SemanticEdgeWeightState::default();
        let input = scene(
            vec![(0, -200.0, 0.0), (1, 200.0, 0.0), (2, 0.0, 400.0)],
            vec![],
        );
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.semantic_similarity.insert((0, 1), 0.9);
        // 2 is dissimilar to both
        let deltas = layout.step(&input, &mut state, 0.0, &viewport(), &extras);
        // 0 and 1 should pull toward each other.
        let d0 = deltas.get(&0).copied().unwrap_or_default();
        let d1 = deltas.get(&1).copied().unwrap_or_default();
        assert!(d0.x > 0.0, "node 0 should pull right toward node 1");
        assert!(d1.x < 0.0, "node 1 should pull left toward node 0");
    }

    #[test]
    fn semantic_edge_weight_respects_similarity_floor() {
        let mut layout = SemanticEdgeWeight::new(SemanticEdgeWeightConfig {
            repulsion_strength: 0.0,
            attraction_strength: 5.0,
            gravity_strength: 0.0,
            similarity_floor: 0.5,
            ..Default::default()
        });
        let input = scene(vec![(0, 0.0, 0.0), (1, 100.0, 0.0)], vec![]);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.semantic_similarity.insert((0, 1), 0.3); // below floor
        let deltas = layout.step(
            &input,
            &mut SemanticEdgeWeightState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        assert!(deltas.is_empty());
    }

    #[test]
    fn semantic_edge_weight_pinned_nodes_emit_no_delta() {
        let mut layout = SemanticEdgeWeight::new(SemanticEdgeWeightConfig::default());
        let input = scene(vec![(0, 0.0, 0.0), (1, 100.0, 0.0)], vec![]);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.pinned.insert(0);
        extras.semantic_similarity.insert((0, 1), 0.9);
        let deltas = layout.step(
            &input,
            &mut SemanticEdgeWeightState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        assert!(!deltas.contains_key(&0));
    }

    #[test]
    fn semantic_edge_weight_include_graph_edges_adds_attraction() {
        let mut layout = SemanticEdgeWeight::new(SemanticEdgeWeightConfig {
            repulsion_strength: 0.1,
            attraction_strength: 0.0,
            edge_strength: 5.0,
            gravity_strength: 0.0,
            include_graph_edges: true,
            ..Default::default()
        });
        let input = scene(vec![(0, -300.0, 0.0), (1, 300.0, 0.0)], vec![(0, 1)]);
        let deltas = layout.step(
            &input,
            &mut SemanticEdgeWeightState::default(),
            0.0,
            &viewport(),
            &LayoutExtras::default(),
        );
        let d0 = deltas.get(&0).copied().unwrap_or_default();
        let d1 = deltas.get(&1).copied().unwrap_or_default();
        assert!(d0.x > 0.0);
        assert!(d1.x < 0.0);
    }
}
