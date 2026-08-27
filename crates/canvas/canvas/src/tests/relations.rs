// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Relation cells: picking a fanned parallel cell, and retracting / hiding one
//! cell without disturbing its neighbours in the same bundle.

use super::*;

#[test]
fn edge_cell_hit_test_picks_fanned_parallel_relation_cells() {
    let mut canvas = Canvas::new();
    let a = canvas.open_member_as_new_node(None, "https://fan-a.test");
    let b = canvas.open_member_as_new_node(None, "https://fan-b.test");
    let ak = canvas.graph().get_node_by_id(a).unwrap().0;
    let bk = canvas.graph().get_node_by_id(b).unwrap().0;
    assert!(canvas.assert_relation_between_members(a, b, SemanticSubKind::Cites));
    assert!(canvas.assert_relation_between_members(a, b, SemanticSubKind::Quotes));
    canvas
        .view
        .set_position(ak, euclid::default::Point2D::new(0.0, 0.0));
    canvas
        .view
        .set_position(bk, euclid::default::Point2D::new(100.0, 0.0));

    let pair = if ak <= bk { (ak, bk) } else { (bk, ak) };
    let segments: Vec<_> = crate::edge_cells::visible_edge_cell_segments(
        canvas.graph(),
        &canvas.view,
        &canvas.hidden_edges,
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
                canvas.graph(),
                &canvas.view,
                &canvas.hidden_edges,
                midpoint,
                1.0,
            ),
            Some(segment.cell),
            "the fanned lane picks its own relation cell"
        );
    }
}

#[test]
fn retract_selected_relation_removes_the_user_relation() {
    let mut canvas = Canvas::new();
    let a = canvas.open_member_as_new_node(None, "https://a.test");
    let b = canvas.open_member_as_new_node(None, "https://b.test");
    let ak = canvas.graph().get_node_by_id(a).unwrap().0;
    let bk = canvas.graph().get_node_by_id(b).unwrap().0;
    canvas.selected.clear();
    canvas.selected.insert(ak);
    canvas.selected.insert(bk);
    canvas.assert_selected_relation(SemanticSubKind::UserGrouped);
    let with_edge = canvas.graph().relations().count();

    // The edge is stored in the asserted (uuid-sorted) direction; select it and
    // retract.
    let mut pair = [ak, bk];
    pair.sort_by_key(|k| canvas.graph().get_node(*k).map(|n| n.id));
    canvas.selected_edges.clear();
    canvas.selected_edges.insert(EdgeCell {
        from: pair[0],
        to: pair[1],
        selector: RelationSelector::Semantic(SemanticSubKind::UserGrouped),
    });
    assert_eq!(
        canvas.retract_selected_relation(),
        1,
        "the user relation is retracted"
    );
    assert_eq!(
        canvas.graph().relations().count(),
        with_edge - 1,
        "the edge is gone (no families left → garbage-collected)",
    );
    assert!(
        canvas.selected_edges.is_empty(),
        "retraction clears the edge selection"
    );
}

#[test]
fn retract_selected_edge_cell_removes_only_that_relation() {
    let mut canvas = Canvas::new();
    let a = canvas.open_member_as_new_node(None, "https://cell-a.test");
    let b = canvas.open_member_as_new_node(None, "https://cell-b.test");
    let ak = canvas.graph().get_node_by_id(a).unwrap().0;
    let bk = canvas.graph().get_node_by_id(b).unwrap().0;
    assert!(canvas.assert_relation_between_members(a, b, SemanticSubKind::Cites));
    assert!(canvas.assert_relation_between_members(a, b, SemanticSubKind::Quotes));

    canvas.selected_edges.insert(EdgeCell {
        from: ak,
        to: bk,
        selector: RelationSelector::Semantic(SemanticSubKind::Cites),
    });

    assert_eq!(canvas.retract_selected_relation(), 1);
    let remaining: Vec<_> = canvas.graph().relations().map(|r| r.kind).collect();
    assert_eq!(
        remaining,
        vec![RelationKind::Semantic(SemanticSubKind::Quotes)],
        "the other semantic cell survives the canvas-cell delete"
    );
}

#[test]
fn hide_selected_edge_cell_hides_only_that_relation() {
    let mut canvas = Canvas::new();
    let a = canvas.open_member_as_new_node(None, "https://hide-cell-a.test");
    let b = canvas.open_member_as_new_node(None, "https://hide-cell-b.test");
    let ak = canvas.graph().get_node_by_id(a).unwrap().0;
    let bk = canvas.graph().get_node_by_id(b).unwrap().0;
    assert!(canvas.assert_relation_between_members(a, b, SemanticSubKind::Cites));
    assert!(canvas.assert_relation_between_members(a, b, SemanticSubKind::Quotes));

    canvas.selected_edges.insert(EdgeCell {
        from: ak,
        to: bk,
        selector: RelationSelector::Semantic(SemanticSubKind::Cites),
    });

    assert_eq!(canvas.hide_selected_edges(), 1);
    assert!(canvas.relation_between_members_hidden(
        a,
        b,
        RelationSelector::Semantic(SemanticSubKind::Cites)
    ));
    assert!(!canvas.relation_between_members_hidden(
        a,
        b,
        RelationSelector::Semantic(SemanticSubKind::Quotes)
    ));
    assert!(
        !canvas.edge_between_members_hidden(a, b),
        "the endpoint bundle is not fully hidden while another cell remains visible"
    );
    let visible: Vec<_> = crate::edge_cells::visible_edge_cell_segments(
        canvas.graph(),
        &canvas.view,
        &canvas.hidden_edges,
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
    let mut canvas = Canvas::new();
    let a = canvas.open_member_as_new_node(None, "https://a.test");
    let b = canvas.open_member_as_new_node(None, "https://b.test");
    let ak = canvas.graph().get_node_by_id(a).unwrap().0;
    let bk = canvas.graph().get_node_by_id(b).unwrap().0;
    let base = canvas.graph().relations().count();
    canvas.selected.clear();
    canvas.selected.insert(ak);
    canvas.selected.insert(bk);

    canvas.assert_selected_relation(SemanticSubKind::UserGrouped);
    assert_eq!(canvas.graph().relations().count(), base + 1, "related");

    // No edge selected — the two-node selection alone retracts the relation.
    assert!(canvas.selected_edges.is_empty());
    assert_eq!(
        canvas.retract_selected_relation(),
        1,
        "the pair selection unrelates"
    );
    assert_eq!(
        canvas.graph().relations().count(),
        base,
        "back to no relation"
    );
}

#[test]
fn visit_dedups_by_url() {
    let mut canvas = Canvas::new();
    let a = canvas.visit("https://example.com");
    let _ = canvas.visit("https://other.com");
    let again = canvas.visit("https://example.com");
    assert_eq!(
        a, again,
        "revisiting a URL selects the existing node, not a duplicate"
    );
    assert_eq!(
        canvas.graph.nodes().count(),
        2,
        "no duplicate node is added"
    );
    assert!(
        canvas.selected.contains(&a),
        "selection returns to the revisited node"
    );
}

#[test]
fn focused_url_is_the_single_selected_nodes_url() {
    let mut canvas = Canvas::new();
    assert_eq!(
        canvas.focused_url(),
        None,
        "nothing selected yet → no focus"
    );
    canvas.visit("https://example.com");
    assert_eq!(
        canvas.focused_url(),
        Some("https://example.com"),
        "visit focuses the node"
    );
    canvas.visit("https://second.com");
    assert_eq!(
        canvas.focused_url(),
        Some("https://second.com"),
        "focus follows the new node"
    );
    canvas.visit("https://example.com");
    assert_eq!(
        canvas.focused_url(),
        Some("https://example.com"),
        "revisit re-focuses"
    );
}
