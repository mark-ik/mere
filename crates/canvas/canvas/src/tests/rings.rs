// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Revision-gated recompute for the overlay rings: the arrangement memo, the
//! community-ring partition, and the bridge-ring broker set.

use super::*;

#[test]
fn arrangement_recompute_is_gated_on_its_inputs() {
    let mut graph = Graph::new();
    let a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let b = graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    graph.assert_semantic_predicate(a, b, "links".to_string());
    let mut canvas = Canvas::with_graph(graph);
    let ak = canvas
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let bk = canvas
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;

    // First time there is no recorded layout, so a recompute is needed; after noting it, the same
    // inputs skip (an analytic layout is computed once, not per frame).
    assert!(
        canvas.needs_strategy_recompute("grid.default", 800, 600, None),
        "first compute"
    );
    canvas.note_strategy_computed("grid.default", 800, 600, None);
    assert!(
        !canvas.needs_strategy_recompute("grid.default", 800, 600, None),
        "unchanged => skip"
    );

    // A viewport change re-triggers.
    assert!(
        canvas.needs_strategy_recompute("grid.default", 1024, 600, None),
        "viewport change"
    );

    // A structural change (the kernel revision moves) re-triggers.
    canvas.ingest_graph(|g| {
        g.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        true
    });
    assert!(
        canvas.needs_strategy_recompute("grid.default", 800, 600, None),
        "revision moved"
    );

    // A non-focus strategy ignores the focus, so a selection change does not invalidate it.
    canvas.note_strategy_computed("grid.default", 800, 600, None);
    assert!(
        !canvas.needs_strategy_recompute("grid.default", 800, 600, Some(ak)),
        "grid ignores focus, so a selection change does not force a recompute"
    );

    // Radial is focus-driven, so a focus change DOES re-trigger it.
    canvas.note_strategy_computed("radial.default", 800, 600, Some(ak));
    assert!(
        canvas.needs_strategy_recompute("radial.default", 800, 600, Some(bk)),
        "radial re-centers on a focus change"
    );
}

#[test]
fn community_rings_toggle_computes_the_partition_on_frame() {
    // Two triangles {0,1,2} and {3,4,5} joined by a bridge => two communities.
    let mut graph = Graph::new();
    let n: Vec<NodeKey> = (0..6)
        .map(|i| {
            graph.add_node(
                format!("https://{i}.example"),
                PortablePoint::new(i as f32, 0.0),
            )
        })
        .collect();
    for &(a, b) in &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)] {
        graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
    }
    let mut canvas = Canvas::with_graph(graph);
    assert!(
        canvas.community().is_none(),
        "no partition until a consumer asks for it"
    );

    // Turning the rings on makes the frame compute the partition and run the ring paint path.
    canvas.set_show_community_rings(true);
    let _ = canvas.frame(800, 600);
    assert!(canvas.show_community_rings(), "the toggle is on");
    assert_eq!(
        canvas.community().map(|c| c.clusters.len()),
        Some(2),
        "the ring frame computed the two-community partition"
    );
}

#[test]
fn bridge_rings_toggle_computes_the_broker_on_frame() {
    // Bowtie: triangles {0,1,2} and {2,3,4} share the broker node 2.
    let mut graph = Graph::new();
    let n: Vec<NodeKey> = (0..5)
        .map(|i| {
            graph.add_node(
                format!("https://{i}.example"),
                PortablePoint::new(i as f32, 0.0),
            )
        })
        .collect();
    for &(a, b) in &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)] {
        graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
    }
    let mut canvas = Canvas::with_graph(graph);
    let k2 = canvas
        .graph()
        .get_node_by_url("https://2.example")
        .unwrap()
        .0;
    assert!(
        canvas.bridges().is_none(),
        "no bridges until the toggle asks for them"
    );

    canvas.set_show_bridge_rings(true);
    let _ = canvas.frame(800, 600);
    assert!(canvas.show_bridge_rings(), "the toggle is on");
    assert_eq!(
        canvas.bridges().unwrap().bridges,
        vec![k2],
        "the broker is the only bridge"
    );
}

#[test]
fn bridge_metric_switch_recomputes_under_the_new_metric() {
    // A 4-cycle distinguishes the metrics: every node is a (tied) betweenness broker, but none is a
    // cut vertex (the cycle is 2-connected). Switching the metric invalidates the cache and
    // recomputes, flipping the set. (Graph signals — articulation points.)
    let mut graph = Graph::new();
    let n: Vec<NodeKey> = (0..4)
        .map(|i| {
            graph.add_node(
                format!("https://{i}.example"),
                PortablePoint::new(i as f32, 0.0),
            )
        })
        .collect();
    for &(a, b) in &[(0, 1), (1, 2), (2, 3), (3, 0)] {
        graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
    }
    let mut canvas = Canvas::with_graph(graph);
    canvas.set_show_bridge_rings(true);

    // Default metric (betweenness): every cycle node is a tied broker.
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.bridges().unwrap().bridges.len(),
        4,
        "all four cycle nodes are tied betweenness brokers"
    );

    // Switching to articulation invalidates the cache; the cycle has no cut vertex.
    canvas.set_bridge_metric(signals::BridgeMetric::Articulation);
    let _ = canvas.frame(800, 600);
    assert!(
        canvas.bridges().unwrap().bridges.is_empty(),
        "a 2-connected cycle has no articulation point"
    );
    assert_eq!(canvas.bridge_metric(), signals::BridgeMetric::Articulation);
}
