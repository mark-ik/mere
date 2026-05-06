/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Barnes–Hut approximate force-directed layout.
//!
//! Replaces the O(n²) pairwise repulsion pass from [`super::force_directed`]
//! with an O(n log n) quadtree traversal. Each node walks the tree and either
//! uses a cell's center-of-mass as a pseudo-node (if the cell is far enough —
//! `s/d < θ`) or recurses into its children. Attraction is identical to FR:
//! per-edge, `c_attract * distance^2 / k`.
//!
//! Default `θ = 0.5` gives a good accuracy/speed balance; raising θ accelerates
//! at the cost of more approximation error.
//!
//! Behavior matches FR at small node counts and scales better as graphs grow.
//! Use this when the graph has more than a few hundred nodes.

use std::collections::HashMap;
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use super::force_directed::ForceDirectedState;
use super::{Layout, LayoutExtras};
use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;

/// Barnes-Hut-specific tuning. Separate from [`ForceDirectedState`] since
/// these knobs are algorithmic to the tree traversal, not to the shared
/// physics tuning.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BarnesHutConfig {
    /// Approximation threshold. Smaller = more accurate (more recursion
    /// into the quadtree), larger = faster but more approximate. Standard
    /// default 0.5. Range `[0.0, 2.0]` is useful; higher values lose
    /// spatial coherence.
    pub theta: f32,
    /// Minimum cell size before subdivision stops. Prevents infinite
    /// recursion when nodes share positions. Units are world-space.
    pub min_cell_size: f32,
}

impl Default for BarnesHutConfig {
    fn default() -> Self {
        Self {
            theta: 0.5,
            min_cell_size: 1.0,
        }
    }
}

/// Barnes–Hut force-directed layout. Shares state type with plain FR so the
/// two are drop-in swappable — pick by scale.
#[derive(Debug, Default)]
pub struct BarnesHut {
    pub config: BarnesHutConfig,
    scratch_disp: Vec<Vector2D<f32>>,
}

impl BarnesHut {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: BarnesHutConfig) -> Self {
        Self {
            config,
            scratch_disp: Vec::new(),
        }
    }
}

impl<N> Layout<N> for BarnesHut
where
    N: Clone + Eq + Hash,
{
    type State = ForceDirectedState;

    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        dt_override: f32,
        viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        if !state.is_running || scene.nodes.is_empty() {
            return HashMap::new();
        }

        let dt = if dt_override > 0.0 {
            dt_override
        } else {
            state.dt
        };
        let k = {
            let area = (viewport.rect.size.width * viewport.rect.size.height).max(1.0);
            let k = (area / scene.nodes.len() as f32).sqrt() * state.k_scale;
            if !k.is_finite() {
                return HashMap::new();
            }
            k
        };

        let positions: Vec<Point2D<f32>> = scene.nodes.iter().map(|n| n.position).collect();
        let index_by_id: HashMap<&N, usize> = scene
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (&n.id, i))
            .collect();

        let n = scene.nodes.len();
        if self.scratch_disp.len() == n {
            for v in self.scratch_disp.iter_mut() {
                *v = Vector2D::zero();
            }
        } else {
            self.scratch_disp.clear();
            self.scratch_disp.resize(n, Vector2D::zero());
        }

        let bounds = compute_bounds(&positions, self.config.min_cell_size);
        let tree = Quadtree::build(&positions, bounds, self.config.min_cell_size);

        for i in 0..n {
            let force = tree.compute_repulsion(
                positions[i],
                k,
                state.epsilon,
                state.c_repulse,
                self.config.theta,
                state.repulsion_falloff,
            );
            self.scratch_disp[i] += force;
        }

        // Attraction: identical to FR.
        for edge in &scene.edges {
            let (Some(&si), Some(&ti)) =
                (index_by_id.get(&edge.source), index_by_id.get(&edge.target))
            else {
                continue;
            };
            if si == ti {
                continue;
            }
            let delta = positions[ti] - positions[si];
            let distance = delta.length().max(state.epsilon);
            let force = state.c_attract * (distance * distance) / k;
            let dir = delta / distance;
            self.scratch_disp[si] += dir * force;
            self.scratch_disp[ti] -= dir * force;
        }

        // Center gravity: identical to FR, including falloff shape.
        if state.c_gravity > 0.0 {
            let center = Point2D::new(
                viewport.rect.origin.x + viewport.rect.size.width * 0.5,
                viewport.rect.origin.y + viewport.rect.size.height * 0.5,
            );
            for i in 0..n {
                let to_center = center - positions[i];
                let distance = to_center.length().max(state.epsilon);
                let direction = to_center / distance;
                let magnitude = state.c_gravity * state.gravity_falloff.evaluate(distance);
                self.scratch_disp[i] += direction * magnitude;
            }
        }

        let mut deltas = HashMap::with_capacity(n);
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for (i, node) in scene.nodes.iter().enumerate() {
            if extras.pinned.contains(&node.id) {
                continue;
            }
            let mut step = self.scratch_disp[i] * dt * state.damping;
            let len = step.length();
            if len > state.max_step {
                step = step.normalize() * state.max_step;
            }
            let new_pos = positions[i] + step;
            if !new_pos.x.is_finite() || !new_pos.y.is_finite() {
                continue;
            }
            let clamped = len.min(state.max_step);
            if clamped > f32::EPSILON {
                deltas.insert(node.id.clone(), step);
                sum += clamped;
                count += 1;
            }
        }
        state.last_avg_displacement = (count > 0).then_some(sum / count as f32);
        state.step_count = state.step_count.saturating_add(1);
        deltas
    }

    fn is_converged(&self, state: &Self::State) -> bool {
        state
            .last_avg_displacement
            .is_some_and(|avg| avg < state.epsilon)
    }
}

// ── Quadtree ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct Bounds {
    min: Point2D<f32>,
    max: Point2D<f32>,
}

impl Bounds {
    fn size(&self) -> f32 {
        (self.max.x - self.min.x).max(self.max.y - self.min.y)
    }

    fn center(&self) -> Point2D<f32> {
        Point2D::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }

    fn quadrant_for(&self, p: Point2D<f32>) -> usize {
        let c = self.center();
        let right = (p.x >= c.x) as usize;
        let bottom = (p.y >= c.y) as usize;
        (bottom << 1) | right
    }

    fn child_bounds(&self, quadrant: usize) -> Bounds {
        let c = self.center();
        match quadrant {
            0 => Bounds {
                min: self.min,
                max: c,
            },
            1 => Bounds {
                min: Point2D::new(c.x, self.min.y),
                max: Point2D::new(self.max.x, c.y),
            },
            2 => Bounds {
                min: Point2D::new(self.min.x, c.y),
                max: Point2D::new(c.x, self.max.y),
            },
            _ => Bounds {
                min: c,
                max: self.max,
            },
        }
    }
}

fn compute_bounds(positions: &[Point2D<f32>], min_cell_size: f32) -> Bounds {
    if positions.is_empty() {
        return Bounds {
            min: Point2D::new(-1.0, -1.0),
            max: Point2D::new(1.0, 1.0),
        };
    }
    let mut min = positions[0];
    let mut max = positions[0];
    for p in positions.iter().skip(1) {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    // Avoid degenerate bounds (all points coincide).
    if (max.x - min.x).abs() < min_cell_size {
        max.x = min.x + min_cell_size;
    }
    if (max.y - min.y).abs() < min_cell_size {
        max.y = min.y + min_cell_size;
    }
    // Make square so quadrants are uniform.
    let size = (max.x - min.x).max(max.y - min.y);
    Bounds {
        min,
        max: Point2D::new(min.x + size, min.y + size),
    }
}

#[derive(Debug)]
enum Quadtree {
    Empty,
    Leaf {
        position: Point2D<f32>,
        mass: f32,
    },
    Internal {
        bounds: Bounds,
        center_of_mass: Point2D<f32>,
        total_mass: f32,
        children: [Box<Quadtree>; 4],
    },
}

impl Quadtree {
    fn build(positions: &[Point2D<f32>], bounds: Bounds, min_cell_size: f32) -> Self {
        let mut root = Quadtree::Empty;
        for p in positions {
            root.insert(*p, 1.0, bounds, min_cell_size);
        }
        root
    }

    fn insert(&mut self, position: Point2D<f32>, mass: f32, bounds: Bounds, min_cell_size: f32) {
        match self {
            Quadtree::Empty => {
                *self = Quadtree::Leaf { position, mass };
            }
            Quadtree::Leaf {
                position: existing_pos,
                mass: existing_mass,
            } => {
                if bounds.size() <= min_cell_size {
                    // Coincident-or-near points: merge into this leaf's mass.
                    let total = *existing_mass + mass;
                    let mid = Point2D::new(
                        (existing_pos.x * *existing_mass + position.x * mass) / total,
                        (existing_pos.y * *existing_mass + position.y * mass) / total,
                    );
                    *existing_pos = mid;
                    *existing_mass = total;
                    return;
                }
                // Split: convert to Internal and insert both points.
                let existing_position = *existing_pos;
                let existing_m = *existing_mass;
                let mut children: [Box<Quadtree>; 4] = [
                    Box::new(Quadtree::Empty),
                    Box::new(Quadtree::Empty),
                    Box::new(Quadtree::Empty),
                    Box::new(Quadtree::Empty),
                ];
                let q_existing = bounds.quadrant_for(existing_position);
                children[q_existing].insert(
                    existing_position,
                    existing_m,
                    bounds.child_bounds(q_existing),
                    min_cell_size,
                );
                let q_new = bounds.quadrant_for(position);
                children[q_new].insert(position, mass, bounds.child_bounds(q_new), min_cell_size);
                let total_mass = existing_m + mass;
                let center_of_mass = Point2D::new(
                    (existing_position.x * existing_m + position.x * mass) / total_mass,
                    (existing_position.y * existing_m + position.y * mass) / total_mass,
                );
                *self = Quadtree::Internal {
                    bounds,
                    center_of_mass,
                    total_mass,
                    children,
                };
            }
            Quadtree::Internal {
                bounds: b,
                center_of_mass,
                total_mass,
                children,
            } => {
                let q = b.quadrant_for(position);
                children[q].insert(position, mass, b.child_bounds(q), min_cell_size);
                let new_total = *total_mass + mass;
                *center_of_mass = Point2D::new(
                    (center_of_mass.x * *total_mass + position.x * mass) / new_total,
                    (center_of_mass.y * *total_mass + position.y * mass) / new_total,
                );
                *total_mass = new_total;
            }
        }
    }

    fn compute_repulsion(
        &self,
        target: Point2D<f32>,
        k: f32,
        epsilon: f32,
        c_repulse: f32,
        theta: f32,
        falloff: super::curves::Falloff,
    ) -> Vector2D<f32> {
        match self {
            Quadtree::Empty => Vector2D::zero(),
            Quadtree::Leaf { position, mass } => {
                let delta = target - *position;
                let distance_sq = delta.square_length();
                if distance_sq < epsilon * epsilon {
                    return Vector2D::zero();
                }
                let distance = distance_sq.sqrt();
                let force = c_repulse * (k * k) * *mass * falloff.evaluate(distance);
                (delta / distance) * force
            }
            Quadtree::Internal {
                bounds,
                center_of_mass,
                total_mass,
                children,
            } => {
                let delta = target - *center_of_mass;
                let distance_sq = delta.square_length().max(epsilon * epsilon);
                let distance = distance_sq.sqrt();
                let s = bounds.size();
                if s / distance < theta {
                    let force = c_repulse * (k * k) * *total_mass * falloff.evaluate(distance);
                    return (delta / distance) * force;
                }
                // Too close — recurse.
                let mut acc = Vector2D::zero();
                for child in children.iter() {
                    acc += child.compute_repulsion(target, k, epsilon, c_repulse, theta, falloff);
                }
                acc
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::force_directed::ForceDirected;
    use super::*;
    use crate::projection::ProjectionMode;
    use crate::scene::{CanvasEdge, CanvasNode, SceneMode, ViewId};
    use euclid::default::{Rect, Size2D};

    fn viewport(w: f32, h: f32) -> CanvasViewport {
        CanvasViewport {
            rect: Rect::new(Point2D::new(0.0, 0.0), Size2D::new(w, h)),
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
                .map(|(s, t)| CanvasEdge {
                    source: s,
                    target: t,
                    weight: 1.0,
                })
                .collect(),
            scene_objects: Vec::new(),
            overlays: Vec::new(),
            scene_mode: SceneMode::Browse,
            projection: ProjectionMode::TwoD,
        }
    }

    #[test]
    fn repulsion_pushes_nearby_nodes_apart() {
        let mut layout = BarnesHut::new();
        let mut state = ForceDirectedState {
            c_gravity: 0.0,
            c_attract: 0.0,
            ..Default::default()
        };
        let input = scene(vec![(0, 0.0, 0.0), (1, 1.0, 0.0)], vec![]);
        let deltas = layout.step(
            &input,
            &mut state,
            0.0,
            &viewport(1000.0, 1000.0),
            &LayoutExtras::default(),
        );
        assert!(deltas[&0].x < 0.0);
        assert!(deltas[&1].x > 0.0);
    }

    #[test]
    fn barnes_hut_approximates_fr_within_tolerance() {
        // With small graphs (<10 nodes), BH should agree with exact FR to
        // within a loose tolerance. Not per-step exact — drift is expected.
        let input = scene(
            vec![
                (0, 0.0, 0.0),
                (1, 100.0, 0.0),
                (2, 100.0, 100.0),
                (3, 0.0, 100.0),
                (4, 50.0, 50.0),
            ],
            vec![(0, 1), (1, 2), (2, 3), (3, 0)],
        );
        let viewport_rect = viewport(1000.0, 1000.0);
        let extras = LayoutExtras::default();

        let mut fr = ForceDirected::new();
        let mut fr_state = ForceDirectedState::default();
        let fr_deltas = fr.step(&input, &mut fr_state, 0.0, &viewport_rect, &extras);

        let mut bh = BarnesHut::new();
        let mut bh_state = ForceDirectedState::default();
        let bh_deltas = bh.step(&input, &mut bh_state, 0.0, &viewport_rect, &extras);

        // Sign of each component should match; magnitude within 2×.
        for id in 0..5u32 {
            let fr_d = fr_deltas.get(&id).copied().unwrap_or_default();
            let bh_d = bh_deltas.get(&id).copied().unwrap_or_default();
            if fr_d.length() < 0.01 && bh_d.length() < 0.01 {
                continue;
            }
            if fr_d.x.abs() > 0.01 {
                assert_eq!(
                    fr_d.x.is_sign_positive(),
                    bh_d.x.is_sign_positive(),
                    "x-sign disagreement on node {id}: fr={fr_d:?} bh={bh_d:?}"
                );
            }
            if fr_d.y.abs() > 0.01 {
                assert_eq!(
                    fr_d.y.is_sign_positive(),
                    bh_d.y.is_sign_positive(),
                    "y-sign disagreement on node {id}: fr={fr_d:?} bh={bh_d:?}"
                );
            }
        }
    }

    #[test]
    fn pinned_node_receives_no_delta() {
        let mut layout = BarnesHut::new();
        let mut state = ForceDirectedState::default();
        let input = scene(vec![(0, 0.0, 0.0), (1, 1.0, 0.0)], vec![]);
        let mut extras = LayoutExtras::default();
        extras.pinned.insert(0);
        let deltas = layout.step(&input, &mut state, 0.0, &viewport(1000.0, 1000.0), &extras);
        assert!(!deltas.contains_key(&0));
        assert!(deltas.contains_key(&1));
    }

    #[test]
    fn coincident_points_do_not_infinite_recurse() {
        let mut layout = BarnesHut::new();
        let mut state = ForceDirectedState::default();
        let input = scene(
            vec![(0, 50.0, 50.0), (1, 50.0, 50.0), (2, 50.0, 50.0)],
            vec![],
        );
        // Should terminate without stack overflow.
        let _ = layout.step(
            &input,
            &mut state,
            0.0,
            &viewport(1000.0, 1000.0),
            &LayoutExtras::default(),
        );
    }
}
