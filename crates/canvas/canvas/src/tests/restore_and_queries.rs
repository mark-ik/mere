//! Restoring a canvas from a stored graph (positions included), framing it with
//! fit-to-content, and the small read-only queries over what was restored.

use super::*;

#[test]
fn with_graph_restores_nodes_and_positions() {
    // Positions are no longer graph truth (S2): `with_graph` parks nodes at the
    // origin and halts, then the host applies the saved layout from the cartography
    // sidecar via `seed_cartography` — the seam that used to ride the graph snapshot.
    let mut graph = Graph::new();
    let one = graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(100.0, 50.0),
    );
    let two = graph.add_node(
        "https://two.example".to_string(),
        PortablePoint::new(-30.0, 80.0),
    );
    let one_id = graph.get_node(one).unwrap().id;
    let two_id = graph.get_node(two).unwrap().id;
    let mut canvas = Canvas::with_graph(graph);
    canvas.seed_cartography([(one_id, (100.0, 50.0)), (two_id, (-30.0, 80.0))]);
    assert_eq!(
        canvas.graph().nodes().count(),
        2,
        "the restored graph keeps its nodes"
    );
    assert_eq!(
        canvas.view.len(),
        2,
        "a layout node per restored graph node"
    );
    assert!(
        !canvas.is_settling(),
        "a restored session does not auto-resettle"
    );
    let key = canvas
        .graph()
        .get_node_by_url("https://one.example")
        .unwrap()
        .0;
    let pos = canvas
        .view
        .position_of(key)
        .expect("a restored node position");
    assert!(
        (pos.x - 100.0).abs() < 1.0 && (pos.y - 50.0).abs() < 1.0,
        "the sidecar position is applied (no spiral re-seed), got {pos:?}",
    );
}

#[test]
fn fit_to_content_frames_a_far_restored_graph() {
    // The restored-session failure: persisted positions settled far from the
    // world origin, so the boot recenter (which frames the origin) shows
    // empty ground. fit_to_content must frame the content instead.
    let mut graph = Graph::new();
    let far = graph.add_node(
        "https://far.example".to_string(),
        PortablePoint::new(4000.0, -2600.0),
    );
    let far_id = graph.get_node(far).unwrap().id;
    let mut canvas = Canvas::with_graph(graph);
    // The host restores the far-settled position from the sidecar (positions no
    // longer ride the graph snapshot, S2).
    canvas.seed_cartography([(far_id, (4000.0, -2600.0))]);
    canvas.resize(800, 600);
    canvas.recenter();
    assert!(
        !canvas.graph_visible(),
        "recenter frames the origin, missing far content (the bug setup)"
    );
    canvas.fit_to_content();
    assert!(
        canvas.graph_visible(),
        "fit_to_content frames the restored positions"
    );
    assert!(
        (canvas.camera().zoom - 1.0).abs() < f32::EPSILON,
        "a lone node is framed at natural size, not zoomed in"
    );
}

#[test]
fn fit_to_content_on_an_empty_graph_recenters() {
    let mut canvas = Canvas::new();
    canvas.resize(800, 600);
    canvas.fit_to_content();
    assert_eq!(canvas.camera().offset, (400.0, 300.0));
    assert!((canvas.camera().zoom - 1.0).abs() < f32::EPSILON);
}

#[test]
fn has_nodes_tracks_the_graph() {
    let mut canvas = Canvas::new();
    assert!(!canvas.has_nodes(), "a fresh empty session has no nodes");
    canvas.visit("mere://welcome");
    assert!(
        canvas.has_nodes(),
        "a visited node makes the graph non-empty"
    );
}

#[test]
fn select_by_url_selects_existing_or_reports_missing() {
    let mut canvas = Canvas::new();
    canvas.visit("https://one.example");
    canvas.visit("https://two.example"); // focus moves to two
    assert!(
        canvas.select_by_url("https://one.example"),
        "an existing url is found + focused"
    );
    assert_eq!(canvas.focused_url(), Some("https://one.example"));
    assert!(
        !canvas.select_by_url("https://absent.example"),
        "a missing url reports false"
    );
    assert_eq!(
        canvas.focused_url(),
        Some("https://one.example"),
        "a miss leaves selection intact"
    );
}

#[test]
fn revisit_stamps_the_durable_recency_clock() {
    let mut canvas = Canvas::new();
    let key = canvas.visit("https://recency.example");
    let id = canvas.graph().get_node(key).unwrap().id;
    kernel::graph::apply::apply_graph_delta(
        &mut canvas.graph,
        kernel::graph::apply::GraphDelta::ReplayTouchNodeLastVisitedById {
            node_id: id,
            timestamp_ms: 1,
        },
    );
    canvas.visit("https://recency.example");
    let stamped = canvas
        .graph()
        .node_last_visited(key)
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    assert!(stamped > 1, "a revisit updates the persisted visit clock");
}
