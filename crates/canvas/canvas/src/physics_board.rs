// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A physics board: the catalog's laws over items that are not a graph.
//!
//! A remote projection arrives as a scene — items with positions the
//! endpoint's arrangement produced — and is drawn from that score. This is
//! the seam that lets the physics catalog move those items too: a seiche
//! simulation with one body per item, the score's positions as **anchor
//! slots** (the arrangement as attractor, [`seiche::AnchorSpring`] at the
//! board's pull), and the chosen law and overlays built through the same
//! attribute builders the canvas uses, graph-free. The host syncs the board
//! whenever the scene changes (a new item spawns at its slot and enters
//! with a settle burst; existing items keep their simulated positions and
//! only their slots move), ticks it each frame, and reads positions back
//! for drawing. The score itself is never written: the board is the
//! viewer's physics over the endpoint's truth. (Physics catalog — P3.)
//!
//! The simulation runs behind [`seiche::Physics`], the stack's inline/actor
//! backend, so a board ticks in the frame loop on wasm and can be
//! [`offload`](PhysicsBoard::offload)ed onto an actor thread on native — the
//! same choice the canvas makes. (2026-09-04.)

use std::collections::HashMap;

use euclid::default::Point2D;
use kernel::graph::NodeKey;
use seiche::{AnchorSpring, DEFAULT_ANCHOR_STIFFNESS, LayoutView, Physics, Simulation};

use crate::SETTLE_TICKS;
use crate::physics_catalog::{
    LawInputs, LawSources, PhysicsDepthSource, PhysicsKindSource, PhysicsLaw, PhysicsMassSource,
    PhysicsOverlay,
};

/// The board's anchor pull toward the score's slots. Gentle by design — a
/// twenty-fourth of the canvas's [`DEFAULT_ANCHOR_STIFFNESS`] — so the
/// score is a seed and a soft attractor and the chosen law is what the
/// viewer sees; the canvas's default would hold every card at its slot and
/// mask the law. A tunable.
pub const DEFAULT_BOARD_PULL: f32 = DEFAULT_ANCHOR_STIFFNESS / 24.0;

/// The physics choice a board runs: what the host's own canvas runs, mirrored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicsChoice {
    pub law: PhysicsLaw,
    pub overlays: Vec<PhysicsOverlay>,
    pub kind: PhysicsKindSource,
    pub mass: PhysicsMassSource,
    pub depth: PhysicsDepthSource,
}

impl Default for PhysicsChoice {
    fn default() -> Self {
        Self {
            law: PhysicsLaw::Springs,
            overlays: Vec::new(),
            kind: PhysicsKindSource::Site,
            mass: PhysicsMassSource::Degree,
            depth: PhysicsDepthSource::Roots,
        }
    }
}

/// One item the board holds: its stable id, its slot (the score position)
/// and its site (the grouping the Kinds law and the group overlay read).
#[derive(Clone, Debug, PartialEq)]
pub struct BoardItem {
    pub id: String,
    pub slot: (f32, f32),
    pub site: String,
}

/// The catalog's physics over a scene's items. See the module docs.
pub struct PhysicsBoard {
    physics: Physics,
    /// What the backend writes positions into, and `position` reads.
    view: LayoutView,
    keys: HashMap<String, NodeKey>,
    next_key: usize,
    items: Vec<BoardItem>,
    choice: PhysicsChoice,
    pull: f32,
}

impl Default for PhysicsBoard {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsBoard {
    pub fn new() -> Self {
        Self {
            physics: Physics::inline(Simulation::new(), 0),
            view: LayoutView::new(),
            keys: HashMap::new(),
            next_key: 0,
            items: Vec::new(),
            choice: PhysicsChoice::default(),
            pull: DEFAULT_BOARD_PULL,
        }
    }

    /// Move the board's simulation onto an actor thread (native hosts; a
    /// no-op once offloaded). `wake` pokes the host's event loop.
    pub fn offload(&mut self, wake: armillary::Wake) {
        self.physics.offload(wake);
    }

    /// The live choice.
    pub fn choice(&self) -> &PhysicsChoice {
        &self.choice
    }

    /// The anchor pull toward the score's slots.
    pub fn pull(&self) -> f32 {
        self.pull
    }

    /// Set the pull toward the slots: `0.0` makes the score an initial
    /// condition only, the default holds its shape against the law.
    pub fn set_pull(&mut self, pull: f32) {
        self.pull = pull.max(0.0);
        self.sync_anchors();
        self.settle_for_choice();
    }

    /// Switch the law, overlays and sources; the force set is replaced
    /// wholesale and a settle (or a continuous run) follows. A no-op when
    /// nothing changed, so a host may mirror its canvas every frame.
    pub fn set_choice(&mut self, choice: PhysicsChoice) {
        if choice == self.choice {
            return;
        }
        self.choice = choice;
        self.rebuild_forces();
        self.settle_for_choice();
    }

    /// Reconcile the bodies to `items`: departed items drop, new ones spawn
    /// at their slot, existing ones keep their simulated position; every
    /// slot is re-anchored. Returns how many items are new. New items earn a
    /// settle burst.
    pub fn sync(&mut self, items: Vec<BoardItem>) -> usize {
        let mut fresh = 0;
        for item in &items {
            if !self.keys.contains_key(&item.id) {
                let key = NodeKey::new(self.next_key);
                self.next_key += 1;
                self.keys.insert(item.id.clone(), key);
                fresh += 1;
            }
        }
        let live: HashMap<&str, NodeKey> = items
            .iter()
            .map(|item| (item.id.as_str(), self.keys[&item.id]))
            .collect();
        self.keys.retain(|id, _| live.contains_key(id.as_str()));
        // Every body, at its slot; `sync_nodes` leaves an existing body where
        // the simulation put it and spawns the new ones where told.
        self.physics.sync_nodes(
            items
                .iter()
                .map(|item| {
                    (
                        live[item.id.as_str()],
                        Point2D::new(item.slot.0, item.slot.1),
                    )
                })
                .collect(),
        );
        self.physics.sync_edges(Vec::new());
        self.items = items;
        self.sync_anchors();
        self.rebuild_forces();
        self.settle_for_choice();
        // So a host that syncs and draws in one frame sees the fresh item.
        self.physics.refresh(&mut self.view);
        fresh
    }

    /// Advance one frame, folding the freshest layout into the board's view.
    /// Returns whether the board is still moving. The backend owns the settle
    /// budget, so a law that never rests simply holds a budget that never runs
    /// out (see [`settle_for_choice`](Self::settle_for_choice)).
    pub fn tick(&mut self) -> bool {
        self.physics.advance_frame(&mut self.view)
    }

    /// Where the item is now, in the score's units.
    pub fn position(&self, id: &str) -> Option<(f32, f32)> {
        let key = self.keys.get(id)?;
        self.view.position_of(*key).map(|p| (p.x, p.y))
    }

    /// The bodies' kinetic energy: live inline, the last snapshot's figure
    /// offloaded.
    pub fn energy(&self) -> f32 {
        self.physics.kinetic_energy()
    }

    /// The distance between two items, in the score's units.
    pub fn gap(&self, a: &str, b: &str) -> Option<f32> {
        let (ax, ay) = self.position(a)?;
        let (bx, by) = self.position(b)?;
        Some(((ax - bx).powi(2) + (ay - by).powi(2)).sqrt())
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn sync_anchors(&mut self) {
        let anchors = (self.pull > 0.0 && !self.items.is_empty()).then(|| {
            AnchorSpring::new(
                self.items
                    .iter()
                    .map(|item| (self.keys[&item.id], item.slot)),
            )
            .with_stiffness(self.pull)
        });
        self.physics.set_anchor_force(anchors);
    }

    fn rebuild_forces(&mut self) {
        let nodes: Vec<NodeKey> = self.items.iter().map(|item| self.keys[&item.id]).collect();
        let sites: HashMap<NodeKey, String> = self
            .items
            .iter()
            .map(|item| (self.keys[&item.id], item.site.clone()))
            .collect();
        let inputs = LawInputs::from_parts(nodes, Vec::new(), sites);
        let sources = LawSources {
            kind: self.choice.kind,
            mass: self.choice.mass,
            depth: self.choice.depth,
            focus: None,
        };
        let forces = inputs.forces(self.choice.law, &self.choice.overlays, sources);
        self.physics.set_forces(forces);
    }

    /// Ask the backend for a settle: the normal burst, or — under a law or
    /// overlay that never rests — a budget that never runs out, which is how
    /// a perpetual law keeps its cards moving.
    fn settle_for_choice(&mut self) {
        let living =
            self.choice.law.never_rests() || self.choice.overlays.iter().any(|o| o.never_rests());
        self.physics
            .settle(if living { u32::MAX } else { SETTLE_TICKS });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, x: f32, y: f32) -> BoardItem {
        BoardItem {
            id: id.to_string(),
            slot: (x, y),
            site: "fixture".to_string(),
        }
    }

    /// Under Springs at the canvas's anchor stiffness, items sit at their
    /// slots; a later item spawns at its slot and the earlier ones are not
    /// re-seeded. Under the board's gentle default pull the boundary force
    /// bends the shape a little (a body 300 units out drifts about thirty
    /// toward the origin), and no further.
    #[test]
    fn items_hold_their_slots_and_a_new_one_joins_without_reseeding() {
        let mut gentle = PhysicsBoard::new();
        gentle.sync(vec![item("a", 0.0, 0.0), item("b", 300.0, 0.0)]);
        for _ in 0..SETTLE_TICKS {
            gentle.tick();
        }
        let b = gentle.position("b").unwrap();
        assert!(
            (b.0 - 300.0).abs() < 60.0 && b.0 > 200.0,
            "the gentle pull keeps the shape within a fifth: {b:?}"
        );

        let mut board = PhysicsBoard::new();
        board.set_pull(DEFAULT_ANCHOR_STIFFNESS);
        assert_eq!(
            board.sync(vec![item("a", 0.0, 0.0), item("b", 300.0, 0.0)]),
            2
        );
        for _ in 0..SETTLE_TICKS {
            board.tick();
        }
        let a = board.position("a").unwrap();
        let b = board.position("b").unwrap();
        assert!(
            a.0.abs() < 20.0 && (b.0 - 300.0).abs() < 20.0,
            "at the slots: {a:?} {b:?}"
        );
        // Nudge a off its slot by moving its slot, then add c: a keeps its
        // simulated position (not teleported to the new slot), c spawns at its own.
        assert_eq!(
            board.sync(vec![
                item("a", 60.0, 0.0),
                item("b", 300.0, 0.0),
                item("c", 150.0, 200.0)
            ]),
            1
        );
        let a_after = board.position("a").unwrap();
        assert!(
            (a_after.0 - a.0).abs() < 1.0,
            "a was not re-seeded: {a_after:?}"
        );
        let c = board.position("c").unwrap();
        assert!(
            (c.0 - 150.0).abs() < 1.0 && (c.1 - 200.0).abs() < 1.0,
            "c at its slot: {c:?}"
        );
        assert_eq!(board.len(), 3);
        // A departed item drops.
        board.sync(vec![item("b", 300.0, 0.0)]);
        assert!(board.position("a").is_none());
        assert_eq!(board.len(), 1);
    }

    /// Under Charge with a weak pull, two items whose slots overlap settle
    /// apart; under Orbit the board keeps moving.
    #[test]
    fn charge_separates_overlapping_slots_and_orbit_never_rests() {
        let mut board = PhysicsBoard::new();
        board.set_pull(0.5);
        board.sync(vec![item("a", 0.0, 0.0), item("b", 4.0, 2.0)]);
        board.set_choice(PhysicsChoice {
            law: PhysicsLaw::Charge,
            ..PhysicsChoice::default()
        });
        for _ in 0..SETTLE_TICKS {
            board.tick();
        }
        let gap = board.gap("a", "b").unwrap();
        assert!(gap > 60.0, "charge pushed the pair apart: {gap:.0}");
        assert!(!board.tick(), "charge comes to rest");

        board.set_choice(PhysicsChoice {
            law: PhysicsLaw::Orbit,
            ..PhysicsChoice::default()
        });
        for _ in 0..120 {
            assert!(board.tick(), "orbit keeps ticking");
        }
        assert!(
            board.energy() > 0.0,
            "orbit carries energy: {}",
            board.energy()
        );
    }
}
