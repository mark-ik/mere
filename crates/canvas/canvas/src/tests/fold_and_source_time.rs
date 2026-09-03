// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Fold projection (a selection folded to one summary body) and source-time
//! scrubbing: a journal-prefix preview that must never rewrite live truth.

use super::*;

#[test]
fn fold_selection_projects_a_summary_and_click_expands_without_mutating_graph() {
    let mut canvas = Canvas::new();
    let a = canvas.open_member_as_new_node(None, "https://fold-a.test");
    let b = canvas.open_member_as_new_node(None, "https://fold-b.test");
    let outside = canvas.open_member_as_new_node(None, "https://fold-outside.test");
    let ak = canvas.graph().get_node_by_id(a).unwrap().0;
    let bk = canvas.graph().get_node_by_id(b).unwrap().0;
    let outside_key = canvas.graph().get_node_by_id(outside).unwrap().0;
    assert!(canvas.assert_relation_between_members(a, b, SemanticSubKind::UserGrouped));
    assert!(canvas.assert_relation_between_members(a, outside, SemanticSubKind::Cites));
    assert!(canvas.assert_relation_between_members(b, outside, SemanticSubKind::Quotes));
    canvas
        .view
        .set_position(ak, euclid::default::Point2D::new(0.0, 0.0));
    canvas
        .view
        .set_position(bk, euclid::default::Point2D::new(100.0, 0.0));
    canvas
        .view
        .set_position(outside_key, euclid::default::Point2D::new(200.0, 0.0));
    let folded_member_positions = [
        (ak, canvas.view.position_of(ak).expect("placed fold member")),
        (bk, canvas.view.position_of(bk).expect("placed fold member")),
    ];
    canvas.select_only(ak);
    assert!(canvas.toggle_select_member(b));
    let source_nodes = canvas.graph().nodes().count();
    let source_relations = canvas.graph().relations().count();
    let source_truth = serde_json::to_vec(&canvas.graph().to_snapshot())
        .expect("the source graph has a stable persisted form");

    let fold_id = canvas
        .fold_selected("canvas:test")
        .expect("two selected nodes make a fold");
    let projection = canvas
        .active_fold_projection()
        .expect("the current graph projects the fold");
    let record = canvas
        .fold_record()
        .cloned()
        .expect("the current fold remains a durable view record");
    assert_eq!(projection.members.len(), 2);
    assert_eq!(projection.internal_relation_count, 1);
    assert_eq!(projection.boundary_bundles.len(), 1);
    assert_eq!(projection.boundary_bundles[0].count, 2);
    assert!(!canvas.node_in_scope(ak));
    assert!(!canvas.node_in_scope(bk));
    assert!(canvas.node_in_scope(outside_key));
    assert_eq!(canvas.graph().nodes().count(), source_nodes);
    assert_eq!(canvas.graph().relations().count(), source_relations);
    assert!(canvas.undo_fold(), "undo restores the pre-fold selection");
    assert!(canvas.active_fold_projection().is_none());
    assert_eq!(canvas.selected, HashSet::from([ak, bk]));
    assert!(canvas.redo_fold(), "redo restores the same synthetic fold");
    assert_eq!(canvas.active_fold_projection().unwrap().fold_id, fold_id);

    let center = canvas
        .camera
        .to_screen(euclid::default::Point2D::new(50.0, 0.0));
    canvas.pointer_down(PointerButton::Left, center.0, center.1);
    assert!(canvas.pointer_up(PointerButton::Left, center.0, center.1));
    assert!(canvas.active_fold_projection().is_none());
    assert_eq!(canvas.selected, HashSet::from([ak, bk]));
    assert!(canvas.restore_fold(None));
    assert!(canvas.restore_fold(Some(record)));
    assert!(canvas.active_fold_projection().is_some());
    assert!(canvas.expand_active_fold());
    assert_eq!(canvas.graph().nodes().count(), source_nodes);
    assert_eq!(canvas.graph().relations().count(), source_relations);
    assert_eq!(
        serde_json::to_vec(&canvas.graph().to_snapshot())
            .expect("the source graph remains serializable"),
        source_truth,
        "folding remains a byte-for-byte view action"
    );
    for (key, expected) in folded_member_positions {
        assert_eq!(
            canvas.view.position_of(key),
            Some(expected),
            "expanding restores the member's source-owned placement"
        );
    }
    assert!(
        !canvas.expand_fold(fold_id),
        "the summary is already expanded"
    );
}

#[test]
fn source_time_canvas_keeps_live_graph_and_arrangement_while_previewing_a_journal_prefix() {
    use kernel::graph::{CapturedDelta, GraphJournal, Seq};

    let mut journal = GraphJournal::new();
    for (id, url) in [
        (1, "https://source-one.test/"),
        (2, "https://source-two.test/"),
        (3, "https://source-three.test/"),
    ] {
        journal.record(CapturedDelta::ReplayAddNodeWithIdIfMissing {
            id: uuid::Uuid::from_u128(id).to_string(),
            url: url.to_string(),
            position: [0.0, 0.0],
        });
    }

    let live = Canvas::with_graph(journal.replay());
    let mut source_canvas = SourceTimeCanvas::new(live);
    let live_one = uuid::Uuid::from_u128(1);
    let live_two = uuid::Uuid::from_u128(2);
    source_canvas
        .live_canvas_mut()
        .seed_cartography([(live_one, (100.0, 200.0)), (live_two, (300.0, 400.0))]);
    source_canvas
        .live_canvas_mut()
        .set_layout_strategy(Some("timeline.default".to_string()));
    let live_one_key = source_canvas
        .live_canvas()
        .graph()
        .get_node_key_by_id(live_one)
        .expect("first live member");
    let live_two_key = source_canvas
        .live_canvas()
        .graph()
        .get_node_key_by_id(live_two)
        .expect("second live member");
    source_canvas.live_canvas_mut().apply_strategy_positions(&[
        (live_one_key, PortablePoint::new(100.0, 200.0)),
        (live_two_key, PortablePoint::new(300.0, 400.0)),
    ]);
    source_canvas
        .live_canvas_mut()
        .set_selected_members(&[live_one]);
    let live_truth = serde_json::to_vec(&source_canvas.live_canvas().graph().to_snapshot())
        .expect("live graph persists");

    assert!(source_canvas.select(&journal, Seq(2)));
    assert_eq!(
        source_canvas.selection(),
        SourceTimeSelection::Historical(Seq(2))
    );
    assert_eq!(source_canvas.canvas().graph().node_count(), 2);
    assert_eq!(
        source_canvas.canvas().layout_strategy(),
        Some("timeline.default"),
        "a source change preserves the chosen arrangement"
    );
    let historical_one = source_canvas
        .canvas()
        .graph()
        .get_node_key_by_id(live_one)
        .expect("first member exists at prefix");
    assert_eq!(
        source_canvas.canvas().node_position(historical_one),
        Some(PortablePoint::new(100.0, 200.0)),
        "members shared by both snapshots retain their layout slot"
    );
    assert_eq!(source_canvas.live_canvas().graph().node_count(), 3);
    assert_eq!(
        serde_json::to_vec(&source_canvas.live_canvas().graph().to_snapshot())
            .expect("live graph remains persistable"),
        live_truth,
        "scrubbing never rewrites the retained live canvas"
    );
    assert_eq!(journal.replay().node_count(), 3, "the journal remains live");

    source_canvas.return_to_live();
    assert_eq!(source_canvas.selection(), SourceTimeSelection::Live);
    assert_eq!(source_canvas.canvas().graph().node_count(), 3);
    assert_eq!(
        source_canvas.canvas().node_position(live_one_key),
        Some(PortablePoint::new(100.0, 200.0)),
        "returning live restores the original canvas rather than rebuilding it"
    );
}

#[test]
fn source_time_canvas_scrubs_every_canvas_arrangement_without_rewriting_live_truth() {
    use crate::cartography_scene::{CANVAS_LAYOUT_STRATEGIES, project_canvas_strategy};
    use kernel::graph::{CapturedDelta, GraphJournal, Seq};

    let mut journal = GraphJournal::new();
    for (id, url) in [
        (1, "https://one.arrangement.test/"),
        (2, "https://two.arrangement.test/"),
        (3, "https://three.other.test/"),
        (4, "https://four.other.test/"),
    ] {
        journal.record(CapturedDelta::ReplayAddNodeWithIdIfMissing {
            id: uuid::Uuid::from_u128(id).to_string(),
            url: url.to_string(),
            position: [0.0, 0.0],
        });
    }

    let arrangement_ids = CANVAS_LAYOUT_STRATEGIES
        .iter()
        .map(|(id, _)| *id)
        .chain(std::iter::once("radial.default"));
    let shared_member = uuid::Uuid::from_u128(1);

    for arrangement_id in arrangement_ids {
        let mut source_canvas = SourceTimeCanvas::new(Canvas::with_graph(journal.replay()));
        let focus = source_canvas
            .live_canvas()
            .graph()
            .get_node_key_by_id(shared_member)
            .expect("the shared member exists in the live graph");
        source_canvas
            .live_canvas_mut()
            .set_layout_strategy(Some(arrangement_id.to_string()));
        source_canvas
            .live_canvas_mut()
            .set_selected_members(&[shared_member]);
        let positions = {
            let live = source_canvas.live_canvas();
            project_canvas_strategy(
                arrangement_id,
                live.graph(),
                Some(focus),
                800,
                600,
                None,
                Some(&live.strategy_extents()),
                false,
            )
        };
        assert!(
            !positions.is_empty(),
            "{arrangement_id} has a projection for the live graph"
        );
        source_canvas
            .live_canvas_mut()
            .apply_strategy_positions(&positions);
        source_canvas.frame(800, 600);

        let live_truth = serde_json::to_vec(&source_canvas.live_canvas().graph().to_snapshot())
            .expect("live graph persists");
        let live_position = source_canvas
            .live_canvas()
            .node_position(focus)
            .expect("the arrangement placed the shared member");

        assert!(source_canvas.select(&journal, Seq(3)));
        assert_eq!(
            source_canvas.selection(),
            SourceTimeSelection::Historical(Seq(3)),
            "{arrangement_id} keeps the selected source cursor"
        );
        assert_eq!(source_canvas.canvas().graph().node_count(), 3);
        assert_eq!(
            source_canvas.canvas().layout_strategy(),
            Some(arrangement_id),
            "{arrangement_id} survives the source change"
        );
        let historical_key = source_canvas
            .canvas()
            .graph()
            .get_node_key_by_id(shared_member)
            .expect("the shared member exists at the historical prefix");
        assert_eq!(
            source_canvas.canvas().node_position(historical_key),
            Some(live_position),
            "{arrangement_id} retains the shared member slot"
        );
        assert_eq!(
            serde_json::to_vec(&source_canvas.live_canvas().graph().to_snapshot())
                .expect("live graph remains persistable"),
            live_truth,
            "{arrangement_id} source scrubbing does not rewrite live truth"
        );

        source_canvas.return_to_live();
        assert_eq!(source_canvas.selection(), SourceTimeSelection::Live);
        assert_eq!(
            source_canvas.canvas().node_position(focus),
            Some(live_position),
            "{arrangement_id} returns to the retained live presentation"
        );
    }
}
