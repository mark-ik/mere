/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! URL update + cold-restore + navigation-memory tests — exercises
//! the snapshot path through per-node navigation state.

use super::super::*;

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
    use crate::persistence::{
        GraphSnapshot, PersistedAddress, PersistedNode, PersistedNodeSessionState,
    };

    let node_id = Uuid::new_v4();
    let snapshot = GraphSnapshot {
        nodes: vec![PersistedNode {
            node_id: node_id.to_string(),
            url: "https://fallback.example".to_string(),
            cached_host: None,
            title: "Node".to_string(),
            position_x: 0.0,
            position_y: 0.0,
            tags: vec![],
            tag_presentation: NodeTagPresentationState::default(),
            import_provenance: vec![],
            is_pinned: false,
            navigation_memory: NodeNavigationMemory::from_linear_history(
                vec![
                    "https://example.com/one".to_string(),
                    "https://example.com/two".to_string(),
                    "https://example.com/three".to_string(),
                ],
                2,
            ),
            thumbnail_png: None,
            thumbnail_width: 0,
            thumbnail_height: 0,
            favicon_rgba: None,
            favicon_width: 0,
            favicon_height: 0,
            session_state: Some(PersistedNodeSessionState {
                scroll_x: Some(4.0),
                scroll_y: Some(120.0),
                form_draft: None,
            }),
            mime_hint: None,
            address: PersistedAddress::Http("https://fallback.example".to_string()),
            classifications: Vec::new(),
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
        }],
        edges: vec![],
        import_records: vec![],
        timestamp_secs: 0,
    };

    let restored = Graph::from_snapshot(&snapshot);
    let (_, node) = restored.get_node_by_id(node_id).unwrap();
    let history = node.history_projection();
    assert_eq!(history.entries.len(), 3);
    assert_eq!(history.current_index, 2);
}

#[test]
fn test_navigation_memory_preserves_alternate_branch_when_history_diverges() {
    let mut memory = NodeNavigationMemory::from_linear_history(
        vec![
            "https://example.com/a".to_string(),
            "https://example.com/b".to_string(),
        ],
        1,
    );

    memory.replace_linear_history(vec!["https://example.com/a".to_string()], 0);
    memory.replace_linear_history(
        vec![
            "https://example.com/a".to_string(),
            "https://example.com/c".to_string(),
        ],
        1,
    );

    let branch = memory.branch_projection();
    assert_eq!(branch.current_index, Some(1));
    assert_eq!(branch.visits.len(), 2);
    assert_eq!(branch.visits[0].url, "https://example.com/a");
    assert_eq!(branch.visits[1].url, "https://example.com/c");
    assert!(branch.visits[1].is_current);
    assert_eq!(branch.visits[0].alternate_children.len(), 1);
    assert_eq!(
        branch.visits[0].alternate_children[0].url,
        "https://example.com/b"
    );
}

#[test]
fn test_cold_restore_reapplies_scroll_offset() {
    use crate::persistence::{
        GraphSnapshot, PersistedAddress, PersistedNode, PersistedNodeSessionState,
    };

    let snapshot = GraphSnapshot {
        nodes: vec![PersistedNode {
            node_id: Uuid::new_v4().to_string(),
            url: "https://example.com".to_string(),
            cached_host: None,
            title: "Node".to_string(),
            position_x: 0.0,
            position_y: 0.0,
            tags: vec![],
            tag_presentation: NodeTagPresentationState::default(),
            import_provenance: vec![],
            is_pinned: false,
            navigation_memory: NodeNavigationMemory::empty(),
            thumbnail_png: None,
            thumbnail_width: 0,
            thumbnail_height: 0,
            favicon_rgba: None,
            favicon_width: 0,
            favicon_height: 0,
            session_state: Some(PersistedNodeSessionState {
                scroll_x: Some(20.0),
                scroll_y: Some(640.0),
                form_draft: None,
            }),
            mime_hint: None,
            address: PersistedAddress::Http("https://example.com".to_string()),
            classifications: Vec::new(),
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
        }],
        edges: vec![],
        import_records: vec![],
        timestamp_secs: 0,
    };

    let restored = Graph::from_snapshot(&snapshot);
    let (_, node) = restored.get_node_by_url("https://example.com").unwrap();
    assert_eq!(node.session_scroll, Some((20.0, 640.0)));
}

#[test]
fn test_restore_fallback_without_session_state() {
    use crate::persistence::{GraphSnapshot, PersistedAddress, PersistedNode};

    let snapshot = GraphSnapshot {
        nodes: vec![PersistedNode {
            node_id: Uuid::new_v4().to_string(),
            url: "https://fallback.example".to_string(),
            cached_host: None,
            title: "Node".to_string(),
            position_x: 0.0,
            position_y: 0.0,
            tags: vec![],
            tag_presentation: NodeTagPresentationState::default(),
            import_provenance: vec![],
            is_pinned: false,
            navigation_memory: NodeNavigationMemory::from_linear_history(
                vec!["https://legacy-one.example".to_string()],
                0,
            ),
            thumbnail_png: None,
            thumbnail_width: 0,
            thumbnail_height: 0,
            favicon_rgba: None,
            favicon_width: 0,
            favicon_height: 0,
            session_state: None,
            mime_hint: None,
            address: PersistedAddress::Http("https://fallback.example".to_string()),
            classifications: Vec::new(),
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
        }],
        edges: vec![],
        import_records: vec![],
        timestamp_secs: 0,
    };

    let restored = Graph::from_snapshot(&snapshot);
    let (_, node) = restored
        .get_node_by_url("https://fallback.example")
        .unwrap();
    assert_eq!(
        node.history_entries(),
        vec!["https://legacy-one.example".to_string()]
    );
    assert_eq!(node.history_index(), 0);
    assert_eq!(node.session_scroll, None);
}
