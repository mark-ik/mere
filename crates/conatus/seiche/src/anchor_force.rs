// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Anchor springs — an arrangement expressed as a *field* rather than an
//! override.
//!
//! A layout that writes positions is an authority: it decides where a node is,
//! and the simulation has nothing to say. An anchor spring makes the same
//! layout a **participant**: each node is pulled toward the slot the
//! arrangement chose, while repulsion, edge springs, collisions, coupled
//! fields, and the user's drag all still act on it. Stiffness is the dial
//! between the two readings — high holds the arrangement's shape, low lets the
//! graph's own forces win, zero is pure physics.
//!
//! This is what lets an arrangement compose with running physics instead of
//! excluding it, and it is deliberately the same shape as
//! [`CouplingForce`](crate::CouplingForce): a target, a response, a strength.

use rapier2d::prelude::*;
use std::collections::HashMap;

use crate::{Force, ForceContext, NodeKey};

/// Default pull toward an anchor, in force per unit of offset. Chosen so a node
/// displaced by roughly a node-width returns without visible overshoot at the
/// default damping.
pub const DEFAULT_ANCHOR_STIFFNESS: f32 = 12.0;

/// Distance (world units) inside which an anchor exerts nothing, so a settled
/// node rests instead of jittering against its own target.
pub const DEFAULT_ANCHOR_SLACK: f32 = 0.5;

/// Per-node springs toward arrangement-chosen target positions.
///
/// Install with
/// [`Simulation::set_anchor_force`](crate::Simulation::set_anchor_force). The
/// force is a snapshot: rebuild it when the arrangement recomputes.
#[derive(Clone, Debug, Default)]
pub struct AnchorSpring {
    anchors: HashMap<NodeKey, Vector>,
    /// Force per unit of offset from the anchor. Public for host tuning
    /// (configurability over opinionated defaults): this is the dial between
    /// "the arrangement holds" and "the graph's own forces win".
    pub stiffness: f32,
    /// Offset below which the anchor is satisfied and exerts nothing.
    pub slack: f32,
}

impl AnchorSpring {
    /// Springs toward `anchors` at the default stiffness.
    pub fn new(anchors: impl IntoIterator<Item = (NodeKey, (f32, f32))>) -> Self {
        Self {
            anchors: anchors
                .into_iter()
                .map(|(k, (x, y))| (k, Vector::new(x, y)))
                .collect(),
            stiffness: DEFAULT_ANCHOR_STIFFNESS,
            slack: DEFAULT_ANCHOR_SLACK,
        }
    }

    /// The same anchors at an explicit stiffness. `0.0` makes an inert force —
    /// the arrangement becomes a pure initial condition.
    pub fn with_stiffness(mut self, stiffness: f32) -> Self {
        self.stiffness = stiffness.max(0.0);
        self
    }

    /// How many nodes this force anchors.
    pub fn len(&self) -> usize {
        self.anchors.len()
    }

    /// Whether the force anchors nothing (the host can skip installing it).
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }
}

impl Force for AnchorSpring {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        if self.stiffness <= 0.0 {
            return; // an arrangement that only seeds
        }
        for (node, anchor) in &self.anchors {
            let Some(&handle) = ctx.bodies_by_node.get(node) else {
                continue; // anchored node has no body (stale / not yet synced)
            };
            let Some(position) = ctx.bodies.get(handle).map(|b| b.translation()) else {
                continue;
            };
            let offset = anchor - position;
            let distance = offset.length();
            if distance <= self.slack || distance < 1e-4 {
                continue;
            }
            let pull = offset / distance * (self.stiffness * (distance - self.slack));
            if let Some(body) = ctx.bodies.get_mut(handle) {
                body.add_force(pull, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_stiffness_is_inert() {
        let force = AnchorSpring::new([(NodeKey::new(0), (10.0, 0.0))]).with_stiffness(0.0);
        assert_eq!(force.stiffness, 0.0);
        assert_eq!(force.len(), 1, "the anchors are kept; only the pull is off");
    }

    #[test]
    fn negative_stiffness_clamps_to_inert() {
        let force = AnchorSpring::new([(NodeKey::new(0), (0.0, 0.0))]).with_stiffness(-5.0);
        assert_eq!(force.stiffness, 0.0, "an anchor never pushes away");
    }

    #[test]
    fn empty_anchors_report_empty() {
        assert!(AnchorSpring::default().is_empty());
    }
}
