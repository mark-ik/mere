/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Semantic-embedding layout.
//!
//! [`SemanticEmbedding`] consumes host-precomputed 2D coordinates from
//! `LayoutExtras::embedding_by_node`. The projection work (UMAP, t-SNE,
//! PCA, whatever the host's ML pipeline runs) happens *outside*
//! graph-canvas. This layout just snaps nodes to their precomputed
//! targets, scaled and positioned per config.
//!
//! (The companion iterative `SemanticEdgeWeight` projection was retired
//! once gyre's affinity force reached parity — similarity now clusters
//! at gyre's cost, not as a projection.)

use std::collections::HashMap;
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{CanvasEdge, CanvasNode};
    use euclid::default::{Rect, Size2D};

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
}
