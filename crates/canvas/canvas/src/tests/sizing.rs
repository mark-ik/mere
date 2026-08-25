//! Size channels — by importance, by recency — and the importance cache that
//! feeds them (invalidation, restore, metric switches).

use super::*;

#[test]
fn size_by_importance_sizes_nodes_by_the_degree_signal() {
    // A hub linked to two leaves: hub degree 2 (importance 1.0), each leaf degree 1 (0.5).
    let mut graph = Graph::new();
    let hub = graph.add_node(
        "https://hub.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    let b = graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(2.0, 0.0),
    );
    graph.assert_semantic_predicate(hub, a, "links".to_string());
    graph.assert_semantic_predicate(hub, b, "links".to_string());
    let mut canvas = Canvas::with_graph(graph);
    let hk = canvas
        .graph()
        .get_node_by_url("https://hub.example")
        .unwrap()
        .0;
    let ak = canvas
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let aid = canvas.graph().get_node(ak).unwrap().id;

    // Off by default: every node is the uniform footprint.
    assert_eq!(
        canvas.node_size(hk),
        36.0,
        "uniform until size-by-importance is on"
    );

    // On: the most-connected node hits the cap (88), a 0.5-importance leaf is 36 + 0.5*52 = 62.
    canvas.set_size_by_importance(true);
    assert_eq!(
        canvas.node_size(hk),
        88.0,
        "the most important node hits the cap"
    );
    assert!(
        (canvas.node_size(ak) - 62.0).abs() < 0.1,
        "a leaf scales by its 0.5 importance"
    );

    // Precedence: a manual override still wins over the importance size.
    canvas.set_node_size(aid, 100.0);
    assert_eq!(
        canvas.node_size(ak),
        100.0,
        "a manual override beats the importance size"
    );

    // Turning it off reverts to the uniform footprint (the hub has no manual override).
    canvas.set_size_by_importance(false);
    assert_eq!(canvas.node_size(hk), 36.0, "off => uniform again");
}

#[test]
fn size_by_recency_grows_the_newest_node_and_shrinks_the_oldest() {
    // Three nodes staggered in last_visited: oldest / middle / newest.
    let mut graph = Graph::new();
    let old = graph.add_node(
        "https://old.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mid = graph.add_node(
        "https://mid.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    let new = graph.add_node(
        "https://new.example".to_string(),
        PortablePoint::new(2.0, 0.0),
    );
    for (key, timestamp_ms) in [(old, 0), (mid, 50_000), (new, 100_000)] {
        let id = graph.get_node(key).unwrap().id;
        kernel::graph::apply::apply_graph_delta(
            &mut graph,
            kernel::graph::apply::GraphDelta::ReplayTouchNodeLastVisitedById {
                node_id: id,
                timestamp_ms,
            },
        );
    }
    let mut canvas = Canvas::with_graph(graph);
    let key = |url: &str| canvas.graph().get_node_by_url(url).unwrap().0;
    let (ok, mk, nk) = (
        key("https://old.example"),
        key("https://mid.example"),
        key("https://new.example"),
    );

    // Off by default: uniform.
    assert_eq!(canvas.node_size(nk), 36.0, "off => uniform");

    canvas.set_size_by_recency(true);
    // Newest at the cap (88), oldest at the default (36), middle halfway
    // (recency 0.5 => 36 + 0.5*(88-36) = 62).
    assert_eq!(canvas.node_size(nk), 88.0, "newest hits the cap");
    assert_eq!(canvas.node_size(ok), 36.0, "oldest reads the default");
    assert!(
        (canvas.node_size(mk) - 62.0).abs() < 0.1,
        "middle scales to its 0.5 recency, got {}",
        canvas.node_size(mk)
    );

    // A manual override still wins.
    let nid = canvas.graph().get_node(nk).unwrap().id;
    canvas.set_node_size(nid, 100.0);
    assert_eq!(
        canvas.node_size(nk),
        100.0,
        "an override beats recency size"
    );

    canvas.set_size_by_recency(false);
    assert_eq!(canvas.node_size(mk), 36.0, "off => uniform again");
}

#[test]
fn importance_cache_invalidates_on_topology_change() {
    // a-b, both degree 1 => importance 1.0 each => both at the cap.
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
    canvas.set_size_by_importance(true);
    let bk = canvas
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;
    assert_eq!(
        canvas.node_size(bk),
        88.0,
        "both nodes degree 1 => b is at the cap"
    );

    // Add c linked to a: a becomes degree 2 (importance 1.0), b stays degree 1 (importance 0.5).
    // The cache must invalidate on the topology change, so b drops from 88 to 62 (36 + 0.5*52);
    // a stale cache would leave b at 88.
    canvas.ingest_graph(|g| {
        let c = g.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        let a = g.get_node_by_url("https://a.example").unwrap().0;
        g.assert_semantic_predicate(a, c, "links".to_string());
        true
    });
    let bk = canvas
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;
    assert!(
        (canvas.node_size(bk) - 62.0).abs() < 0.1,
        "after the edge add, b's importance recomputed (the cache invalidated): {}",
        canvas.node_size(bk),
    );
}

#[test]
fn size_by_importance_restore_recomputes_on_a_reused_canvas() {
    // a-b, both degree 1 => importance 1.0 => size 88 when the mode is on.
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

    // Cycle the mode so the cache ends clean + empty with the mode OFF — the state a *reused*
    // canvas is in at a session switch, just before the sidecar restore.
    canvas.set_size_by_importance(true);
    canvas.set_size_by_importance(false);
    assert_eq!(canvas.node_size(ak), 36.0, "off => default size");

    // The restore turns it back on: it must force a recompute, not leave the cache empty (the
    // bug the review caught). Without the fix, the node would stay at the default 36.
    canvas.apply_cartography_sizing(Vec::<(uuid::Uuid, f32)>::new(), false, true);
    assert_eq!(
        canvas.node_size(ak),
        88.0,
        "restored size-by-importance recomputes, not an empty cache"
    );
}

#[test]
fn importance_metric_switches_degree_vs_betweenness() {
    // Bowtie: triangles {0,1,2} and {2,3,4} share the bridge node 2.
    let mut graph = Graph::new();
    let n: Vec<_> = (0..5)
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
    let k0 = canvas
        .graph()
        .get_node_by_url("https://0.example")
        .unwrap()
        .0;
    let k2 = canvas
        .graph()
        .get_node_by_url("https://2.example")
        .unwrap()
        .0;
    canvas.set_size_by_importance(true);

    // Degree (default): the bridge (degree 4) is the max; a peripheral (degree 2) is mid (62).
    assert_eq!(
        canvas.node_size(k2),
        88.0,
        "the bridge is the max under degree"
    );
    assert!(
        (canvas.node_size(k0) - 62.0).abs() < 0.1,
        "a peripheral is mid-sized under degree"
    );

    // Betweenness: the bridge stays max; the peripheral lies on no cross-paths => ~0 => default.
    canvas.set_importance_metric(ImportanceMetric::Betweenness);
    assert_eq!(
        canvas.node_size(k2),
        88.0,
        "the bridge is still the max under betweenness"
    );
    assert!(
        (canvas.node_size(k0) - 36.0).abs() < 0.5,
        "a peripheral shrinks to ~default under betweenness"
    );
}
