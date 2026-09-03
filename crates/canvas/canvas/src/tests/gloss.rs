// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The gloss lens: its own assembled positions, cached independently of the
//! canvas layout, with its scope, weights, and size-by-importance encoding.

use super::*;

#[test]
fn gloss_lens_assembles_and_caches_independent_positions() {
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

    // No gloss strategy => mirror the main view, no recompute needed.
    assert!(canvas.gloss_strategy().is_none());
    assert!(
        !canvas.gloss_needs_recompute(200, 200),
        "mirroring needs no recompute"
    );

    // Setting a gloss lens asks for a recompute; the host supplies positions, which assemble into
    // the gloss geometry independent of the live layout (here, custom positions a force-directed
    // layout would never produce).
    canvas.set_gloss_strategy(Some("spectral.default".to_string()));
    assert!(
        canvas.gloss_needs_recompute(200, 200),
        "a fresh lens needs computing"
    );
    canvas.set_gloss_positions(
        vec![
            (ak, PortablePoint::new(10.0, 20.0)),
            (bk, PortablePoint::new(30.0, 40.0)),
        ],
        Vec::new(),
        200,
        200,
    );
    assert!(
        !canvas.gloss_needs_recompute(200, 200),
        "unchanged inputs => cached"
    );
    let (nodes, _edges, _rings) = canvas.gloss_geometry_cached();
    assert_eq!(nodes.len(), 2, "both positioned nodes appear");
    // The independent-lens property: the gloss node sits at the SUPPLIED position (10, 20), not
    // wherever the live force-directed layout would have put it.
    let a_id = canvas.graph().get_node(ak).unwrap().id;
    let a_node = nodes
        .iter()
        .find(|(id, _, _, _)| *id == a_id)
        .expect("node a is in the gloss");
    assert_eq!(
        a_node.1,
        (10.0, 20.0),
        "the gloss uses the lens position, not the live layout"
    );

    // A viewport change re-triggers; so does a topology change.
    assert!(
        canvas.gloss_needs_recompute(300, 200),
        "a viewport change re-triggers"
    );
    canvas.ingest_graph(|g| {
        g.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        true
    });
    assert!(
        canvas.gloss_needs_recompute(200, 200),
        "a topology change re-triggers"
    );
}

#[test]
fn gloss_overlays_resolve_into_rings_at_lens_positions() {
    // A community halo over {a, b} plus a bridge emphasis on a: the gloss resolves the stored
    // overlays into rings at the SUPPLIED lens positions (one ring per halo member + one bridge
    // ring), so the second lens shows the cluster/broker structure under its own layout. (P6b.)
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

    canvas.set_gloss_strategy(Some("spectral.default".to_string()));
    let overlays = vec![
        signals::Overlay::ClusterHalo {
            cluster_id: "c0".into(),
            members: vec![ak, bk],
            label: None,
            confidence: 1.0,
        },
        signals::Overlay::BridgeEmphasis {
            node: ak,
            weight: 1.0,
        },
    ];
    canvas.set_gloss_positions(
        vec![
            (ak, PortablePoint::new(10.0, 20.0)),
            (bk, PortablePoint::new(30.0, 40.0)),
        ],
        overlays,
        200,
        200,
    );
    let (_nodes, _edges, rings) = canvas.gloss_geometry_cached();
    assert_eq!(rings.len(), 3, "two cluster-halo rings + one bridge ring");
    assert!(
        rings
            .iter()
            .any(|((x, y), _, _)| (*x - 10.0).abs() < 1e-3 && (*y - 20.0).abs() < 1e-3),
        "a ring is placed at node a's lens position (10, 20)"
    );
}

#[test]
fn gloss_scope_selection_crops_the_lens_to_the_selection() {
    // Three nodes a-b-c; the gloss lens places all three. Scoping to the selection (just a) crops
    // the gloss to that node, dropping the induced edges to out-of-scope neighbours. (P6c, scope.)
    let mut graph = Graph::new();
    let a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let b = graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    let c = graph.add_node(
        "https://c.example".to_string(),
        PortablePoint::new(2.0, 0.0),
    );
    graph.assert_semantic_predicate(a, b, "links".to_string());
    graph.assert_semantic_predicate(b, c, "links".to_string());
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
    let ck = canvas
        .graph()
        .get_node_by_url("https://c.example")
        .unwrap()
        .0;

    canvas.set_gloss_strategy(Some("spectral.default".to_string()));
    canvas.set_gloss_positions(
        vec![
            (ak, PortablePoint::new(0.0, 0.0)),
            (bk, PortablePoint::new(10.0, 0.0)),
            (ck, PortablePoint::new(20.0, 0.0)),
        ],
        Vec::new(),
        200,
        200,
    );
    let (nodes, _e, _r) = canvas.gloss_geometry_cached();
    assert_eq!(nodes.len(), 3, "unscoped gloss shows the whole graph");

    canvas.select_by_url("https://a.example");
    canvas.set_gloss_scope_selection(true);
    let (nodes, edges, _r) = canvas.gloss_geometry_cached();
    assert_eq!(nodes.len(), 1, "scoped gloss shows only the selected node");
    assert_eq!(
        edges.len(),
        0,
        "no induced edge (a's neighbour b is out of scope)"
    );

    canvas.set_gloss_scope_selection(false);
    let (nodes, _e, _r) = canvas.gloss_geometry_cached();
    assert_eq!(nodes.len(), 3, "scope off shows the whole graph again");
}

#[test]
fn gloss_scope_is_part_of_the_lens_cache_key() {
    // The scope is in the gloss cache key, so turning on selection-scope (or changing the selection)
    // forces the host to recompute the lens — re-laying-out the induced subgraph, not just cropping.
    // (Graph signals — P6c, subgraph re-layout.)
    let mut graph = Graph::new();
    let _a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    let ak = canvas
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;

    canvas.set_gloss_strategy(Some("spectral.default".to_string()));
    assert_eq!(
        canvas.gloss_scope_keys(),
        None,
        "no scope until selection-scoping is on"
    );
    canvas.set_gloss_positions(
        vec![(ak, PortablePoint::new(0.0, 0.0))],
        Vec::new(),
        200,
        200,
    );
    assert!(
        !canvas.gloss_needs_recompute(200, 200),
        "unchanged inputs => cached"
    );

    // Selecting + scoping changes the scope key, so the lens must recompute (subgraph re-layout).
    canvas.select_by_url("https://a.example");
    canvas.set_gloss_scope_selection(true);
    assert_eq!(
        canvas.gloss_scope_keys(),
        Some(vec![ak]),
        "the scope is the sorted selection"
    );
    assert!(
        canvas.gloss_needs_recompute(200, 200),
        "a scope change re-triggers the lens"
    );
}

#[test]
fn gloss_edges_carry_multiplicity_weight() {
    // a-b connected by two distinct semantic relations (multiplicity 2), b-c by one: the gloss edge
    // for a-b weighs more than the one for b-c, so the swatch draws it thicker. (P6c, edge thickness.)
    let mut graph = Graph::new();
    let a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let b = graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    let c = graph.add_node(
        "https://c.example".to_string(),
        PortablePoint::new(2.0, 0.0),
    );
    graph.assert_relation(a, b, crate::build::hyperlink());
    graph.assert_relation(
        a,
        b,
        EdgeAssertion::Semantic {
            sub_kind: SemanticSubKind::UserGrouped,
            label: None,
            decay_progress: None,
        },
    );
    graph.assert_relation(b, c, crate::build::hyperlink());
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
    let ck = canvas
        .graph()
        .get_node_by_url("https://c.example")
        .unwrap()
        .0;

    canvas.set_gloss_strategy(Some("spectral.default".to_string()));
    canvas.set_gloss_positions(
        vec![
            (ak, PortablePoint::new(0.0, 0.0)),
            (bk, PortablePoint::new(10.0, 0.0)),
            (ck, PortablePoint::new(20.0, 0.0)),
        ],
        Vec::new(),
        200,
        200,
    );
    let (_n, edges, _r) = canvas.gloss_geometry_cached();
    assert_eq!(edges.len(), 2, "two collapsed pairs");
    let mut weights: Vec<f32> = edges.iter().map(|(_, _, w)| *w).collect();
    weights.sort_by(|x, y| x.partial_cmp(y).unwrap());
    assert_eq!(weights[0], 1.0, "the single-relation pair weighs 1");
    assert!(
        weights[1] >= 2.0,
        "the double-relation pair weighs at least 2, got {}",
        weights[1]
    );
}

#[test]
fn weighted_edges_memo_skips_recompute_on_static_frames() {
    // The kernel-query memo (cache C): a static frame must reuse the cached collapsed edge topology,
    // and a structural change must refresh it. (Graph signals — query memos.)
    let mut graph = Graph::new();
    let a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let b = graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    graph.assert_relation(a, b, crate::build::hyperlink());
    let mut canvas = Canvas::with_graph(graph);

    let _ = canvas.frame(800, 600);
    let after_first = canvas.weighted_edges_rebuilds();
    assert!(after_first >= 1, "the first frame builds the memo");
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.weighted_edges_rebuilds(),
        after_first,
        "a static frame reuses the cached edge topology"
    );

    // A structural change bumps the kernel revision, so the next frame refreshes the memo.
    canvas.ingest_graph(|g| {
        g.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        true
    });
    let _ = canvas.frame(800, 600);
    assert!(
        canvas.weighted_edges_rebuilds() > after_first,
        "a structural change refreshes the memo"
    );
}

#[test]
fn gloss_size_by_importance_scales_nodes_by_the_signal() {
    // A hub (degree 2) + two leaves (degree 1): degree importance gives the hub 1.0, a leaf 0.5, so
    // the gloss size factor is larger for the hub when size-by-importance is on. (P6c, encoding.)
    let mut graph = Graph::new();
    let hub = graph.add_node(
        "https://hub.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let l1 = graph.add_node(
        "https://l1.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    let l2 = graph.add_node(
        "https://l2.example".to_string(),
        PortablePoint::new(2.0, 0.0),
    );
    graph.assert_semantic_predicate(hub, l1, "links".to_string());
    graph.assert_semantic_predicate(hub, l2, "links".to_string());
    let mut canvas = Canvas::with_graph(graph);
    let hubk = canvas
        .graph()
        .get_node_by_url("https://hub.example")
        .unwrap()
        .0;
    let l1k = canvas
        .graph()
        .get_node_by_url("https://l1.example")
        .unwrap()
        .0;
    let l2k = canvas
        .graph()
        .get_node_by_url("https://l2.example")
        .unwrap()
        .0;
    let hub_id = canvas.graph().get_node(hubk).unwrap().id;
    let l1_id = canvas.graph().get_node(l1k).unwrap().id;

    canvas.set_gloss_strategy(Some("spectral.default".to_string()));
    canvas.set_gloss_positions(
        vec![
            (hubk, PortablePoint::new(0.0, 0.0)),
            (l1k, PortablePoint::new(10.0, 0.0)),
            (l2k, PortablePoint::new(20.0, 0.0)),
        ],
        Vec::new(),
        200,
        200,
    );
    // Off: every size factor is exactly 1.0.
    let (nodes, _e, _r) = canvas.gloss_geometry_cached();
    assert!(
        nodes.iter().all(|(_, _, _, f)| (*f - 1.0).abs() < 1e-6),
        "uniform size when the encoding is off"
    );

    // On: a frame recomputes importance; the hub's factor exceeds a leaf's.
    canvas.set_gloss_size_by_importance(true);
    let _ = canvas.frame(200, 200);
    let (nodes, _e, _r) = canvas.gloss_geometry_cached();
    let factor = |id| nodes.iter().find(|(nid, _, _, _)| *nid == id).unwrap().3;
    assert!(
        factor(hub_id) > factor(l1_id),
        "the hub scales larger than a leaf: {} vs {}",
        factor(hub_id),
        factor(l1_id)
    );
}

#[test]
fn gloss_size_by_importance_survives_disabling_main_size_by_importance() {
    // Review regression: turning the MAIN-view size-by-importance off cleared the importance cache
    // (and used to leave it clean-empty), so a later gloss-only size encoding rendered every node at
    // the uniform floor. The cache is now re-dirtied on disable, so the gloss still differentiates.
    let mut graph = Graph::new();
    let hub = graph.add_node(
        "https://hub.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let l1 = graph.add_node(
        "https://l1.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    let l2 = graph.add_node(
        "https://l2.example".to_string(),
        PortablePoint::new(2.0, 0.0),
    );
    graph.assert_relation(hub, l1, crate::build::hyperlink());
    graph.assert_relation(hub, l2, crate::build::hyperlink());
    let mut canvas = Canvas::with_graph(graph);
    let hubk = canvas
        .graph()
        .get_node_by_url("https://hub.example")
        .unwrap()
        .0;
    let l1k = canvas
        .graph()
        .get_node_by_url("https://l1.example")
        .unwrap()
        .0;
    let l2k = canvas
        .graph()
        .get_node_by_url("https://l2.example")
        .unwrap()
        .0;
    let hub_id = canvas.graph().get_node(hubk).unwrap().id;
    let l1_id = canvas.graph().get_node(l1k).unwrap().id;

    // Main-view size-by-importance on, then off (clears the importance cache) — no structural change.
    canvas.set_size_by_importance(true);
    canvas.set_size_by_importance(false);

    canvas.set_gloss_strategy(Some("spectral.default".to_string()));
    canvas.set_gloss_positions(
        vec![
            (hubk, PortablePoint::new(0.0, 0.0)),
            (l1k, PortablePoint::new(10.0, 0.0)),
            (l2k, PortablePoint::new(20.0, 0.0)),
        ],
        Vec::new(),
        200,
        200,
    );
    canvas.set_gloss_size_by_importance(true);
    let _ = canvas.frame(200, 200);
    let (nodes, _e, _r) = canvas.gloss_geometry_cached();
    let factor = |id| nodes.iter().find(|(nid, _, _, _)| *nid == id).unwrap().3;
    assert!(
        factor(hub_id) > factor(l1_id),
        "the gloss still scales the hub above a leaf after main size-by-importance was toggled off: {} vs {}",
        factor(hub_id),
        factor(l1_id)
    );
}

#[test]
fn gloss_kanban_by_site_lens_always_recomputes() {
    // Review regression: kanban.default groups by URL host (node content, which does NOT bump the
    // structural revision), so its gloss lens must recompute every frame rather than cache on the
    // revision — mirroring the main-view layout cache's special case.
    let mut graph = Graph::new();
    graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    let ak = canvas
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    canvas.set_gloss_strategy(Some("kanban.default".to_string()));
    canvas.set_gloss_positions(
        vec![(ak, PortablePoint::new(0.0, 0.0))],
        Vec::new(),
        200,
        200,
    );
    assert!(
        canvas.gloss_needs_recompute(200, 200),
        "the by-site kanban gloss lens recomputes every frame (URL-host content the revision misses)"
    );
}
