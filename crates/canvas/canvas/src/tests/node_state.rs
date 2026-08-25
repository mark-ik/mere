//! Per-node state, shape, and hidden-edge bookkeeping, plus the pick geometry
//! that follows a node's resolved size.

use super::*;

#[test]
fn remove_focused_drops_the_node_and_reports_its_id() {
    let mut canvas = Canvas::new();
    canvas.visit("https://a.example");
    canvas.visit("https://b.example"); // focus on b
    let before = canvas.graph().nodes().count();
    let removed = canvas.remove_focused();
    assert!(
        removed.is_some(),
        "the focused node is removed and its id returned"
    );
    assert_eq!(
        canvas.graph().nodes().count(),
        before - 1,
        "the graph shrinks by one"
    );
    assert!(
        canvas
            .graph()
            .get_node_by_url("https://b.example")
            .is_none(),
        "the node is gone"
    );
    assert_eq!(canvas.focused_url(), None, "the selection clears");
    assert_eq!(
        canvas.view.len(),
        before - 1,
        "the removed node is reconciled out of the layout"
    );
    // Nothing focused → a second remove is a no-op.
    assert!(canvas.remove_focused().is_none(), "no focus → no removal");
}

#[test]
fn hide_selected_edges_then_show_all_round_trips() {
    let mut canvas = Canvas::new();
    canvas.visit("https://a.example");
    canvas.visit("https://b.example"); // the browse trail links a — b
    let a = canvas
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let b = canvas
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;
    canvas
        .selected_edges
        .insert(first_edge_cell_between(&canvas, a, b)); // the host edge-picks this normally

    assert_eq!(
        canvas.hide_selected_edges(),
        1,
        "the selected edge is hidden"
    );
    assert!(
        canvas.selected_edges.is_empty(),
        "hiding clears the selection"
    );
    assert_eq!(canvas.hidden_edges.len(), 1);
    // The relation itself survives (hiding is display-only).
    assert!(
        canvas.graph().relations().count() >= 1,
        "the relation is not deleted"
    );

    assert_eq!(
        canvas.show_all_edges(),
        1,
        "show-all reveals the hidden edge"
    );
    assert!(canvas.hidden_edges.is_empty());
}

#[test]
fn set_node_states_resolves_uuids_to_keys() {
    let mut canvas = Canvas::new();
    canvas.visit("https://a.example");
    let (key, id) = {
        let (k, n) = canvas.graph().get_node_by_url("https://a.example").unwrap();
        (k, n.id)
    };
    let mut states = HashMap::new();
    states.insert(id, NodeState::Open);
    canvas.set_node_states(states);
    assert_eq!(
        canvas.node_states.get(&key),
        Some(&NodeState::Open),
        "uuid resolves to its key"
    );

    // An unknown node id is filtered out (no panic, no stale entry).
    let mut other = HashMap::new();
    other.insert(uuid::Uuid::from_u128(0xdead_beef), NodeState::Closed);
    canvas.set_node_states(other);
    assert!(
        canvas.node_states.is_empty(),
        "an unknown node id is dropped"
    );
}

#[test]
fn set_node_shapes_resolves_uuids_to_keys() {
    let mut canvas = Canvas::new();
    canvas.visit("https://a.example");
    let (key, id) = {
        let (k, n) = canvas.graph().get_node_by_url("https://a.example").unwrap();
        (k, n.id)
    };
    let mut shapes = HashMap::new();
    shapes.insert(id, NodeShape::Circle);
    canvas.set_node_shapes(shapes);
    assert_eq!(
        canvas.node_shapes.get(&key),
        Some(&NodeShape::Circle),
        "uuid resolves to its key"
    );

    // An unknown node id is filtered out (no panic, no stale entry).
    let mut other = HashMap::new();
    other.insert(uuid::Uuid::from_u128(0xdead_beef), NodeShape::Rounded);
    canvas.set_node_shapes(other);
    assert!(
        canvas.node_shapes.is_empty(),
        "an unknown node id is dropped"
    );
}

#[test]
fn connected_members_reaches_the_whole_trail() {
    let mut canvas = Canvas::new();
    let a = canvas.visit("https://a.example");
    canvas.visit("https://b.example"); // a — b (browse trail)
    canvas.visit("https://c.example"); // b — c
    let a_id = canvas.graph().get_node(a).unwrap().id;
    let comp = canvas.connected_members(a_id);
    assert_eq!(
        comp.len(),
        3,
        "the a — b — c trail is one connected component"
    );
    assert_eq!(comp.first(), Some(&a_id), "BFS leads with the queried node");
    assert!(
        canvas
            .connected_members(uuid::Uuid::from_u128(0xabcd))
            .is_empty(),
        "an unknown member yields nothing",
    );
}

#[test]
fn selected_members_reflects_the_selection() {
    let mut canvas = Canvas::new();
    let a = canvas.visit("https://a.example"); // visit selects the node
    let a_id = canvas.graph().get_node(a).unwrap().id;
    assert_eq!(canvas.selected_members(), vec![a_id]);
}

#[test]
fn size_tiers_step_snap_and_clamp() {
    // The on-graph size editor steps a node through the five presets. (Node-rep — size tiers.)
    let a_id = uuid::Uuid::from_u128(0xb);
    let mut g = Graph::new();
    g.add_node_with_id(
        a_id,
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(g);
    let (key, _) = canvas.graph().get_node_by_id(a_id).unwrap();

    // The default size (36) reads as tier 1, the second notch.
    assert_eq!(canvas.node_size_tier(key), 1);
    // Step up to tier 2 — the override snaps onto the ladder at that preset.
    assert_eq!(canvas.step_node_size_tier(a_id, 1), 2);
    assert_eq!(canvas.node_size(key), crate::SIZE_TIERS[2]);
    // Step down past the bottom clamps at tier 0 (no underflow).
    assert_eq!(canvas.step_node_size_tier(a_id, -5), 0);
    assert_eq!(canvas.node_size(key), crate::SIZE_TIERS[0]);
}

#[test]
fn node_size_drives_the_pick_radius() {
    // Decision 5: a node sized in the view picks (and collides) at its true face,
    // not the uniform default — so a grown node is grabbable across its whole face.
    let mut canvas = Canvas::new();
    let id = canvas.add_node_at((0.0, 0.0), "mere://big");
    let (key, _) = canvas.graph().get_node_by_id(id).unwrap();
    let center = canvas
        .view
        .position_of(key)
        .expect("the new node has a position");

    // 30px from the center misses at the default 36px footprint (radius 18)...
    let probe = euclid::default::Point2D::new(center.x + 30.0, center.y);
    assert!(
        canvas.view.hit_test(probe).is_none(),
        "default footprint shouldn't reach 30px out"
    );

    // ...but an 80px face (radius 40) reaches it.
    canvas.set_node_size(id, 80.0);
    assert_eq!(
        canvas.view.hit_test(probe),
        Some(key),
        "the grown face grabs from 30px out"
    );

    // Clearing the override reverts the pick radius to the default.
    canvas.clear_node_size(id);
    assert!(
        canvas.view.hit_test(probe).is_none(),
        "cleared footprint reverts to the default"
    );
}

#[test]
fn alt_left_drag_orbits_the_camera() {
    // The deferred iso-camera orbit gesture: with Alt held, a left-drag yaws + reclines the camera
    // instead of picking / marqueeing. (Isometric camera — orbit gesture.)
    let mut canvas = Canvas::new();
    let (yaw0, tilt0) = (canvas.yaw(), canvas.tilt());

    canvas.set_alt(true);
    canvas.pointer_down(PointerButton::Left, 400.0, 300.0);
    canvas.cursor_moved(500.0, 360.0); // drag right (yaw) + down (recline tilt)
    assert!(canvas.yaw() != yaw0, "horizontal Alt+drag yawed the camera");
    assert!(canvas.tilt() < tilt0, "downward Alt+drag reclined the tilt");

    // Releasing ends the orbit: a later move (Alt still held, no press) must not keep orbiting.
    canvas.pointer_up(PointerButton::Left, 500.0, 360.0);
    let (yaw1, tilt1) = (canvas.yaw(), canvas.tilt());
    canvas.cursor_moved(600.0, 420.0);
    assert_eq!(
        (canvas.yaw(), canvas.tilt()),
        (yaw1, tilt1),
        "no orbit after the drag ends"
    );

    // And a plain (no-Alt) left drag does not orbit — it stays the pick / marquee gesture.
    canvas.set_alt(false);
    let yaw2 = canvas.yaw();
    canvas.pointer_down(PointerButton::Left, 100.0, 100.0);
    canvas.cursor_moved(220.0, 160.0);
    assert_eq!(canvas.yaw(), yaw2, "a non-Alt left drag does not orbit");
}
