/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Fruchterman–Reingold force-directed layout with center gravity.
//!
//! Algorithm (per Fruchterman & Reingold, 1991):
//!
//! - Optimal edge length `k = sqrt(area / n) * k_scale`
//! - Pairwise repulsion: `F_rep = c_repulse * k^2 / distance`
//! - Edge-based attraction: `F_att = c_attract * distance^2 / k`
//! - Center gravity (extension): `F_grav = c_gravity * (center - pos)`
//! - Step: `delta = sum_forces * dt * damping`, capped at `max_step`
//!
//! Returns per-node deltas; pinned nodes receive none. Behavior matches the
//! existing `egui_graphs::FruchtermanReingoldWithCenterGravity` tuning when
//! the legacy `GraphPhysicsTuning` parameters are applied to the state.

use std::collections::HashMap;
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use super::curves::Falloff;
use super::{Layout, LayoutExtras};
use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;

/// Persistent state for the force-directed layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceDirectedState {
    /// Gate for the whole simulation. When false, `step()` is a no-op.
    pub is_running: bool,
    /// Simulation timestep. 0.03 for Graphshell defaults.
    pub dt: f32,
    /// Minimum distance; prevents division by zero when two nodes coincide.
    pub epsilon: f32,
    /// Damping multiplier applied to the raw displacement vector.
    pub damping: f32,
    /// Hard cap on per-step displacement magnitude (in world units).
    pub max_step: f32,
    /// Multiplier on the viewport-derived optimal edge length `k`.
    pub k_scale: f32,
    /// Coefficient on the edge-attraction force.
    pub c_attract: f32,
    /// Coefficient on the pairwise repulsion force.
    pub c_repulse: f32,
    /// Coefficient on the center-gravity extension force.
    pub c_gravity: f32,
    /// Shape of the repulsion force vs distance. Default `Inverse`
    /// matches classic FR (`F ∝ k²/d`). `InverseSquare` is Coulomb-
    /// style (sharper far-field decay). `Linear` flips the sign of
    /// "further = weaker" — rarely useful but exposed for completeness.
    /// `Exponential(rate)` smooths the cutoff.
    pub repulsion_falloff: Falloff,
    /// Shape of the center-gravity force vs distance. Default `Linear`
    /// matches classic `(center − pos) × c_gravity` pull (stronger
    /// further out). `Inverse` creates a "sticky center" feel (stronger
    /// close). `InverseSquare` even sharper.
    pub gravity_falloff: Falloff,
    /// Running mean of per-step displacement magnitude; used for
    /// convergence detection. Never serialized.
    #[serde(skip)]
    pub last_avg_displacement: Option<f32>,
    /// Total number of simulation steps executed since state init.
    pub step_count: u64,
}

impl Default for ForceDirectedState {
    fn default() -> Self {
        Self {
            is_running: true,
            dt: 0.05,
            epsilon: 1e-3,
            damping: 0.3,
            max_step: 10.0,
            k_scale: 1.0,
            c_attract: 1.0,
            c_repulse: 1.0,
            c_gravity: 0.3,
            repulsion_falloff: Falloff::Inverse,
            gravity_falloff: Falloff::Linear,
            last_avg_displacement: None,
            step_count: 0,
        }
    }
}

/// Fruchterman–Reingold with center gravity.
///
/// Stateless apart from a reusable displacement scratch buffer.
#[derive(Debug, Default)]
pub struct ForceDirected {
    scratch_disp: Vec<Vector2D<f32>>,
}

impl ForceDirected {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<N> Layout<N> for ForceDirected
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
        let Some(k) = optimal_edge_length(viewport, scene.nodes.len(), state.k_scale) else {
            return HashMap::new();
        };

        // Positions snapshot — indexed by scene.nodes order for force compute.
        let positions: Vec<Point2D<f32>> = scene.nodes.iter().map(|n| n.position).collect();

        // Build an id → index lookup so edges can resolve to scratch slots.
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

        compute_repulsion(
            &positions,
            &mut self.scratch_disp,
            k,
            state.epsilon,
            state.c_repulse,
            state.repulsion_falloff,
        );
        compute_attraction(
            &scene.edges,
            &positions,
            &index_by_id,
            &mut self.scratch_disp,
            k,
            state.epsilon,
            state.c_attract,
        );
        if state.c_gravity > 0.0 {
            let center = viewport_center(viewport);
            compute_center_gravity(
                &positions,
                &mut self.scratch_disp,
                center,
                state.c_gravity,
                state.gravity_falloff,
                state.epsilon,
            );
        }

        let (deltas, avg) = apply_displacements(
            scene,
            &positions,
            &self.scratch_disp,
            dt,
            state.damping,
            state.max_step,
            extras,
        );
        state.last_avg_displacement = avg;
        state.step_count = state.step_count.saturating_add(1);
        deltas
    }

    fn is_converged(&self, state: &Self::State) -> bool {
        state
            .last_avg_displacement
            .is_some_and(|avg| avg < state.epsilon)
    }
}

// ── Math helpers ──────────────────────────────────────────────────────────────

fn optimal_edge_length(viewport: &CanvasViewport, node_count: usize, k_scale: f32) -> Option<f32> {
    if node_count == 0 {
        return None;
    }
    let area = (viewport.rect.size.width * viewport.rect.size.height).max(1.0);
    let k = (area / node_count as f32).sqrt() * k_scale;
    k.is_finite().then_some(k)
}

fn viewport_center(viewport: &CanvasViewport) -> Point2D<f32> {
    Point2D::new(
        viewport.rect.origin.x + viewport.rect.size.width * 0.5,
        viewport.rect.origin.y + viewport.rect.size.height * 0.5,
    )
}

fn compute_repulsion(
    positions: &[Point2D<f32>],
    disp: &mut [Vector2D<f32>],
    k: f32,
    epsilon: f32,
    c_repulse: f32,
    falloff: Falloff,
) {
    let n = positions.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let delta = positions[i] - positions[j];
            let distance = delta.length().max(epsilon);
            let force = c_repulse * (k * k) * falloff.evaluate(distance);
            let dir = delta / distance;
            disp[i] += dir * force;
            disp[j] -= dir * force;
        }
    }
}

fn compute_attraction<N>(
    edges: &[crate::scene::CanvasEdge<N>],
    positions: &[Point2D<f32>],
    index_by_id: &HashMap<&N, usize>,
    disp: &mut [Vector2D<f32>],
    k: f32,
    epsilon: f32,
    c_attract: f32,
) where
    N: Clone + Eq + Hash,
{
    for edge in edges {
        let (Some(&i), Some(&j)) = (index_by_id.get(&edge.source), index_by_id.get(&edge.target))
        else {
            continue;
        };
        if i == j {
            continue;
        }
        let delta = positions[j] - positions[i];
        let distance = delta.length().max(epsilon);
        let force = c_attract * (distance * distance) / k;
        let dir = delta / distance;
        disp[i] += dir * force;
        disp[j] -= dir * force;
    }
}

fn compute_center_gravity(
    positions: &[Point2D<f32>],
    disp: &mut [Vector2D<f32>],
    center: Point2D<f32>,
    c_gravity: f32,
    falloff: Falloff,
    epsilon: f32,
) {
    for (i, pos) in positions.iter().enumerate() {
        let to_center = center - *pos;
        let distance = to_center.length().max(epsilon);
        let direction = to_center / distance;
        let magnitude = c_gravity * falloff.evaluate(distance);
        disp[i] += direction * magnitude;
    }
}

fn apply_displacements<N>(
    scene: &CanvasSceneInput<N>,
    positions: &[Point2D<f32>],
    disp: &[Vector2D<f32>],
    dt: f32,
    damping: f32,
    max_step: f32,
    extras: &LayoutExtras<N>,
) -> (HashMap<N, Vector2D<f32>>, Option<f32>)
where
    N: Clone + Eq + Hash,
{
    let mut deltas = HashMap::with_capacity(scene.nodes.len());
    let mut sum = 0.0f32;
    let mut count = 0usize;
    for (i, node) in scene.nodes.iter().enumerate() {
        if extras.pinned.contains(&node.id) {
            continue;
        }
        let mut step = disp[i] * dt * damping;
        let len = step.length();
        if len > max_step {
            step = step.normalize() * max_step;
        }
        let new_pos = positions[i] + step;
        if !new_pos.x.is_finite() || !new_pos.y.is_finite() {
            continue;
        }
        let clamped_len = len.min(max_step);
        if clamped_len > f32::EPSILON {
            deltas.insert(node.id.clone(), step);
            sum += clamped_len;
            count += 1;
        }
    }
    let avg = if count == 0 {
        None
    } else {
        Some(sum / count as f32)
    };
    (deltas, avg)
}

#[cfg(test)]
mod tests {
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
                .map(|(s, t)| CanvasEdge::untagged(s, t))
                .collect(),
            scene_objects: Vec::new(),
            overlays: Vec::new(),
            scene_mode: SceneMode::Browse,
            projection: ProjectionMode::default(),
        }
    }

    #[test]
    fn repulsion_moves_nearby_nodes_apart() {
        let mut layout = ForceDirected::new();
        let mut state = ForceDirectedState {
            c_gravity: 0.0,
            c_attract: 0.0,
            ..Default::default()
        };
        let input = scene(vec![(0, 0.0, 0.0), (1, 1.0, 0.0)], vec![]);
        let extras = LayoutExtras::default();
        let deltas = layout.step(&input, &mut state, 0.0, &viewport(1000.0, 1000.0), &extras);
        let d0 = deltas.get(&0).copied().unwrap_or_default();
        let d1 = deltas.get(&1).copied().unwrap_or_default();
        // Node 0 (left) should get pushed further left, node 1 further right.
        assert!(d0.x < 0.0);
        assert!(d1.x > 0.0);
    }

    #[test]
    fn attraction_pulls_connected_nodes_together() {
        let mut layout = ForceDirected::new();
        let mut state = ForceDirectedState {
            c_gravity: 0.0,
            c_repulse: 0.0,
            ..Default::default()
        };
        let input = scene(vec![(0, 0.0, 0.0), (1, 1200.0, 0.0)], vec![(0, 1)]);
        let extras = LayoutExtras::default();
        let deltas = layout.step(&input, &mut state, 0.0, &viewport(1000.0, 1000.0), &extras);
        let d0 = deltas.get(&0).copied().unwrap_or_default();
        let d1 = deltas.get(&1).copied().unwrap_or_default();
        // Connected pair: left node pulled right, right node pulled left.
        assert!(d0.x > 0.0);
        assert!(d1.x < 0.0);
    }

    #[test]
    fn pinned_node_receives_no_delta() {
        let mut layout = ForceDirected::new();
        let mut state = ForceDirectedState::default();
        let input = scene(vec![(0, 0.0, 0.0), (1, 1.0, 0.0)], vec![]);
        let mut extras = LayoutExtras::default();
        extras.pinned.insert(0);
        let deltas = layout.step(&input, &mut state, 0.0, &viewport(1000.0, 1000.0), &extras);
        assert!(!deltas.contains_key(&0));
        assert!(deltas.contains_key(&1));
    }

    #[test]
    fn non_running_state_returns_empty_deltas() {
        let mut layout = ForceDirected::new();
        let mut state = ForceDirectedState {
            is_running: false,
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
        assert!(deltas.is_empty());
    }

    #[test]
    fn gravity_pulls_node_toward_viewport_center() {
        let mut layout = ForceDirected::new();
        let mut state = ForceDirectedState {
            c_attract: 0.0,
            c_repulse: 0.0,
            c_gravity: 0.5,
            ..Default::default()
        };
        let input = scene(vec![(0, 800.0, 0.0)], vec![]);
        let deltas = layout.step(
            &input,
            &mut state,
            0.0,
            &viewport(1000.0, 1000.0),
            &LayoutExtras::default(),
        );
        let d0 = deltas.get(&0).copied().unwrap_or_default();
        // Center is (500, 500). Node at (800, 0) should be pulled toward center.
        assert!(d0.x < 0.0);
        assert!(d0.y > 0.0);
    }

    #[test]
    fn repulsion_falloff_changes_force_magnitude() {
        let input = scene(vec![(0, 0.0, 0.0), (1, 200.0, 0.0)], vec![]);
        let viewport = viewport(1000.0, 1000.0);
        let extras = LayoutExtras::default();

        let mut inverse = ForceDirected::new();
        let mut state_inverse = ForceDirectedState {
            c_gravity: 0.0,
            c_attract: 0.0,
            repulsion_falloff: super::super::curves::Falloff::Inverse,
            ..Default::default()
        };
        let inv_delta = inverse
            .step(&input, &mut state_inverse, 0.0, &viewport, &extras)
            .get(&0)
            .copied()
            .unwrap_or_default();

        let mut sq = ForceDirected::new();
        let mut state_sq = ForceDirectedState {
            c_gravity: 0.0,
            c_attract: 0.0,
            repulsion_falloff: super::super::curves::Falloff::InverseSquare,
            ..Default::default()
        };
        let sq_delta = sq
            .step(&input, &mut state_sq, 0.0, &viewport, &extras)
            .get(&0)
            .copied()
            .unwrap_or_default();

        // At distance=200, InverseSquare should give a much smaller
        // force than Inverse (1/40000 vs 1/200). Same direction.
        assert!(inv_delta.x.is_sign_negative() && sq_delta.x.is_sign_negative());
        assert!(inv_delta.length() > sq_delta.length());
    }

    #[test]
    fn step_count_advances() {
        let mut layout = ForceDirected::new();
        let mut state = ForceDirectedState::default();
        let input = scene(vec![(0, 0.0, 0.0), (1, 10.0, 0.0)], vec![]);
        let _ = layout.step(
            &input,
            &mut state,
            0.0,
            &viewport(1000.0, 1000.0),
            &LayoutExtras::default(),
        );
        let _ = layout.step(
            &input,
            &mut state,
            0.0,
            &viewport(1000.0, 1000.0),
            &LayoutExtras::default(),
        );
        assert_eq!(state.step_count, 2);
    }
}
