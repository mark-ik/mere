// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Node + edge CRUD, lifecycle, and iterator tests.

use super::super::*;

fn hyperlink() -> EdgeAssertion {
    EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Hyperlink,
        label: None,
        decay_progress: None,
    }
}

fn user_grouped(label: Option<&str>) -> EdgeAssertion {
    EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::UserGrouped,
        label: label.map(str::to_string),
        decay_progress: None,
    }
}

fn cites() -> EdgeAssertion {
    EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Cites,
        label: None,
        decay_progress: None,
    }
}

#[test]
fn test_graph_new() {
    let graph = Graph::new();
    assert_eq!(graph.node_count(), 0);
    assert_eq!(graph.edge_count(), 0);
}

#[test]
fn revision_advances_on_structural_change_only() {
    let mut graph = Graph::new();
    assert_eq!(graph.revision(), 0, "a fresh graph starts at revision 0");

    let a = graph.add_node("https://a.example".to_string(), Point2D::new(0.0, 0.0));
    let after_a = graph.revision();
    assert!(after_a > 0, "adding a node advances the revision");

    let b = graph.add_node("https://b.example".to_string(), Point2D::new(1.0, 0.0));
    assert!(
        graph.revision() > after_a,
        "a second node advances it again"
    );

    // A new edge (relation) is structural.
    let before_edge = graph.revision();
    graph.assert_relation(a, b, hyperlink());
    let after_edge = graph.revision();
    assert!(after_edge > before_edge, "a new edge advances the revision");

    // A content edit (the node url) is not structural: the revision holds, so a structural cache
    // is not needlessly invalidated by navigation/renames.
    graph.update_node_url(a, "https://a2.example".to_string());
    assert_eq!(
        graph.revision(),
        after_edge,
        "a url edit is content, not structure"
    );

    // Removing a node is structural.
    graph.remove_node(b);
    assert!(
        graph.revision() > after_edge,
        "removing a node advances the revision"
    );
}

#[test]
fn test_add_node() {
    let mut graph = Graph::new();
    let pos = Point2D::new(100.0, 200.0);
    let key = graph.add_node("https://example.com".to_string(), pos);

    let node = graph.get_node(key).unwrap();
    assert_eq!(node.url(), "https://example.com");
    assert_eq!(node.title, "https://example.com");
    assert_eq!(graph.node_is_pinned(key), Some(false));
}

#[test]
fn node_namespace_id_is_deterministic_v5() {
    // The deriver the ingest layer uses for cross-host identity: a URL maps to a
    // stable name-based UUIDv5, and distinct URLs map to distinct ids. (Raw
    // add_node keeps random ids; see test_duplicate_url_nodes_have_distinct_ids.)
    let a = Graph::node_namespace_id("https://example.com/x");
    let b = Graph::node_namespace_id("https://example.com/x");
    assert_eq!(a, b, "same URL yields the same id on any host");
    assert_eq!(a.get_version_num(), 5, "a name-based UUIDv5, not random");
    assert_ne!(
        a,
        Graph::node_namespace_id("https://example.com/y"),
        "a different URL gets a different id",
    );
}

#[test]
fn test_add_multiple_nodes() {
    let mut graph = Graph::new();
    let key1 = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let key2 = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));
    let key3 = graph.add_node("https://c.com".to_string(), Point2D::new(2.0, 2.0));

    assert_eq!(graph.node_count(), 3);
    assert!(graph.get_node(key1).is_some());
    assert!(graph.get_node(key2).is_some());
    assert!(graph.get_node(key3).is_some());
}

#[test]
fn test_duplicate_url_nodes_have_distinct_ids() {
    let mut graph = Graph::new();
    let key1 = graph.add_node("https://same.com".to_string(), Point2D::new(0.0, 0.0));
    let key2 = graph.add_node("https://same.com".to_string(), Point2D::new(10.0, 10.0));

    assert_ne!(key1, key2);
    let node1 = graph.get_node(key1).unwrap();
    let node2 = graph.get_node(key2).unwrap();
    assert_ne!(node1.id, node2.id);
    assert_eq!(graph.get_nodes_by_url("https://same.com").len(), 2);
}

#[test]
fn test_get_node_by_url() {
    let mut graph = Graph::new();
    graph.add_node("https://example.com".to_string(), Point2D::new(0.0, 0.0));

    let (_, node) = graph.get_node_by_url("https://example.com").unwrap();
    assert_eq!(node.url(), "https://example.com");

    assert!(graph.get_node_by_url("https://notfound.com").is_none());
}

#[test]
fn test_get_node_mut() {
    let mut graph = Graph::new();
    let key = graph.add_node("https://example.com".to_string(), Point2D::new(0.0, 0.0));

    assert!(graph.set_node_pinned(key, true));
    assert_eq!(graph.node_is_pinned(key), Some(true));
}

// `test_projected_position_is_the_single_node_position` left with `Node.position`
// (S2): position is no longer graph truth (not a node field, not in the
// snapshot). The live position is seiche's; the durable one the cartography
// sidecar's.

#[test]
fn test_assert_relation() {
    let mut graph = Graph::new();
    let node1 = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let node2 = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));

    graph.assert_relation(node1, node2, hyperlink()).unwrap();

    // Check adjacency via graph methods
    assert!(graph.has_edge_between(node1, node2));
    assert!(!graph.has_edge_between(node2, node1));
    assert_eq!(graph.out_neighbors(node1).count(), 1);
    assert_eq!(graph.in_neighbors(node2).count(), 1);
}

#[test]
fn test_assert_relation_invalid_nodes() {
    let mut graph = Graph::new();
    let node1 = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));

    let invalid_key = NodeIndex::new(999);

    assert!(
        graph
            .assert_relation(invalid_key, node1, hyperlink())
            .is_none()
    );
    assert!(
        graph
            .assert_relation(node1, invalid_key, hyperlink())
            .is_none()
    );
}

#[test]
fn test_assert_multiple_relations() {
    let mut graph = Graph::new();
    let node1 = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let node2 = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));
    let node3 = graph.add_node("https://c.com".to_string(), Point2D::new(2.0, 2.0));

    graph.assert_relation(node1, node2, hyperlink()).unwrap();
    graph.assert_relation(node1, node3, hyperlink()).unwrap();
    graph.assert_relation(node2, node3, hyperlink()).unwrap();

    assert_eq!(graph.edge_count(), 3);

    // Check node1 has 2 outgoing neighbors
    assert_eq!(graph.out_neighbors(node1).count(), 2);

    // Check node3 has 2 incoming neighbors
    assert_eq!(graph.in_neighbors(node3).count(), 2);
}

#[test]
fn test_retract_relation_by_sub_kind_between_nodes() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));

    graph.assert_relation(a, b, hyperlink()).unwrap();
    graph.assert_relation(a, b, user_grouped(None)).unwrap();

    let removed = graph.retract_relations(
        a,
        b,
        RelationSelector::Semantic(SemanticSubKind::UserGrouped),
    );
    assert_eq!(removed, 1);
    assert_eq!(graph.edge_count(), 1);
    let edge_key = graph.find_edge_key(a, b).expect("remaining hyperlink edge");
    let payload = graph.get_edge(edge_key).expect("remaining edge payload");
    assert!(payload.has_relation(RelationSelector::Semantic(SemanticSubKind::Hyperlink)));
    assert!(!payload.has_relation(RelationSelector::Semantic(SemanticSubKind::UserGrouped)));
}

/// The statement-aware write API (petgraph-RDF Phase 1): assert returns a
/// device-safe fact handle; content-dedup returns the SAME handle (updating
/// metadata in place); retract-by-id removes exactly that fact, clears the
/// bucket, and removes the petgraph edge when the payload empties.
#[test]
fn statement_assert_dedups_by_content_and_retracts_by_id() {
    use crate::graph::SemanticStatementSpec;
    use crate::types::GraphScope;

    let mut graph = Graph::new();
    let a = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));

    let cites = SemanticStatementSpec {
        predicate: "http://purl.org/spar/cito/cites".to_string(),
        graph_scope: GraphScope::User,
        provenance_iri: Some("mere://persona/mark".to_string()),
        asserted_at_ms: Some(1_000),
        ..Default::default()
    };
    let (edge, first) = graph
        .assert_semantic_statement(a, b, cites.clone())
        .unwrap();
    assert!(first.changed, "fresh statement asserts");

    // Same content on a different scope = a SECOND fact on the same pair.
    let source_scope = SemanticStatementSpec {
        graph_scope: GraphScope::Source,
        ..cites.clone()
    };
    let (edge2, second) = graph.assert_semantic_statement(a, b, source_scope).unwrap();
    assert_eq!(edge, edge2, "one pair bucket");
    assert!(second.changed);
    assert_ne!(first.statement_id, second.statement_id);
    assert_eq!(graph.get_edge(edge).unwrap().semantic_statements().len(), 2);

    // Exact re-assert dedups to the same handle, no change.
    let (_, again) = graph
        .assert_semantic_statement(a, b, cites.clone())
        .unwrap();
    assert_eq!(again.statement_id, first.statement_id);
    assert!(!again.changed, "identical statement is a no-op");

    // Metadata update on the deduped statement keeps the handle, reports change.
    let relabeled = SemanticStatementSpec {
        label: Some("cited in the intro".to_string()),
        ..cites
    };
    let (_, updated) = graph.assert_semantic_statement(a, b, relabeled).unwrap();
    assert_eq!(updated.statement_id, first.statement_id);
    assert!(updated.changed);

    // Precise retract: the other statement survives; retracting the last
    // semantic statement (with no other family payload) removes the edge.
    assert!(graph.retract_semantic_statement(a, b, &first.statement_id));
    assert_eq!(graph.get_edge(edge).unwrap().semantic_statements().len(), 1);
    assert!(
        !graph.retract_semantic_statement(a, b, &first.statement_id),
        "retract is idempotent per id"
    );
    assert!(graph.retract_semantic_statement(a, b, &second.statement_id));
    assert!(
        graph.find_edge_key(a, b).is_none(),
        "an emptied payload removes the petgraph edge"
    );
}

#[test]
fn test_assert_relation_merges_semantics_on_single_stored_edge() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));

    graph.assert_relation(a, b, hyperlink()).unwrap();
    graph
        .assert_relation(a, b, user_grouped(Some("tab-group")))
        .unwrap();

    assert_eq!(graph.edge_count(), 1);
    let edge_key = graph.find_edge_key(a, b).unwrap();
    let payload = graph.get_edge(edge_key).unwrap();
    assert!(payload.has_relation(RelationSelector::Semantic(SemanticSubKind::Hyperlink)));
    assert!(payload.has_relation(RelationSelector::Semantic(SemanticSubKind::UserGrouped)));
    assert_eq!(payload.label(), Some("tab-group"));
    // Two semantic sub-kinds on the same stored edge → two relation rows.
    assert_eq!(graph.relations().count(), 2);
}

#[test]
fn test_statement_bucket_keeps_multiple_predicates_on_one_stored_edge() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));

    graph.assert_relation(a, b, cites()).unwrap();
    graph
        .assert_semantic_predicate(a, b, "https://schema.org/citation".to_string())
        .unwrap();

    assert_eq!(graph.edge_count(), 1);
    let edge_key = graph.find_edge_key(a, b).unwrap();
    let payload = graph.get_edge(edge_key).unwrap();
    assert_eq!(payload.semantic_statements().len(), 2);
    assert!(payload.semantic_statements().iter().any(|statement| {
        statement.recognized_sub_kind == Some(SemanticSubKind::Cites)
            && statement.predicate == "https://mere.computer/ns/rel#cites"
    }));
    assert!(payload.semantic_statements().iter().any(|statement| {
        statement.recognized_sub_kind.is_none()
            && statement.predicate == "https://schema.org/citation"
    }));
}

#[test]
fn test_assert_relation_preserves_generic_semantic_subkind() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));

    graph
        .assert_relation(
            a,
            b,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::SameEntityAs,
                label: Some("identity".to_string()),
                decay_progress: None,
            },
        )
        .expect("semantic relation should be asserted");

    let edge_key = graph
        .find_edge_key(a, b)
        .expect("semantic edge should exist");
    let payload = graph
        .get_edge(edge_key)
        .expect("semantic payload should exist");
    assert!(payload.has_relation(RelationSelector::Semantic(SemanticSubKind::SameEntityAs)));
    assert_eq!(payload.label(), Some("identity"));

    let semantic_edges = graph.semantic_edges().collect::<Vec<_>>();
    assert!(semantic_edges.iter().any(|edge| {
        edge.from == a
            && edge.to == b
            && edge.sub_kind == SemanticSubKind::SameEntityAs
            && edge.label.as_deref() == Some("identity")
    }));
}

#[test]
fn test_remove_node() {
    let mut graph = Graph::new();
    let n1 = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let n2 = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));
    let _ = graph.assert_relation(n1, n2, hyperlink());

    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.edge_count(), 1);

    assert!(graph.remove_node(n1));
    assert_eq!(graph.node_count(), 1);
    assert_eq!(graph.edge_count(), 0); // edge auto-removed
    assert!(graph.get_node(n1).is_none());
    assert!(graph.get_node_by_url("https://a.com").is_none());

    // n2 still exists
    assert!(graph.get_node(n2).is_some());
}

#[test]
fn test_remove_nonexistent_node() {
    let mut graph = Graph::new();
    assert!(!graph.remove_node(NodeIndex::new(999)));
}

#[test]
fn test_nodes_iterator() {
    let mut graph = Graph::new();
    graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));
    graph.add_node("https://c.com".to_string(), Point2D::new(2.0, 2.0));

    let urls: Vec<String> = graph.nodes().map(|(_, n)| n.url().to_string()).collect();
    assert_eq!(urls.len(), 3);
    assert!(urls.contains(&"https://a.com".to_string()));
    assert!(urls.contains(&"https://b.com".to_string()));
    assert!(urls.contains(&"https://c.com".to_string()));
}

#[test]
fn test_relations_iterator() {
    let mut graph = Graph::new();
    let node1 = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let node2 = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));
    let node3 = graph.add_node("https://c.com".to_string(), Point2D::new(2.0, 2.0));

    let _ = graph.assert_relation(node1, node2, hyperlink());
    let _ = graph.assert_relation(node1, node3, hyperlink());

    let relation_count = graph.relations().count();
    assert_eq!(relation_count, 2);

    assert!(graph.inner.inner().edge_references().all(|edge| {
        edge.weight()
            .has_relation(RelationSelector::Semantic(SemanticSubKind::Hyperlink))
    }));
}

#[test]
fn test_empty_graph_operations() {
    let graph = Graph::new();

    assert_eq!(graph.node_count(), 0);
    assert_eq!(graph.edge_count(), 0);
    assert!(graph.get_node_by_url("https://example.com").is_none());

    let invalid_key = NodeIndex::new(999);
    assert!(graph.get_node(invalid_key).is_none());
}

#[test]
fn test_node_count() {
    let mut graph = Graph::new();
    assert_eq!(graph.node_count(), 0);

    graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    assert_eq!(graph.node_count(), 1);

    graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));
    assert_eq!(graph.node_count(), 2);
}

#[test]
fn test_edge_count() {
    let mut graph = Graph::new();
    let node1 = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let node2 = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 1.0));

    assert_eq!(graph.edge_count(), 0);

    let _ = graph.assert_relation(node1, node2, hyperlink());
    assert_eq!(graph.edge_count(), 1);

    let _ = graph.assert_relation(node2, node1, hyperlink());
    assert_eq!(graph.edge_count(), 2);
}
