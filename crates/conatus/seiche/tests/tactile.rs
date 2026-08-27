// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Tactile tier receipts (T1-T3 of the tactile tier plan, 2026-08-14):
//! materials as data, the sieve, and support proposals.
//!
//! The doctrine under test: every physical parameter is a data binding
//! (weight is opt-in per node, a sieve is a collision predicate felt as
//! a wall), and physics proposes while the record disposes (supports
//! are read-only, like containments). Each receipt moves something a
//! workflow can read, not just something that looks physical.

use euclid::default::Point2D;
use seiche::{
    Kinds, NodeCollider, NodeKey, NodeMaterial, SceneBodySpec, SceneSpec, Simulation, Support,
};

const DT: f32 = 1.0 / 60.0;

fn node(i: u32) -> NodeKey {
    NodeKey::new(i as usize)
}

/// A world with `+y` down gravity and the listed nodes at their spawns.
/// No layout forces are registered: only gravity and contact act, so
/// the receipts read cause and effect without a tug-of-war.
fn falling_world(nodes: &[(NodeKey, Point2D<f32>)]) -> Simulation {
    let mut sim = Simulation::new();
    sim.sync_nodes(nodes.iter().copied());
    sim.set_gravity((0.0, 500.0));
    sim
}

/// A single fixed floor as a scene: a wide flat square below the spawn
/// line. `restitution` is the floor's own bounciness.
fn floor_scene(restitution: f32) -> SceneSpec {
    SceneSpec {
        bodies: vec![
            SceneBodySpec::fixed(NodeCollider::Square { half: 200.0 }, (0.0, 400.0))
                .restitution(restitution),
        ],
        gravity: (0.0, 500.0),
        default_tangible: false,
        perpetual: false,
        joints: vec![],
    }
}

// T1 receipt: a material read back off the live body matches what was
// set — collider restitution / friction / density and body gravity
// scale, from rapier state, not from the remembered override.
#[test]
fn the_live_body_carries_the_material_that_was_set() {
    let mut sim = falling_world(&[(node(0), Point2D::new(0.0, 0.0))]);
    let material = NodeMaterial {
        restitution: 0.4,
        friction: 0.2,
        density: 0.005,
        gravity_scale: 0.7,
    };
    sim.set_node_materials(vec![(node(0), material)]);
    assert_eq!(sim.node_material(node(0)), Some(material));

    // And the memory survives a re-sync: drop the body, bring it back,
    // the tuned material comes back with it rather than the spawn
    // defaults.
    sim.sync_nodes(std::iter::empty());
    assert_eq!(sim.node_material(node(0)), None);
    sim.sync_nodes([(node(0), Point2D::new(0.0, 0.0))]);
    assert_eq!(sim.node_material(node(0)), Some(material));
}

// T1 receipt: weight is opt-in per node — a weighted node falls under
// world gravity while a default node does not move.
#[test]
fn a_weighted_node_falls_while_a_default_node_hangs() {
    let heavy = node(0);
    let weightless = node(1);
    let mut sim = falling_world(&[
        (heavy, Point2D::new(-100.0, 0.0)),
        (weightless, Point2D::new(100.0, 0.0)),
    ]);
    sim.set_node_materials(vec![(
        heavy,
        NodeMaterial {
            gravity_scale: 1.0,
            ..NodeMaterial::default()
        },
    )]);

    for _ in 0..120 {
        sim.tick(DT);
    }

    let heavy_y = sim.position_of(heavy).unwrap().y;
    let weightless_y = sim.position_of(weightless).unwrap().y;
    assert!(heavy_y > 100.0, "weighted node should fall, at y={heavy_y}");
    assert!(
        weightless_y.abs() < 1.0,
        "default node must not move, at y={weightless_y}"
    );
}

// T1 receipt: restitution changes what a fall does — a bouncy node
// rebounds off the floor, a dead one lands and stays.
#[test]
fn restitution_decides_whether_a_fall_bounces() {
    let bouncy = node(0);
    let dead = node(1);
    let mut sim = falling_world(&[
        (bouncy, Point2D::new(-80.0, 0.0)),
        (dead, Point2D::new(80.0, 0.0)),
    ]);
    sim.load_scene(&floor_scene(0.0));
    sim.set_nodes_tangible(true);
    // Low damping so the rebound is the material's doing, not the
    // damping's undoing.
    sim.set_linear_damping(0.1);
    sim.set_node_materials(vec![
        (
            bouncy,
            NodeMaterial {
                restitution: 0.9,
                gravity_scale: 1.0,
                ..NodeMaterial::default()
            },
        ),
        (
            dead,
            NodeMaterial {
                restitution: 0.0,
                gravity_scale: 1.0,
                ..NodeMaterial::default()
            },
        ),
    ]);

    // Track each node's deepest descent and how far it climbed back
    // above it afterward: the rebound.
    let mut deepest = [f32::MIN, f32::MIN];
    let mut rebound = [0.0f32, 0.0f32];
    for _ in 0..600 {
        sim.tick(DT);
        for (i, key) in [bouncy, dead].into_iter().enumerate() {
            let y = sim.position_of(key).unwrap().y;
            if y > deepest[i] {
                deepest[i] = y;
            }
            rebound[i] = rebound[i].max(deepest[i] - y);
        }
    }

    assert!(
        rebound[0] > 10.0,
        "bouncy node should rebound, got {}px",
        rebound[0]
    );
    assert!(
        rebound[1] < 2.0,
        "dead node should land and stay, got {}px rebound",
        rebound[1]
    );
    assert!(rebound[0] > rebound[1] * 5.0);
}

// T2 + T3 receipts, one run: over the same floor declared as a sieve,
// the node of a blocked kind lands and rests while the node of another
// kind falls straight through — and the support proposal names the
// floor for the blocked node, nothing for the passed one.
#[test]
fn a_sieve_blocks_its_kinds_and_supports_name_the_holder() {
    const PAPER: Kinds = Kinds(0b01);
    const STONE: Kinds = Kinds(0b10);

    let paper = node(0);
    let stone = node(1);
    let mut sim = falling_world(&[
        (paper, Point2D::new(-80.0, 0.0)),
        (stone, Point2D::new(80.0, 0.0)),
    ]);
    // The floor loads *intangible* on purpose: a sieve blocks by kind,
    // independent of the scene-tangibility lever.
    sim.load_scene(&floor_scene(0.0));
    let floor = sim
        .scene_bodies()
        .next()
        .expect("the scene has its floor")
        .id;
    sim.set_scene_sieve(floor, PAPER);
    assert_eq!(sim.scene_sieve(floor), PAPER);

    sim.set_node_kinds(paper, PAPER);
    sim.set_node_kinds(stone, STONE);
    let weighted = NodeMaterial {
        gravity_scale: 1.0,
        ..NodeMaterial::default()
    };
    sim.set_node_materials(vec![(paper, weighted), (stone, weighted)]);

    for _ in 0..600 {
        sim.tick(DT);
    }

    // Floor top edge is at y=200 (centre 400, half 200); a resting ball
    // sits a node radius above it. The blocked kind rests there; the
    // other kind is well past it and still going.
    let paper_y = sim.position_of(paper).unwrap().y;
    let stone_y = sim.position_of(stone).unwrap().y;
    assert!(
        (150.0..200.0).contains(&paper_y),
        "blocked kind should rest on the sieve, at y={paper_y}"
    );
    assert!(
        stone_y > 400.0,
        "other kind should fall straight through, at y={stone_y}"
    );

    // T3: the contact graph proposes the floor as the resting node's
    // support; the falling node is held by nothing.
    assert_eq!(sim.supports_of(paper), vec![Support::Scene(floor)]);
    assert_eq!(sim.supports_of(stone), Vec::<Support>::new());
}
