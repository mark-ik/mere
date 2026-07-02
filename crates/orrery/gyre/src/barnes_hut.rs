/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Barnes–Hut quadtree for O(n log n) approximate n-body repulsion.
//!
//! Harvested from the retired `graph-layout` force-directed module (its
//! deterministic arrangements moved to the `arrangements` crate; its physics
//! belongs here, with gyre's live simulation). The quadtree groups distant
//! bodies into their center-of-mass so each body's long-range repulsion is
//! computed against ~log(n) pseudo-bodies instead of all n−1 peers — the
//! standard acceleration for force-directed layout at scale. `θ`
//! ([`BarnesHutConfig::theta`]) controls the accuracy/speed trade-off: a cell
//! of size `s` at distance `d` is treated as a single mass when `s/d < θ`.
//!
//! This module is the spatial structure plus the [`repulsion_forces`]
//! primitive. Wiring it into the live [`crate::Simulation`] tick as a
//! [`crate::Force`] — a *global* charge-repulsion that would supplement
//! `NodeExclusion`'s *local* overlap separation (or replace it at scale) — is
//! a deliberate integration step, not done here: the two are different forces
//! and which role Barnes-Hut plays in the orrery is a tuning decision.

use euclid::default::{Point2D, Vector2D};
use rapier2d::prelude::*;

use crate::{Force, ForceContext};

/// Barnes–Hut approximation tuning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarnesHutConfig {
    /// Approximation threshold. Smaller = more accurate (more recursion into
    /// the quadtree), larger = faster but more approximate. Standard default
    /// 0.5; `[0.0, 2.0]` is the useful range.
    pub theta: f32,
    /// Minimum cell size before subdivision stops. Prevents infinite recursion
    /// when bodies share a position. World-space units.
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

/// Per-body Barnes–Hut repulsion forces, one per input position in input order.
///
/// `k` is the ideal edge length (force-directed layouts use
/// `sqrt(area / n) * scale`); the repulsion between two bodies at distance `d`
/// is `c_repulse * k² * mass / d` (standard inverse-distance charge). `epsilon`
/// floors the distance so coincident bodies don't blow up.
pub fn repulsion_forces(
    positions: &[Point2D<f32>],
    config: BarnesHutConfig,
    k: f32,
    c_repulse: f32,
    epsilon: f32,
) -> Vec<Vector2D<f32>> {
    if positions.is_empty() {
        return Vec::new();
    }
    let bounds = compute_bounds(positions, config.min_cell_size);
    let tree = Quadtree::build(positions, bounds, config.min_cell_size);
    positions
        .iter()
        .map(|p| tree.compute_repulsion(*p, k, epsilon, c_repulse, config.theta))
        .collect()
}

/// Global Barnes–Hut charge repulsion as a composable [`Force`].
///
/// The O(n log n) counterpart to [`crate::NodeExclusion`]'s spatial-index
/// pairwise charge: every body repels every other, with distant bodies
/// approximated by their cell center-of-mass. The host chooses the role by
/// which forces it registers — compose it *alongside* `NodeExclusion` (local
/// overlap separation + global spread) or *instead of* it at scale (where the
/// quadtree's complexity wins). Both modes are intended to be host-toggleable
/// and tunable; that surfacing is a host concern, this is the building block.
///
/// Charge falloff is the force-directed `strength * mass / distance` form (not
/// `NodeExclusion`'s inverse-square); `strength` will want re-calibrating
/// against `NodeExclusion` when this is wired into the live tick.
#[derive(Clone, Copy, Debug)]
pub struct BarnesHutRepulsion {
    /// Repulsion strength (force per unit mass at unit distance).
    pub strength: f32,
    /// Distance floor, so coincident bodies do not produce infinite force.
    pub min_distance: f32,
    /// Quadtree approximation tuning (θ accuracy/speed, min cell size).
    pub config: BarnesHutConfig,
}

impl Default for BarnesHutRepulsion {
    fn default() -> Self {
        Self {
            strength: 2_400.0,
            min_distance: 8.0,
            config: BarnesHutConfig::default(),
        }
    }
}

impl Force for BarnesHutRepulsion {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        // Snapshot bodies in a stable (handle, position) order, then build the
        // tree over the positions and apply each body's approximated repulsion.
        let mut handles: Vec<RigidBodyHandle> = Vec::with_capacity(ctx.bodies_by_node.len());
        let mut positions: Vec<Point2D<f32>> = Vec::with_capacity(ctx.bodies_by_node.len());
        for &handle in ctx.bodies_by_node.values() {
            if let Some(body) = ctx.bodies.get(handle) {
                let t = body.translation();
                handles.push(handle);
                positions.push(Point2D::new(t.x, t.y));
            }
        }
        // k = 1.0 folds the ideal-edge-length term out, leaving
        // `strength * mass / distance` per (pseudo-)body.
        let forces = repulsion_forces(&positions, self.config, 1.0, self.strength, self.min_distance);
        for (handle, f) in handles.iter().zip(forces) {
            if let Some(body) = ctx.bodies.get_mut(*handle) {
                body.add_force(Vector::new(f.x, f.y), true);
            }
        }
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

    /// Accumulated repulsion on a body at `target`. Standard inverse-distance
    /// charge: `c_repulse * k² * mass / d` per (pseudo-)body, directed away.
    fn compute_repulsion(
        &self,
        target: Point2D<f32>,
        k: f32,
        epsilon: f32,
        c_repulse: f32,
        theta: f32,
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
                let force = c_repulse * (k * k) * *mass / distance;
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
                    let force = c_repulse * (k * k) * *total_mass / distance;
                    return (delta / distance) * force;
                }
                // Too close to approximate — recurse.
                let mut acc = Vector2D::zero();
                for child in children.iter() {
                    acc += child.compute_repulsion(target, k, epsilon, c_repulse, theta);
                }
                acc
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::fixtures::GraphFixtures;

    #[test]
    fn repulsion_pushes_nearby_bodies_apart() {
        let positions = [Point2D::new(0.0, 0.0), Point2D::new(1.0, 0.0)];
        let forces = repulsion_forces(&positions, BarnesHutConfig::default(), 30.0, 1.0, 0.01);
        assert_eq!(forces.len(), 2);
        // Body 0 is pushed left (−x), body 1 right (+x).
        assert!(forces[0].x < 0.0, "left body should be pushed left: {forces:?}");
        assert!(forces[1].x > 0.0, "right body should be pushed right: {forces:?}");
    }

    #[test]
    fn coincident_bodies_do_not_infinite_recurse() {
        let positions = [
            Point2D::new(50.0, 50.0),
            Point2D::new(50.0, 50.0),
            Point2D::new(50.0, 50.0),
        ];
        // Must terminate (the min-cell-size merge prevents unbounded recursion).
        let forces = repulsion_forces(&positions, BarnesHutConfig::default(), 30.0, 1.0, 0.01);
        assert_eq!(forces.len(), 3);
    }

    #[test]
    fn empty_input_yields_no_forces() {
        let forces = repulsion_forces(&[], BarnesHutConfig::default(), 30.0, 1.0, 0.01);
        assert!(forces.is_empty());
    }

    #[test]
    fn far_apart_bodies_repel_less_than_near_ones() {
        let near = [Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)];
        let far = [Point2D::new(0.0, 0.0), Point2D::new(1000.0, 0.0)];
        let cfg = BarnesHutConfig::default();
        let near_f = repulsion_forces(&near, cfg, 30.0, 1.0, 0.01);
        let far_f = repulsion_forces(&far, cfg, 30.0, 1.0, 0.01);
        assert!(near_f[0].length() > far_f[0].length());
    }

    #[test]
    fn repulsion_force_separates_bodies_over_a_tick() {
        use crate::Simulation;
        use kernel::graph::Graph;

        let mut g = Graph::new();
        g.add_node_with_id(
            uuid::Uuid::from_u128(1),
            "mere://a".to_string(),
            Point2D::new(0.0, 0.0),
        );
        g.add_node_with_id(
            uuid::Uuid::from_u128(2),
            "mere://b".to_string(),
            Point2D::new(20.0, 0.0),
        );
        let keys: Vec<_> = g.nodes().map(|(k, _)| k).collect();

        let mut sim = Simulation::new();
        sim.sync_with_graph(&g);
        sim.add_force(BarnesHutRepulsion::default());

        let dist0 = (sim.position_of(keys[0]).unwrap() - sim.position_of(keys[1]).unwrap()).length();
        for _ in 0..60 {
            sim.tick(1.0 / 60.0);
        }
        let dist1 = (sim.position_of(keys[0]).unwrap() - sim.position_of(keys[1]).unwrap()).length();
        assert!(
            dist1 > dist0,
            "barnes-hut repulsion should push bodies apart: {dist0} -> {dist1}"
        );
    }
}
