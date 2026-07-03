/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! [`AffinitySpring`] — the pairwise *affinity* force (graph-signals P4).
//!
//! Where [`EdgeSpring`](crate::EdgeSpring) pulls along the **topology** (one
//! spring per graph edge, uniform stiffness), `AffinitySpring` pulls along a
//! **signal**: a list of `(a, b, weight)` triples where `weight` is a `0..=1`
//! affinity score (structural Jaccard now, a content-embedding cosine later).
//! Structurally-similar nodes draw together even when they share no direct edge,
//! so communities tighten into legible clusters — the "cluster by affinity"
//! lever on force-directed.
//!
//! It composes *on top of* the built-in layout forces rather than replacing
//! them: [`NodeExclusion`](crate::NodeExclusion) still spreads everything,
//! [`EdgeSpring`](crate::EdgeSpring) still binds the topology, and affinity adds
//! a gentle extra attraction between similar pairs. So it is **attract-only** —
//! it pulls a stretched pair together but never pushes a close pair apart (that
//! is exclusion's job); a pair already within [`rest_length`](Self::rest_length)
//! contributes no force.
//!
//! The pair list is a **snapshot**: the affinity signal is recomputed on graph
//! mutation (a signal, not a per-frame quantity), and the host rebuilds the
//! force wholesale via [`Simulation::set_affinity_force`](crate::Simulation::set_affinity_force) —
//! exactly the rebuild-on-mutation discipline the field couplings follow. All
//! fields are `Send` (node keys + floats), so the force rides onto the physics
//! actor thread like the rest.

use kernel::graph::NodeKey;

use crate::{Force, ForceContext};

/// Default attraction stiffness at affinity `1.0` (scaled linearly by each
/// pair's weight). Deliberately below [`EdgeSpring`](crate::EdgeSpring)'s
/// stiffness — affinity is a nudge toward clustering, not a hard bond, and many
/// similar pairs sum up.
pub const DEFAULT_AFFINITY_STIFFNESS: f32 = 5.0;

/// Default separation a fully-similar (`weight = 1.0`) pair settles toward —
/// tighter than the topology spring's rest length so an affinity cluster reads
/// as visibly drawn-in against the wider force-directed spread.
pub const DEFAULT_AFFINITY_REST_LENGTH: f32 = 120.0;

/// A weighted, attract-only pairwise spring driven by an affinity signal.
///
/// Pulls each `(a, b, weight)` pair together with force
/// `stiffness · weight · (dist − rest_length)` along their separation, applied
/// only while `dist > rest_length` (attract-only; see the module docs). Build it
/// from an [`cartography::AffinityScores`](https://docs.rs)-shaped pair list and
/// install it with [`Simulation::set_affinity_force`](crate::Simulation::set_affinity_force).
#[derive(Clone, Debug)]
pub struct AffinitySpring {
    /// `(a, b, weight)` — the affinity between `a` and `b` in `0..=1`. A snapshot;
    /// rebuild the force when the signal recomputes.
    pairs: Vec<(NodeKey, NodeKey, f32)>,
    /// Attraction strength at `weight = 1.0` (force per unit of stretch beyond
    /// the rest length). Public for host tuning (configurability-over-defaults).
    pub stiffness: f32,
    /// The separation a `weight = 1.0` pair settles toward. No force acts while a
    /// pair is closer than this (attract-only).
    pub rest_length: f32,
}

impl AffinitySpring {
    /// A force over `pairs` (each `(a, b, weight)` with `weight` in `0..=1`) at
    /// the default stiffness / rest length. Tune the public fields after building
    /// if the host wants a different feel.
    pub fn new(pairs: impl IntoIterator<Item = (NodeKey, NodeKey, f32)>) -> Self {
        Self {
            pairs: pairs.into_iter().collect(),
            stiffness: DEFAULT_AFFINITY_STIFFNESS,
            rest_length: DEFAULT_AFFINITY_REST_LENGTH,
        }
    }

    /// The number of affinity pairs this force pulls along.
    pub fn pair_count(&self) -> usize {
        self.pairs.len()
    }

    /// Whether this force has any pairs (an empty affinity signal makes an inert
    /// force — the host can skip installing it).
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

impl Force for AffinitySpring {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        for &(a, b, weight) in &self.pairs {
            if a == b || weight <= 0.0 {
                continue; // a self-pair or a zero weight exerts nothing
            }
            let (Some(&ha), Some(&hb)) = (ctx.bodies_by_node.get(&a), ctx.bodies_by_node.get(&b))
            else {
                continue; // endpoint without a body (stale pair / not yet synced)
            };
            let (Some(pa), Some(pb)) = (
                ctx.bodies.get(ha).map(|x| x.translation()),
                ctx.bodies.get(hb).map(|x| x.translation()),
            ) else {
                continue;
            };
            let delta = pb - pa;
            let dist = delta.length();
            // Attract-only: pull a stretched pair together, but leave a pair that
            // is already within the rest length to NodeExclusion (no push-apart).
            if dist <= self.rest_length || dist < 1e-3 {
                continue;
            }
            let pull = delta / dist * (self.stiffness * weight * (dist - self.rest_length));
            if let Some(body) = ctx.bodies.get_mut(ha) {
                body.add_force(pull, true);
            }
            if let Some(body) = ctx.bodies.get_mut(hb) {
                body.add_force(-pull, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeExclusion, Simulation};
    use euclid::default::Point2D;
    use kernel::graph::Graph;
    use kernel::graph::fixtures::GraphFixtures;

    fn node_at(g: &mut Graph, id: u128, x: f32, y: f32) -> NodeKey {
        g.add_node_with_id(
            uuid::Uuid::from_u128(id),
            format!("mere://{id}"),
            Point2D::new(x, y),
        )
    }

    fn distance(sim: &Simulation, a: NodeKey, b: NodeKey) -> f32 {
        (sim.position_of(a).unwrap() - sim.position_of(b).unwrap()).length()
    }

    #[test]
    fn a_high_affinity_pair_is_pulled_together() {
        // Two nodes far apart, affinity 1.0: the spring draws them in toward the
        // rest length over a settle.
        let mut g = Graph::new();
        let a = node_at(&mut g, 1, -300.0, 0.0);
        let b = node_at(&mut g, 2, 300.0, 0.0);

        let mut sim = Simulation::new();
        sim.sync_with_graph(&g);
        let start = distance(&sim, a, b);
        sim.set_affinity_force(Some(AffinitySpring::new([(a, b, 1.0)])));
        for _ in 0..240 {
            sim.tick(1.0 / 60.0);
        }
        let end = distance(&sim, a, b);
        assert!(
            end < start,
            "affinity should pull the pair closer: {start} -> {end}"
        );
    }

    #[test]
    fn a_zero_affinity_pair_exerts_no_force() {
        // Weight 0: the spring is inert, so an otherwise force-free sim keeps the
        // pair where it started (damping bleeds off any residual, no drift).
        let mut g = Graph::new();
        let a = node_at(&mut g, 1, -300.0, 0.0);
        let b = node_at(&mut g, 2, 300.0, 0.0);

        let mut sim = Simulation::new();
        sim.sync_with_graph(&g);
        let start = distance(&sim, a, b);
        sim.set_affinity_force(Some(AffinitySpring::new([(a, b, 0.0)])));
        for _ in 0..120 {
            sim.tick(1.0 / 60.0);
        }
        assert!(
            (distance(&sim, a, b) - start).abs() < 1.0,
            "a zero-weight pair should not move"
        );
    }

    #[test]
    fn attract_only_does_not_push_a_close_pair_apart() {
        // A pair already well within the rest length: affinity must not shove them
        // apart (that is NodeExclusion's job). With only the affinity force present
        // they stay put.
        let mut g = Graph::new();
        let a = node_at(&mut g, 1, -20.0, 0.0);
        let b = node_at(&mut g, 2, 20.0, 0.0); // 40 apart, rest length 120
        let mut sim = Simulation::new();
        sim.sync_with_graph(&g);
        let start = distance(&sim, a, b);
        sim.set_affinity_force(Some(AffinitySpring::new([(a, b, 1.0)])));
        for _ in 0..120 {
            sim.tick(1.0 / 60.0);
        }
        assert!(
            (distance(&sim, a, b) - start).abs() < 2.0,
            "attract-only force must not push a close pair apart: {start} -> {}",
            distance(&sim, a, b)
        );
    }

    #[test]
    fn a_high_affinity_pair_draws_in_more_than_a_low_affinity_pair() {
        // Two independent pairs at the same start distance, one weight 0.9 one 0.1:
        // the stronger pair ends closer. Exclusion is present (the realistic
        // composition), so this checks affinity wins the tug against it.
        let mut g = Graph::new();
        let a = node_at(&mut g, 1, -250.0, -200.0);
        let b = node_at(&mut g, 2, 250.0, -200.0);
        let c = node_at(&mut g, 3, -250.0, 200.0);
        let d = node_at(&mut g, 4, 250.0, 200.0);

        let mut sim = Simulation::new();
        sim.sync_with_graph(&g);
        sim.add_force(NodeExclusion::default());
        sim.set_affinity_force(Some(AffinitySpring::new([(a, b, 0.9), (c, d, 0.1)])));
        for _ in 0..300 {
            sim.tick(1.0 / 60.0);
        }
        let strong = distance(&sim, a, b);
        let weak = distance(&sim, c, d);
        assert!(
            strong < weak,
            "the higher-affinity pair should end closer: strong={strong}, weak={weak}"
        );
    }

    #[test]
    fn clearing_the_force_leaves_the_layout_in_place() {
        // Setting the force to None must be position-preserving (forces never touch
        // body state) — the swap leaves the pair exactly where the last tick left it.
        let mut g = Graph::new();
        let a = node_at(&mut g, 1, -300.0, 0.0);
        let b = node_at(&mut g, 2, 300.0, 0.0);
        let mut sim = Simulation::new();
        sim.sync_with_graph(&g);
        sim.set_affinity_force(Some(AffinitySpring::new([(a, b, 1.0)])));
        for _ in 0..120 {
            sim.tick(1.0 / 60.0);
        }
        let before = distance(&sim, a, b);
        sim.set_affinity_force(None);
        let after = distance(&sim, a, b);
        assert_eq!(before, after, "clearing the force must not move bodies");
        assert_eq!(sim.affinity_pair_count(), 0, "the force is gone");
    }
}
