use super::build::hyperlink;
use super::*;
use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeFamily, Graph, RelationKind, RelationSelector, SemanticSubKind};
use std::collections::HashMap;

mod selection;

fn selector_for_relation_kind(kind: RelationKind) -> RelationSelector {
    match kind {
        RelationKind::Semantic(sub) => RelationSelector::Semantic(sub),
        RelationKind::Traversal => RelationSelector::Family(EdgeFamily::Traversal),
        RelationKind::Containment(sub) => RelationSelector::Containment(sub),
        RelationKind::Arrangement(sub) => RelationSelector::Arrangement(sub),
        RelationKind::Imported(sub) => RelationSelector::Imported(sub),
        RelationKind::Provenance(sub) => RelationSelector::Provenance(sub),
    }
}

fn first_edge_cell_between(
    orrery: &Orrery,
    a: kernel::graph::NodeKey,
    b: kernel::graph::NodeKey,
) -> EdgeCell {
    orrery
        .graph()
        .relations()
        .find_map(|relation| {
            let same_pair = (relation.from == a && relation.to == b)
                || (relation.from == b && relation.to == a);
            same_pair.then_some(EdgeCell {
                from: relation.from,
                to: relation.to,
                selector: selector_for_relation_kind(relation.kind),
            })
        })
        .expect("relation cell between the endpoints")
}

#[test]
fn edge_cell_hit_test_picks_fanned_parallel_relation_cells() {
    let mut orrery = Orrery::new();
    let a = orrery.open_member_as_new_node(None, "https://fan-a.test");
    let b = orrery.open_member_as_new_node(None, "https://fan-b.test");
    let ak = orrery.graph().get_node_by_id(a).unwrap().0;
    let bk = orrery.graph().get_node_by_id(b).unwrap().0;
    assert!(orrery.assert_relation_between_members(a, b, SemanticSubKind::Cites));
    assert!(orrery.assert_relation_between_members(a, b, SemanticSubKind::Quotes));
    orrery
        .view
        .set_position(ak, euclid::default::Point2D::new(0.0, 0.0));
    orrery
        .view
        .set_position(bk, euclid::default::Point2D::new(100.0, 0.0));

    let pair = if ak <= bk { (ak, bk) } else { (bk, ak) };
    let segments: Vec<_> = crate::edge_cells::visible_edge_cell_segments(
        orrery.graph(),
        &orrery.view,
        &orrery.hidden_edges,
    )
    .into_iter()
    .filter(|segment| segment.cell.endpoint_pair() == pair)
    .collect();
    assert_eq!(
        segments.len(),
        2,
        "the pair exposes two relation-cell lanes"
    );

    for segment in segments {
        let midpoint = euclid::default::Point2D::new(
            (segment.from.x + segment.to.x) * 0.5,
            (segment.from.y + segment.to.y) * 0.5,
        );
        assert_eq!(
            crate::edge_cells::edge_cell_hit_test(
                orrery.graph(),
                &orrery.view,
                &orrery.hidden_edges,
                midpoint,
                1.0,
            ),
            Some(segment.cell),
            "the fanned lane picks its own relation cell"
        );
    }
}

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
    assert_eq!(
        orrery.graph.nodes().count(),
        1,
        "the first visit seeds the root node"
    );
    assert!(orrery.selected.contains(&a), "the visited node is selected");

    let b = orrery.visit("https://example.com");
    assert_eq!(orrery.graph.nodes().count(), 2, "a fresh URL adds a node");
    assert_ne!(a, b);
    assert!(
        orrery.selected.contains(&b) && !orrery.selected.contains(&a),
        "selection moves to the newly visited node",
    );
    assert!(
        orrery.graph.relations().count() >= 1,
        "an edge links the browse trail"
    );
    assert_eq!(
        orrery.view.len(),
        2,
        "the layout grew a node for the new browse target"
    );
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
    assert_eq!(
        orrery.graph.nodes().count(),
        before + 2,
        "both ingested nodes joined"
    );
    assert!(orrery.graph.get_node_by_url("mere://x").is_some());
    assert_eq!(
        orrery.view.len(),
        before + 2,
        "the layout grew a node per ingested node"
    );
    // The origin-minted nodes are seeded apart, so the settle runs clean (no
    // coincident-point blow-up / NaN).
    let _ = orrery.frame(800, 600);
    // A closure that reports no change is a no-op.
    assert!(
        !orrery.ingest_graph(|_| false),
        "no-op mutation reports no change"
    );
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
    assert_ne!(
        new_id, a_id,
        "a new browsing surface, not the deduped origin"
    );
    assert_eq!(
        orrery.graph().nodes().count(),
        before_nodes + 1,
        "a node was minted"
    );
    assert_eq!(
        orrery.graph().relations().count(),
        before_edges + 1,
        "a navigated-from edge links it back to the origin",
    );
    assert_eq!(
        orrery.focused_member(),
        Some(new_id),
        "the new node is focused"
    );
    // The new node opens on its URL — its own within-node history is seeded.
    let (new_key, _) = orrery.graph().get_node_by_id(new_id).unwrap();
    assert_eq!(
        orrery.graph().node_current_url(new_key).as_deref(),
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
fn add_node_at_mints_an_unlinked_node_at_the_cursor_world_point() {
    let mut orrery = Orrery::new();
    orrery.camera.offset = (100.0, 50.0);
    orrery.camera.zoom = 2.0;
    let before_nodes = orrery.graph().nodes().count();
    let before_edges = orrery.graph().relations().count();

    let id = orrery.add_node_at((300.0, 150.0), "mere://welcome");
    assert_eq!(
        orrery.graph().nodes().count(),
        before_nodes + 1,
        "a node was added"
    );
    assert_eq!(
        orrery.graph().relations().count(),
        before_edges,
        "the add-node is unlinked (no navigated-from edge)",
    );
    assert_eq!(
        orrery.focused_url(),
        Some("mere://welcome"),
        "the new node is selected"
    );

    // world = ((300-100)/2, (150-50)/2) = (100, 50); the seed is set before any
    // settle runs (settle is consumed by frame(), not called here).
    let (key, _) = orrery.graph().get_node_by_id(id).unwrap();
    let pos = orrery
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
    let mut orrery = Orrery::new();
    orrery.camera.offset = (100.0, 50.0);
    orrery.camera.zoom = 2.0;
    let before = orrery.graph().fields().count();
    let before_couplings = orrery.graph().couplings().count();

    let id = orrery.add_field_at((300.0, 150.0));
    assert_eq!(
        orrery.graph().fields().count(),
        before + 1,
        "a field was placed"
    );

    // world = ((300-100)/2, (150-50)/2) = (100, 50); a square Region centered there.
    let field = orrery
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
        orrery.graph().couplings().count(),
        before_couplings + 1,
        "a default gather coupling was placed with the field"
    );
    let coupling = orrery
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
    let mut orrery = Orrery::new();
    let _a = orrery.visit("https://a.test");
    let b = orrery.visit("https://b.test");
    let b_id = orrery.graph().get_node(b).unwrap().id;

    // Replace to a single selection, then additively toggle the second in.
    assert!(orrery.select_by_url("https://a.test"));
    assert_eq!(orrery.selected_members().len(), 1);
    assert!(
        orrery.toggle_select_member(b_id),
        "a known member toggles in"
    );
    assert_eq!(
        orrery.selected_members().len(),
        2,
        "the selection grew to the pair"
    );
    assert!(
        orrery.assert_selected_relation(SemanticSubKind::UserGrouped),
        "the two-node selection can be related",
    );

    // Toggling the same member again removes it; an unknown member is a no-op.
    assert!(orrery.toggle_select_member(b_id));
    assert_eq!(
        orrery.selected_members().len(),
        1,
        "toggling off shrinks back"
    );
    assert!(
        !orrery.toggle_select_member(uuid::Uuid::new_v4()),
        "unknown member → false"
    );
    assert_eq!(
        orrery.selected_members().len(),
        1,
        "and leaves the selection intact"
    );
}

#[test]
fn assert_selected_relation_links_exactly_two_selected_nodes() {
    let mut orrery = Orrery::new();
    // Two unlinked nodes (origin-less mints carry no edge).
    let a = orrery.open_member_as_new_node(None, "https://a.test");
    let b = orrery.open_member_as_new_node(None, "https://b.test");
    let edges_before = orrery.graph().relations().count();
    let ak = orrery.graph().get_node_by_id(a).unwrap().0;
    let bk = orrery.graph().get_node_by_id(b).unwrap().0;

    // A single-node selection is not a clean pair → no edge.
    orrery.selected.clear();
    orrery.selected.insert(ak);
    assert!(!orrery.assert_selected_relation(SemanticSubKind::UserGrouped));
    assert_eq!(orrery.graph().relations().count(), edges_before);

    // Both selected → one user-grouped semantic edge.
    orrery.selected.insert(bk);
    assert!(orrery.assert_selected_relation(SemanticSubKind::UserGrouped));
    assert_eq!(
        orrery.graph().relations().count(),
        edges_before + 1,
        "exactly one edge is created between the pair",
    );

    // Idempotent: re-asserting the same sub-kind succeeds but does not multiply
    // (the relation is already present, so the edge count is unchanged).
    assert!(orrery.assert_selected_relation(SemanticSubKind::UserGrouped));
    assert_eq!(orrery.graph().relations().count(), edges_before + 1);
}

#[test]
fn tag_selected_inserts_the_trimmed_tag_on_every_selected_node() {
    let mut orrery = Orrery::new();
    let a = orrery.open_member_as_new_node(None, "https://a.test");
    let b = orrery.open_member_as_new_node(None, "https://b.test");
    let ak = orrery.graph().get_node_by_id(a).unwrap().0;
    let bk = orrery.graph().get_node_by_id(b).unwrap().0;
    orrery.selected.clear();
    orrery.selected.insert(ak);
    orrery.selected.insert(bk);
    // Both selected nodes gain the (trimmed) tag; re-tagging adds nothing new.
    assert_eq!(
        orrery.tag_selected("  reading  "),
        2,
        "both nodes newly tagged"
    );
    assert_eq!(orrery.tag_selected("reading"), 0, "re-tag is idempotent");
    assert!(orrery.graph().node_tags(ak).unwrap().contains("reading"));
    assert!(orrery.graph().node_tags(bk).unwrap().contains("reading"));
    // An all-whitespace tag is a no-op.
    assert_eq!(orrery.tag_selected("   "), 0, "blank tag is ignored");
}

#[test]
fn recover_node_re_mints_with_restored_title_and_tags() {
    let mut orrery = Orrery::new();
    let before = orrery.graph().nodes().count();

    let id = orrery.recover_node(
        "https://recovered.test",
        Some("Recovered Page"),
        &["reading".to_string(), "archived".to_string()],
    );
    assert_eq!(
        orrery.graph().nodes().count(),
        before + 1,
        "a node was re-minted"
    );
    let (key, node) = orrery.graph().get_node_by_id(id).unwrap();
    assert_eq!(node.url(), "https://recovered.test", "the url is restored");
    assert_eq!(node.title, "Recovered Page", "the title is restored");
    let tags = orrery.graph().node_tags(key).unwrap();
    assert!(
        tags.contains("reading") && tags.contains("archived"),
        "both tags restored"
    );

    // A tombstone with no stored title still re-mints a node (its title is left to
    // the mint default / a later fetch, not forced empty).
    let id2 = orrery.recover_node("https://untitled.test", None, &[]);
    assert!(
        orrery.graph().get_node_by_id(id2).is_some(),
        "the untitled node re-mints"
    );
}

#[test]
fn retract_selected_relation_removes_the_user_relation() {
    let mut orrery = Orrery::new();
    let a = orrery.open_member_as_new_node(None, "https://a.test");
    let b = orrery.open_member_as_new_node(None, "https://b.test");
    let ak = orrery.graph().get_node_by_id(a).unwrap().0;
    let bk = orrery.graph().get_node_by_id(b).unwrap().0;
    orrery.selected.clear();
    orrery.selected.insert(ak);
    orrery.selected.insert(bk);
    orrery.assert_selected_relation(SemanticSubKind::UserGrouped);
    let with_edge = orrery.graph().relations().count();

    // The edge is stored in the asserted (uuid-sorted) direction; select it and
    // retract.
    let mut pair = [ak, bk];
    pair.sort_by_key(|k| orrery.graph().get_node(*k).map(|n| n.id));
    orrery.selected_edges.clear();
    orrery.selected_edges.insert(EdgeCell {
        from: pair[0],
        to: pair[1],
        selector: RelationSelector::Semantic(SemanticSubKind::UserGrouped),
    });
    assert_eq!(
        orrery.retract_selected_relation(),
        1,
        "the user relation is retracted"
    );
    assert_eq!(
        orrery.graph().relations().count(),
        with_edge - 1,
        "the edge is gone (no families left → garbage-collected)",
    );
    assert!(
        orrery.selected_edges.is_empty(),
        "retraction clears the edge selection"
    );
}

#[test]
fn retract_selected_edge_cell_removes_only_that_relation() {
    let mut orrery = Orrery::new();
    let a = orrery.open_member_as_new_node(None, "https://cell-a.test");
    let b = orrery.open_member_as_new_node(None, "https://cell-b.test");
    let ak = orrery.graph().get_node_by_id(a).unwrap().0;
    let bk = orrery.graph().get_node_by_id(b).unwrap().0;
    assert!(orrery.assert_relation_between_members(a, b, SemanticSubKind::Cites));
    assert!(orrery.assert_relation_between_members(a, b, SemanticSubKind::Quotes));

    orrery.selected_edges.insert(EdgeCell {
        from: ak,
        to: bk,
        selector: RelationSelector::Semantic(SemanticSubKind::Cites),
    });

    assert_eq!(orrery.retract_selected_relation(), 1);
    let remaining: Vec<_> = orrery.graph().relations().map(|r| r.kind).collect();
    assert_eq!(
        remaining,
        vec![RelationKind::Semantic(SemanticSubKind::Quotes)],
        "the other semantic cell survives the canvas-cell delete"
    );
}

#[test]
fn hide_selected_edge_cell_hides_only_that_relation() {
    let mut orrery = Orrery::new();
    let a = orrery.open_member_as_new_node(None, "https://hide-cell-a.test");
    let b = orrery.open_member_as_new_node(None, "https://hide-cell-b.test");
    let ak = orrery.graph().get_node_by_id(a).unwrap().0;
    let bk = orrery.graph().get_node_by_id(b).unwrap().0;
    assert!(orrery.assert_relation_between_members(a, b, SemanticSubKind::Cites));
    assert!(orrery.assert_relation_between_members(a, b, SemanticSubKind::Quotes));

    orrery.selected_edges.insert(EdgeCell {
        from: ak,
        to: bk,
        selector: RelationSelector::Semantic(SemanticSubKind::Cites),
    });

    assert_eq!(orrery.hide_selected_edges(), 1);
    assert!(orrery.relation_between_members_hidden(
        a,
        b,
        RelationSelector::Semantic(SemanticSubKind::Cites)
    ));
    assert!(!orrery.relation_between_members_hidden(
        a,
        b,
        RelationSelector::Semantic(SemanticSubKind::Quotes)
    ));
    assert!(
        !orrery.edge_between_members_hidden(a, b),
        "the endpoint bundle is not fully hidden while another cell remains visible"
    );
    let visible: Vec<_> = crate::edge_cells::visible_edge_cell_segments(
        orrery.graph(),
        &orrery.view,
        &orrery.hidden_edges,
    )
    .into_iter()
    .filter(|segment| segment.cell.endpoint_pair() == (ak.min(bk), ak.max(bk)))
    .collect();
    assert_eq!(visible.len(), 1, "only one relation lane remains visible");
    assert_eq!(
        visible[0].cell.selector,
        RelationSelector::Semantic(SemanticSubKind::Quotes)
    );
}

#[test]
fn unrelate_is_symmetric_with_relate_on_a_two_node_selection() {
    // `>unrelate` mirrors `>relate`: the same two-node selection that related a
    // pair also unrelates it, without having to click the edge.
    let mut orrery = Orrery::new();
    let a = orrery.open_member_as_new_node(None, "https://a.test");
    let b = orrery.open_member_as_new_node(None, "https://b.test");
    let ak = orrery.graph().get_node_by_id(a).unwrap().0;
    let bk = orrery.graph().get_node_by_id(b).unwrap().0;
    let base = orrery.graph().relations().count();
    orrery.selected.clear();
    orrery.selected.insert(ak);
    orrery.selected.insert(bk);

    orrery.assert_selected_relation(SemanticSubKind::UserGrouped);
    assert_eq!(orrery.graph().relations().count(), base + 1, "related");

    // No edge selected — the two-node selection alone retracts the relation.
    assert!(orrery.selected_edges.is_empty());
    assert_eq!(
        orrery.retract_selected_relation(),
        1,
        "the pair selection unrelates"
    );
    assert_eq!(
        orrery.graph().relations().count(),
        base,
        "back to no relation"
    );
}

#[test]
fn visit_dedups_by_url() {
    let mut orrery = Orrery::new();
    let a = orrery.visit("https://example.com");
    let _ = orrery.visit("https://other.com");
    let again = orrery.visit("https://example.com");
    assert_eq!(
        a, again,
        "revisiting a URL selects the existing node, not a duplicate"
    );
    assert_eq!(
        orrery.graph.nodes().count(),
        2,
        "no duplicate node is added"
    );
    assert!(
        orrery.selected.contains(&a),
        "selection returns to the revisited node"
    );
}

#[test]
fn focused_url_is_the_single_selected_nodes_url() {
    let mut orrery = Orrery::new();
    assert_eq!(
        orrery.focused_url(),
        None,
        "nothing selected yet → no focus"
    );
    orrery.visit("https://example.com");
    assert_eq!(
        orrery.focused_url(),
        Some("https://example.com"),
        "visit focuses the node"
    );
    orrery.visit("https://second.com");
    assert_eq!(
        orrery.focused_url(),
        Some("https://second.com"),
        "focus follows the new node"
    );
    orrery.visit("https://example.com");
    assert_eq!(
        orrery.focused_url(),
        Some("https://example.com"),
        "revisit re-focuses"
    );
}

#[test]
fn with_graph_restores_nodes_and_positions() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(100.0, 50.0),
    );
    graph.add_node(
        "https://two.example".to_string(),
        PortablePoint::new(-30.0, 80.0),
    );
    let orrery = Orrery::with_graph(graph);
    assert_eq!(
        orrery.graph().nodes().count(),
        2,
        "the restored graph keeps its nodes"
    );
    assert_eq!(
        orrery.view.len(),
        2,
        "a layout node per restored graph node"
    );
    assert!(
        !orrery.is_settling(),
        "a restored session does not auto-resettle"
    );
    let key = orrery
        .graph()
        .get_node_by_url("https://one.example")
        .unwrap()
        .0;
    let pos = orrery
        .view
        .position_of(key)
        .expect("a restored node position");
    assert!(
        (pos.x - 100.0).abs() < 1.0 && (pos.y - 50.0).abs() < 1.0,
        "the saved position is preserved (no spiral re-seed), got {pos:?}",
    );
}

#[test]
fn layout_strategy_overrides_node_positions_until_reverted() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut orrery = Orrery::with_graph(graph);
    let key = orrery
        .graph()
        .get_node_by_url("https://one.example")
        .unwrap()
        .0;
    assert_eq!(
        orrery.layout_strategy(),
        None,
        "force-directed (gyre) by default"
    );

    // Activate a strategy and push a position; the overlay (what `frame` runs after
    // the physics snapshot) writes it into the read model, winning over gyre.
    orrery.set_layout_strategy(Some("test.grid".to_string()));
    assert_eq!(orrery.layout_strategy(), Some("test.grid"));
    orrery.apply_strategy_positions(&[(key, PortablePoint::new(321.0, 654.0))]);
    orrery.apply_strategy_to_view();
    let pos = orrery.view.position_of(key).expect("node position");
    assert!(
        (pos.x - 321.0).abs() < 0.01 && (pos.y - 654.0).abs() < 0.01,
        "the strategy position overrides the physics position, got {pos:?}",
    );

    // Reverting drops the buffer and clears the strategy (gyre resumes).
    orrery.set_layout_strategy(None);
    assert_eq!(orrery.layout_strategy(), None, "revert clears the strategy");
}

#[test]
fn node_face_defaults_to_favicon_and_takes_a_per_node_override() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut orrery = Orrery::with_graph(graph);
    let (key, id) = {
        let (key, node) = orrery
            .graph()
            .get_node_by_url("https://one.example")
            .unwrap();
        (key, node.id)
    };
    assert_eq!(
        orrery.node_face(key),
        Face::Favicon,
        "the default face is Favicon, so the look is unchanged",
    );

    // A per-node override is the user's face choice; it wins over the default.
    orrery.set_node_face(id, Face::Bare);
    assert_eq!(
        orrery.node_face(key),
        Face::Bare,
        "the per-node override wins over the default"
    );

    // Clearing the override reverts the node to the default face.
    orrery.clear_node_face(id);
    assert_eq!(
        orrery.node_face(key),
        Face::Favicon,
        "clearing the override reverts to Favicon"
    );
}

#[test]
fn face_and_body_are_independent_axes() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut orrery = Orrery::with_graph(graph);
    let (key, id) = {
        let (key, node) = orrery
            .graph()
            .get_node_by_url("https://one.example")
            .unwrap();
        (key, node.id)
    };

    // Dropping an image gives the node a sprite face and a traced body hull (the import path
    // sets the two together).
    orrery.set_node_sprite(id, "data:image/png;base64,AAAA".to_string());
    orrery.set_node_sprite_hull(id, vec![(-0.4, -0.4), (0.4, -0.4), (0.0, 0.4)]);
    assert_eq!(orrery.node_sprite(key), Some("data:image/png;base64,AAAA"));
    assert_eq!(
        orrery.node_face(key),
        Face::Sprite,
        "a sprite node wears the Sprite face"
    );
    assert!(
        orrery.node_sprite_hull(key).is_some(),
        "and carries the traced body hull"
    );

    // DECOUPLE: switching the face back to Favicon keeps the body hull AND the sprite image —
    // face and body are independent axes (a custom-bodied node can wear a favicon).
    orrery.set_node_face(id, Face::Favicon);
    assert_eq!(orrery.node_face(key), Face::Favicon);
    assert!(
        orrery.node_sprite_hull(key).is_some(),
        "a face switch never reshapes the body"
    );
    assert_eq!(
        orrery.node_sprite(key),
        Some("data:image/png;base64,AAAA"),
        "a face switch never discards the imported sprite",
    );

    // Resetting the body drops the hull (back to the silhouette) but leaves the face alone.
    orrery.clear_node_body(id);
    assert!(
        orrery.node_sprite_hull(key).is_none(),
        "reset body drops the hull"
    );
    assert_eq!(
        orrery.node_face(key),
        Face::Favicon,
        "reset body leaves the face untouched"
    );

    // Removing the sprite drops the image and reverts a still-Sprite face to Favicon.
    orrery.set_node_face(id, Face::Sprite);
    orrery.clear_node_sprite(id);
    assert_eq!(
        orrery.node_sprite(key),
        None,
        "remove sprite drops the image"
    );
    assert_eq!(
        orrery.node_face(key),
        Face::Favicon,
        "and reverts a Sprite face to Favicon"
    );
}

#[test]
fn node_material_overrides_default_and_round_trips_through_cartography() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut orrery = Orrery::with_graph(graph);
    let (key, id) = {
        let (key, node) = orrery
            .graph()
            .get_node_by_url("https://one.example")
            .unwrap();
        (key, node.id)
    };

    // A node takes the default material until overridden.
    assert_eq!(orrery.node_material(key), NodeMaterial::default());

    // An override sets the body's physics (restitution / friction / density).
    orrery.set_node_material(
        id,
        NodeMaterial {
            restitution: 0.6,
            friction: 0.3,
            density: 0.002,
        },
    );
    assert_eq!(orrery.node_material(key).restitution, 0.6);
    assert_eq!(orrery.node_material(key).density, 0.002);

    // The override travels to the cartography sidecar as a (restitution, friction, density) tuple.
    let geom = orrery.cartography_geometry();
    let exported: std::collections::HashMap<_, _> = geom.material_iter().collect();
    assert_eq!(
        exported.get(&id),
        Some(&(0.6, 0.3, 0.002)),
        "the material is exported"
    );

    // Clearing reverts to default; re-applying from the sidecar restores it.
    orrery.clear_node_material(id);
    assert_eq!(
        orrery.node_material(key),
        NodeMaterial::default(),
        "cleared reverts to default"
    );
    orrery.apply_cartography_materials(geom.material_iter());
    assert_eq!(
        orrery.node_material(key).restitution,
        0.6,
        "the sidecar round-trips the material"
    );
}

#[test]
fn node_face_override_round_trips_through_cartography() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut orrery = Orrery::with_graph(graph);
    let (key, id) = {
        let (key, node) = orrery
            .graph()
            .get_node_by_url("https://one.example")
            .unwrap();
        (key, node.id)
    };

    // A per-node face override travels to the sidecar as a string code.
    orrery.set_node_face(id, Face::Bare);
    let geom = orrery.cartography_geometry();
    let faces: std::collections::HashMap<_, _> = geom.face_iter().collect();
    assert_eq!(
        faces.get(&id),
        Some(&"bare"),
        "the face override is exported"
    );

    // Clearing reverts to the default; re-applying from the sidecar restores it.
    orrery.clear_node_face(id);
    assert_eq!(
        orrery.node_face(key),
        Face::Favicon,
        "cleared reverts to the default face"
    );
    orrery.apply_cartography_faces(geom.face_iter());
    assert_eq!(
        orrery.node_face(key),
        Face::Bare,
        "the sidecar round-trips the face"
    );
}

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
    let mut orrery = Orrery::with_graph(graph);
    let hk = orrery
        .graph()
        .get_node_by_url("https://hub.example")
        .unwrap()
        .0;
    let ak = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let aid = orrery.graph().get_node(ak).unwrap().id;

    // Off by default: every node is the uniform footprint.
    assert_eq!(
        orrery.node_size(hk),
        36.0,
        "uniform until size-by-importance is on"
    );

    // On: the most-connected node hits the cap (88), a 0.5-importance leaf is 36 + 0.5*52 = 62.
    orrery.set_size_by_importance(true);
    assert_eq!(
        orrery.node_size(hk),
        88.0,
        "the most important node hits the cap"
    );
    assert!(
        (orrery.node_size(ak) - 62.0).abs() < 0.1,
        "a leaf scales by its 0.5 importance"
    );

    // Precedence: a manual override still wins over the importance size.
    orrery.set_node_size(aid, 100.0);
    assert_eq!(
        orrery.node_size(ak),
        100.0,
        "a manual override beats the importance size"
    );

    // Turning it off reverts to the uniform footprint (the hub has no manual override).
    orrery.set_size_by_importance(false);
    assert_eq!(orrery.node_size(hk), 36.0, "off => uniform again");
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
    let mut orrery = Orrery::with_graph(graph);
    orrery.set_size_by_importance(true);
    let bk = orrery
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;
    assert_eq!(
        orrery.node_size(bk),
        88.0,
        "both nodes degree 1 => b is at the cap"
    );

    // Add c linked to a: a becomes degree 2 (importance 1.0), b stays degree 1 (importance 0.5).
    // The cache must invalidate on the topology change, so b drops from 88 to 62 (36 + 0.5*52);
    // a stale cache would leave b at 88.
    orrery.ingest_graph(|g| {
        let c = g.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        let a = g.get_node_by_url("https://a.example").unwrap().0;
        g.assert_semantic_predicate(a, c, "links".to_string());
        true
    });
    let bk = orrery
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;
    assert!(
        (orrery.node_size(bk) - 62.0).abs() < 0.1,
        "after the edge add, b's importance recomputed (the cache invalidated): {}",
        orrery.node_size(bk),
    );
}

#[test]
fn size_by_importance_restore_recomputes_on_a_reused_orrery() {
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
    let mut orrery = Orrery::with_graph(graph);
    let ak = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;

    // Cycle the mode so the cache ends clean + empty with the mode OFF — the state a *reused*
    // orrery is in at a session switch, just before the sidecar restore.
    orrery.set_size_by_importance(true);
    orrery.set_size_by_importance(false);
    assert_eq!(orrery.node_size(ak), 36.0, "off => default size");

    // The restore turns it back on: it must force a recompute, not leave the cache empty (the
    // bug the review caught). Without the fix, the node would stay at the default 36.
    orrery.apply_cartography_sizing(Vec::<(uuid::Uuid, f32)>::new(), false, true);
    assert_eq!(
        orrery.node_size(ak),
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
    let mut orrery = Orrery::with_graph(graph);
    let k0 = orrery
        .graph()
        .get_node_by_url("https://0.example")
        .unwrap()
        .0;
    let k2 = orrery
        .graph()
        .get_node_by_url("https://2.example")
        .unwrap()
        .0;
    orrery.set_size_by_importance(true);

    // Degree (default): the bridge (degree 4) is the max; a peripheral (degree 2) is mid (62).
    assert_eq!(
        orrery.node_size(k2),
        88.0,
        "the bridge is the max under degree"
    );
    assert!(
        (orrery.node_size(k0) - 62.0).abs() < 0.1,
        "a peripheral is mid-sized under degree"
    );

    // Betweenness: the bridge stays max; the peripheral lies on no cross-paths => ~0 => default.
    orrery.set_importance_metric(ImportanceMetric::Betweenness);
    assert_eq!(
        orrery.node_size(k2),
        88.0,
        "the bridge is still the max under betweenness"
    );
    assert!(
        (orrery.node_size(k0) - 36.0).abs() < 0.5,
        "a peripheral shrinks to ~default under betweenness"
    );
}

#[test]
fn community_cache_fills_and_invalidates_on_topology_change() {
    // Two triangles {0,1,2} and {3,4,5} joined by a bridge => two Louvain communities.
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
    let mut orrery = Orrery::with_graph(graph);

    // No partition until a cluster strategy refreshes it; a non-cluster strategy is a no-op.
    assert!(orrery.community().is_none(), "no partition computed yet");
    orrery.refresh_community_cache("phyllotaxis.default");
    assert!(
        orrery.community().is_none(),
        "a non-cluster strategy does not compute community"
    );

    // Cluster-kanban fills the generation-gated cache: two communities.
    orrery.refresh_community_cache("kanban.community");
    assert_eq!(
        orrery.community().unwrap().clusters.len(),
        2,
        "two triangles => two communities"
    );

    // A topology change bumps the generation; the next refresh recomputes against the new graph,
    // so the added node appears in the partition (the cache invalidated, not stale).
    orrery.ingest_graph(|g| {
        let extra = g.add_node(
            "https://6.example".to_string(),
            PortablePoint::new(6.0, 0.0),
        );
        let a = g.get_node_by_url("https://0.example").unwrap().0;
        g.assert_semantic_predicate(a, extra, "links".to_string());
        true
    });
    orrery.refresh_community_cache("kanban.community");
    let total_members: usize = orrery
        .community()
        .unwrap()
        .clusters
        .iter()
        .map(|c| c.members.len())
        .sum();
    assert_eq!(
        total_members, 7,
        "the new node joined the recomputed partition"
    );
}

#[test]
fn cluster_by_affinity_installs_and_clears_the_affinity_force() {
    // Two triangles joined by a bridge: structural affinity is high within each triangle, so the
    // signal yields pairs. The toggle should install the force on the next frame and clear it on
    // the frame after the toggle goes off. (Graph signals — P4.)
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
    let mut orrery = Orrery::with_graph(graph);

    // Off by default: no force even after a frame.
    let _ = orrery.frame(800, 600);
    assert_eq!(orrery.affinity_pair_count(), 0, "off => no affinity force");

    // Toggle on: the next frame installs the force with the clustered pairs.
    orrery.set_cluster_by_affinity(true);
    assert!(orrery.cluster_by_affinity(), "the toggle reads on");
    let _ = orrery.frame(800, 600);
    assert!(
        orrery.affinity_pair_count() > 0,
        "the affinity force is installed with the signal's pairs"
    );

    // Toggle off: the next frame clears it.
    orrery.set_cluster_by_affinity(false);
    let _ = orrery.frame(800, 600);
    assert_eq!(
        orrery.affinity_pair_count(),
        0,
        "off => the force is cleared"
    );
}

#[test]
fn affinity_force_refreshes_on_a_topology_change() {
    // With clustering on, a structural mutation must refresh the installed signal (the cache is
    // revision-gated, not frozen at first compute). An edgeless start has no affinity pairs; wiring
    // a triangle in creates them. (Graph signals — P4.)
    let mut graph = Graph::new();
    for i in 0..3 {
        graph.add_node(
            format!("https://{i}.example"),
            PortablePoint::new(i as f32, 0.0),
        );
    }
    let mut orrery = Orrery::with_graph(graph);
    orrery.set_cluster_by_affinity(true);
    let _ = orrery.frame(800, 600);
    assert_eq!(
        orrery.affinity_pair_count(),
        0,
        "an edgeless graph has no affinity pairs"
    );

    // Wire the three nodes into a triangle: every pair now shares the third node.
    orrery.ingest_graph(|g| {
        let a = g.get_node_by_url("https://0.example").unwrap().0;
        let b = g.get_node_by_url("https://1.example").unwrap().0;
        let c = g.get_node_by_url("https://2.example").unwrap().0;
        g.assert_semantic_predicate(a, b, "links".to_string());
        g.assert_semantic_predicate(b, c, "links".to_string());
        g.assert_semantic_predicate(c, a, "links".to_string());
        true
    });
    let _ = orrery.frame(800, 600);
    assert_eq!(
        orrery.affinity_pair_count(),
        3,
        "the triangle's three pairs refresh into the live force"
    );
}

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
    let mut orrery = Orrery::with_graph(graph);
    let ak = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let bk = orrery
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;

    // First time there is no recorded layout, so a recompute is needed; after noting it, the same
    // inputs skip (an analytic layout is computed once, not per frame).
    assert!(
        orrery.needs_strategy_recompute("grid.default", 800, 600, None),
        "first compute"
    );
    orrery.note_strategy_computed("grid.default", 800, 600, None);
    assert!(
        !orrery.needs_strategy_recompute("grid.default", 800, 600, None),
        "unchanged => skip"
    );

    // A viewport change re-triggers.
    assert!(
        orrery.needs_strategy_recompute("grid.default", 1024, 600, None),
        "viewport change"
    );

    // A structural change (the kernel revision moves) re-triggers.
    orrery.ingest_graph(|g| {
        g.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        true
    });
    assert!(
        orrery.needs_strategy_recompute("grid.default", 800, 600, None),
        "revision moved"
    );

    // A non-focus strategy ignores the focus, so a selection change does not invalidate it.
    orrery.note_strategy_computed("grid.default", 800, 600, None);
    assert!(
        !orrery.needs_strategy_recompute("grid.default", 800, 600, Some(ak)),
        "grid ignores focus, so a selection change does not force a recompute"
    );

    // Radial is focus-driven, so a focus change DOES re-trigger it.
    orrery.note_strategy_computed("radial.default", 800, 600, Some(ak));
    assert!(
        orrery.needs_strategy_recompute("radial.default", 800, 600, Some(bk)),
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
    let mut orrery = Orrery::with_graph(graph);
    assert!(
        orrery.community().is_none(),
        "no partition until a consumer asks for it"
    );

    // Turning the rings on makes the frame compute the partition and run the ring paint path.
    orrery.set_show_community_rings(true);
    let _ = orrery.frame(800, 600);
    assert!(orrery.show_community_rings(), "the toggle is on");
    assert_eq!(
        orrery.community().map(|c| c.clusters.len()),
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
    let mut orrery = Orrery::with_graph(graph);
    let k2 = orrery
        .graph()
        .get_node_by_url("https://2.example")
        .unwrap()
        .0;
    assert!(
        orrery.bridges().is_none(),
        "no bridges until the toggle asks for them"
    );

    orrery.set_show_bridge_rings(true);
    let _ = orrery.frame(800, 600);
    assert!(orrery.show_bridge_rings(), "the toggle is on");
    assert_eq!(
        orrery.bridges().unwrap().bridges,
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
    let mut orrery = Orrery::with_graph(graph);
    orrery.set_show_bridge_rings(true);

    // Default metric (betweenness): every cycle node is a tied broker.
    let _ = orrery.frame(800, 600);
    assert_eq!(
        orrery.bridges().unwrap().bridges.len(),
        4,
        "all four cycle nodes are tied betweenness brokers"
    );

    // Switching to articulation invalidates the cache; the cycle has no cut vertex.
    orrery.set_bridge_metric(signals::BridgeMetric::Articulation);
    let _ = orrery.frame(800, 600);
    assert!(
        orrery.bridges().unwrap().bridges.is_empty(),
        "a 2-connected cycle has no articulation point"
    );
    assert_eq!(orrery.bridge_metric(), signals::BridgeMetric::Articulation);
}

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
    let mut orrery = Orrery::with_graph(graph);
    let ak = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let bk = orrery
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;

    // No gloss strategy => mirror the main view, no recompute needed.
    assert!(orrery.gloss_strategy().is_none());
    assert!(
        !orrery.gloss_needs_recompute(200, 200),
        "mirroring needs no recompute"
    );

    // Setting a gloss lens asks for a recompute; the host supplies positions, which assemble into
    // the gloss geometry independent of the live layout (here, custom positions a force-directed
    // layout would never produce).
    orrery.set_gloss_strategy(Some("spectral.default".to_string()));
    assert!(
        orrery.gloss_needs_recompute(200, 200),
        "a fresh lens needs computing"
    );
    orrery.set_gloss_positions(
        vec![
            (ak, PortablePoint::new(10.0, 20.0)),
            (bk, PortablePoint::new(30.0, 40.0)),
        ],
        Vec::new(),
        200,
        200,
    );
    assert!(
        !orrery.gloss_needs_recompute(200, 200),
        "unchanged inputs => cached"
    );
    let (nodes, _edges, _rings) = orrery.gloss_geometry_cached();
    assert_eq!(nodes.len(), 2, "both positioned nodes appear");
    // The independent-lens property: the gloss node sits at the SUPPLIED position (10, 20), not
    // wherever the live force-directed layout would have put it.
    let a_id = orrery.graph().get_node(ak).unwrap().id;
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
        orrery.gloss_needs_recompute(300, 200),
        "a viewport change re-triggers"
    );
    orrery.ingest_graph(|g| {
        g.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        true
    });
    assert!(
        orrery.gloss_needs_recompute(200, 200),
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
    let mut orrery = Orrery::with_graph(graph);
    let ak = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let bk = orrery
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;

    orrery.set_gloss_strategy(Some("spectral.default".to_string()));
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
    orrery.set_gloss_positions(
        vec![
            (ak, PortablePoint::new(10.0, 20.0)),
            (bk, PortablePoint::new(30.0, 40.0)),
        ],
        overlays,
        200,
        200,
    );
    let (_nodes, _edges, rings) = orrery.gloss_geometry_cached();
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
    let mut orrery = Orrery::with_graph(graph);
    let ak = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let bk = orrery
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;
    let ck = orrery
        .graph()
        .get_node_by_url("https://c.example")
        .unwrap()
        .0;

    orrery.set_gloss_strategy(Some("spectral.default".to_string()));
    orrery.set_gloss_positions(
        vec![
            (ak, PortablePoint::new(0.0, 0.0)),
            (bk, PortablePoint::new(10.0, 0.0)),
            (ck, PortablePoint::new(20.0, 0.0)),
        ],
        Vec::new(),
        200,
        200,
    );
    let (nodes, _e, _r) = orrery.gloss_geometry_cached();
    assert_eq!(nodes.len(), 3, "unscoped gloss shows the whole graph");

    orrery.select_by_url("https://a.example");
    orrery.set_gloss_scope_selection(true);
    let (nodes, edges, _r) = orrery.gloss_geometry_cached();
    assert_eq!(nodes.len(), 1, "scoped gloss shows only the selected node");
    assert_eq!(
        edges.len(),
        0,
        "no induced edge (a's neighbour b is out of scope)"
    );

    orrery.set_gloss_scope_selection(false);
    let (nodes, _e, _r) = orrery.gloss_geometry_cached();
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
    let mut orrery = Orrery::with_graph(graph);
    let ak = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;

    orrery.set_gloss_strategy(Some("spectral.default".to_string()));
    assert_eq!(
        orrery.gloss_scope_keys(),
        None,
        "no scope until selection-scoping is on"
    );
    orrery.set_gloss_positions(
        vec![(ak, PortablePoint::new(0.0, 0.0))],
        Vec::new(),
        200,
        200,
    );
    assert!(
        !orrery.gloss_needs_recompute(200, 200),
        "unchanged inputs => cached"
    );

    // Selecting + scoping changes the scope key, so the lens must recompute (subgraph re-layout).
    orrery.select_by_url("https://a.example");
    orrery.set_gloss_scope_selection(true);
    assert_eq!(
        orrery.gloss_scope_keys(),
        Some(vec![ak]),
        "the scope is the sorted selection"
    );
    assert!(
        orrery.gloss_needs_recompute(200, 200),
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
    let mut orrery = Orrery::with_graph(graph);
    let ak = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let bk = orrery
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;
    let ck = orrery
        .graph()
        .get_node_by_url("https://c.example")
        .unwrap()
        .0;

    orrery.set_gloss_strategy(Some("spectral.default".to_string()));
    orrery.set_gloss_positions(
        vec![
            (ak, PortablePoint::new(0.0, 0.0)),
            (bk, PortablePoint::new(10.0, 0.0)),
            (ck, PortablePoint::new(20.0, 0.0)),
        ],
        Vec::new(),
        200,
        200,
    );
    let (_n, edges, _r) = orrery.gloss_geometry_cached();
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
    let mut orrery = Orrery::with_graph(graph);

    let _ = orrery.frame(800, 600);
    let after_first = orrery.weighted_edges_rebuilds();
    assert!(after_first >= 1, "the first frame builds the memo");
    let _ = orrery.frame(800, 600);
    assert_eq!(
        orrery.weighted_edges_rebuilds(),
        after_first,
        "a static frame reuses the cached edge topology"
    );

    // A structural change bumps the kernel revision, so the next frame refreshes the memo.
    orrery.ingest_graph(|g| {
        g.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        true
    });
    let _ = orrery.frame(800, 600);
    assert!(
        orrery.weighted_edges_rebuilds() > after_first,
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
    let mut orrery = Orrery::with_graph(graph);
    let hubk = orrery
        .graph()
        .get_node_by_url("https://hub.example")
        .unwrap()
        .0;
    let l1k = orrery
        .graph()
        .get_node_by_url("https://l1.example")
        .unwrap()
        .0;
    let l2k = orrery
        .graph()
        .get_node_by_url("https://l2.example")
        .unwrap()
        .0;
    let hub_id = orrery.graph().get_node(hubk).unwrap().id;
    let l1_id = orrery.graph().get_node(l1k).unwrap().id;

    orrery.set_gloss_strategy(Some("spectral.default".to_string()));
    orrery.set_gloss_positions(
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
    let (nodes, _e, _r) = orrery.gloss_geometry_cached();
    assert!(
        nodes.iter().all(|(_, _, _, f)| (*f - 1.0).abs() < 1e-6),
        "uniform size when the encoding is off"
    );

    // On: a frame recomputes importance; the hub's factor exceeds a leaf's.
    orrery.set_gloss_size_by_importance(true);
    let _ = orrery.frame(200, 200);
    let (nodes, _e, _r) = orrery.gloss_geometry_cached();
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
    let mut orrery = Orrery::with_graph(graph);
    let hubk = orrery
        .graph()
        .get_node_by_url("https://hub.example")
        .unwrap()
        .0;
    let l1k = orrery
        .graph()
        .get_node_by_url("https://l1.example")
        .unwrap()
        .0;
    let l2k = orrery
        .graph()
        .get_node_by_url("https://l2.example")
        .unwrap()
        .0;
    let hub_id = orrery.graph().get_node(hubk).unwrap().id;
    let l1_id = orrery.graph().get_node(l1k).unwrap().id;

    // Main-view size-by-importance on, then off (clears the importance cache) — no structural change.
    orrery.set_size_by_importance(true);
    orrery.set_size_by_importance(false);

    orrery.set_gloss_strategy(Some("spectral.default".to_string()));
    orrery.set_gloss_positions(
        vec![
            (hubk, PortablePoint::new(0.0, 0.0)),
            (l1k, PortablePoint::new(10.0, 0.0)),
            (l2k, PortablePoint::new(20.0, 0.0)),
        ],
        Vec::new(),
        200,
        200,
    );
    orrery.set_gloss_size_by_importance(true);
    let _ = orrery.frame(200, 200);
    let (nodes, _e, _r) = orrery.gloss_geometry_cached();
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
    let mut orrery = Orrery::with_graph(graph);
    let ak = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    orrery.set_gloss_strategy(Some("kanban.default".to_string()));
    orrery.set_gloss_positions(
        vec![(ak, PortablePoint::new(0.0, 0.0))],
        Vec::new(),
        200,
        200,
    );
    assert!(
        orrery.gloss_needs_recompute(200, 200),
        "the by-site kanban gloss lens recomputes every frame (URL-host content the revision misses)"
    );
}

#[test]
fn isolate_selection_scopes_the_orrery() {
    let mut graph = Graph::new();
    let a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(50.0, 0.0),
    );
    let mut orrery = Orrery::with_graph(graph);
    assert!(!orrery.is_scoped(), "the whole graph shows by default");

    // No selection: isolate is a no-op.
    orrery.isolate_selection();
    assert!(
        !orrery.is_scoped(),
        "isolating with no selection does nothing"
    );

    // With a selection, isolate scopes the orrery to (at least) the selected node.
    orrery.selected.insert(a);
    orrery.isolate_selection();
    assert!(
        orrery.is_scoped(),
        "isolating a selection scopes the orrery"
    );
    assert!(
        orrery.scope.as_ref().unwrap().contains(&a),
        "the scope includes the selected node",
    );

    orrery.clear_scope();
    assert!(!orrery.is_scoped(), "show all clears the scope");
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
    let mut orrery = Orrery::with_graph(g);
    let a_id = orrery.graph().get_node(a).unwrap().id;
    assert!(!orrery.is_scoped(), "the whole graph by default");

    orrery.scope_to_members([a_id]);
    assert!(
        orrery.is_scoped(),
        "scoping to a member set (the workbench tiles) scopes the orrery"
    );
    assert!(
        orrery.scope.as_ref().unwrap().contains(&a),
        "the scope holds the member's key"
    );

    orrery.scope_to_members(std::iter::empty());
    assert!(
        !orrery.is_scoped(),
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
    let mut orrery = Orrery::with_graph(g);
    let a_id = orrery.graph().get_node(a).unwrap().id;
    orrery
        .view
        .set_position(a, euclid::default::Point2D::new(111.0, 222.0));
    let geom = orrery.cartography_geometry();
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
    // re-apply onto a fresh orrery over the same node id (a reload). (Node-rep — persistence.)
    let a_id = uuid::Uuid::from_u128(0xa);
    let mut g = Graph::new();
    g.add_node_with_id(
        a_id,
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut orrery = Orrery::with_graph(g);
    orrery.set_node_size(a_id, 80.0);
    orrery.set_size_by_degree(true);
    orrery.set_size_by_importance(true);
    orrery.set_importance_metric(ImportanceMetric::Betweenness);

    let geom = orrery.cartography_geometry();
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

    // Re-apply onto a fresh orrery whose graph carries the same node id (the reload). The metric
    // restore runs first, so the sizing restore recomputes with it.
    let mut g2 = Graph::new();
    g2.add_node_with_id(
        a_id,
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut reloaded = Orrery::with_graph(g2);
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
    let mut orrery = Orrery::with_graph(g);
    let a_id = orrery.graph().get_node(a).unwrap().id;
    orrery.seed_cartography([(a_id, (321.0, 654.0))]);
    let pos = orrery.view.position_of(a).expect("a position");
    assert!(
        (pos.x - 321.0).abs() < 0.01 && (pos.y - 654.0).abs() < 0.01,
        "seed_cartography overrides the load-time seed, got {pos:?}",
    );
}

#[test]
fn park_physics_halts_an_in_progress_settle() {
    let mut orrery = Orrery::with_sample_graph();
    assert!(
        orrery.is_settling(),
        "the sample graph settles its initial spiral"
    );
    orrery.park_physics();
    assert!(
        !orrery.is_settling(),
        "park halts the settle so a backgrounded graph stops ticking and waking the loop",
    );
}

#[test]
fn camera_round_trips_and_guards_bad_zoom() {
    let mut orrery = Orrery::new();
    orrery.set_camera(CameraView {
        offset: (123.0, -45.0),
        zoom: 2.5,
    });
    let cv = orrery.camera();
    assert_eq!(cv.offset, (123.0, -45.0));
    assert_eq!(cv.zoom, 2.5);
    // A zero / non-finite zoom falls back to 1.0 rather than collapsing.
    orrery.set_camera(CameraView {
        offset: (0.0, 0.0),
        zoom: 0.0,
    });
    assert_eq!(orrery.camera().zoom, 1.0);
}

#[test]
fn resize_keeps_the_viewport_center_world_point_fixed() {
    let mut orrery = Orrery::new();
    // A non-trivial pan + zoom at the starting viewport.
    orrery.camera.offset = (512.0, 300.0);
    orrery.camera.zoom = 1.5;
    let center_world =
        |o: &Orrery| o.screen_to_world((o.view_w as f32 / 2.0, o.view_h as f32 / 2.0));
    let before = center_world(&orrery);
    // Grow the surface the way startup does (1024x600 -> 2560x1504).
    orrery.resize(2560, 1504);
    let after = center_world(&orrery);
    assert!(
        (after.x - before.x).abs() < 0.01,
        "center world x fixed across resize"
    );
    assert!(
        (after.y - before.y).abs() < 0.01,
        "center world y fixed across resize"
    );
    assert_eq!(orrery.camera.zoom, 1.5, "resize leaves zoom untouched");
}

#[test]
fn has_nodes_tracks_the_graph() {
    let mut orrery = Orrery::new();
    assert!(!orrery.has_nodes(), "a fresh empty session has no nodes");
    orrery.visit("mere://welcome");
    assert!(
        orrery.has_nodes(),
        "a visited node makes the graph non-empty"
    );
}

#[test]
fn select_by_url_selects_existing_or_reports_missing() {
    let mut orrery = Orrery::new();
    orrery.visit("https://one.example");
    orrery.visit("https://two.example"); // focus moves to two
    assert!(
        orrery.select_by_url("https://one.example"),
        "an existing url is found + focused"
    );
    assert_eq!(orrery.focused_url(), Some("https://one.example"));
    assert!(
        !orrery.select_by_url("https://absent.example"),
        "a missing url reports false"
    );
    assert_eq!(
        orrery.focused_url(),
        Some("https://one.example"),
        "a miss leaves selection intact"
    );
}

#[test]
fn remove_focused_drops_the_node_and_reports_its_id() {
    let mut orrery = Orrery::new();
    orrery.visit("https://a.example");
    orrery.visit("https://b.example"); // focus on b
    let before = orrery.graph().nodes().count();
    let removed = orrery.remove_focused();
    assert!(
        removed.is_some(),
        "the focused node is removed and its id returned"
    );
    assert_eq!(
        orrery.graph().nodes().count(),
        before - 1,
        "the graph shrinks by one"
    );
    assert!(
        orrery
            .graph()
            .get_node_by_url("https://b.example")
            .is_none(),
        "the node is gone"
    );
    assert_eq!(orrery.focused_url(), None, "the selection clears");
    assert_eq!(
        orrery.view.len(),
        before - 1,
        "the removed node is reconciled out of the layout"
    );
    // Nothing focused → a second remove is a no-op.
    assert!(orrery.remove_focused().is_none(), "no focus → no removal");
}

#[test]
fn hide_selected_edges_then_show_all_round_trips() {
    let mut orrery = Orrery::new();
    orrery.visit("https://a.example");
    orrery.visit("https://b.example"); // the browse trail links a — b
    let a = orrery
        .graph()
        .get_node_by_url("https://a.example")
        .unwrap()
        .0;
    let b = orrery
        .graph()
        .get_node_by_url("https://b.example")
        .unwrap()
        .0;
    orrery
        .selected_edges
        .insert(first_edge_cell_between(&orrery, a, b)); // the host edge-picks this normally

    assert_eq!(
        orrery.hide_selected_edges(),
        1,
        "the selected edge is hidden"
    );
    assert!(
        orrery.selected_edges.is_empty(),
        "hiding clears the selection"
    );
    assert_eq!(orrery.hidden_edges.len(), 1);
    // The relation itself survives (hiding is display-only).
    assert!(
        orrery.graph().relations().count() >= 1,
        "the relation is not deleted"
    );

    assert_eq!(
        orrery.show_all_edges(),
        1,
        "show-all reveals the hidden edge"
    );
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
    assert_eq!(
        orrery.node_states.get(&key),
        Some(&NodeState::Open),
        "uuid resolves to its key"
    );

    // An unknown node id is filtered out (no panic, no stale entry).
    let mut other = HashMap::new();
    other.insert(uuid::Uuid::from_u128(0xdead_beef), NodeState::Closed);
    orrery.set_node_states(other);
    assert!(
        orrery.node_states.is_empty(),
        "an unknown node id is dropped"
    );
}

#[test]
fn set_node_shapes_resolves_uuids_to_keys() {
    let mut orrery = Orrery::new();
    orrery.visit("https://a.example");
    let (key, id) = {
        let (k, n) = orrery.graph().get_node_by_url("https://a.example").unwrap();
        (k, n.id)
    };
    let mut shapes = HashMap::new();
    shapes.insert(id, NodeShape::Circle);
    orrery.set_node_shapes(shapes);
    assert_eq!(
        orrery.node_shapes.get(&key),
        Some(&NodeShape::Circle),
        "uuid resolves to its key"
    );

    // An unknown node id is filtered out (no panic, no stale entry).
    let mut other = HashMap::new();
    other.insert(uuid::Uuid::from_u128(0xdead_beef), NodeShape::Rounded);
    orrery.set_node_shapes(other);
    assert!(
        orrery.node_shapes.is_empty(),
        "an unknown node id is dropped"
    );
}

#[test]
fn connected_members_reaches_the_whole_trail() {
    let mut orrery = Orrery::new();
    let a = orrery.visit("https://a.example");
    orrery.visit("https://b.example"); // a — b (browse trail)
    orrery.visit("https://c.example"); // b — c
    let a_id = orrery.graph().get_node(a).unwrap().id;
    let comp = orrery.connected_members(a_id);
    assert_eq!(
        comp.len(),
        3,
        "the a — b — c trail is one connected component"
    );
    assert_eq!(comp.first(), Some(&a_id), "BFS leads with the queried node");
    assert!(
        orrery
            .connected_members(uuid::Uuid::from_u128(0xabcd))
            .is_empty(),
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
    let mut orrery = Orrery::with_graph(g);
    let (key, _) = orrery.graph().get_node_by_id(a_id).unwrap();

    // The default size (36) reads as tier 1, the second notch.
    assert_eq!(orrery.node_size_tier(key), 1);
    // Step up to tier 2 — the override snaps onto the ladder at that preset.
    assert_eq!(orrery.step_node_size_tier(a_id, 1), 2);
    assert_eq!(orrery.node_size(key), crate::SIZE_TIERS[2]);
    // Step down past the bottom clamps at tier 0 (no underflow).
    assert_eq!(orrery.step_node_size_tier(a_id, -5), 0);
    assert_eq!(orrery.node_size(key), crate::SIZE_TIERS[0]);
}

#[test]
fn node_size_drives_the_pick_radius() {
    // Decision 5: a node sized in the view picks (and collides) at its true face,
    // not the uniform default — so a grown node is grabbable across its whole face.
    let mut orrery = Orrery::new();
    let id = orrery.add_node_at((0.0, 0.0), "mere://big");
    let (key, _) = orrery.graph().get_node_by_id(id).unwrap();
    let center = orrery
        .view
        .position_of(key)
        .expect("the new node has a position");

    // 30px from the center misses at the default 36px footprint (radius 18)...
    let probe = euclid::default::Point2D::new(center.x + 30.0, center.y);
    assert!(
        orrery.view.hit_test(probe).is_none(),
        "default footprint shouldn't reach 30px out"
    );

    // ...but an 80px face (radius 40) reaches it.
    orrery.set_node_size(id, 80.0);
    assert_eq!(
        orrery.view.hit_test(probe),
        Some(key),
        "the grown face grabs from 30px out"
    );

    // Clearing the override reverts the pick radius to the default.
    orrery.clear_node_size(id);
    assert!(
        orrery.view.hit_test(probe).is_none(),
        "cleared footprint reverts to the default"
    );
}

#[test]
fn alt_left_drag_orbits_the_camera() {
    // The deferred iso-camera orbit gesture: with Alt held, a left-drag yaws + reclines the camera
    // instead of picking / marqueeing. (Isometric camera — orbit gesture.)
    let mut orrery = Orrery::new();
    let (yaw0, tilt0) = (orrery.yaw(), orrery.tilt());

    orrery.set_alt(true);
    orrery.pointer_down(PointerButton::Left, 400.0, 300.0);
    orrery.cursor_moved(500.0, 360.0); // drag right (yaw) + down (recline tilt)
    assert!(orrery.yaw() != yaw0, "horizontal Alt+drag yawed the camera");
    assert!(orrery.tilt() < tilt0, "downward Alt+drag reclined the tilt");

    // Releasing ends the orbit: a later move (Alt still held, no press) must not keep orbiting.
    orrery.pointer_up(PointerButton::Left, 500.0, 360.0);
    let (yaw1, tilt1) = (orrery.yaw(), orrery.tilt());
    orrery.cursor_moved(600.0, 420.0);
    assert_eq!(
        (orrery.yaw(), orrery.tilt()),
        (yaw1, tilt1),
        "no orbit after the drag ends"
    );

    // And a plain (no-Alt) left drag does not orbit — it stays the pick / marquee gesture.
    orrery.set_alt(false);
    let yaw2 = orrery.yaw();
    orrery.pointer_down(PointerButton::Left, 100.0, 100.0);
    orrery.cursor_moved(220.0, 160.0);
    assert_eq!(orrery.yaw(), yaw2, "a non-Alt left drag does not orbit");
}
