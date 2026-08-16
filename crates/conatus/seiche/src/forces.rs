//! Built-in force forces for the orrery's force-directed layout.
//!
//! Three forces compose into a Fruchterman-Reingold-shaped layout:
//!
//! - [`NodeExclusion`] — every node repels every other, spreading them apart
//!   (the "charge"). This is the long-range push *beyond* the hard ball-collider
//!   contact that already prevents overlap.
//! - [`EdgeSpring`] — connected nodes attract along their edge (Hooke's law
//!   toward a rest length), pulling neighbors together.
//! - [`Boundary`] — a weak centering pull toward the origin so disconnected
//!   pieces stay on screen and the whole layout stays bounded.
//!
//! Each force reads body positions and accumulates forces through
//! `add_force`; [`crate::Simulation::tick`] walks the registered forces before
//! stepping, and rapier's per-body damping settles the result to rest.
//!
//! All three constants are public so the host can tune feel (per the
//! configurability-over-defaults stance); the defaults give a readable layout
//! at Mere's world scale (1 unit ~= 1 px, node radius
//! [`crate::NODE_BODY_RADIUS`]).

use rapier2d::prelude::*;

use crate::{Force, ForceContext, RepulsionRequest};

/// Pairwise repulsion that spreads nodes apart (the force-directed charge).
///
/// Inverse-square falloff with a `min_distance` floor (no singularity at
/// near-zero separation) and a `cutoff` beyond which a pair does not interact.
/// An all-pairs scan over the node bodies, summed once per side so the result is
/// symmetric; at the orrery's scale (dozens–hundreds of nodes) the O(n^2) cost is
/// negligible. (rapier's collision `QueryPipeline` went ephemeral in 0.33, so the
/// spatial-index narrowing this once used was dropped — reinstate one, or the
/// Barnes–Hut tree in this crate, only if a graph grows large enough to feel it.)
#[derive(Clone, Copy, Debug)]
pub struct NodeExclusion {
    /// Repulsion strength (force at unit distance, before the inverse-square).
    pub strength: f32,
    /// Pairs farther apart than this exert no force.
    pub cutoff: f32,
    /// Distance floor, so coincident nodes do not produce infinite force.
    pub min_distance: f32,
}

impl Default for NodeExclusion {
    fn default() -> Self {
        Self {
            // The charge has to stay relevant out at the spread distances, or
            // the (distance-proportional) Boundary pull collapses the graph back
            // to a clump. Tuned with Boundary ~0.08 to settle unconnected nodes
            // ~140 px apart at Mere's 1-unit ~= 1-px world scale.
            strength: 220_000.0,
            cutoff: 1_000.0,
            min_distance: 8.0,
        }
    }
}

impl Force for NodeExclusion {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        // Snapshot every node's (handle, position) immutably before touching forces.
        let nodes: Vec<(RigidBodyHandle, Vector)> = ctx
            .bodies_by_node
            .values()
            .filter_map(|&handle| ctx.bodies.get(handle).map(|b| (handle, b.translation())))
            .collect();

        // Above the threshold, a host may stage this exact layout law through a
        // different evaluator. This remains a CPU-integrator seam: positions and
        // forces cross it as slices, unlike quint's resident-buffer lane.
        if let Some(solver) = ctx.repulsion_solver {
            if nodes.len() >= ctx.gpu_repulsion_threshold {
                let xs: Vec<f32> = nodes.iter().map(|(_, p)| p.x).collect();
                let ys: Vec<f32> = nodes.iter().map(|(_, p)| p.y).collect();
                let request = RepulsionRequest {
                    strength: self.strength,
                    cutoff: self.cutoff,
                    min_distance: self.min_distance,
                };
                match solver(&xs, &ys, request) {
                    Ok(forces) => {
                        let (fx, fy) = forces.components();
                        for (idx, (handle, _)) in nodes.iter().enumerate() {
                            if let Some(body) = ctx.bodies.get_mut(*handle) {
                                body.add_force(Vector::new(fx[idx], fy[idx]), true);
                            }
                        }
                        return;
                    }
                    Err(error) => tracing::warn!(
                        ?error,
                        nodes = nodes.len(),
                        "staged repulsion failed; falling back to NodeExclusion's CPU law"
                    ),
                }
            }
        }

        // All-pairs inverse-square repulsion within `cutoff`. The rapier spatial
        // index this once narrowed against went ephemeral in rapier 0.33, so the
        // O(n^2) scan is the version-clean replacement; at the orrery's scale
        // (dozens–hundreds of nodes) it is cheap, and each pair is summed once per
        // side so the result stays symmetric. (Reinstate a spatial index — or the
        // Barnes–Hut tree already in this crate — if a graph grows large enough to
        // feel it.)
        let cutoff2 = self.cutoff * self.cutoff;
        let mut forces = vec![Vector::ZERO; nodes.len()];
        for i in 0..nodes.len() {
            let pos_i = nodes[i].1;
            let mut force_i = Vector::ZERO;
            for j in 0..nodes.len() {
                if j == i {
                    continue;
                }
                let delta = pos_i - nodes[j].1;
                if delta.length_squared() > cutoff2 {
                    continue;
                }
                let dist = delta.length().max(self.min_distance);
                force_i += delta / dist * (self.strength / (dist * dist));
            }
            forces[i] = force_i;
        }

        for (idx, (handle, _)) in nodes.iter().enumerate() {
            if let Some(body) = ctx.bodies.get_mut(*handle) {
                body.add_force(forces[idx], true);
            }
        }
    }
}

/// Hooke's-law attraction along edges: connected nodes pull toward a rest
/// length. Reads [`ForceContext::edges`], so it pulls along whatever topology
/// the caller synced via [`crate::Simulation::sync_edges`].
#[derive(Clone, Copy, Debug)]
pub struct EdgeSpring {
    /// Spring stiffness (force per unit of stretch beyond the rest length).
    pub stiffness: f32,
    /// The separation the spring settles toward when no other force acts.
    pub rest_length: f32,
}

impl Default for EdgeSpring {
    fn default() -> Self {
        Self {
            stiffness: 10.0,
            rest_length: 170.0,
        }
    }
}

impl Force for EdgeSpring {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        for &(a, b) in ctx.edges {
            if a == b {
                continue; // no self-springs
            }
            let (Some(&ha), Some(&hb)) = (ctx.bodies_by_node.get(&a), ctx.bodies_by_node.get(&b))
            else {
                continue; // endpoint without a body (stale edge / not yet synced)
            };
            let (Some(pa), Some(pb)) = (
                ctx.bodies.get(ha).map(|x| x.translation()),
                ctx.bodies.get(hb).map(|x| x.translation()),
            ) else {
                continue;
            };
            let delta = pb - pa;
            let dist = delta.length();
            if dist < 1e-3 {
                continue;
            }
            // Positive when stretched past the rest length: pulls a toward b and
            // b toward a; negative (compressed) pushes them apart to rest.
            let pull = delta / dist * (self.stiffness * (dist - self.rest_length));
            if let Some(body) = ctx.bodies.get_mut(ha) {
                body.add_force(pull, true);
            }
            if let Some(body) = ctx.bodies.get_mut(hb) {
                body.add_force(-pull, true);
            }
        }
    }
}

/// Weak centering pull toward the origin, proportional to distance. Keeps
/// disconnected components from drifting off and bounds the whole layout.
#[derive(Clone, Copy, Debug)]
pub struct Boundary {
    /// Centering strength (force per unit of distance from the origin).
    pub strength: f32,
}

impl Default for Boundary {
    fn default() -> Self {
        // Gentle: just enough to keep disconnected components on-screen. Strong
        // centering (the old 1.5) grows with distance and overpowers the
        // inverse-square charge, collapsing the whole graph back to a clump.
        Self { strength: 0.08 }
    }
}

impl Force for Boundary {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let handles: Vec<RigidBodyHandle> = ctx.bodies_by_node.values().copied().collect();
        for handle in handles {
            let Some(pos) = ctx.bodies.get(handle).map(|b| b.translation()) else {
                continue;
            };
            if let Some(body) = ctx.bodies.get_mut(handle) {
                body.add_force(-pos * self.strength, true);
            }
        }
    }
}
