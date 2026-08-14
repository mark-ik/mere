//! Containment proposals: the read-only half of "physics proposes, the
//! record disposes" (spatial compute plan P1, 2026-08-13).
//!
//! seiche answers the geometric question and nothing else: which scene
//! bodies contain a node right now. Which containments *mean* anything,
//! and when one becomes a recorded fact, is the host's decision at an
//! explicit commitment (a release gesture, a standing rule). No fact
//! type lives here, no log, no callback: a physics engine that owned
//! the record would be the projection ruling's refusal 2 built into the
//! substrate.

use rapier2d::prelude::*;

use crate::scene_spec::SceneBodyId;
use crate::{NodeKey, Simulation};

/// A body proposed as holding a node up — another node, or a scene body.
/// What being held *means* ("resting on", "stacked on") is the host's to
/// decide at a commitment; this is only the geometric answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    Node(NodeKey),
    Scene(SceneBodyId),
}

impl Simulation {
    /// Every scene body whose collider contains the node's centre right
    /// now, in ascending id order. Empty for an unknown node, a node
    /// with no body, or a node contained by nothing.
    ///
    /// The centre is the containment test on purpose: a card is "in"
    /// the bin when its heart is, which is how a hand judges it, and it
    /// keeps the answer stable while the card's edge dithers on the
    /// boundary.
    pub fn containments_of(&self, node: NodeKey) -> Vec<SceneBodyId> {
        let Some(&handle) = self.bodies_by_node.get(&node) else {
            return Vec::new();
        };
        let Some(body) = self.bodies.get(handle) else {
            return Vec::new();
        };
        let centre = body.translation();

        let mut inside: Vec<SceneBodyId> = self
            .scene_bodies
            .iter()
            .filter_map(|(&id, &(scene_handle, ref collider))| {
                let scene_body = self.bodies.get(scene_handle)?;
                let shape = collider.to_shared_shape();
                shape
                    .contains_point(scene_body.position(), centre)
                    .then_some(id)
            })
            .collect();
        inside.sort_unstable_by_key(|id| id.0);
        inside
    }

    /// Every body currently holding this node up, read from rapier's live
    /// contact graph: an active contact whose normal, taken from the
    /// supporter toward the node, opposes world gravity (mostly upward).
    /// Empty for an unknown node, a node in free fall, or a world with no
    /// gravity to be held against. Read-only, like [`containments_of`]
    /// (Self::containments_of): a proposal the host may promote to a fact
    /// at a commitment, never a stored relation. (Tactile T3.)
    pub fn supports_of(&self, node: NodeKey) -> Vec<Support> {
        let g = self.gravity;
        let g_len = (g.x * g.x + g.y * g.y).sqrt();
        if g_len < f32::EPSILON {
            return Vec::new();
        }
        let down = Vector::new(g.x / g_len, g.y / g_len);

        let Some(&handle) = self.bodies_by_node.get(&node) else {
            return Vec::new();
        };
        let Some(body) = self.bodies.get(handle) else {
            return Vec::new();
        };

        let mut supports = Vec::new();
        for &ch in body.colliders() {
            for pair in self.narrow_phase.contact_pairs_with(ch) {
                if !pair.has_any_active_contact() {
                    continue;
                }
                // The manifold normal is world-space, pointing from
                // `collider1` toward `collider2`; orient it toward the
                // node so "the supporter pushes up" is one comparison.
                let ours_is_first = pair.collider1 == ch;
                let holds_up = pair.manifolds.iter().any(|m| {
                    let toward_node = if ours_is_first {
                        -m.data.normal
                    } else {
                        m.data.normal
                    };
                    toward_node.dot(down) < -0.5
                });
                if !holds_up {
                    continue;
                }
                let other = if ours_is_first {
                    pair.collider2
                } else {
                    pair.collider1
                };
                let Some(other_body) = self.colliders.get(other).and_then(|c| c.parent()) else {
                    continue;
                };
                if let Some((&key, _)) = self.bodies_by_node.iter().find(|&(_, &h)| h == other_body)
                {
                    supports.push(Support::Node(key));
                } else if let Some((&id, _)) = self
                    .scene_bodies
                    .iter()
                    .find(|&(_, &(h, _))| h == other_body)
                {
                    supports.push(Support::Scene(id));
                }
            }
        }
        supports.sort_unstable_by_key(|s| match *s {
            Support::Node(n) => (0, n.index() as u64),
            Support::Scene(id) => (1, id.0),
        });
        supports.dedup();
        supports
    }
}
