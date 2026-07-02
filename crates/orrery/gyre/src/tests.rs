/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Unit tests for the simulation + built-in forces. Split out of `lib.rs` to
//! keep both files under the workspace's per-file size ceiling.

use super::*;
use kernel::graph::fixtures::GraphFixtures;

fn graph_with_two_nodes() -> Graph {
    let mut g = Graph::new();
    g.add_node_with_id(
        uuid::Uuid::from_u128(1),
        "mere://a".to_string(),
        Point2D::new(0.0, 0.0),
    );
    g.add_node_with_id(
        uuid::Uuid::from_u128(2),
        "mere://b".to_string(),
        Point2D::new(100.0, 0.0),
    );
    g
}

#[test]
fn sync_with_graph_creates_bodies_for_new_nodes() {
    let mut sim = Simulation::new();
    let graph = graph_with_two_nodes();
    sim.sync_with_graph(&graph);
    assert_eq!(sim.body_count(), 2);
    for (key, _) in graph.nodes() {
        assert!(sim.body_for(key).is_some());
    }
}

#[test]
fn sync_is_idempotent() {
    let mut sim = Simulation::new();
    let graph = graph_with_two_nodes();
    sim.sync_with_graph(&graph);
    sim.sync_with_graph(&graph);
    sim.sync_with_graph(&graph);
    assert_eq!(sim.body_count(), 2);
}

#[test]
fn empty_simulation_settles_to_rest() {
    let mut sim = Simulation::new();
    let graph = graph_with_two_nodes();
    sim.sync_with_graph(&graph);
    for _ in 0..120 {
        sim.tick(1.0 / 60.0);
    }
    assert!(sim.is_at_rest(0.01));
}

#[test]
fn write_positions_to_returns_zero_when_nothing_moves() {
    let mut sim = Simulation::new();
    let mut graph = graph_with_two_nodes();
    sim.sync_with_graph(&graph);
    // No forces, no time elapsed — bodies are at the same
    // position as the graph reports.
    let changed = sim.write_positions_to(&mut graph);
    assert_eq!(changed, 0);
}

#[test]
fn hit_test_and_cull_resolve_nodes_by_position() {
    // a@(0,0), b@(100,0), each an 18px-radius ball collider.
    let mut sim = Simulation::new();
    let graph = graph_with_two_nodes();
    sim.sync_with_graph(&graph);
    // No tick: refresh the index so queries see the synced positions.
    sim.refresh_spatial_index();

    let a = sim.hit_test(Point2D::new(0.0, 0.0)).expect("point inside node a");
    let b = sim
        .hit_test(Point2D::new(100.0, 0.0))
        .expect("point inside node b");
    assert_ne!(a, b, "the two centers resolve to different nodes");
    assert!(
        sim.hit_test(Point2D::new(5000.0, 5000.0)).is_none(),
        "empty space hits nothing"
    );

    // A small box around the origin catches a (radius 18 reaches ±18) but
    // not b (its AABB starts at x=82).
    let near_origin =
        sim.cull_aabb(Box2D::new(Point2D::new(-10.0, -10.0), Point2D::new(10.0, 10.0)));
    assert_eq!(near_origin, vec![a]);

    // A wide box catches both.
    let everything = sim.cull_aabb(Box2D::new(
        Point2D::new(-1000.0, -1000.0),
        Point2D::new(1000.0, 1000.0),
    ));
    assert_eq!(everything.len(), 2);
    assert!(everything.contains(&a) && everything.contains(&b));
}

fn node_at(g: &mut Graph, id: u128, x: f32, y: f32) -> NodeKey {
    g.add_node_with_id(
        uuid::Uuid::from_u128(id),
        format!("mere://{id}"),
        Point2D::new(x, y),
    )
}

fn separation(sim: &Simulation, a: NodeKey, b: NodeKey) -> f32 {
    (sim.position_of(a).unwrap() - sim.position_of(b).unwrap()).length()
}

fn radius_from_origin(sim: &Simulation, a: NodeKey) -> f32 {
    sim.position_of(a).unwrap().to_vector().length()
}

#[test]
fn node_exclusion_pushes_apart() {
    let mut sim = Simulation::new();
    let mut g = Graph::new();
    // 50px apart: clear of the 36px contact range, inside the 600px cutoff,
    // so only the repulsion force acts.
    let a = node_at(&mut g, 1, 0.0, 0.0);
    let b = node_at(&mut g, 2, 50.0, 0.0);
    sim.sync_with_graph(&g);
    sim.add_force(NodeExclusion::default());
    let before = separation(&sim, a, b);
    for _ in 0..60 {
        sim.tick(1.0 / 60.0);
    }
    assert!(
        separation(&sim, a, b) > before,
        "repulsion should increase separation"
    );
}

#[test]
fn edge_spring_pulls_together() {
    let mut sim = Simulation::new();
    let mut g = Graph::new();
    // 400px apart, well past the 140px rest length; no repulsion force.
    let a = node_at(&mut g, 1, 0.0, 0.0);
    let b = node_at(&mut g, 2, 400.0, 0.0);
    sim.sync_with_graph(&g);
    sim.sync_edges([(a, b)]);
    assert_eq!(sim.edge_count(), 1);
    sim.add_force(EdgeSpring::default());
    let before = separation(&sim, a, b);
    for _ in 0..60 {
        sim.tick(1.0 / 60.0);
    }
    assert!(
        separation(&sim, a, b) < before,
        "a stretched spring should contract"
    );
}

#[test]
fn boundary_centers_toward_origin() {
    let mut sim = Simulation::new();
    let mut g = Graph::new();
    let a = node_at(&mut g, 1, 500.0, 0.0);
    sim.sync_with_graph(&g);
    sim.add_force(Boundary::default());
    let before = radius_from_origin(&sim, a);
    for _ in 0..60 {
        sim.tick(1.0 / 60.0);
    }
    assert!(
        radius_from_origin(&sim, a) < before,
        "centering should pull the node toward the origin"
    );
}

#[test]
fn full_layout_settles_separated_and_bounded() {
    let mut sim = Simulation::new();
    let mut g = Graph::new();
    // A connected triangle from distinct, near-coincident starts.
    let a = node_at(&mut g, 1, 10.0, 0.0);
    let b = node_at(&mut g, 2, -5.0, 8.0);
    let c = node_at(&mut g, 3, -5.0, -8.0);
    sim.sync_with_graph(&g);
    sim.sync_edges([(a, b), (b, c), (c, a)]);
    sim.add_force(NodeExclusion::default());
    sim.add_force(EdgeSpring::default());
    sim.add_force(Boundary::default());
    for _ in 0..4000 {
        sim.tick(1.0 / 60.0);
    }
    assert!(
        sim.is_at_rest(0.5),
        "the layout should settle to rest under damping"
    );
    for (x, y) in [(a, b), (b, c), (c, a)] {
        let d = separation(&sim, x, y);
        assert!(d.is_finite(), "non-finite separation");
        assert!(d >= 2.0 * NODE_BODY_RADIUS, "nodes overlap: {d}");
    }
    for k in [a, b, c] {
        assert!(
            radius_from_origin(&sim, k) < 5_000.0,
            "centering failed; node escaped"
        );
    }
}

/// Hit-test + cull stay correct at orrery scale: a ~1k-node grid, pinpointing one
/// node among ~1000 and a selective window cull. Position-based (over each body's
/// live translation + radius) since rapier's `QueryPipeline` went ephemeral in
/// 0.33 — this confirms the O(n) scan holds up at scale.
#[test]
fn node_queries_handle_orrery_scale() {
    let mut sim = Simulation::new();
    let mut g = Graph::new();
    // 32x32 grid, 60px apart (clear of the 36px contact range): ~1024 nodes.
    let n: u128 = 32;
    let spacing = 60.0_f32;
    let mut keys = Vec::new();
    for i in 0..n {
        for j in 0..n {
            keys.push(node_at(
                &mut g,
                i * n + j + 1,
                i as f32 * spacing,
                j as f32 * spacing,
            ));
        }
    }
    sim.sync_with_graph(&g);
    sim.refresh_spatial_index();
    assert_eq!(sim.body_count(), (n * n) as usize);

    // Hit-test pinpoints one node among ~1000 (balls don't overlap at 60px).
    let target = keys[500];
    let at = sim.position_of(target).unwrap();
    assert_eq!(sim.hit_test(at), Some(target));
    assert!(sim.hit_test(Point2D::new(-500.0, -500.0)).is_none());

    // Cull is selective: a ~200px window near the origin returns a handful of
    // the grid's corner nodes, not the whole graph.
    let window = sim.cull_aabb(Box2D::new(
        Point2D::new(-10.0, -10.0),
        Point2D::new(190.0, 190.0),
    ));
    assert!(
        (9..64).contains(&window.len()),
        "cull window not selective at scale: {}",
        window.len()
    );
}

#[test]
fn seed_positions_overrides_placement_and_round_trips() {
    // The cartography bridge: a strategy's projected positions seed the layout
    // in place of the graph's stored positions.
    let mut sim = Simulation::new();
    let graph = graph_with_two_nodes(); // a@(0,0), b@(100,0)
    sim.sync_with_graph(&graph);
    let keys: Vec<NodeKey> = graph.nodes().map(|(k, _)| k).collect();

    sim.seed_positions([
        (keys[0], Point2D::new(300.0, 300.0)),
        (keys[1], Point2D::new(-300.0, -300.0)),
    ]);
    assert_eq!(sim.position_of(keys[0]), Some(Point2D::new(300.0, 300.0)));
    assert_eq!(sim.position_of(keys[1]), Some(Point2D::new(-300.0, -300.0)));

    // positions() reads the live layout back (the caller would rebuild a
    // Projection from these).
    let read: Vec<(NodeKey, Point2D<f32>)> = sim.positions().collect();
    assert!(read.contains(&(keys[0], Point2D::new(300.0, 300.0))));
    assert!(read.contains(&(keys[1], Point2D::new(-300.0, -300.0))));
}

#[test]
fn seeded_overlap_separates_under_physics() {
    // A degenerate strategy output (two nodes nearly coincident) is refined by
    // the physics the pure strategy lacks: collision + repulsion separate them.
    let mut sim = Simulation::new();
    let mut g = Graph::new();
    let a = node_at(&mut g, 1, 0.0, 0.0);
    let b = node_at(&mut g, 2, 0.0, 0.0);
    sim.sync_with_graph(&g);
    sim.seed_positions([(a, Point2D::new(0.0, 0.0)), (b, Point2D::new(1.0, 0.0))]);
    sim.add_force(NodeExclusion::default());
    for _ in 0..240 {
        sim.tick(1.0 / 60.0);
    }
    assert!(
        separation(&sim, a, b) >= 2.0 * NODE_BODY_RADIUS,
        "physics should separate a seeded overlap"
    );
}

#[test]
fn node_exclusion_scales_to_many_nodes() {
    // A 10x10 grid (no initial overlap, so the contact solver stays cheap)
    // spread under cull-based repulsion + centering. Exercises the spatial-index
    // neighbor path at scale; asserts a finite, bounded layout that spread out.
    let mut sim = Simulation::new();
    let mut g = Graph::new();
    let mut id: u128 = 0;
    for i in 0..10 {
        for j in 0..10 {
            id += 1;
            node_at(&mut g, id, (i as f32 - 4.5) * 50.0, (j as f32 - 4.5) * 50.0);
        }
    }
    sim.sync_with_graph(&g);
    sim.add_force(NodeExclusion::default());
    sim.add_force(Boundary::default());
    let keys: Vec<NodeKey> = g.nodes().map(|(k, _)| k).collect();
    assert_eq!(keys.len(), 100);

    for _ in 0..60 {
        sim.tick(1.0 / 60.0);
    }

    let mut max_radius = 0.0_f32;
    for &k in &keys {
        let p = sim.position_of(k).unwrap();
        assert!(p.x.is_finite() && p.y.is_finite(), "non-finite position");
        let r = radius_from_origin(&sim, k);
        assert!(r < 50_000.0, "node escaped: {r}");
        max_radius = max_radius.max(r);
    }
    // Repulsion pushed the grid outward (it did not collapse to a point).
    assert!(max_radius > 50.0, "layout failed to spread: {max_radius}");
}

#[test]
fn node_collider_lowers_to_the_matching_parry_shape() {
    // The collider geometry follows the node's visible face: a ball for a circle, a cuboid
    // for a square, a round cuboid for a rounded square, and a convex polygon for a custom
    // hull (with a ball fallback when the hull is degenerate). (Node-rep — collider shape.)
    assert_eq!(NodeCollider::Ball { radius: 10.0 }.to_shared_shape().shape_type(), ShapeType::Ball);
    assert_eq!(NodeCollider::Square { half: 10.0 }.to_shared_shape().shape_type(), ShapeType::Cuboid);
    assert_eq!(
        NodeCollider::RoundedSquare { half: 10.0, border: 3.0 }.to_shared_shape().shape_type(),
        ShapeType::RoundCuboid,
    );
    let square_hull =
        NodeCollider::Hull { points: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], fallback: 5.0 };
    assert_eq!(square_hull.to_shared_shape().shape_type(), ShapeType::ConvexPolygon);
    // A degenerate hull (a single point, no area) falls back to the ball.
    let degenerate = NodeCollider::Hull { points: vec![(0.0, 0.0)], fallback: 5.0 };
    assert_eq!(degenerate.to_shared_shape().shape_type(), ShapeType::Ball);
}
