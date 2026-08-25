//! Layout strategies as an override of the physics snapshot — preview, apply,
//! revert, persist — and the drag / keyboard-nudge paths that write into them.

use super::*;

#[test]
fn layout_strategy_overrides_node_positions_until_reverted() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    let key = canvas
        .graph()
        .get_node_by_url("https://one.example")
        .unwrap()
        .0;
    assert_eq!(
        canvas.layout_strategy(),
        None,
        "force-directed (seiche) by default"
    );

    // Activate a strategy and push a position; the overlay (what `frame` runs after
    // the physics snapshot) writes it into the read model, winning over seiche.
    canvas.set_layout_strategy(Some("test.grid".to_string()));
    assert_eq!(canvas.layout_strategy(), Some("test.grid"));
    canvas.apply_strategy_positions(&[(key, PortablePoint::new(321.0, 654.0))]);
    canvas.apply_strategy_to_view();
    let pos = canvas.view.position_of(key).expect("node position");
    assert!(
        (pos.x - 321.0).abs() < 0.01 && (pos.y - 654.0).abs() < 0.01,
        "the strategy position overrides the physics position, got {pos:?}",
    );

    // Reverting drops the buffer and clears the strategy (seiche resumes).
    canvas.set_layout_strategy(None);
    assert_eq!(canvas.layout_strategy(), None, "revert clears the strategy");
}

#[test]
fn transition_preview_uses_the_strategy_buffer_until_the_final_snap() {
    let mut canvas = Canvas::new();
    let key = canvas.visit("https://transition-preview.example");
    canvas.set_layout_strategy(Some("test.grid".to_string()));
    canvas.apply_strategy_positions(&[(key, PortablePoint::new(10.0, 20.0))]);

    canvas.preview_strategy_positions(&[(key, PortablePoint::new(40.0, 50.0))]);
    canvas.apply_strategy_to_view();
    assert_eq!(
        canvas.view.position_of(key),
        Some(PortablePoint::new(40.0, 50.0)),
        "a host-clock sample reaches the same render buffer as a snapped strategy",
    );

    canvas.apply_strategy_positions(&[(key, PortablePoint::new(70.0, 80.0))]);
    canvas.apply_strategy_to_view();
    assert_eq!(
        canvas.view.position_of(key),
        Some(PortablePoint::new(70.0, 80.0)),
        "the final placement still goes through the ordinary strategy authority",
    );
}

#[test]
fn dragging_a_node_updates_its_active_strategy_slot() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://drag-strategy.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    let key = canvas
        .graph()
        .get_node_by_url("https://drag-strategy.example")
        .unwrap()
        .0;
    canvas.set_layout_strategy(Some("test.grid".to_string()));
    canvas.apply_strategy_positions(&[(key, PortablePoint::new(100.0, 120.0))]);
    canvas.apply_strategy_to_view();

    let start = canvas.camera.to_screen(PortablePoint::new(100.0, 120.0));
    canvas.pointer_down(PointerButton::Left, start.0, start.1);
    canvas.cursor_moved(start.0 + 80.0, start.1 + 40.0);
    canvas.pointer_up(PointerButton::Left, start.0 + 80.0, start.1 + 40.0);
    canvas.apply_strategy_to_view();

    let moved = canvas.view.position_of(key).expect("moved node position");
    assert!((moved.x - 180.0).abs() < 0.01);
    assert!((moved.y - 160.0).abs() < 0.01);
    let slot = canvas
        .strategy_positions
        .as_ref()
        .and_then(|positions| positions.iter().find(|(node, _)| *node == key))
        .map(|(_, position)| *position)
        .expect("updated strategy slot");
    assert_eq!(slot, PortablePoint::new(180.0, 160.0));
}

#[test]
fn focused_keyboard_nudge_is_local_and_requires_explicit_release() {
    let mut canvas = Canvas::new();
    let id = canvas.open_member_as_new_node(None, "https://curation.example");
    let key = canvas.graph().get_node_by_id(id).expect("added node").0;
    canvas.select_member(id);
    let before = canvas.view.position_of(key).expect("placed node");
    let relation_count = canvas.graph().relations().count();

    assert!(canvas.pin_focused());
    assert!(canvas.pinned_nodes.contains(&key));
    assert!(canvas.nudge_focused(32.0, -16.0));
    let moved = canvas.view.position_of(key).expect("nudged node");
    assert_eq!(moved, Point2D::new(before.x + 32.0, before.y - 16.0));
    assert!(canvas.pinned_nodes.contains(&key));
    assert_eq!(
        canvas.graph().relations().count(),
        relation_count,
        "moving a view node does not edit graph relations"
    );
    assert_eq!(
        canvas.graph().get_node(key).map(|node| node.id),
        Some(id),
        "moving a view node does not replace graph membership"
    );

    assert!(canvas.release_focused());
    assert!(!canvas.pinned_nodes.contains(&key));
    assert!(
        !canvas.release_focused(),
        "release is explicit and idempotent"
    );
}

#[test]
fn pointer_pull_preserves_an_existing_explicit_pin() {
    let mut canvas = Canvas::new();
    let id = canvas.open_member_as_new_node(None, "https://held.example");
    let key = canvas.graph().get_node_by_id(id).expect("added node").0;
    canvas.select_member(id);
    assert!(canvas.pin_focused());
    let start = canvas.view.position_of(key).expect("placed node");
    let start = canvas
        .camera
        .to_screen(PortablePoint::new(start.x, start.y));

    canvas.pointer_down(PointerButton::Left, start.0, start.1);
    canvas.cursor_moved(start.0 + 48.0, start.1 + 12.0);
    canvas.pointer_up(PointerButton::Left, start.0 + 48.0, start.1 + 12.0);

    assert!(
        canvas.pinned_nodes.contains(&key),
        "a transient pointer pull does not undo an explicit pin"
    );
    assert!(canvas.release_focused());
}

#[test]
fn persisted_spiral_score_reinstates_the_local_strategy_positions() {
    let mut canvas = Canvas::new();
    let first = canvas.visit("https://score-first.example");
    let second = canvas.visit("https://score-second.example");
    let first_id = canvas.graph().get_node(first).unwrap().id;
    let second_id = canvas.graph().get_node(second).unwrap().id;
    let mut score = sceno::Score::new(sceno::Arrangement::Spiral(sceno::Spiral::default()));
    score.items = [first_id, second_id]
        .into_iter()
        .enumerate()
        .map(|(ordinal, id)| sceno::ScoreItem {
            source: sceno::SourceRef::new(::cartography::MERE_GRAPH_ADAPTER, id.to_string()),
            ordinal: ordinal as u32,
            footprint: sceno::Footprint::Circle { radius: 18.0 },
            representation: sceno::Representation::Glyph,
            placement: sceno::Placement::Ordinal,
            layer: 0,
            visible: true,
            axis: None,
            embedding: None,
            weight: None,
        })
        .collect();
    assert!(canvas.restore_projection_score(score));
    assert_eq!(canvas.layout_strategy(), Some("phyllotaxis.default"));
    assert_eq!(canvas.strategy_positions.as_ref().unwrap().len(), 2);
    assert!(canvas.projection_score().is_some());
}
