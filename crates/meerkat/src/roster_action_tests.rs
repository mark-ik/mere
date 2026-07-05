/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-level proofs for Roster detail-card actions.

use super::*;
use forme::GraphMemberId;
use kernel::graph::{FieldId, RelationKind, RelationSelector, SemanticSubKind};

use crate::roster::{RosterDetail, RosterSubject};

fn test_app() -> crate::test_support::TestShell {
    let (_tx, rx) = std::sync::mpsc::channel();
    let temp = crate::test_support::temp_session_dir("mere-roster-action-tests");
    let shell = Shell::new_with_session_dir(
        crate::test_support::event_loop_proxy(),
        rx,
        temp.path().to_path_buf(),
    );
    crate::test_support::TestShell::new(shell, temp)
}

fn push_roster_intent(app: &mut Shell, intent: crate::roster_view::RosterIntent) {
    push_roster_intent_with_shift(app, false, intent);
}

fn push_roster_intent_with_shift(
    app: &mut Shell,
    shift: bool,
    intent: crate::roster_view::RosterIntent,
) {
    let mut wc = app.ctx();
    wc.view.modifiers.shift = shift;
    wc.view.runner.update(|s| s.roster.pending.push(intent));
    wc.drain_roster_intents();
}

fn add_member(app: &mut Shell, url: &str) -> GraphMemberId {
    app.orrery_mut().open_member_as_new_node(None, url)
}

fn active_graph(app: &mut Shell) -> GraphId {
    app.ctx().active_graph_id()
}

fn has_semantic_relation(
    app: &Shell,
    from: GraphMemberId,
    to: GraphMemberId,
    kind: SemanticSubKind,
) -> bool {
    let graph = app.orrery().graph();
    let Some(from_key) = graph.get_node_key_by_id(from) else {
        return false;
    };
    let Some(to_key) = graph.get_node_key_by_id(to) else {
        return false;
    };
    graph
        .relations()
        .any(|r| r.from == from_key && r.to == to_key && r.kind == RelationKind::Semantic(kind))
}

fn linked_anchor_count(app: &Shell, graph: GraphId, graphlet: forme::GraphletId) -> usize {
    app.graphlets
        .get(&graph)
        .and_then(|idx| idx.get(graphlet))
        .map(|g| g.anchors.len())
        .expect("linked graphlet exists")
}

#[test]
fn roster_action_relate_as_asserts_relation() {
    let mut app = test_app();
    let from = add_member(&mut app, "https://roster-action-a.test");
    let to = add_member(&mut app, "https://roster-action-b.test");

    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::RelateAs {
            from,
            to,
            kind: SemanticSubKind::Cites,
        },
    );

    assert!(
        has_semantic_relation(&app, from, to, SemanticSubKind::Cites),
        "Roster RelateAs should assert the selected semantic relation"
    );
    assert_eq!(
        app.ctx().view.roster_subject(),
        Some(RosterSubject::RelationCell {
            from,
            to,
            selector: RelationSelector::Semantic(SemanticSubKind::Cites),
        }),
        "Roster RelateAs should preselect the new relation cell"
    );
}

#[test]
fn roster_action_retract_relation_targets_one_relation_cell() {
    let mut app = test_app();
    let from = add_member(&mut app, "https://roster-retract-a.test");
    let to = add_member(&mut app, "https://roster-retract-b.test");
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(from, to, SemanticSubKind::Cites)
    );
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(from, to, SemanticSubKind::Quotes)
    );
    app.ctx()
        .view
        .set_roster_subject(Some(RosterSubject::RelationCell {
            from,
            to,
            selector: RelationSelector::Semantic(SemanticSubKind::Cites),
        }));

    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::RetractRelation {
            from,
            to,
            selector: RelationSelector::Semantic(SemanticSubKind::Cites),
        },
    );

    assert!(
        app.orrery().graph().get_node_by_id(from).is_some(),
        "source endpoint remains after retracting one relation cell"
    );
    assert!(
        app.orrery().graph().get_node_by_id(to).is_some(),
        "target endpoint remains after retracting one relation cell"
    );
    assert!(
        !has_semantic_relation(&app, from, to, SemanticSubKind::Cites),
        "selected semantic relation should be removed"
    );
    assert!(
        has_semantic_relation(&app, from, to, SemanticSubKind::Quotes),
        "other semantic relation cells in the bundle should remain"
    );
    assert_eq!(
        app.ctx().view.roster_subject(),
        Some(RosterSubject::LinkBundle { from, to }),
        "after retracting the selected cell, keep the remaining endpoint bundle open"
    );
}

#[test]
fn roster_action_retract_last_relation_clears_link_subject() {
    let mut app = test_app();
    let from = add_member(&mut app, "https://roster-retract-last-a.test");
    let to = add_member(&mut app, "https://roster-retract-last-b.test");
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(from, to, SemanticSubKind::Cites)
    );
    app.ctx()
        .view
        .set_roster_subject(Some(RosterSubject::RelationCell {
            from,
            to,
            selector: RelationSelector::Semantic(SemanticSubKind::Cites),
        }));

    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::RetractRelation {
            from,
            to,
            selector: RelationSelector::Semantic(SemanticSubKind::Cites),
        },
    );

    assert_eq!(
        app.ctx().view.roster_subject(),
        None,
        "the detail subject clears when the endpoint bundle has no remaining relations"
    );
}

#[test]
fn roster_action_select_endpoint_supports_single_and_additive_selection() {
    let mut app = test_app();
    let from = add_member(&mut app, "https://roster-select-a.test");
    let to = add_member(&mut app, "https://roster-select-b.test");

    push_roster_intent(&mut app, crate::roster_view::RosterIntent::Select(from));
    assert_eq!(app.orrery().selected_members(), vec![from]);

    push_roster_intent_with_shift(&mut app, true, crate::roster_view::RosterIntent::Select(to));
    let mut selected = app.orrery().selected_members();
    let mut expected = vec![from, to];
    selected.sort();
    expected.sort();
    assert_eq!(
        selected, expected,
        "Shift-select from the Link Card endpoint action should add to the node selection"
    );
}

#[test]
fn roster_snapshot_relation_cell_marks_link_card_selection() {
    let mut app = test_app();
    let from = add_member(&mut app, "https://roster-card-a.test");
    let to = add_member(&mut app, "https://roster-card-b.test");
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(from, to, SemanticSubKind::Cites)
    );
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(from, to, SemanticSubKind::Quotes)
    );
    let selector = RelationSelector::Semantic(SemanticSubKind::Cites);
    let snapshot = {
        let wc = app.ctx();
        wc.roster_snapshot(Some(&RosterSubject::RelationCell { from, to, selector }))
    };

    assert_eq!(snapshot.link_rows.len(), 2);
    assert_eq!(
        snapshot.link_rows.iter().filter(|row| row.selected).count(),
        1
    );
    let card = match snapshot.detail {
        Some(RosterDetail::Link(card)) => card,
        _ => panic!("expected a Link Card detail"),
    };
    assert_eq!(card.relations.len(), 2);
    let selected: Vec<_> = card.relations.iter().filter(|row| row.selected).collect();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].selector, selector);
    assert!(selected[0].editable);
}

#[test]
fn roster_action_hides_and_shows_link_bundle_without_deleting_relations() {
    let mut app = test_app();
    let from = add_member(&mut app, "https://roster-hide-link-a.test");
    let to = add_member(&mut app, "https://roster-hide-link-b.test");
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(from, to, SemanticSubKind::Cites)
    );

    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::HideLinkBundle { from, to },
    );
    assert!(
        app.orrery().edge_between_members_hidden(from, to),
        "hide bundle should mark only the drawn endpoint bundle hidden"
    );
    assert!(
        has_semantic_relation(&app, from, to, SemanticSubKind::Cites),
        "hide bundle must not retract the relation cell"
    );
    let hidden_card = {
        let wc = app.ctx();
        wc.roster_snapshot(Some(&RosterSubject::LinkBundle { from, to }))
    };
    let Some(RosterDetail::Link(card)) = hidden_card.detail else {
        panic!("expected a Link Card detail");
    };
    assert!(card.hidden);

    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::ShowLinkBundle { from, to },
    );
    assert!(
        !app.orrery().edge_between_members_hidden(from, to),
        "show bundle should reveal the existing endpoint bundle"
    );
}

#[test]
fn roster_action_hides_and_shows_one_relation_cell() {
    let mut app = test_app();
    let from = add_member(&mut app, "https://roster-hide-cell-a.test");
    let to = add_member(&mut app, "https://roster-hide-cell-b.test");
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(from, to, SemanticSubKind::Cites)
    );
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(from, to, SemanticSubKind::Quotes)
    );
    let cites = RelationSelector::Semantic(SemanticSubKind::Cites);
    let quotes = RelationSelector::Semantic(SemanticSubKind::Quotes);

    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::HideRelation {
            from,
            to,
            selector: cites,
        },
    );

    assert!(
        app.orrery()
            .relation_between_members_hidden(from, to, cites)
    );
    assert!(
        !app.orrery()
            .relation_between_members_hidden(from, to, quotes)
    );
    assert!(
        !app.orrery().edge_between_members_hidden(from, to),
        "the bundle stays partially visible while Quotes remains visible"
    );
    let hidden_card = {
        let wc = app.ctx();
        wc.roster_snapshot(Some(&RosterSubject::RelationCell {
            from,
            to,
            selector: cites,
        }))
    };
    let Some(RosterDetail::Link(card)) = hidden_card.detail else {
        panic!("expected a Link Card detail");
    };
    assert!(
        card.relations
            .iter()
            .any(|row| row.selector == cites && row.hidden),
        "the selected relation row reports hidden"
    );
    assert!(
        card.relations
            .iter()
            .any(|row| row.selector == quotes && !row.hidden),
        "the sibling relation row remains visible"
    );

    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::ShowRelation {
            from,
            to,
            selector: cites,
        },
    );
    assert!(
        !app.orrery()
            .relation_between_members_hidden(from, to, cites)
    );
}

#[test]
fn roster_action_graphlet_intents_queue_and_apply_host_commands() {
    let mut app = test_app();
    let graph = active_graph(&mut app);
    let a = add_member(&mut app, "https://roster-graphlet-a.test");
    let b = add_member(&mut app, "https://roster-graphlet-b.test");
    let c = add_member(&mut app, "https://roster-graphlet-c.test");
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(a, b, SemanticSubKind::Hyperlink)
    );
    let graphlet = app
        .linked_graphlet(a, graph, forme::GraphletKind::Component, Vec::new())
        .expect("linked graphlet");
    assert_eq!(linked_anchor_count(&app, graph, graphlet), 2);

    assert!(
        app.orrery_mut()
            .assert_relation_between_members(b, c, SemanticSubKind::Hyperlink)
    );
    let delta = app
        .graphlets
        .get(&graph)
        .and_then(|idx| idx.preview_reconcile(app.orrery().graph(), graphlet))
        .expect("linked graphlet drift is visible");
    assert_eq!(delta.added, vec![c]);
    assert_eq!(
        linked_anchor_count(&app, graph, graphlet),
        2,
        "dry preview must not mutate the stored linked roster"
    );

    app.commands.clear();
    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::ReconcileGraphlet(graphlet),
    );
    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::KeepGraphletAsSession(graphlet),
    );
    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::BranchGraphlet(graphlet),
    );
    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::OpenGraphlet(graphlet),
    );

    assert!(matches!(
        app.commands.first(),
        Some(ShellCommand::ReconcileGraphlet {
            graph: queued_graph,
            graphlet: queued_graphlet,
        }) if *queued_graph == graph && *queued_graphlet == graphlet
    ));
    assert!(matches!(
        app.commands.get(1),
        Some(ShellCommand::KeepGraphletAsSession {
            graph: queued_graph,
            graphlet: queued_graphlet,
        }) if *queued_graph == graph && *queued_graphlet == graphlet
    ));
    assert!(matches!(
        app.commands.get(2),
        Some(ShellCommand::BranchGraphlet {
            graph: queued_graph,
            graphlet: queued_graphlet,
        }) if *queued_graph == graph && *queued_graphlet == graphlet
    ));
    assert!(matches!(
        app.commands.get(3),
        Some(ShellCommand::OpenExistingGraphlet {
            graph: queued_graph,
            graphlet: queued_graphlet,
        }) if *queued_graph == graph && *queued_graphlet == graphlet
    ));

    app.reconcile_linked_graphlet(graph, graphlet);
    assert_eq!(
        linked_anchor_count(&app, graph, graphlet),
        3,
        "reconcile apply should update the linked roster after the command runs"
    );
}

#[test]
fn roster_snapshot_graphlet_card_surfaces_drift_without_applying() {
    let mut app = test_app();
    let graph = active_graph(&mut app);
    let a = add_member(&mut app, "https://roster-drift-a.test");
    let b = add_member(&mut app, "https://roster-drift-b.test");
    let c = add_member(&mut app, "https://roster-drift-c.test");
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(a, b, SemanticSubKind::Hyperlink)
    );
    let graphlet = app
        .linked_graphlet(a, graph, forme::GraphletKind::Component, Vec::new())
        .expect("linked graphlet");
    assert_eq!(linked_anchor_count(&app, graph, graphlet), 2);
    assert!(
        app.orrery_mut()
            .assert_relation_between_members(b, c, SemanticSubKind::Hyperlink)
    );

    let snapshot = {
        let wc = app.ctx();
        wc.roster_snapshot(Some(&RosterSubject::Graphlet(graphlet)))
    };

    let row = snapshot
        .graphlet_rows
        .iter()
        .find(|row| row.id == graphlet)
        .expect("graphlet row");
    assert!(row.selected);
    assert_eq!(row.drift_label, "+1 -0");
    let card = match snapshot.detail {
        Some(RosterDetail::Graphlet(card)) => card,
        _ => panic!("expected a Graphlet Card detail"),
    };
    assert!(card.drift_tracking);
    assert_eq!(card.binding_label, "Linked");
    assert_eq!(card.members.len(), 2);
    assert_eq!(card.added.len(), 1);
    assert!(card.removed.is_empty());
    assert_eq!(
        linked_anchor_count(&app, graph, graphlet),
        2,
        "Roster drift preview must not reconcile until the apply action runs"
    );
}

#[test]
fn roster_action_field_facet_intents_update_orrery_field_state() {
    let mut app = test_app();
    let field = FieldId::from_uuid(app.orrery_mut().add_field_at((300.0, 150.0)));
    let was_visible = app.orrery().field_visible(field);
    let strength = app.orrery().field_strength(field).expect("field strength");

    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::ToggleFieldVisibility(field),
    );
    assert_ne!(
        app.orrery().field_visible(field),
        was_visible,
        "field visibility facet action should drain to Orrery"
    );

    push_roster_intent(
        &mut app,
        crate::roster_view::RosterIntent::AdjustFieldStrength(field, 1000.0),
    );
    assert_eq!(
        app.orrery().field_strength(field),
        Some(strength + 1000.0),
        "field strength facet action should drain to Orrery"
    );
}

#[test]
fn folded_roster_renders_visible_tab_buttons_in_shell_document() {
    use serval_layout::ScrollOffsets;

    let mut app = test_app();
    add_member(&mut app, "https://roster-shell-tabs.test");
    let snapshot = {
        let wc = app.ctx();
        wc.roster_snapshot(None)
    };
    {
        let wc = app.ctx();
        wc.view
            .set_roster(snapshot, Some([740.0, 150.0, 1020.0, 590.0]));
        let roster_css = crate::roster::roster_sheet(&wc.shared.presentation.chrome_theme);
        let mut sheet = wc.shared.presentation.chrome_sheet_refs();
        sheet.extend(roster_css.iter().map(String::as_str));
        crate::pane_session::PaneSession::scene(
            &mut wc.view.chrome_session,
            &wc.view.dom,
            &sheet,
            wc.shared.presentation.scheme_dark(),
            1024,
            600,
            None,
            &ScrollOffsets::default(),
        );
    }

    let wc = app.ctx();
    let dom = wc.view.dom.borrow();
    let session = wc
        .view
        .chrome_session
        .as_ref()
        .expect("chrome session built");
    let tabs = crate::all_with_class(&dom, dom.document(), "roster-tab");
    assert_eq!(tabs.len(), 4, "all roster tabs render");
    let first_tab = tabs[0];
    let first_tab_rect = session
        .fragments()
        .rect_of(first_tab)
        .expect("first tab laid out");
    assert!(
        first_tab_rect.size.width > 20.0 && first_tab_rect.size.height > 10.0,
        "first tab has a visible box: {:?}",
        first_tab_rect.size
    );
    let origins = serval_layout::accumulate_painted_origins(
        &*dom,
        session.fragments(),
        session.element_scroll(),
    );
    let first_tab_origin = origins.get(&first_tab).expect("first tab painted origin");
    assert!(
        first_tab_origin.x >= 740.0 && first_tab_origin.y >= 120.0,
        "first tab is inside the roster pane: origin={first_tab_origin:?} size={:?}",
        first_tab_rect.size
    );
    let first_row =
        crate::first_with_class(&dom, dom.document(), "roster-row").expect("first roster row");
    let row_origin = origins.get(&first_row).expect("first row painted origin");
    assert!(
        row_origin.y >= first_tab_origin.y + first_tab_rect.size.height,
        "first row must not paint over tab strip: tab={first_tab_origin:?}/{:?} row={row_origin:?}",
        first_tab_rect.size
    );
}
