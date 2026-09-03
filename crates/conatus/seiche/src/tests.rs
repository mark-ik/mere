// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Unit tests for the simulation + built-in forces. Split out of `lib.rs` to
//! keep both files under the workspace's per-file size ceiling.
//!
//! seiche is kernel-free: these tests drive the physics through the
//! `(NodeKey, position)` API ([`Simulation::sync_nodes`] / `sync_edges`), never a
//! mere `Graph`. [`Nodes`] is a tiny builder that mints sequential [`NodeKey`]s
//! and remembers each body's start position, replacing the old graph fixtures.

use super::*;

/// A kernel-free stand-in for the graph fixtures: mints sequential node keys and
/// records each body's start position, so a test drives [`Simulation::sync_nodes`]
/// directly.
#[derive(Default)]
struct Nodes {
    list: Vec<(NodeKey, Point2D<f32>)>,
}

impl Nodes {
    /// Add a node at `(x, y)`, returning its minted key.
    fn at(&mut self, x: f32, y: f32) -> NodeKey {
        let key = NodeKey::new(self.list.len());
        self.list.push((key, Point2D::new(x, y)));
        key
    }

    /// The keys, in insertion order.
    fn keys(&self) -> Vec<NodeKey> {
        self.list.iter().map(|(k, _)| *k).collect()
    }

    /// Reconcile a simulation's bodies to these nodes.
    fn sync(&self, sim: &mut Simulation) {
        sim.sync_nodes(self.list.iter().copied());
    }
}

fn two_nodes() -> Nodes {
    let mut n = Nodes::default();
    n.at(0.0, 0.0);
    n.at(100.0, 0.0);
    n
}

fn n_nodes(count: usize) -> Nodes {
    let mut n = Nodes::default();
    for i in 0..count {
        n.at(i as f32 * 10.0, 0.0);
    }
    n
}

fn separation(sim: &Simulation, a: NodeKey, b: NodeKey) -> f32 {
    (sim.position_of(a).unwrap() - sim.position_of(b).unwrap()).length()
}

fn radius_from_origin(sim: &Simulation, a: NodeKey) -> f32 {
    sim.position_of(a).unwrap().to_vector().length()
}

/// A mock `RepulsionSolver` that records its call count and pushes every node a
/// fixed amount in +x — a force the symmetric naive scan can never produce, so
/// its footprint proves the solver's output reached the bodies.
fn recording_push_solver(calls: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> RepulsionSolver {
    std::sync::Arc::new(move |xs: &[f32], _ys: &[f32], _request: RepulsionRequest| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        RepulsionForces::new(xs.len(), vec![1_000_000.0; xs.len()], vec![0.0; xs.len()])
    })
}

#[test]
fn repulsion_solver_routes_only_above_threshold() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Below the threshold: the solver is installed but never consulted (naive path).
    let mut sim = Simulation::new();
    sim.add_force(NodeExclusion::default());
    n_nodes(3).sync(&mut sim);
    let calls = Arc::new(AtomicUsize::new(0));
    sim.set_repulsion_solver(Some(recording_push_solver(calls.clone())), 10);
    for _ in 0..5 {
        sim.tick(1.0 / 60.0);
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "below threshold stays naive"
    );

    // At/above the threshold: the solver is consulted every tick, and its
    // distinctive +x push moves the whole layout's center of mass right —
    // something the symmetric naive repulsion could never do.
    let mut sim = Simulation::new();
    sim.add_force(NodeExclusion::default());
    let nodes = n_nodes(3);
    nodes.sync(&mut sim);
    let calls = Arc::new(AtomicUsize::new(0));
    sim.set_repulsion_solver(Some(recording_push_solver(calls.clone())), 3);
    for _ in 0..10 {
        sim.tick(1.0 / 60.0);
    }
    let keys = nodes.keys();
    let mean_x: f32 = keys
        .iter()
        .filter_map(|&k| sim.position_of(k).map(|p| p.x))
        .sum::<f32>()
        / keys.len() as f32;
    assert!(
        calls.load(Ordering::SeqCst) >= 10,
        "solver consulted each tick"
    );
    assert!(
        mean_x > 0.0,
        "the solver's +x push moved the layout right: {mean_x}"
    );
}

#[test]
fn repulsion_solver_output_is_checked_before_rapier_sees_it() {
    assert!(matches!(
        RepulsionForces::new(2, vec![1.0], vec![0.0, 0.0]),
        Err(RepulsionSolverError::OutputLength {
            expected: 2,
            x: 1,
            y: 2,
        })
    ));
    assert!(matches!(
        RepulsionForces::new(1, vec![f32::NAN], vec![0.0]),
        Err(RepulsionSolverError::NonFinite {
            component: RepulsionComponent::X,
            index: 0,
        })
    ));
}

/// End-to-end staging timing: CPU `NodeExclusion` vs the identical Burn WGPU
/// round trip. This is deliberately not a resident-GPU benchmark: full position
/// uploads and force readbacks remain in the timed work. Ignored + behind
/// `gpu-bench` (the only path that compiles Burn into seiche's build). Run:
/// `cargo test -p seiche --features gpu-bench --release -- --ignored settle_timing --nocapture`
#[cfg(feature = "gpu-bench")]
#[test]
#[ignore]
fn settle_timing_naive_vs_gpu_solver() {
    use std::sync::Arc;

    const TICKS: usize = 20;
    let solver: RepulsionSolver = Arc::new(|xs: &[f32], ys: &[f32], request: RepulsionRequest| {
        node_exclusion_wgpu_roundtrip(
            xs,
            ys,
            NodeExclusionParams {
                strength: request.strength,
                cutoff: request.cutoff,
                min_distance: request.min_distance,
            },
        )
        .map_err(|error| RepulsionSolverError::Backend(error.to_string()))
        .and_then(|(fx, fy)| RepulsionForces::new(xs.len(), fx, fy))
    });

    // This staging helper materializes O(N²) tensor intermediates. Stop at the
    // bounded receipt size rather than making an ignored test allocate GiBs.
    for n in [1_000usize, 2_000, 4_000] {
        let nodes = n_nodes(n);
        let build = || {
            let mut sim = Simulation::new();
            sim.add_force(NodeExclusion::default());
            sim.add_force(EdgeSpring::default());
            sim.add_force(Boundary::default());
            nodes.sync(&mut sim);
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
            "N={n}: cpu={cpu_ms:.2}ms/tick burn-roundtrip={gpu_ms:.2}ms/tick ({:.2}x)",
            cpu_ms / gpu_ms
        );
    }
}

#[test]
fn sync_creates_bodies_for_new_nodes() {
    let mut sim = Simulation::new();
    let nodes = two_nodes();
    nodes.sync(&mut sim);
    assert_eq!(sim.body_count(), 2);
    for key in nodes.keys() {
        assert!(sim.body_for(key).is_some());
    }
}

#[test]
fn sync_is_idempotent() {
    let mut sim = Simulation::new();
    let nodes = two_nodes();
    nodes.sync(&mut sim);
    nodes.sync(&mut sim);
    nodes.sync(&mut sim);
    assert_eq!(sim.body_count(), 2);
}

#[test]
fn empty_simulation_settles_to_rest() {
    let mut sim = Simulation::new();
    two_nodes().sync(&mut sim);
    for _ in 0..120 {
        sim.tick(1.0 / 60.0);
    }
    assert!(sim.is_at_rest(0.01));
}

#[test]
fn hit_test_and_cull_resolve_nodes_by_position() {
    // a@(0,0), b@(100,0), each an 18px-radius ball collider.
    let mut sim = Simulation::new();
    two_nodes().sync(&mut sim);
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

#[test]
fn node_exclusion_pushes_apart() {
    let mut sim = Simulation::new();
    // 50px apart: clear of the 36px contact range, inside the 600px cutoff,
    // so only the repulsion force acts.
    let mut nodes = Nodes::default();
    let a = nodes.at(0.0, 0.0);
    let b = nodes.at(50.0, 0.0);
    nodes.sync(&mut sim);
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
    // 400px apart, well past the 140px rest length; no repulsion force.
    let mut nodes = Nodes::default();
    let a = nodes.at(0.0, 0.0);
    let b = nodes.at(400.0, 0.0);
    nodes.sync(&mut sim);
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
    let mut nodes = Nodes::default();
    let a = nodes.at(500.0, 0.0);
    nodes.sync(&mut sim);
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
    // A connected triangle from distinct, near-coincident starts.
    let mut nodes = Nodes::default();
    let a = nodes.at(10.0, 0.0);
    let b = nodes.at(-5.0, 8.0);
    let c = nodes.at(-5.0, -8.0);
    nodes.sync(&mut sim);
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
    // 32x32 grid, 60px apart (clear of the 36px contact range): ~1024 nodes.
    let n: usize = 32;
    let spacing = 60.0_f32;
    let mut nodes = Nodes::default();
    let mut keys = Vec::new();
    for i in 0..n {
        for j in 0..n {
            keys.push(nodes.at(i as f32 * spacing, j as f32 * spacing));
        }
    }
    nodes.sync(&mut sim);
    sim.refresh_spatial_index();
    assert_eq!(sim.body_count(), n * n);

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
    let nodes = two_nodes(); // a@(0,0), b@(100,0)
    nodes.sync(&mut sim);
    let keys = nodes.keys();

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
    let mut nodes = Nodes::default();
    let a = nodes.at(0.0, 0.0);
    let b = nodes.at(0.0, 0.0);
    nodes.sync(&mut sim);
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
    let mut nodes = Nodes::default();
    for i in 0..10 {
        for j in 0..10 {
            nodes.at((i as f32 - 4.5) * 50.0, (j as f32 - 4.5) * 50.0);
        }
    }
    nodes.sync(&mut sim);
    sim.add_force(NodeExclusion::default());
    sim.add_force(Boundary::default());
    let keys = nodes.keys();
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
