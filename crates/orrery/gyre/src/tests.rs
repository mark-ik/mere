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

fn graph_with_n_nodes(n: usize) -> Graph {
    let mut g = Graph::new();
    for i in 0..n {
        g.add_node_with_id(
            uuid::Uuid::from_u128(1 + i as u128),
            format!("mere://n{i}"),
            Point2D::new(i as f32 * 10.0, 0.0),
        );
    }
    g
}

/// A mock `RepulsionSolver` that records its call count and pushes every node a
/// fixed amount in +x — a force the symmetric naive scan can never produce, so
/// its footprint proves the solver's output reached the bodies.
fn recording_push_solver(calls: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> RepulsionSolver {
    std::sync::Arc::new(move |xs: &[f32], _ys: &[f32], _s: f32, _m: f32| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        (vec![1_000_000.0; xs.len()], vec![0.0; xs.len()])
    })
}

/// Physics runs on abstract node keys with no graph at all. gyre's graph-free
/// surface (`sync_nodes` + `sync_edges` + `position_of` / `positions`) is the primary
/// interface, so any app — a raw chartulary graph, a bespoke store — drives the
/// simulation by feeding `(key, position)` pairs and reading positions back, with no
/// `kernel::graph::Graph` in sight. Two nodes started nearly on top of each other are
/// pushed apart by exclusion.
#[test]
fn physics_runs_on_abstract_nodes_without_a_graph() {
    // Node keys are minted directly (petgraph indices), not read from any graph.
    let a = NodeKey::new(0);
    let b = NodeKey::new(1);

    let mut sim = Simulation::new();
    sim.add_force(NodeExclusion::default());
    sim.sync_nodes([(a, Point2D::new(0.0, 0.0)), (b, Point2D::new(1.0, 0.0))]);

    let before = (sim.position_of(a).unwrap() - sim.position_of(b).unwrap()).length();
    for _ in 0..120 {
        sim.tick(1.0 / 60.0);
    }
    let after = (sim.position_of(a).unwrap() - sim.position_of(b).unwrap()).length();

    assert!(
        after > before,
        "exclusion pushed the two nodes apart with no graph ({after} > {before})"
    );
    assert_eq!(sim.positions().count(), 2, "positions read back, still no graph");
}

#[test]
fn repulsion_solver_routes_only_above_threshold() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Below the threshold: the solver is installed but never consulted (naive path).
    let mut sim = Simulation::new();
    sim.add_force(NodeExclusion::default());
    sim.sync_with_graph(&graph_with_n_nodes(3));
    let calls = Arc::new(AtomicUsize::new(0));
    sim.set_repulsion_solver(Some(recording_push_solver(calls.clone())), 10);
    for _ in 0..5 {
        sim.tick(1.0 / 60.0);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0, "below threshold stays naive");

    // At/above the threshold: the solver is consulted every tick, and its
    // distinctive +x push moves the whole layout's center of mass right —
    // something the symmetric naive repulsion could never do.
    let mut sim = Simulation::new();
    sim.add_force(NodeExclusion::default());
    let graph = graph_with_n_nodes(3);
    sim.sync_with_graph(&graph);
    let calls = Arc::new(AtomicUsize::new(0));
    sim.set_repulsion_solver(Some(recording_push_solver(calls.clone())), 3);
    for _ in 0..10 {
        sim.tick(1.0 / 60.0);
    }
    let keys: Vec<_> = graph.nodes().map(|(k, _)| k).collect();
    let mean_x: f32 = keys
        .iter()
        .filter_map(|&k| sim.position_of(k).map(|p| p.x))
        .sum::<f32>()
        / keys.len() as f32;
    assert!(calls.load(Ordering::SeqCst) >= 10, "solver consulted each tick");
    assert!(mean_x > 0.0, "the solver's +x push moved the layout right: {mean_x}");
}

/// End-to-end settle timing: naive CPU repulsion vs the aether wgpu solver,
/// at a large node count. Times whole ticks — rapier's step is identical both
/// ways, so the delta is the repulsion step moving off the CPU. Ignored + behind
/// `gpu-bench` (the only path that compiles burn into gyre's build). Run:
/// `cargo test -p gyre --features gpu-bench --release -- --ignored settle_timing --nocapture`
#[cfg(feature = "gpu-bench")]
#[test]
#[ignore]
fn settle_timing_naive_vs_gpu_solver() {
    use std::sync::Arc;

    const TICKS: usize = 20;
    let solver: RepulsionSolver = Arc::new(|xs: &[f32], ys: &[f32], strength: f32, min_d: f32| {
        aether::forces::repulsion_wgpu(
            xs,
            ys,
            aether::forces::RepulsionParams {
                strength,
                softening: min_d,
            },
        )
    });

    for n in [2_000usize, 4_000, 8_000, 16_000] {
        let graph = graph_with_n_nodes(n);
        let build = || {
            let mut sim = Simulation::new();
            sim.add_force(NodeExclusion::default());
            sim.add_force(EdgeSpring::default());
            sim.add_force(Boundary::default());
            sim.sync_with_graph(&graph);
            sim
        };

        let mut cpu = build();
        let t = std::time::Instant::now();
        for _ in 0..TICKS {
            cpu.tick(1.0 / 60.0);
        }
        let cpu_ms = t.elapsed().as_millis() as f64 / TICKS as f64;

        let mut gpu = build();
        gpu.set_repulsion_solver(Some(solver.clone()), 0);
        gpu.tick(1.0 / 60.0); // warm the wgpu kernel
        let t = std::time::Instant::now();
        for _ in 0..TICKS {
            gpu.tick(1.0 / 60.0);
        }
        let gpu_ms = t.elapsed().as_millis() as f64 / TICKS as f64;

        println!(
            "N={n}: naive-cpu={cpu_ms:.2}ms/tick gpu-solver={gpu_ms:.2}ms/tick ({:.2}x)",
            cpu_ms / gpu_ms
        );
    }
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

    let a = sim
        .hit_test(Point2D::new(0.0, 0.0))
        .expect("point inside node a");
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
    let near_origin = sim.cull_aabb(Box2D::new(
        Point2D::new(-10.0, -10.0),
        Point2D::new(10.0, 10.0),
    ));
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
    assert_eq!(
        NodeCollider::Ball { radius: 10.0 }
            .to_shared_shape()
            .shape_type(),
        ShapeType::Ball
    );
    assert_eq!(
        NodeCollider::Square { half: 10.0 }
            .to_shared_shape()
            .shape_type(),
        ShapeType::Cuboid
    );
    assert_eq!(
        NodeCollider::RoundedSquare {
            half: 10.0,
            border: 3.0
        }
        .to_shared_shape()
        .shape_type(),
        ShapeType::RoundCuboid,
    );
    let square_hull = NodeCollider::Hull {
        points: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
        fallback: 5.0,
    };
    assert_eq!(
        square_hull.to_shared_shape().shape_type(),
        ShapeType::ConvexPolygon
    );
    // A degenerate hull (a single point, no area) falls back to the ball.
    let degenerate = NodeCollider::Hull {
        points: vec![(0.0, 0.0)],
        fallback: 5.0,
    };
    assert_eq!(degenerate.to_shared_shape().shape_type(), ShapeType::Ball);
}
