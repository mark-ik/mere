/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tests for the per-session graphlet index (`graphlets.rs`). Split into a
//! sibling file per the 600-LOC ceiling.

use crate::graphlets::*;
use forme::{GraphletBinding, GraphletKind, GraphletSpec};
use kernel::graph::{EdgeFamily, Graph};

#[test]
fn default_session_seeds_one_graphlet() {
    let g = SessionGraphlets::new().with_default_session();
    assert_eq!(g.graphlets().len(), 1);
    assert!(matches!(
        g.graphlets()[0].binding,
        GraphletBinding::UnlinkedSession
    ));
}

#[test]
fn record_session_freezes_the_selection_with_its_kind() {
    let a = uuid::Uuid::from_u128(1);
    let b = uuid::Uuid::from_u128(2);
    let mut idx = SessionGraphlets::new();
    let id = idx.record_session(GraphletKind::Corridor, vec![a, b]);
    let g = idx.get(id).expect("the session graphlet exists");
    assert_eq!(
        g.anchors,
        vec![a, b],
        "the exact selection is frozen as the roster"
    );
    assert_eq!(
        g.kind,
        Some(GraphletKind::Corridor),
        "tagged with the classified kind"
    );
    assert!(
        matches!(g.binding, GraphletBinding::UnlinkedSession),
        "a frozen Session graphlet, not a derived one"
    );
}

#[test]
fn record_branch_mints_a_branched_graphlet_anchored_on_the_node() {
    let anchor = uuid::Uuid::from_u128(0x42);
    let mut g = SessionGraphlets::new().with_default_session();
    let id = g.record_branch(anchor, default_spec_for(anchor));

    let branch = g.get(id).expect("the branch graphlet exists");
    assert_eq!(branch.primary_anchor, Some(anchor));
    match &branch.binding {
        GraphletBinding::Branched { reason, .. } => assert_eq!(reason, "tearout-branch"),
        other => panic!("expected a Branched binding, got {other:?}"),
    }
    // It is distinct from the default session graphlet.
    assert_eq!(g.graphlets().len(), 2);
}

#[test]
fn add_member_grows_the_roster_and_dedups() {
    let anchor = uuid::Uuid::from_u128(0x42);
    let mut g = SessionGraphlets::new().with_default_session();
    let id = g.record_branch(anchor, default_spec_for(anchor));
    assert!(
        g.get(id).unwrap().anchors.contains(&anchor),
        "seeded with its anchor"
    );

    let visited = uuid::Uuid::from_u128(0x99);
    assert!(g.add_member(id, visited), "a newly-navigated node is added");
    assert!(!g.add_member(id, visited), "a revisit is a dedup'd no-op");
    assert!(!g.add_member(id, anchor), "the anchor is already present");
    assert_eq!(
        g.get(id).unwrap().anchors.len(),
        2,
        "anchor + the one visited node"
    );
}

#[test]
fn linked_component_graphlet_derives_and_reconciles_on_drift() {
    use euclid::default::Point2D;
    use kernel::graph::{EdgeAssertion, SemanticSubKind};
    let mut graph = Graph::new();
    let a = graph.add_node("https://a".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b".to_string(), Point2D::new(1.0, 0.0));
    let c = graph.add_node("https://c".to_string(), Point2D::new(2.0, 0.0));
    let link = |g: &mut Graph, x, y| {
        g.assert_relation(
            x,
            y,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
    };
    link(&mut graph, a, b); // A–B connected; C isolated
    let a_id = graph.get_node(a).unwrap().id;
    let c_id = graph.get_node(c).unwrap().id;

    // A Linked Component graphlet seeded on A derives {A, B}.
    let mut idx = SessionGraphlets::new();
    let spec = GraphletSpec {
        kind: GraphletKind::Component,
        anchors: vec![a_id.to_string()],
        primary_anchor: Some(a_id.to_string()),
        selectors: Vec::new(),
    };
    let id = idx.record_linked(&graph, spec);
    assert_eq!(
        idx.get(id).unwrap().anchors.len(),
        2,
        "component of A is A plus B"
    );
    assert!(
        idx.reconcile(&graph, id).is_none(),
        "no drift yet, reconcile is a no-op"
    );

    // Connect C into the component; reconcile re-derives + auto-applies.
    link(&mut graph, b, c);
    let delta = idx.reconcile(&graph, id).expect("the component grew");
    assert_eq!(delta.added, vec![c_id], "C joined A's component");
    assert!(delta.removed.is_empty());
    assert_eq!(
        idx.get(id).unwrap().anchors.len(),
        3,
        "live roster is now A, B, C"
    );
}

#[test]
fn linked_ego_graphlet_is_radius_bounded() {
    use euclid::default::Point2D;
    use kernel::graph::{EdgeAssertion, SemanticSubKind};
    let mut graph = Graph::new();
    let a = graph.add_node("https://a".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b".to_string(), Point2D::new(1.0, 0.0));
    let c = graph.add_node("https://c".to_string(), Point2D::new(2.0, 0.0));
    let link = |g: &mut Graph, x, y| {
        g.assert_relation(
            x,
            y,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
    };
    link(&mut graph, a, b);
    link(&mut graph, b, c); // A–B–C chain
    let a_id = graph.get_node(a).unwrap().id;

    let mut idx = SessionGraphlets::new();
    let spec = GraphletSpec {
        kind: GraphletKind::Ego { radius: 1 },
        anchors: vec![a_id.to_string()],
        primary_anchor: Some(a_id.to_string()),
        selectors: Vec::new(),
    };
    let id = idx.record_linked(&graph, spec);
    assert_eq!(
        idx.get(id).unwrap().anchors.len(),
        2,
        "ego radius 1 from A is A plus B; C is two hops away"
    );
}

#[test]
fn reconcile_all_updates_linked_rosters_on_drift() {
    use euclid::default::Point2D;
    use kernel::graph::{EdgeAssertion, SemanticSubKind};
    let mut graph = Graph::new();
    let a = graph.add_node("https://a".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b".to_string(), Point2D::new(1.0, 0.0));
    let c = graph.add_node("https://c".to_string(), Point2D::new(2.0, 0.0));
    let link = |g: &mut Graph, x, y| {
        g.assert_relation(
            x,
            y,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
    };
    link(&mut graph, a, b); // A–B; C isolated
    let a_id = graph.get_node(a).unwrap().id;

    let mut idx = SessionGraphlets::new();
    let spec = GraphletSpec {
        kind: GraphletKind::Component,
        anchors: vec![a_id.to_string()],
        primary_anchor: Some(a_id.to_string()),
        selectors: Vec::new(),
    };
    let id = idx.record_linked(&graph, spec);
    assert!(idx.has_linked(), "the index has a Linked graphlet");
    assert!(!idx.reconcile_all(&graph), "no drift yet → no change");

    link(&mut graph, b, c); // C joins A's component
    assert!(idx.reconcile_all(&graph), "reconcile_all reports the drift");
    assert_eq!(
        idx.get(id).unwrap().anchors.len(),
        3,
        "the Linked roster grew to A, B, C"
    );
}

#[test]
fn toggle_family_selector_mutates_a_linked_specs_selectors_and_is_a_noop_elsewhere() {
    let mut idx = SessionGraphlets::new().with_default_session();
    let session_id = idx.graphlets()[0].id;
    let spec = GraphletSpec {
        kind: GraphletKind::Component,
        anchors: Vec::new(),
        primary_anchor: None,
        selectors: Vec::new(),
    };
    let linked_id = idx.record_linked(&Graph::new(), spec);

    assert!(
        !idx.toggle_family_selector(session_id, EdgeFamily::Semantic),
        "a Session binding has no spec to edit"
    );

    assert!(idx.toggle_family_selector(linked_id, EdgeFamily::Semantic));
    assert!(spec_has_family(
        match &idx.get(linked_id).unwrap().binding {
            GraphletBinding::Linked { spec } => spec,
            _ => unreachable!(),
        },
        EdgeFamily::Semantic
    ));

    assert!(idx.toggle_family_selector(linked_id, EdgeFamily::Semantic));
    assert!(!spec_has_family(
        match &idx.get(linked_id).unwrap().binding {
            GraphletBinding::Linked { spec } => spec,
            _ => unreachable!(),
        },
        EdgeFamily::Semantic
    ));
}

#[test]
fn toggle_family_selector_narrows_a_linked_graphlets_derivation() {
    use euclid::default::Point2D;
    use kernel::graph::{ContainmentSubKind, EdgeAssertion, SemanticSubKind};
    let mut graph = Graph::new();
    let a = graph.add_node("https://a".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b".to_string(), Point2D::new(1.0, 0.0));
    let c = graph.add_node("https://c".to_string(), Point2D::new(2.0, 0.0));
    graph.assert_relation(
        a,
        b,
        EdgeAssertion::Semantic {
            sub_kind: SemanticSubKind::Hyperlink,
            label: None,
            decay_progress: None,
        },
    );
    graph.assert_relation(
        b,
        c,
        EdgeAssertion::Containment {
            sub_kind: ContainmentSubKind::Domain,
        },
    );
    let a_id = graph.get_node(a).unwrap().id;

    let mut idx = SessionGraphlets::new();
    let spec = GraphletSpec {
        kind: GraphletKind::Component,
        anchors: vec![a_id.to_string()],
        primary_anchor: Some(a_id.to_string()),
        selectors: Vec::new(),
    };
    let id = idx.record_linked(&graph, spec);
    assert_eq!(
        idx.get(id).unwrap().anchors.len(),
        3,
        "an unfiltered component of A follows both families to reach A, B, C"
    );

    assert!(idx.toggle_family_selector(id, EdgeFamily::Semantic));
    let delta = idx
        .reconcile(&graph, id)
        .expect("narrowing to Semantic drops C, which only reaches B by Containment");
    assert_eq!(delta.removed, vec![graph.get_node(c).unwrap().id]);
    assert_eq!(idx.get(id).unwrap().anchors.len(), 2, "now just A, B");
}
