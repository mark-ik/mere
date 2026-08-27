// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The sieve: collision predicates as data (tactile tier plan T2).
//!
//! A node carries [`Kinds`], a 16-bit band whose meaning belongs to the
//! host; a scene body declared as a sieve blocks the kinds it names and
//! passes everything else. rapier's interaction groups compute the
//! predicate, so a sieve costs nothing per frame: the physics engine's
//! own broad phase is the query engine.
//!
//! The bit algebra: with And-mode groups, two colliders interact iff
//! `A.memberships ∩ B.filter ≠ ∅` in both directions. Nodes carry their
//! kinds in memberships and the whole kind band in filter; a sieve
//! carries its blocked kinds in both. The pairwise test then reduces to
//! `kinds ∩ blocks ≠ ∅`: a node meets a sieve exactly when the sieve
//! blocks one of its kinds, a kindless node passes every sieve, and the
//! tangibility lever is untouched because kinds ride in different bits
//! than [`SCENE_GROUP`](crate::SCENE_GROUP).

use std::collections::HashMap;

use rapier2d::prelude::*;

use crate::scene_spec::SceneBodyId;
use crate::{NODE_GROUP, NodeKey, SCENE_GROUP, Simulation};

/// A node's kinds, or a sieve's blocked set: sixteen bits of
/// host-defined meaning. seiche never interprets them, the same way it
/// never interprets a [`NodeKey`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kinds(pub u16);

impl Kinds {
    pub const NONE: Kinds = Kinds(0);

    pub fn intersects(self, other: Kinds) -> bool {
        self.0 & other.0 != 0
    }
}

/// The kind band sits above the two structural groups: bits 16..32 of
/// rapier's group space.
const KIND_SHIFT: u32 = 16;

fn kind_bits(kinds: Kinds) -> Group {
    Group::from_bits_truncate((kinds.0 as u32) << KIND_SHIFT)
}

/// Every kind bit at once: the node-side filter admits any sieve.
fn kind_band() -> Group {
    Group::from_bits_truncate(0xFFFF << KIND_SHIFT)
}

/// A node collider's groups, from its kinds and the tangibility lever.
pub(crate) fn node_mask(kinds: Kinds, tangible: bool) -> InteractionGroups {
    let memberships = NODE_GROUP | kind_bits(kinds);
    let mut filter = NODE_GROUP | kind_band();
    if tangible {
        filter |= SCENE_GROUP;
    }
    InteractionGroups::new(memberships, filter, InteractionTestMode::And)
}

/// A sieve collider's groups: an ordinary scene body toward everything
/// else, plus the blocked kinds toward nodes.
pub(crate) fn sieve_mask(blocks: Kinds) -> InteractionGroups {
    let bits = kind_bits(blocks);
    InteractionGroups::new(
        SCENE_GROUP | bits,
        SCENE_GROUP | bits,
        InteractionTestMode::And,
    )
}

/// Per-simulation sift state: what each node is, what each sieve
/// blocks. Remembered so re-syncs and tangibility remasks compose.
#[derive(Default)]
pub(crate) struct Sift {
    pub(crate) node_kinds: HashMap<NodeKey, Kinds>,
    pub(crate) sieves: HashMap<SceneBodyId, Kinds>,
}

impl Simulation {
    /// Declare what kinds a node is. The meaning of each bit is the
    /// host's; a mere's profile decides which data facts light which
    /// bits. Takes effect immediately on the live collider and persists
    /// across body re-syncs.
    pub fn set_node_kinds(&mut self, node: NodeKey, kinds: Kinds) {
        self.sift.node_kinds.insert(node, kinds);
        // Re-mask preserving whatever the tangibility lever last set on
        // this body (the per-node lever may override the scene-wide
        // flag, and the live filter is the record of it).
        let tangible = self.node_is_tangible(node);
        self.remask_node(node, tangible);
    }

    pub fn node_kinds(&self, node: NodeKey) -> Kinds {
        self.sift
            .node_kinds
            .get(&node)
            .copied()
            .unwrap_or(Kinds::NONE)
    }

    /// Declare a scene body as a sieve blocking the given kinds. A
    /// blocked node collides with it; every other node passes through
    /// as if it were not there. `Kinds::NONE` retires the sieve back to
    /// an ordinary scene body.
    pub fn set_scene_sieve(&mut self, id: SceneBodyId, blocks: Kinds) {
        if blocks == Kinds::NONE {
            self.sift.sieves.remove(&id);
        } else {
            self.sift.sieves.insert(id, blocks);
        }
        let Some(handle) = self.scene_bodies.get(&id).map(|(h, _)| *h) else {
            return;
        };
        let groups = if blocks == Kinds::NONE {
            crate::scene_groups()
        } else {
            sieve_mask(blocks)
        };
        let colliders: Vec<ColliderHandle> = self
            .bodies
            .get(handle)
            .map(|body| body.colliders().to_vec())
            .unwrap_or_default();
        for collider in colliders {
            if let Some(collider) = self.colliders.get_mut(collider) {
                collider.set_collision_groups(groups);
            }
        }
    }

    pub fn scene_sieve(&self, id: SceneBodyId) -> Kinds {
        self.sift.sieves.get(&id).copied().unwrap_or(Kinds::NONE)
    }

    /// Whether this node's body currently collides with the scene, read
    /// off the live collider filter (the record of what the tangibility
    /// lever last set, per-node overrides included). Falls back to the
    /// scene-wide flag for a node with no body yet.
    pub(crate) fn node_is_tangible(&self, node: NodeKey) -> bool {
        self.bodies_by_node
            .get(&node)
            .and_then(|&handle| self.bodies.get(handle))
            .and_then(|body| body.colliders().first())
            .and_then(|&ch| self.colliders.get(ch))
            .map(|c| c.collision_groups().filter.contains(SCENE_GROUP))
            .unwrap_or(self.nodes_tangible)
    }

    /// Re-mask one node body's collider(s) from its stored kinds and the
    /// given tangibility. The one place node groups are computed, so the
    /// two axes cannot drift apart. (Physics scenes P2 + tactile T2.)
    pub(crate) fn remask_node(&mut self, node: NodeKey, tangible: bool) {
        let kinds = self.node_kinds(node);
        let Some(&handle) = self.bodies_by_node.get(&node) else {
            return;
        };
        let groups = node_mask(kinds, tangible);
        let colliders: Vec<ColliderHandle> = self
            .bodies
            .get(handle)
            .map(|body| body.colliders().to_vec())
            .unwrap_or_default();
        for collider in colliders {
            if let Some(collider) = self.colliders.get_mut(collider) {
                collider.set_collision_groups(groups);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pairwise_test_is_kinds_intersect_blocks() {
        let a = Kinds(0b01);
        let b = Kinds(0b10);
        let sieve = sieve_mask(Kinds(0b01));

        // A blocked node meets the sieve in both directions; a passed
        // node fails the sieve-side test. Tangibility plays no part.
        for tangible in [false, true] {
            let blocked = node_mask(a, tangible);
            let passed = node_mask(b, tangible);
            assert!(blocked.test(sieve), "blocked kind must meet the sieve");
            assert!(!passed.test(sieve), "other kinds must pass through");
        }
    }

    #[test]
    fn kinds_do_not_disturb_the_existing_axes() {
        // Node-node exclusion holds regardless of kinds, and ordinary
        // scene tangibility is exactly what it was: intangible nodes
        // pass ordinary scene bodies, tangible ones do not.
        let plain = node_mask(Kinds::NONE, false);
        let kinded = node_mask(Kinds(0xFFFF), false);
        assert!(plain.test(kinded));

        let scene = crate::scene_groups();
        assert!(!node_mask(Kinds(0b1), false).test(scene));
        assert!(node_mask(Kinds(0b1), true).test(scene));

        // And a kindless node passes every sieve: blocked-ness is
        // opt-in.
        assert!(!node_mask(Kinds::NONE, true).test(sieve_mask(Kinds(0xFFFF))));
    }
}
