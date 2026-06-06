use super::*;
use super::build::hyperlink;
use kernel::geometry::PortablePoint;
use kernel::graph::Graph;
use std::collections::HashMap;

#[test]
fn zoom_at_keeps_the_anchor_world_point_fixed() {
    let mut orrery = Orrery::new();
    orrery.camera.offset = (100.0, 50.0);
    orrery.camera.zoom = 1.0;
    let anchor = (200.0, 80.0);
    let world = |o: &Orrery| {
        (
            (anchor.0 - o.camera.offset.0) / o.camera.zoom,
            (anchor.1 - o.camera.offset.1) / o.camera.zoom,
        )
    };
    let before = world(&orrery);
    orrery.zoom_at(anchor, 2.0);
    let after = world(&orrery);
    assert!((after.0 - before.0).abs() < 0.01, "anchor world x fixed");
    assert!((after.1 - before.1).abs() < 0.01, "anchor world y fixed");
    assert_eq!(orrery.camera.zoom, 2.0, "zoom applied");
}

#[test]
fn screen_to_world_inverts_the_camera() {
    let mut orrery = Orrery::new();
    orrery.camera.offset = (100.0, 50.0);
    orrery.camera.zoom = 2.0;
    let w = orrery.screen_to_world((300.0, 150.0));
    assert!((w.x - 100.0).abs() < 0.01, "world x = (300-100)/2");
    assert!((w.y - 50.0).abs() < 0.01, "world y = (150-50)/2");
}

#[test]
fn visit_adds_a_linked_node_and_selects_it() {
    let mut orrery = Orrery::new(); // empty session
    let a = orrery.visit("mere://welcome");
    assert_eq!(orrery.graph.nodes().count(), 1, "the first visit seeds the root node");
    assert!(orrery.selected.contains(&a), "the visited node is selected");

    let b = orrery.visit("https://example.com");
    assert_eq!(orrery.graph.nodes().count(), 2, "a fresh URL adds a node");
    assert_ne!(a, b);
    assert!(
        orrery.selected.contains(&b) && !orrery.selected.contains(&a),
        "selection moves to the newly visited node",
    );
    assert!(orrery.graph.relations().count() >= 1, "an edge links the browse trail");
    assert_eq!(orrery.view.len(), 2, "the layout grew a node for the new browse target");
}

#[test]
fn ingest_graph_merges_nodes_and_grows_the_sim() {
    let mut orrery = Orrery::new();
    orrery.visit("mere://seed");
    let before = orrery.graph.nodes().count();
    let changed = orrery.ingest_graph(|g| {
        let a = g.add_node("mere://x".to_string(), Default::default());
        let b = g.add_node("mere://y".to_string(), Default::default());
        let _ = g.assert_relation(a, b, hyperlink());
        true
    });
    assert!(changed, "a mutating closure reports a change");
    assert_eq!(orrery.graph.nodes().count(), before + 2, "both ingested nodes joined");
    assert!(orrery.graph.get_node_by_url("mere://x").is_some());
    assert_eq!(orrery.view.len(), before + 2, "the layout grew a node per ingested node");
    // The origin-minted nodes are seeded apart, so the settle runs clean (no
    // coincident-point blow-up / NaN).
    let _ = orrery.frame(800, 600);
    // A closure that reports no change is a no-op.
    assert!(!orrery.ingest_graph(|_| false), "no-op mutation reports no change");
}

#[test]
fn open_as_new_node_mints_distinct_node_with_navigated_from_edge() {
    let mut orrery = Orrery::new();
    let a = orrery.visit("https://example.com");
    let a_id = orrery.graph().get_node(a).unwrap().id;
    let before_nodes = orrery.graph().nodes().count();
    let before_edges = orrery.graph().relations().count();

    // Opening the *same* URL as a new node mints a distinct surface (no dedup)
    // plus a navigated-from edge from the origin.
    let new_id = orrery.open_member_as_new_node(Some(a_id), "https://example.com");
    assert_ne!(new_id, a_id, "a new browsing surface, not the deduped origin");
    assert_eq!(orrery.graph().nodes().count(), before_nodes + 1, "a node was minted");
    assert_eq!(
        orrery.graph().relations().count(),
        before_edges + 1,
        "a navigated-from edge links it back to the origin",
    );
    assert_eq!(orrery.focused_member(), Some(new_id), "the new node is focused");
    // The new node opens on its URL — its own within-node history is seeded.
    let (new_key, _) = orrery.graph().get_node_by_id(new_id).unwrap();
    assert_eq!(
        orrery.graph().get_node(new_key).unwrap().navigation_memory.current_url().as_deref(),
        Some("https://example.com"),
        "the minted node carries its opening page as its first visit",
    );

    // No origin → an unlinked node (a graphlet candidate), no extra edge.
    let edges_now = orrery.graph().relations().count();
    let orphan = orrery.open_member_as_new_node(None, "https://orphan.example");
    assert_ne!(orphan, new_id);
    assert_eq!(
        orrery.graph().relations().count(),
        edges_now,
        "an origin-less open mints no navigated-from edge",
    );
}

#[test]
fn visit_dedups_by_url() {
    let mut orrery = Orrery::new();
    let a = orrery.visit("https://example.com");
    let _ = orrery.visit("https://other.com");
    let again = orrery.visit("https://example.com");
    assert_eq!(a, again, "revisiting a URL selects the existing node, not a duplicate");
    assert_eq!(orrery.graph.nodes().count(), 2, "no duplicate node is added");
    assert!(orrery.selected.contains(&a), "selection returns to the revisited node");
}

#[test]
fn focused_url_is_the_single_selected_nodes_url() {
    let mut orrery = Orrery::new();
    assert_eq!(orrery.focused_url(), None, "nothing selected yet → no focus");
    orrery.visit("https://example.com");
    assert_eq!(orrery.focused_url(), Some("https://example.com"), "visit focuses the node");
    orrery.visit("https://second.com");
    assert_eq!(orrery.focused_url(), Some("https://second.com"), "focus follows the new node");
    orrery.visit("https://example.com");
    assert_eq!(orrery.focused_url(), Some("https://example.com"), "revisit re-focuses");
}

#[test]
fn with_graph_restores_nodes_and_positions() {
    let mut graph = Graph::new();
    graph.add_node("https://one.example".to_string(), PortablePoint::new(100.0, 50.0));
    graph.add_node("https://two.example".to_string(), PortablePoint::new(-30.0, 80.0));
    let orrery = Orrery::with_graph(graph);
    assert_eq!(orrery.graph().nodes().count(), 2, "the restored graph keeps its nodes");
    assert_eq!(orrery.view.len(), 2, "a layout node per restored graph node");
    assert!(!orrery.is_settling(), "a restored session does not auto-resettle");
    let key = orrery.graph().get_node_by_url("https://one.example").unwrap().0;
    let pos = orrery.view.position_of(key).expect("a restored node position");
    assert!(
        (pos.x - 100.0).abs() < 1.0 && (pos.y - 50.0).abs() < 1.0,
        "the saved position is preserved (no spiral re-seed), got {pos:?}",
    );
}

#[test]
fn camera_round_trips_and_guards_bad_zoom() {
    let mut orrery = Orrery::new();
    orrery.set_camera(CameraView { offset: (123.0, -45.0), zoom: 2.5 });
    let cv = orrery.camera();
    assert_eq!(cv.offset, (123.0, -45.0));
    assert_eq!(cv.zoom, 2.5);
    // A zero / non-finite zoom falls back to 1.0 rather than collapsing.
    orrery.set_camera(CameraView { offset: (0.0, 0.0), zoom: 0.0 });
    assert_eq!(orrery.camera().zoom, 1.0);
}

#[test]
fn select_by_url_selects_existing_or_reports_missing() {
    let mut orrery = Orrery::new();
    orrery.visit("https://one.example");
    orrery.visit("https://two.example"); // focus moves to two
    assert!(orrery.select_by_url("https://one.example"), "an existing url is found + focused");
    assert_eq!(orrery.focused_url(), Some("https://one.example"));
    assert!(!orrery.select_by_url("https://absent.example"), "a missing url reports false");
    assert_eq!(orrery.focused_url(), Some("https://one.example"), "a miss leaves selection intact");
}

#[test]
fn remove_focused_drops_the_node_and_reports_its_id() {
    let mut orrery = Orrery::new();
    orrery.visit("https://a.example");
    orrery.visit("https://b.example"); // focus on b
    let before = orrery.graph().nodes().count();
    let removed = orrery.remove_focused();
    assert!(removed.is_some(), "the focused node is removed and its id returned");
    assert_eq!(orrery.graph().nodes().count(), before - 1, "the graph shrinks by one");
    assert!(orrery.graph().get_node_by_url("https://b.example").is_none(), "the node is gone");
    assert_eq!(orrery.focused_url(), None, "the selection clears");
    assert_eq!(orrery.view.len(), before - 1, "the removed node is reconciled out of the layout");
    // Nothing focused → a second remove is a no-op.
    assert!(orrery.remove_focused().is_none(), "no focus → no removal");
}

#[test]
fn hide_selected_edges_then_show_all_round_trips() {
    let mut orrery = Orrery::new();
    orrery.visit("https://a.example");
    orrery.visit("https://b.example"); // the browse trail links a — b
    let a = orrery.graph().get_node_by_url("https://a.example").unwrap().0;
    let b = orrery.graph().get_node_by_url("https://b.example").unwrap().0;
    orrery.selected_edges.insert((a, b)); // the host edge-picks this normally

    assert_eq!(orrery.hide_selected_edges(), 1, "the selected edge is hidden");
    assert!(orrery.selected_edges.is_empty(), "hiding clears the selection");
    assert_eq!(orrery.hidden_edges.len(), 1);
    // The relation itself survives (hiding is display-only).
    assert!(orrery.graph().relations().count() >= 1, "the relation is not deleted");

    assert_eq!(orrery.show_all_edges(), 1, "show-all reveals the hidden edge");
    assert!(orrery.hidden_edges.is_empty());
}

#[test]
fn set_node_states_resolves_uuids_to_keys() {
    let mut orrery = Orrery::new();
    orrery.visit("https://a.example");
    let (key, id) = {
        let (k, n) = orrery.graph().get_node_by_url("https://a.example").unwrap();
        (k, n.id)
    };
    let mut states = HashMap::new();
    states.insert(id, NodeState::Open);
    orrery.set_node_states(states);
    assert_eq!(orrery.node_states.get(&key), Some(&NodeState::Open), "uuid resolves to its key");

    // An unknown node id is filtered out (no panic, no stale entry).
    let mut other = HashMap::new();
    other.insert(uuid::Uuid::from_u128(0xdead_beef), NodeState::Closed);
    orrery.set_node_states(other);
    assert!(orrery.node_states.is_empty(), "an unknown node id is dropped");
}

#[test]
fn connected_members_reaches_the_whole_trail() {
    let mut orrery = Orrery::new();
    let a = orrery.visit("https://a.example");
    orrery.visit("https://b.example"); // a — b (browse trail)
    orrery.visit("https://c.example"); // b — c
    let a_id = orrery.graph().get_node(a).unwrap().id;
    let comp = orrery.connected_members(a_id);
    assert_eq!(comp.len(), 3, "the a — b — c trail is one connected component");
    assert_eq!(comp.first(), Some(&a_id), "BFS leads with the queried node");
    assert!(
        orrery.connected_members(uuid::Uuid::from_u128(0xabcd)).is_empty(),
        "an unknown member yields nothing",
    );
}

#[test]
fn selected_members_reflects_the_selection() {
    let mut orrery = Orrery::new();
    let a = orrery.visit("https://a.example"); // visit selects the node
    let a_id = orrery.graph().get_node(a).unwrap().id;
    assert_eq!(orrery.selected_members(), vec![a_id]);
}
