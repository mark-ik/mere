/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! URL update + cold-restore + navigation-history tests — exercises the snapshot
//! path through the graph-level shared navigation history (one visit space, owner
//! per node; the (b) anchor design).

use super::super::*;

/// A `GraphSnapshot` carrying one node and the given shared navigation history.
fn snapshot_with(node: crate::persistence::PersistedNode, nav: SharedNavigationMemory) -> crate::persistence::GraphSnapshot {
    crate::persistence::GraphSnapshot {
        nodes: vec![node],
        edges: vec![],
        import_records: vec![],
        timestamp_secs: 0,
        fields: vec![],
        couplings: vec![],
        navigation: nav,
    }
}

/// A minimal `PersistedNode` at `url` with id `node_id` (no session state).
fn persisted_node(node_id: Uuid, url: &str) -> crate::persistence::PersistedNode {
    use crate::persistence::{PersistedAddress, PersistedNode};
    PersistedNode {
        node_id: node_id.to_string(),
        url: url.to_string(),
        cached_host: None,
        title: "Node".to_string(),
        tags: vec![],
        tag_presentation: NodeTagPresentationState::default(),
        import_provenance: vec![],
        is_pinned: false,
        thumbnail_png: None,
        thumbnail_width: 0,
        thumbnail_height: 0,
        favicon_rgba: None,
        favicon_width: 0,
        favicon_height: 0,
        session_state: None,
        mime_hint: None,
        address: PersistedAddress::Http(url.to_string()),
        classifications: Vec::new(),
        frame_layout_hints: Vec::new(),
        frame_split_offer_suppressed: false,
        properties: Vec::new(),
        derivations: Vec::new(),
        body: None,
    }
}

#[test]
fn node_body_round_trips() {
    // The inline `Node` body (a knot note's source) survives the snapshot
    // round-trip: PersistedNode.body -> Node.body -> PersistedNode.body. (Slice 3.)
    let id = Uuid::new_v4();
    let mut pnode = persisted_node(id, "knot://notes");
    pnode.body = Some("# Notes\n\nbody".to_string());
    let graph = Graph::from_snapshot(&snapshot_with(pnode, SharedNavigationMemory::empty()));
    let node = graph.nodes().next().expect("node restored").1;
    assert_eq!(node.body.as_deref(), Some("# Notes\n\nbody"));
    assert_eq!(
        graph.to_snapshot().nodes[0].body.as_deref(),
        Some("# Notes\n\nbody"),
    );
}

#[test]
fn test_update_node_url() {
    let mut graph = Graph::new();
    let key = graph.add_node("old".to_string(), Point2D::new(0.0, 0.0));

    let old = graph.update_node_url(key, "new".to_string());

    assert_eq!(old, Some("old".to_string()));
    assert_eq!(graph.get_node(key).unwrap().url(), "new");
    assert!(graph.get_node_by_url("new").is_some());
    assert!(graph.get_node_by_url("old").is_none());
}

#[test]
fn test_update_node_url_nonexistent() {
    let mut graph = Graph::new();
    let fake_key = NodeKey::new(999);

    assert_eq!(graph.update_node_url(fake_key, "x".to_string()), None);
}

#[test]
fn test_cold_restore_reapplies_history_index() {
    let node_id = Uuid::new_v4();
    let mut nav = SharedNavigationMemory::empty();
    nav.seed_linear(
        node_id,
        vec![
            "https://example.com/one".to_string(),
            "https://example.com/two".to_string(),
            "https://example.com/three".to_string(),
        ],
        2,
    );
    let snapshot = snapshot_with(persisted_node(node_id, "https://fallback.example"), nav);

    let restored = Graph::from_snapshot(&snapshot);
    let (key, _) = restored.get_node_by_id(node_id).unwrap();
    let history = restored.node_history_projection(key);
    assert_eq!(history.entries.len(), 3);
    assert_eq!(history.current_index, 2);
    // The restored current page follows the history cursor.
    assert_eq!(restored.node_current_url(key).as_deref(), Some("https://example.com/three"));
}

#[test]
fn navigate_forward_fork_preserves_the_alternate_branch() {
    // The real user path (navigate / back / navigate) forward-forks: from `a`,
    // navigating to `b`, stepping back, then navigating to `c` keeps `b` as an
    // alternate child of `a` (temporal integrity), now over the shared space.
    let mut graph = Graph::new();
    let key = graph.add_node("https://example.com/a".to_string(), Point2D::new(0.0, 0.0));
    graph.navigate_node(key, "https://example.com/a");
    graph.navigate_node(key, "https://example.com/b");
    assert_eq!(graph.node_history_back(key).as_deref(), Some("https://example.com/a"));
    graph.navigate_node(key, "https://example.com/c");

    let branch = graph.node_history_branch_projection(key);
    let a_visit = branch
        .visits
        .iter()
        .find(|v| v.url == "https://example.com/a")
        .expect("a on the active path");
    assert!(
        a_visit.alternate_children.iter().any(|alt| alt.url == "https://example.com/b"),
        "b is preserved as an alternate branch off a after the forward-fork",
    );
    assert_eq!(graph.node_current_url(key).as_deref(), Some("https://example.com/c"));
}

#[test]
fn test_cold_restore_reapplies_scroll_offset() {
    use crate::persistence::PersistedNodeSessionState;

    let node_id = Uuid::new_v4();
    let mut node = persisted_node(node_id, "https://example.com");
    node.session_state = Some(PersistedNodeSessionState {
        scroll_x: Some(20.0),
        scroll_y: Some(640.0),
        form_draft: None,
    });
    let snapshot = snapshot_with(node, SharedNavigationMemory::empty());

    let restored = Graph::from_snapshot(&snapshot);
    let (_, node) = restored.get_node_by_url("https://example.com").unwrap();
    assert_eq!(node.session_scroll, Some((20.0, 640.0)));
}

#[test]
fn test_restore_fallback_without_session_state() {
    let node_id = Uuid::new_v4();
    let mut nav = SharedNavigationMemory::empty();
    nav.seed_linear(node_id, vec!["https://legacy-one.example".to_string()], 0);
    let snapshot = snapshot_with(persisted_node(node_id, "https://fallback.example"), nav);

    let restored = Graph::from_snapshot(&snapshot);
    let (key, _) = restored.get_node_by_id(node_id).unwrap();
    let history = restored.node_history_projection(key);
    assert_eq!(history.entries, vec!["https://legacy-one.example".to_string()]);
    assert_eq!(history.current_index, 0);
    assert_eq!(restored.get_node(key).unwrap().session_scroll, None);
}
