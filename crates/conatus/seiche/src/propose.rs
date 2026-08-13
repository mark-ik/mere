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

use crate::scene_spec::SceneBodyId;
use crate::{NodeKey, Simulation};

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
}
