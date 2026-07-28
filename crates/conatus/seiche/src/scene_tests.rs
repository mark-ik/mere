//! Integration tests for the scene / fluid / field / emitter tiers — the cross-module behaviour
//! that exercises [`Simulation`] as a whole (intangible vs tangible bodies, gravity-while-nodes-
//! float, fluid displacement, vortex swirl, bounded emitter counts). Split from `lib.rs` alongside
//! the tiers they cover.

use crate::NodeKey;
use euclid::default::Point2D;

use crate::{NODE_BODY_RADIUS, NodeCollider, Simulation};

#[test]
fn scene_body_is_intangible_to_nodes_and_ignored_by_forces() {
    let mut sim = Simulation::new();
    let node = NodeKey::new(0);
    sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
    // A scene body overlapping the node (centers 5px apart, radii sum 36) — a hard
    // collision would shove the node off the origin.
    let id = sim.add_scene_body(
        NodeCollider::Ball {
            radius: NODE_BODY_RADIUS,
        },
        Point2D::new(5.0, 0.0),
        (0.0, 0.0),
    );
    assert_eq!(sim.scene_body_count(), 1);
    assert_eq!(
        sim.body_count(),
        1,
        "the scene body is not counted as a node"
    );
    assert_eq!(sim.scene_bodies().count(), 1);

    // No forces registered, so the node only moves if the scene body collides with
    // it. Under the default groups it is intangible: tick and the node stays put.
    for _ in 0..60 {
        sim.tick(1.0 / 60.0);
    }
    let p = sim.position_of(node).expect("node has a position");
    assert!(
        p.x.abs() < 1.0 && p.y.abs() < 1.0,
        "an intangible scene body must not push the node off the origin (was {p:?})",
    );

    // Clearing the scene leaves the node untouched; a stale remove is a no-op.
    sim.clear_scene();
    assert_eq!(sim.scene_body_count(), 0);
    assert_eq!(sim.body_count(), 1);
    sim.remove_scene_body(id);
}

#[test]
fn node_tangibility_toggles_scene_collision() {
    let mut sim = Simulation::new();
    let node = NodeKey::new(0);
    sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
    sim.add_scene_body(
        NodeCollider::Ball {
            radius: NODE_BODY_RADIUS,
        },
        Point2D::new(5.0, 0.0),
        (0.0, 0.0),
    );

    // Intangible (default): the overlapping scene body never pushes the node.
    for _ in 0..60 {
        sim.tick(1.0 / 60.0);
    }
    let intangible = sim.position_of(node).expect("node has a position");
    assert!(
        intangible.x.abs() < 1.0,
        "intangible: node not pushed (was {intangible:?})"
    );

    // Flip the node tangible: now the overlapping scene body's hard collision shoves it
    // off the origin (away from the body at +x, so toward -x).
    sim.set_nodes_tangible(true);
    for _ in 0..60 {
        sim.tick(1.0 / 60.0);
    }
    let tangible = sim.position_of(node).expect("node has a position");
    assert!(
        tangible.x < -1.0,
        "tangible: the scene body pushed the node off the origin (was {tangible:?})",
    );
}

#[test]
fn load_scene_falls_under_gravity_while_nodes_float() {
    let mut sim = Simulation::new();
    let node = NodeKey::new(0);
    sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
    sim.load_scene(&crate::drop_bowl_scene());
    assert_eq!(sim.scene_body_count(), 13, "5 floor + 8 falling balls");

    let min_before = sim
        .scene_bodies()
        .map(|b| b.position.y)
        .fold(f32::INFINITY, f32::min);
    for _ in 0..60 {
        sim.tick(1.0 / 60.0);
    }
    let min_after = sim
        .scene_bodies()
        .map(|b| b.position.y)
        .fold(f32::INFINITY, f32::min);
    assert!(
        min_after > min_before + 5.0,
        "gravity pulled the falling balls down"
    );

    // The node carries gravity_scale(0), so scene gravity never drags the graph down.
    let np = sim.position_of(node).expect("node has a position");
    assert!(
        np.y.abs() < 5.0,
        "nodes float under scene gravity (was {np:?})"
    );
}

#[test]
fn tangible_node_displaces_the_fluid_pool() {
    use crate::{Basin, FluidParams};
    use euclid::default::Vector2D;
    // A node at the origin with a fluid block sitting on it (gravity off, so it stays put).
    let mut sim = Simulation::new();
    let node = NodeKey::new(0);
    sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
    let params = FluidParams {
        gravity: Vector2D::zero(),
        ..FluidParams::default()
    };
    let basin = Basin {
        min_x: -200.0,
        max_x: 200.0,
        floor_y: 300.0,
    };
    sim.load_fluid(params, basin, Point2D::new(-30.0, -30.0), 5, 5, 14.0);

    // Intangible (default): the node does not push the fluid — a particle can sit near the origin.
    for _ in 0..8 {
        sim.tick(1.0 / 60.0);
    }
    let nearest_intangible = sim
        .fluid_particles()
        .map(|p| p.to_vector().length())
        .fold(f32::INFINITY, f32::min);

    // Tangible: the node now displaces the pool — the nearest particle is pushed out to ~its radius.
    sim.set_nodes_tangible(true);
    for _ in 0..8 {
        sim.tick(1.0 / 60.0);
    }
    let nearest_tangible = sim
        .fluid_particles()
        .map(|p| p.to_vector().length())
        .fold(f32::INFINITY, f32::min);

    assert!(
        nearest_tangible > nearest_intangible,
        "tangible node pushed the fluid out ({nearest_intangible} -> {nearest_tangible})"
    );
    assert!(
        nearest_tangible >= NODE_BODY_RADIUS - 2.0,
        "nearest particle pushed out to ~the node radius (was {nearest_tangible})"
    );
}

#[test]
fn vortex_field_swirls_scene_bodies() {
    use crate::SceneField;
    // A loose scene ball on the +x axis; a counter-clockwise vortex at the origin.
    let mut sim = Simulation::new();
    sim.add_scene_body(
        NodeCollider::Ball { radius: 14.0 },
        Point2D::new(100.0, 0.0),
        (0.0, 0.0),
    );
    sim.set_scene_field(Some(SceneField::Vortex {
        center: (0.0, 0.0),
        strength: 90.0,
        inward: 30.0,
    }));
    assert!(
        sim.wants_continuous_tick(),
        "a scene field keeps the actor ticking"
    );

    for _ in 0..60 {
        sim.tick(1.0 / 60.0);
    }
    // A CCW vortex pushes a body on the +x axis toward +y.
    let p = sim
        .scene_bodies()
        .next()
        .map(|b| b.position)
        .expect("the scene body");
    assert!(
        p.y > 5.0,
        "CCW vortex swirled the body toward +y (was at +x): {p:?}"
    );

    // Clearing the scene drops the field too.
    sim.clear_scene();
    assert!(
        !sim.wants_continuous_tick(),
        "clearing the scene clears the field"
    );
}

#[test]
fn emitter_spawns_then_reaps_to_a_bounded_count() {
    use crate::SceneEmitter;
    let mut sim = Simulation::new();
    sim.add_emitter(SceneEmitter {
        collider: NodeCollider::Ball { radius: 6.0 },
        position: (0.0, 0.0),
        position_jitter: (4.0, 4.0),
        velocity: (0.0, -100.0),
        velocity_jitter: (20.0, 20.0),
        rate_per_sec: 30.0,
        lifetime_secs: 0.5,
        max_alive: 40,
    });
    assert_eq!(sim.emitter_count(), 1);
    assert!(
        sim.wants_continuous_tick(),
        "an emitter keeps the actor ticking"
    );

    // After more than one lifetime, spawning (~30/s) and reaping (age > 0.5s) balance near
    // rate*lifetime (~15) — a bounded steady count, not unbounded growth.
    for _ in 0..90 {
        sim.tick(1.0 / 60.0);
    }
    let steady = sim.scene_body_count();
    assert!(
        (6..=40).contains(&steady),
        "emitter holds a bounded steady count (was {steady})"
    );

    // Clearing the emitters removes the bodies it spawned.
    sim.clear_emitters();
    assert_eq!(
        sim.scene_body_count(),
        0,
        "clearing emitters removes its bodies"
    );
    assert_eq!(sim.emitter_count(), 0);
}

#[test]
fn scene_prop_carries_its_sprite_handle_through_to_the_view() {
    use crate::{SceneBodySpec, SceneSpec};
    // One prop wears a sprite, the other does not; the handle must ride through to the paint view.
    let spec = SceneSpec {
        bodies: vec![
            SceneBodySpec::dynamic(NodeCollider::Square { half: 20.0 }, (0.0, 0.0)).sprite("crate"),
            SceneBodySpec::dynamic(NodeCollider::Ball { radius: 14.0 }, (60.0, 0.0)),
        ],
        gravity: (0.0, 0.0),
        default_tangible: false,
        perpetual: false,
        joints: Vec::new(),
    };
    let mut sim = Simulation::new();
    sim.load_scene(&spec);
    let sprites: Vec<Option<String>> = sim.scene_bodies().map(|b| b.sprite).collect();
    // Order is unspecified (a HashMap), so check the multiset: exactly one "crate", one bare.
    assert_eq!(
        sprites
            .iter()
            .filter(|s| s.as_deref() == Some("crate"))
            .count(),
        1,
        "one prop wears the sprite"
    );
    assert_eq!(
        sprites.iter().filter(|s| s.is_none()).count(),
        1,
        "the other carries none"
    );

    // Clearing drops the sprite mapping (no leak into the next scene).
    sim.clear_scene();
    assert_eq!(sim.scene_bodies().count(), 0);
    assert!(
        sim.scene_bodies().all(|b| b.sprite.is_none()),
        "no stale sprites after clear"
    );
}
