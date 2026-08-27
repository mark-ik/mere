// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The scope lens (a curated canvas of a subset) and the cartography sidecar
//! that captures live geometry and seeds it back.

use super::*;

#[test]
fn isolate_selection_scopes_the_canvas() {
    let mut graph = Graph::new();
    let a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(50.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    assert!(!canvas.is_scoped(), "the whole graph shows by default");

    // No selection: isolate is a no-op.
    canvas.isolate_selection();
    assert!(
        !canvas.is_scoped(),
        "isolating with no selection does nothing"
    );

    // With a selection, isolate scopes the canvas to (at least) the selected node.
    canvas.selected.insert(a);
    canvas.isolate_selection();
    assert!(
        canvas.is_scoped(),
        "isolating a selection scopes the canvas"
    );
    assert!(
        canvas.scope.as_ref().unwrap().contains(&a),
        "the scope includes the selected node",
    );

    canvas.clear_scope();
    assert!(!canvas.is_scoped(), "show all clears the scope");
}

#[test]
fn scope_to_members_scopes_to_the_given_set() {
    let mut g = Graph::new();
    let a = g.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    g.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(g);
    let a_id = canvas.graph().get_node(a).unwrap().id;
    assert!(!canvas.is_scoped(), "the whole graph by default");

    canvas.scope_to_members([a_id]);
    assert!(
        canvas.is_scoped(),
        "scoping to a member set (the workbench tiles) scopes the canvas"
    );
    assert!(
        canvas.scope.as_ref().unwrap().contains(&a),
        "the scope holds the member's key"
    );

    canvas.scope_to_members(std::iter::empty());
    assert!(
        !canvas.is_scoped(),
        "an empty member set shows the whole graph"
    );
}

#[test]
fn cartography_geometry_captures_live_positions() {
    let mut g = Graph::new();
    let a = g.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(g);
    let a_id = canvas.graph().get_node(a).unwrap().id;
    canvas
        .view
        .set_position(a, euclid::default::Point2D::new(111.0, 222.0));
    let geom = canvas.cartography_geometry();
    let by_member: std::collections::HashMap<_, _> = geom.iter().collect();
    assert_eq!(
        by_member.get(&a_id).copied(),
        Some((111.0, 222.0)),
        "the accessor captures the live node position",
    );
}

#[test]
fn cartography_sizing_round_trips_through_the_sidecar() {
    // A sized graph re-opens sized: the override + the scene flag survive an export and a
    // re-apply onto a fresh canvas over the same node id (a reload). (Node-rep — persistence.)
    let a_id = uuid::Uuid::from_u128(0xa);
    let mut g = Graph::new();
    g.add_node_with_id(
        a_id,
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(g);
    canvas.set_node_size(a_id, 80.0);
    canvas.set_size_by_degree(true);
    canvas.set_size_by_importance(true);
    canvas.set_importance_metric(ImportanceMetric::Betweenness);

    let geom = canvas.cartography_geometry();
    assert!(
        geom.size_by_degree(),
        "the export carries the size-by-degree flag"
    );
    assert!(
        geom.size_by_importance(),
        "the export carries the size-by-importance flag"
    );
    assert_eq!(
        geom.importance_metric(),
        "betweenness",
        "the export carries the metric"
    );

    // Re-apply onto a fresh canvas whose graph carries the same node id (the reload). The metric
    // restore runs first, so the sizing restore recomputes with it.
    let mut g2 = Graph::new();
    g2.add_node_with_id(
        a_id,
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut reloaded = Canvas::with_graph(g2);
    reloaded.apply_cartography_importance_metric(geom.importance_metric());
    reloaded.apply_cartography_sizing(
        geom.size_iter(),
        geom.size_by_degree(),
        geom.size_by_importance(),
    );
    let (key, _) = reloaded.graph().get_node_by_id(a_id).unwrap();
    assert_eq!(
        reloaded.node_size(key),
        80.0,
        "the per-node override is restored"
    );
    assert!(
        reloaded.size_by_degree(),
        "the size-by-degree flag is restored"
    );
    assert!(
        reloaded.size_by_importance(),
        "the size-by-importance flag is restored"
    );
    assert_eq!(
        reloaded.importance_metric(),
        ImportanceMetric::Betweenness,
        "the importance metric is restored (a betweenness scene re-opens betweenness-sized)",
    );
}

#[test]
fn seed_cartography_overrides_node_positions() {
    let mut g = Graph::new();
    let a = g.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(g);
    let a_id = canvas.graph().get_node(a).unwrap().id;
    canvas.seed_cartography([(a_id, (321.0, 654.0))]);
    let pos = canvas.view.position_of(a).expect("a position");
    assert!(
        (pos.x - 321.0).abs() < 0.01 && (pos.y - 654.0).abs() < 0.01,
        "seed_cartography overrides the load-time seed, got {pos:?}",
    );
}

#[test]
fn park_physics_halts_an_in_progress_settle() {
    let mut canvas = Canvas::with_sample_graph();
    assert!(
        canvas.is_settling(),
        "the sample graph settles its initial spiral"
    );
    canvas.park_physics();
    assert!(
        !canvas.is_settling(),
        "park halts the settle so a backgrounded graph stops ticking and waking the loop",
    );
}
