//! Minting nodes onto the canvas — visit, ingest, open-as-new, add-at-cursor,
//! add-field-at — plus member selection, tagging, and identity recovery.

use super::*;

#[test]
fn visit_adds_a_linked_node_and_selects_it() {
    let mut canvas = Canvas::new(); // empty session
    let a = canvas.visit("mere://welcome");
    assert_eq!(
        canvas.graph.nodes().count(),
        1,
        "the first visit seeds the root node"
    );
    assert!(canvas.selected.contains(&a), "the visited node is selected");

    let b = canvas.visit("https://example.com");
    assert_eq!(canvas.graph.nodes().count(), 2, "a fresh URL adds a node");
    assert_ne!(a, b);
    assert!(
        canvas.selected.contains(&b) && !canvas.selected.contains(&a),
        "selection moves to the newly visited node",
    );
    assert!(
        canvas.graph.relations().count() >= 1,
        "an edge links the browse trail"
    );
    assert_eq!(
        canvas.view.len(),
        2,
        "the layout grew a node for the new browse target"
    );
}

#[test]
fn ingest_graph_merges_nodes_and_grows_the_sim() {
    let mut canvas = Canvas::new();
    canvas.visit("mere://seed");
    let before = canvas.graph.nodes().count();
    let changed = canvas.ingest_graph(|g| {
        let a = g.add_node("mere://x".to_string(), Default::default());
        let b = g.add_node("mere://y".to_string(), Default::default());
        let _ = g.assert_relation(a, b, hyperlink());
        true
    });
    assert!(changed, "a mutating closure reports a change");
    assert_eq!(
        canvas.graph.nodes().count(),
        before + 2,
        "both ingested nodes joined"
    );
    assert!(canvas.graph.get_node_by_url("mere://x").is_some());
    assert_eq!(
        canvas.view.len(),
        before + 2,
        "the layout grew a node per ingested node"
    );
    // The origin-minted nodes are seeded apart, so the settle runs clean (no
    // coincident-point blow-up / NaN).
    let _ = canvas.frame(800, 600);
    // A closure that reports no change is a no-op.
    assert!(
        !canvas.ingest_graph(|_| false),
        "no-op mutation reports no change"
    );
}

#[test]
fn open_as_new_node_mints_distinct_node_with_navigated_from_edge() {
    let mut canvas = Canvas::new();
    let a = canvas.visit("https://example.com");
    let a_id = canvas.graph().get_node(a).unwrap().id;
    let before_nodes = canvas.graph().nodes().count();
    let before_edges = canvas.graph().relations().count();

    // Opening the *same* URL as a new node mints a distinct surface (no dedup)
    // plus a navigated-from edge from the origin.
    let new_id = canvas.open_member_as_new_node(Some(a_id), "https://example.com");
    assert_ne!(
        new_id, a_id,
        "a new browsing surface, not the deduped origin"
    );
    assert_eq!(
        canvas.graph().nodes().count(),
        before_nodes + 1,
        "a node was minted"
    );
    assert_eq!(
        canvas.graph().relations().count(),
        before_edges + 1,
        "a navigated-from edge links it back to the origin",
    );
    assert_eq!(
        canvas.focused_member(),
        Some(new_id),
        "the new node is focused"
    );
    // The new node opens on its URL — its own within-node history is seeded.
    let (new_key, _) = canvas.graph().get_node_by_id(new_id).unwrap();
    assert_eq!(
        canvas.graph().node_current_url(new_key).as_deref(),
        Some("https://example.com"),
        "the minted node carries its opening page as its first visit",
    );

    // No origin → an unlinked node (a graphlet candidate), no extra edge.
    let edges_now = canvas.graph().relations().count();
    let orphan = canvas.open_member_as_new_node(None, "https://orphan.example");
    assert_ne!(orphan, new_id);
    assert_eq!(
        canvas.graph().relations().count(),
        edges_now,
        "an origin-less open mints no navigated-from edge",
    );
}

#[test]
fn add_node_at_mints_an_unlinked_node_at_the_cursor_world_point() {
    let mut canvas = Canvas::new();
    canvas.camera.offset = (100.0, 50.0);
    canvas.camera.zoom = 2.0;
    let before_nodes = canvas.graph().nodes().count();
    let before_edges = canvas.graph().relations().count();

    let id = canvas.add_node_at((300.0, 150.0), "mere://welcome");
    assert_eq!(
        canvas.graph().nodes().count(),
        before_nodes + 1,
        "a node was added"
    );
    assert_eq!(
        canvas.graph().relations().count(),
        before_edges,
        "the add-node is unlinked (no navigated-from edge)",
    );
    assert_eq!(
        canvas.focused_url(),
        Some("mere://welcome"),
        "the new node is selected"
    );

    // world = ((300-100)/2, (150-50)/2) = (100, 50); the seed is set before any
    // settle runs (settle is consumed by frame(), not called here).
    let (key, _) = canvas.graph().get_node_by_id(id).unwrap();
    let pos = canvas
        .view
        .position_of(key)
        .expect("the new node has a position");
    assert!(
        (pos.x - 100.0).abs() < 0.5,
        "minted at the cursor world x, got {pos:?}"
    );
    assert!(
        (pos.y - 50.0).abs() < 0.5,
        "minted at the cursor world y, got {pos:?}"
    );
}

#[test]
fn add_field_at_places_a_region_field_at_the_cursor_world_point() {
    use kernel::graph::{CouplingResponse, FieldExtent, FieldId};
    let mut canvas = Canvas::new();
    canvas.camera.offset = (100.0, 50.0);
    canvas.camera.zoom = 2.0;
    let before = canvas.graph().fields().count();
    let before_couplings = canvas.graph().couplings().count();

    let id = canvas.add_field_at((300.0, 150.0));
    assert_eq!(
        canvas.graph().fields().count(),
        before + 1,
        "a field was placed"
    );

    // world = ((300-100)/2, (150-50)/2) = (100, 50); a square Region centered there.
    let field = canvas
        .graph()
        .field(FieldId::from_uuid(id))
        .expect("the placed field exists by id");
    let FieldExtent::Region {
        min_x,
        min_y,
        max_x,
        max_y,
    } = field.extent
    else {
        panic!(
            "a placed field carries a Region extent, got {:?}",
            field.extent
        );
    };
    assert!(
        ((min_x + max_x) / 2.0 - 100.0).abs() < 0.5,
        "centered at world x, got {min_x}..{max_x}"
    );
    assert!(
        ((min_y + max_y) / 2.0 - 50.0).abs() < 0.5,
        "centered at world y, got {min_y}..{max_y}"
    );

    // The no-placebo default coupling: it couples the placed field and gathers nodes
    // toward the disk peak (`RepelFromMax` = force up the gradient). Gyre's own tests
    // verify the force *behavior*; this verifies the field places the coupling.
    assert_eq!(
        canvas.graph().couplings().count(),
        before_couplings + 1,
        "a default gather coupling was placed with the field"
    );
    let coupling = canvas
        .graph()
        .couplings_for_field(FieldId::from_uuid(id))
        .next()
        .expect("a coupling targets the placed field");
    assert!(
        matches!(coupling.response, CouplingResponse::RepelFromMax),
        "the default coupling gathers toward the disk peak, got {:?}",
        coupling.response
    );
}

#[test]
fn toggle_select_member_builds_a_multi_selection() {
    let mut canvas = Canvas::new();
    let _a = canvas.visit("https://a.test");
    let b = canvas.visit("https://b.test");
    let b_id = canvas.graph().get_node(b).unwrap().id;

    // Replace to a single selection, then additively toggle the second in.
    assert!(canvas.select_by_url("https://a.test"));
    assert_eq!(canvas.selected_members().len(), 1);
    assert!(
        canvas.toggle_select_member(b_id),
        "a known member toggles in"
    );
    assert_eq!(
        canvas.selected_members().len(),
        2,
        "the selection grew to the pair"
    );
    assert!(
        canvas.assert_selected_relation(SemanticSubKind::UserGrouped),
        "the two-node selection can be related",
    );

    // Toggling the same member again removes it; an unknown member is a no-op.
    assert!(canvas.toggle_select_member(b_id));
    assert_eq!(
        canvas.selected_members().len(),
        1,
        "toggling off shrinks back"
    );
    assert!(
        !canvas.toggle_select_member(uuid::Uuid::new_v4()),
        "unknown member → false"
    );
    assert_eq!(
        canvas.selected_members().len(),
        1,
        "and leaves the selection intact"
    );
}

#[test]
fn assert_selected_relation_links_exactly_two_selected_nodes() {
    let mut canvas = Canvas::new();
    // Two unlinked nodes (origin-less mints carry no edge).
    let a = canvas.open_member_as_new_node(None, "https://a.test");
    let b = canvas.open_member_as_new_node(None, "https://b.test");
    let edges_before = canvas.graph().relations().count();
    let ak = canvas.graph().get_node_by_id(a).unwrap().0;
    let bk = canvas.graph().get_node_by_id(b).unwrap().0;

    // A single-node selection is not a clean pair → no edge.
    canvas.selected.clear();
    canvas.selected.insert(ak);
    assert!(!canvas.assert_selected_relation(SemanticSubKind::UserGrouped));
    assert_eq!(canvas.graph().relations().count(), edges_before);

    // Both selected → one user-grouped semantic edge.
    canvas.selected.insert(bk);
    assert!(canvas.assert_selected_relation(SemanticSubKind::UserGrouped));
    assert_eq!(
        canvas.graph().relations().count(),
        edges_before + 1,
        "exactly one edge is created between the pair",
    );

    // Idempotent: re-asserting the same sub-kind succeeds but does not multiply
    // (the relation is already present, so the edge count is unchanged).
    assert!(canvas.assert_selected_relation(SemanticSubKind::UserGrouped));
    assert_eq!(canvas.graph().relations().count(), edges_before + 1);
}

#[test]
fn tag_selected_inserts_the_trimmed_tag_on_every_selected_node() {
    let mut canvas = Canvas::new();
    let a = canvas.open_member_as_new_node(None, "https://a.test");
    let b = canvas.open_member_as_new_node(None, "https://b.test");
    let ak = canvas.graph().get_node_by_id(a).unwrap().0;
    let bk = canvas.graph().get_node_by_id(b).unwrap().0;
    canvas.selected.clear();
    canvas.selected.insert(ak);
    canvas.selected.insert(bk);
    // Both selected nodes gain the (trimmed) tag; re-tagging adds nothing new.
    assert_eq!(
        canvas.tag_selected("  reading  "),
        2,
        "both nodes newly tagged"
    );
    assert_eq!(canvas.tag_selected("reading"), 0, "re-tag is idempotent");
    assert!(canvas.graph().node_tags(ak).unwrap().contains("reading"));
    assert!(canvas.graph().node_tags(bk).unwrap().contains("reading"));
    // An all-whitespace tag is a no-op.
    assert_eq!(canvas.tag_selected("   "), 0, "blank tag is ignored");
}

#[test]
fn member_tagging_does_not_guess_between_duplicate_urls() {
    let mut canvas = Canvas::new();
    let first = canvas.open_member_as_new_node(None, "https://same.test");
    let second = canvas.open_member_as_new_node(None, "https://same.test");

    assert!(canvas.tag_node(second, "unread"));
    let first_key = canvas.graph().get_node_by_id(first).unwrap().0;
    let second_key = canvas.graph().get_node_by_id(second).unwrap().0;
    assert!(
        !canvas
            .graph()
            .node_tags(first_key)
            .unwrap()
            .contains("unread")
    );
    assert!(
        canvas
            .graph()
            .node_tags(second_key)
            .unwrap()
            .contains("unread")
    );

    assert!(canvas.untag_node(second, "unread"));
    assert!(
        !canvas
            .graph()
            .node_tags(second_key)
            .unwrap()
            .contains("unread")
    );
}

#[test]
fn recover_node_restores_the_original_identity() {
    let mut canvas = Canvas::new();
    // A real node lives, dies, and its bin record carries its identity.
    let original = canvas.open_member_as_new_node(None, "https://recovered.test");
    canvas.remove_focused().expect("the fresh node is focused");
    assert!(
        canvas.graph().get_node_by_id(original).is_none(),
        "deleted from the graph"
    );
    let before = canvas.graph().nodes().count();

    let id = canvas.recover_node(
        original,
        "https://recovered.test",
        Some("Recovered Page"),
        &["reading".to_string(), "archived".to_string()],
    );
    assert_eq!(id, original, "recovery restores the ORIGINAL member id");
    assert_eq!(
        canvas.graph().nodes().count(),
        before + 1,
        "a node was re-minted"
    );
    let (key, node) = canvas.graph().get_node_by_id(original).unwrap();
    assert_eq!(node.url(), "https://recovered.test", "the url is restored");
    assert_eq!(node.title, "Recovered Page", "the title is restored");
    let tags = canvas.graph().node_tags(key).unwrap();
    assert!(
        tags.contains("reading") && tags.contains("archived"),
        "both tags restored"
    );

    // Idempotent: recovering an already-recovered id selects it instead of
    // minting a twin under the same identity.
    let again = canvas.recover_node(original, "https://recovered.test", None, &[]);
    assert_eq!(again, original);
    assert_eq!(
        canvas.graph().nodes().count(),
        before + 1,
        "no twin node under one identity"
    );

    // A record with no stored title still re-mints (its title is left to the
    // mint default / a later fetch, not forced empty).
    let id2 = canvas.recover_node(uuid::Uuid::new_v4(), "https://untitled.test", None, &[]);
    assert!(
        canvas.graph().get_node_by_id(id2).is_some(),
        "the untitled node re-mints"
    );
}
