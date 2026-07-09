use mere::forme::GraphMemberId;
use mere::kernel::graph::{EdgeFamily, RelationSelector, SemanticSubKind};
use layout_dom_api::LayoutDom;
use register_theme::chrome::ChromeTheme;
use serval_scripted_dom::{NodeId, ScriptedDom};
use xilem_serval::PointerClick;

use super::*;
use crate::roster::{
    FacetSubject, GraphletCard, LinkCard, LinkRelationRow, RosterSnapshot, RosterSubject, RosterTab,
};

fn nodes_by_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Vec<NodeId> {
    let mut nodes = Vec::new();
    collect_by_class(dom, id, class, &mut nodes);
    nodes
}

fn collect_by_class(dom: &ScriptedDom, id: NodeId, class: &str, out: &mut Vec<NodeId>) {
    if dom.attributes(id).any(|attr| {
        attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
    }) {
        out.push(id);
    }
    for child in dom.dom_children(id) {
        collect_by_class(dom, child, class, out);
    }
}

fn card_actions(pane: &RosterPane) -> Vec<NodeId> {
    let dom = pane.dom();
    let dom = dom.borrow();
    nodes_by_class(&dom, dom.document(), "roster-card-action")
}

#[test]
fn opening_subject_switches_to_its_natural_tab() {
    let from = GraphMemberId::from_u128(1);
    let to = GraphMemberId::from_u128(2);
    let mut state = RosterState::default();

    state.open_subject(RosterSubject::Graphlet(7));
    assert_eq!(state.active_tab, RosterTab::Graphlets);
    assert_eq!(
        state.pending,
        vec![RosterIntent::OpenDetail(RosterSubject::Graphlet(7))]
    );

    state.pending.clear();
    state.open_subject(RosterSubject::Facet(FacetSubject::LinkFamily {
        from,
        to,
        family: EdgeFamily::Semantic,
    }));
    assert_eq!(state.active_tab, RosterTab::Links);
    assert!(matches!(
        state.pending.as_slice(),
        [RosterIntent::OpenDetail(RosterSubject::Facet(
            FacetSubject::LinkFamily { .. }
        ))]
    ));
}

#[test]
fn link_card_actions_queue_endpoint_relate_and_retract_intents() {
    let from = GraphMemberId::from_u128(1);
    let to = GraphMemberId::from_u128(2);
    let selector = RelationSelector::Semantic(SemanticSubKind::Cites);
    let mut pane = RosterPane::new();
    pane.set_snapshot(
        &ChromeTheme::default(),
        RosterSnapshot {
            detail: Some(RosterDetail::Link(LinkCard {
                from,
                to,
                source_title: "A".to_string(),
                source_url: "https://a.example".to_string(),
                target_title: "B".to_string(),
                target_url: "https://b.example".to_string(),
                hidden: false,
                relations: vec![LinkRelationRow {
                    from,
                    to,
                    family: EdgeFamily::Semantic,
                    family_label: "Semantic".to_string(),
                    kind_label: "Cites".to_string(),
                    label: None,
                    selector,
                    editable: true,
                    selected: false,
                    hidden: false,
                }],
                facets: Vec::new(),
            })),
            ..Default::default()
        },
    );
    let _ = pane.frame(320, 320, 0.0);
    let actions = card_actions(&pane);
    assert_eq!(
        actions.len(),
        4 + 2,
        "the relation picker starts collapsed, plus endpoint and relation-row actions"
    );

    pane.dispatch_click(actions[0], PointerClick::at((24.0, 80.0)));
    assert_eq!(pane.take_intents(), vec![RosterIntent::Select(from)]);

    let _ = pane.frame(320, 320, 0.0);
    let actions = card_actions(&pane);
    pane.dispatch_click(actions[3], PointerClick::at((24.0, 96.0)));
    assert_eq!(
        pane.take_intents(),
        vec![RosterIntent::HideLinkBundle { from, to }]
    );

    let _ = pane.frame(320, 320, 0.0);
    let actions = card_actions(&pane);
    pane.dispatch_click(actions[4], PointerClick::at((24.0, 116.0)));
    assert_eq!(
        pane.take_intents(),
        vec![RosterIntent::HideRelation { from, to, selector }]
    );

    let _ = pane.frame(320, 320, 0.0);
    let actions = card_actions(&pane);
    pane.dispatch_click(actions[2], PointerClick::at((24.0, 104.0)));
    assert_eq!(pane.take_intents(), Vec::<RosterIntent>::new());
    let _ = pane.frame(320, 320, 0.0);
    let actions = card_actions(&pane);
    assert_eq!(
        actions.len(),
        4 + crate::roster::RELATE_PICKER_KINDS.len() + 2
    );

    pane.dispatch_click(actions[4], PointerClick::at((24.0, 112.0)));
    assert_eq!(
        pane.take_intents(),
        vec![RosterIntent::RelateAs {
            from,
            to,
            kind: SemanticSubKind::Cites,
        }]
    );

    let _ = pane.frame(320, 320, 0.0);
    let actions = card_actions(&pane);
    pane.dispatch_click(
        *actions.last().expect("retract action"),
        PointerClick::at((24.0, 220.0)),
    );
    assert_eq!(
        pane.take_intents(),
        vec![RosterIntent::RetractRelation { from, to, selector }]
    );
}

#[test]
fn graphlet_card_actions_queue_graphlet_intents() {
    let mut pane = RosterPane::new();
    pane.set_snapshot(
        &ChromeTheme::default(),
        RosterSnapshot {
            detail: Some(RosterDetail::Graphlet(GraphletCard {
                id: 7,
                kind_label: "Component".to_string(),
                binding_label: "Linked".to_string(),
                members: vec!["A".to_string()],
                selectors_label: "Semantic".to_string(),
                family_selectors: None,
                drift_tracking: true,
                drift_summary: "drift proposal: +1 -0".to_string(),
                added: vec!["B".to_string()],
                removed: Vec::new(),
            })),
            ..Default::default()
        },
    );
    let _ = pane.frame(320, 320, 0.0);
    let actions = card_actions(&pane);
    assert_eq!(actions.len(), 4);

    for index in 0..4 {
        let actions = card_actions(&pane);
        let action = actions[index];
        pane.dispatch_click(action, PointerClick::at((24.0, 180.0)));
        let _ = pane.frame(320, 320, 0.0);
    }
    assert_eq!(
        pane.take_intents(),
        vec![
            RosterIntent::ReconcileGraphlet(7),
            RosterIntent::KeepGraphletAsSession(7),
            RosterIntent::BranchGraphlet(7),
            RosterIntent::OpenGraphlet(7),
        ]
    );
}
