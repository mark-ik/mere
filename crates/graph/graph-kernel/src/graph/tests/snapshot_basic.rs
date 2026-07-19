// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Snapshot round-trip basics — graph, edges, edge types, semantic
//! sub-kinds, favicon, thumbnail, UUID, duplicate-URL handling.

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
fn assert_semantic_predicate_creates_open_predicate_edge() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.test/".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.test/".to_string(), Point2D::new(0.0, 0.0));
    let key = graph
        .assert_semantic_predicate(a, b, "https://schema.org/citation".to_string())
        .expect("edge created");
    let payload = graph.get_edge(key).expect("payload");
    // Carries no sub-kinds, yet still reports the Semantic family.
    assert!(payload.has_relation(RelationSelector::Family(EdgeFamily::Semantic)));
    assert!(!payload.is_empty());
    assert!(
        payload
            .semantic_data()
            .is_some_and(|d| d.sub_kinds.is_empty())
    );
    assert_eq!(
        payload.semantic_data().and_then(|d| d.predicate.as_deref()),
        Some("https://schema.org/citation")
    );
}

#[test]
fn open_predicate_only_edge_survives_snapshot_roundtrip() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.test/".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.test/".to_string(), Point2D::new(0.0, 0.0));
    graph.assert_semantic_predicate(a, b, "https://schema.org/citation".to_string());

    let restored = Graph::from_snapshot(&graph.to_snapshot());

    assert_eq!(restored.edge_count(), 1);
    let (ra, _) = restored.get_node_by_url("https://a.test/").unwrap();
    let (rb, _) = restored.get_node_by_url("https://b.test/").unwrap();
    let key = restored.find_edge_key(ra, rb).expect("edge restored");
    let payload = restored.get_edge(key).expect("payload");
    assert!(payload.has_relation(RelationSelector::Family(EdgeFamily::Semantic)));
    assert_eq!(
        payload.semantic_data().and_then(|d| d.predicate.as_deref()),
        Some("https://schema.org/citation")
    );
}

#[test]
fn statement_bucket_survives_snapshot_roundtrip() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.test/".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.test/".to_string(), Point2D::new(0.0, 0.0));
    graph.assert_relation(a, b, cites()).unwrap();
    graph.assert_semantic_predicate_in_scope(
        a,
        b,
        "https://schema.org/citation".to_string(),
        crate::types::GraphScope::Source,
    );
    let edge_key = graph.find_edge_key(a, b).expect("edge key");
    let payload = graph.get_edge_mut(edge_key).expect("payload");
    let statements = &mut payload.semantic.as_mut().expect("semantic").statements;
    statements[0].statement_id = "stmt-edge-1".to_string();
    statements[0].provenance_iri = Some("https://people.test/alice".to_string());
    statements[0].asserted_at_ms = Some(1_720_000_000_123);
    statements[1].statement_id = "stmt-edge-2".to_string();
    statements[1].provenance_iri = Some("https://people.test/bob".to_string());
    statements[1].asserted_at_ms = Some(1_720_000_100_456);

    let restored = Graph::from_snapshot(&graph.to_snapshot());

    let (ra, _) = restored.get_node_by_url("https://a.test/").unwrap();
    let (rb, _) = restored.get_node_by_url("https://b.test/").unwrap();
    let key = restored.find_edge_key(ra, rb).expect("edge restored");
    let payload = restored.get_edge(key).expect("payload");
    assert_eq!(payload.semantic_statements().len(), 2);
    assert!(payload.semantic_statements().iter().any(|statement| {
        statement.recognized_sub_kind == Some(SemanticSubKind::Cites)
            && statement.predicate == "https://mere.computer/ns/rel#cites"
            && statement.graph_scope == crate::types::GraphScope::Default
            && statement.statement_id == "stmt-edge-1"
            && statement.provenance_iri.as_deref() == Some("https://people.test/alice")
            && statement.asserted_at_ms == Some(1_720_000_000_123)
    }));
    assert!(payload.semantic_statements().iter().any(|statement| {
        statement.recognized_sub_kind.is_none()
            && statement.predicate == "https://schema.org/citation"
            && statement.graph_scope == crate::types::GraphScope::Source
            && statement.statement_id == "stmt-edge-2"
            && statement.provenance_iri.as_deref() == Some("https://people.test/bob")
            && statement.asserted_at_ms == Some(1_720_000_100_456)
    }));
}

#[test]
fn node_properties_survive_snapshot_roundtrip() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.test/".to_string(), Point2D::new(0.0, 0.0));
    let mut property = crate::types::NodeProperty::new(
        "https://schema.org/datePublished".to_string(),
        "2026-06-02".to_string(),
    )
    .with_graph_scope(crate::types::GraphScope::Source)
    .with_metadata(
        Some("https://people.test/alice".to_string()),
        Some(1_720_000_000_123),
    );
    property.statement_id = "stmt-property-1".to_string();
    property.datatype = Some("http://www.w3.org/2001/XMLSchema#date".to_string());
    graph.get_node_mut(a).unwrap().properties.push(property);

    let restored = Graph::from_snapshot(&graph.to_snapshot());

    let (_, node) = restored.get_node_by_url("https://a.test/").unwrap();
    assert_eq!(node.properties.len(), 1);
    assert_eq!(
        node.properties[0].predicate,
        "https://schema.org/datePublished"
    );
    assert_eq!(node.properties[0].value, "2026-06-02");
    assert_eq!(
        node.properties[0].datatype.as_deref(),
        Some("http://www.w3.org/2001/XMLSchema#date")
    );
    assert_eq!(node.properties[0].lang.as_deref(), None);
    assert_eq!(
        node.properties[0].graph_scope,
        crate::types::GraphScope::Source
    );
    assert_eq!(node.properties[0].statement_id, "stmt-property-1");
    assert_eq!(
        node.properties[0].provenance_iri.as_deref(),
        Some("https://people.test/alice")
    );
    assert_eq!(node.properties[0].asserted_at_ms, Some(1_720_000_000_123));
}

#[test]
fn test_snapshot_roundtrip() {
    let mut graph = Graph::new();
    let n1 = graph.add_node("https://a.com".to_string(), Point2D::new(10.0, 20.0));
    let n2 = graph.add_node("https://b.com".to_string(), Point2D::new(30.0, 40.0));
    let _ = graph.assert_relation(n1, n2, hyperlink());

    graph.get_node_mut(n1).unwrap().title = "Site A".to_string();
    graph.get_node_mut(n2).unwrap().is_pinned = true;

    let snapshot = graph.to_snapshot();
    let restored = Graph::from_snapshot(&snapshot);

    assert_eq!(restored.node_count(), 2);
    assert_eq!(restored.edge_count(), 1);

    let (_, ra) = restored.get_node_by_url("https://a.com").unwrap();
    assert_eq!(ra.title, "Site A");
    // Positions are no longer graph truth (not a node field, not in the snapshot);
    // they live in the cartography sidecar. (Position gut.)

    let (_, rb) = restored.get_node_by_url("https://b.com").unwrap();
    assert!(rb.is_pinned);
}

#[test]
fn test_snapshot_empty_graph() {
    let graph = Graph::new();
    let snapshot = graph.to_snapshot();
    let restored = Graph::from_snapshot(&snapshot);

    assert_eq!(restored.node_count(), 0);
    assert_eq!(restored.edge_count(), 0);
}

#[test]
fn test_snapshot_preserves_edge_types() {
    let mut graph = Graph::new();
    let n1 = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let n2 = graph.add_node("https://b.com".to_string(), Point2D::new(100.0, 0.0));
    let n3 = graph.add_node("https://c.com".to_string(), Point2D::new(200.0, 0.0));
    let _ = graph.assert_relation(n1, n2, hyperlink());
    let _ = graph.append_traversal(n2, n1, NavigationTrigger::LinkClick, Some(1_000_000));
    let _ = graph.assert_relation(n1, n3, user_grouped(None));

    let snapshot = graph.to_snapshot();
    let restored = Graph::from_snapshot(&snapshot);

    assert_eq!(restored.edge_count(), 3);

    let has_hyperlink = restored
        .find_edge_key(n1, n2)
        .and_then(|edge_key| restored.get_edge(edge_key))
        .is_some_and(|payload| {
            payload.has_relation(RelationSelector::Semantic(SemanticSubKind::Hyperlink))
        });
    let has_history = restored
        .find_edge_key(n2, n1)
        .and_then(|edge_key| restored.get_edge(edge_key))
        .is_some_and(|payload| {
            payload.has_relation(RelationSelector::Family(EdgeFamily::Traversal))
        });
    let has_user_grouped = restored
        .find_edge_key(n1, n3)
        .and_then(|edge_key| restored.get_edge(edge_key))
        .is_some_and(|payload| {
            payload.has_relation(RelationSelector::Semantic(SemanticSubKind::UserGrouped))
        });
    assert!(has_hyperlink);
    assert!(has_history);
    assert!(has_user_grouped);
}

#[test]
fn test_snapshot_preserves_user_grouped_edge_label() {
    let mut graph = Graph::new();
    let from = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let to = graph.add_node("https://b.com".to_string(), Point2D::new(100.0, 0.0));
    graph
        .assert_relation(from, to, user_grouped(Some("tab-group")))
        .unwrap();

    let snapshot = graph.to_snapshot();
    let restored = Graph::from_snapshot(&snapshot);
    let edge_key = restored.find_edge_key(from, to).unwrap();
    let payload = restored.get_edge(edge_key).unwrap();
    assert_eq!(payload.label(), Some("tab-group"));
}

#[test]
fn test_snapshot_preserves_generic_semantic_relations() {
    let mut graph = Graph::new();
    let from = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let to = graph.add_node("https://b.com".to_string(), Point2D::new(100.0, 0.0));
    graph
        .assert_relation(
            from,
            to,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::CanonicalMirrorOf,
                label: Some("profile".to_string()),
                decay_progress: None,
            },
        )
        .expect("canonical mirror relation should be asserted");

    let snapshot = graph.to_snapshot();
    let restored = Graph::from_snapshot(&snapshot);
    let edge_key = restored
        .find_edge_key(from, to)
        .expect("semantic edge should restore");
    let payload = restored
        .get_edge(edge_key)
        .expect("semantic payload should restore");
    assert!(payload.has_relation(RelationSelector::Semantic(
        SemanticSubKind::CanonicalMirrorOf,
    )));
    assert_eq!(payload.label(), Some("profile"));
}

#[test]
fn test_snapshot_preserves_favicon_data() {
    let mut graph = Graph::new();
    let key = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let favicon = vec![255, 0, 0, 255];
    if let Some(node) = graph.get_node_mut(key) {
        node.favicon_rgba = Some(favicon.clone());
        node.favicon_width = 1;
        node.favicon_height = 1;
    }

    let snapshot = graph.to_snapshot();
    let restored = Graph::from_snapshot(&snapshot);
    let (_, restored_node) = restored.get_node_by_url("https://a.com").unwrap();
    assert_eq!(restored_node.favicon_rgba.as_ref(), Some(&favicon));
    assert_eq!(restored_node.favicon_width, 1);
    assert_eq!(restored_node.favicon_height, 1);
}

#[test]
fn test_snapshot_preserves_thumbnail_data() {
    let mut graph = Graph::new();
    let key = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let thumbnail = vec![137, 80, 78, 71];
    if let Some(node) = graph.get_node_mut(key) {
        node.thumbnail_png = Some(thumbnail.clone());
        node.thumbnail_width = 64;
        node.thumbnail_height = 48;
    }

    let snapshot = graph.to_snapshot();
    let restored = Graph::from_snapshot(&snapshot);
    let (_, restored_node) = restored.get_node_by_url("https://a.com").unwrap();
    assert_eq!(restored_node.thumbnail_png.as_ref(), Some(&thumbnail));
    assert_eq!(restored_node.thumbnail_width, 64);
    assert_eq!(restored_node.thumbnail_height, 48);
}

#[test]
fn test_snapshot_preserves_uuid_identity() {
    let mut graph = Graph::new();
    let key = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let node_id = graph.get_node(key).unwrap().id;

    let snapshot = graph.to_snapshot();
    let restored = Graph::from_snapshot(&snapshot);
    let (_, restored_node) = restored.get_node_by_id(node_id).unwrap();
    assert_eq!(restored_node.url(), "https://a.com");
}

// --- TEST-3: from_snapshot edge cases ---

#[test]
fn test_snapshot_edge_with_missing_url_is_dropped() {
    use crate::persistence::{GraphSnapshot, PersistedAddress, PersistedEdge, PersistedNode};

    let snapshot = GraphSnapshot {
        nodes: vec![PersistedNode {
            node_id: Uuid::new_v4().to_string(),
            url: "https://a.com".to_string(),
            cached_host: None,
            title: String::new(),
            body: None,
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
            address: PersistedAddress::Http("https://a.com".to_string()),
            classifications: Vec::new(),
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
            properties: Vec::new(),
            derivations: Vec::new(),
            last_session_visited: 0,
        }],
        edges: vec![PersistedEdge {
            from_node_id: Uuid::new_v4().to_string(),
            to_node_id: Uuid::new_v4().to_string(),
            families: vec![PersistedEdgeFamily::Semantic],
            semantic: Some(PersistedSemanticEdgeData {
                sub_kinds: vec![PersistedSemanticSubKind::Hyperlink],
                label: None,
                agent_decay_progress: None,
                predicate: None,
                statements: vec![],
            }),
            traversal: None,
            containment: None,
            arrangement: None,
            imported: None,
            provenance: None,
        }],
        import_records: vec![],
        timestamp_secs: 0,
        fields: vec![],
        couplings: vec![],
        navigation: SharedNavigationMemory::empty(),
    };

    let graph = Graph::from_snapshot(&snapshot);

    // Node should be restored, edge should be silently dropped
    assert_eq!(graph.node_count(), 1);
    assert_eq!(graph.edge_count(), 0);
}

#[test]
fn test_snapshot_duplicate_urls_last_wins() {
    use crate::persistence::{GraphSnapshot, PersistedAddress, PersistedNode};

    let snapshot = GraphSnapshot {
        nodes: vec![
            PersistedNode {
                node_id: Uuid::new_v4().to_string(),
                url: "https://same.com".to_string(),
                cached_host: None,
                title: "First".to_string(),
                body: None,
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
                address: PersistedAddress::Http("https://same.com".to_string()),
                classifications: Vec::new(),
                frame_layout_hints: Vec::new(),
                frame_split_offer_suppressed: false,
                properties: Vec::new(),
                derivations: Vec::new(),
                last_session_visited: 0,
            },
            PersistedNode {
                node_id: Uuid::new_v4().to_string(),
                url: "https://same.com".to_string(),
                cached_host: None,
                title: "Second".to_string(),
                body: None,
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
                address: PersistedAddress::Http("https://same.com".to_string()),
                classifications: Vec::new(),
                frame_layout_hints: Vec::new(),
                frame_split_offer_suppressed: false,
                properties: Vec::new(),
                derivations: Vec::new(),
                last_session_visited: 0,
            },
        ],
        edges: vec![],
        import_records: vec![],
        timestamp_secs: 0,
        fields: vec![],
        couplings: vec![],
        navigation: SharedNavigationMemory::empty(),
    };

    let graph = Graph::from_snapshot(&snapshot);

    // Both nodes are created and lookup keeps last inserted semantics.
    assert_eq!(graph.node_count(), 2);
    let (_, node) = graph.get_node_by_url("https://same.com").unwrap();
    assert_eq!(node.title, "Second");
}

// --- Field layer (field-system Phase 2) ---

#[test]
fn test_snapshot_roundtrips_fields_and_couplings() {
    let mut graph = Graph::new();
    graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));

    let fid = FieldId::from_uuid(Uuid::from_u128(0xF1));
    graph.add_field(
        Field::new(
            fid,
            FieldDefinition::Scalar(ScalarField::gaussian_at(1.0, 2.0, 10.0)),
        )
        .with_name("focus")
        .with_extent(FieldExtent::Region {
            min_x: -1.0,
            min_y: -2.0,
            max_x: 3.0,
            max_y: 4.0,
        }),
    );
    // A retired field must keep its definition through the round-trip.
    let retired = FieldId::from_uuid(Uuid::from_u128(0xF2));
    graph.add_field(Field::new(
        retired,
        FieldDefinition::Scalar(ScalarField::Const(1.0)),
    ));
    assert!(graph.retire_field(retired));

    let cid = CouplingId::from_uuid(Uuid::from_u128(0xC1));
    graph.add_coupling(Coupling::new(
        cid,
        fid,
        NodeSelector::Kind("paper".into()),
        CouplingResponse::DampenInside { factor: 0.4 },
        2.5,
    ));

    let restored = Graph::from_snapshot(&graph.to_snapshot());

    let f = restored.field(fid).expect("field restored");
    assert_eq!(f.name.as_deref(), Some("focus"));
    assert_eq!(
        f.extent,
        FieldExtent::Region {
            min_x: -1.0,
            min_y: -2.0,
            max_x: 3.0,
            max_y: 4.0
        }
    );
    assert!(matches!(
        f.definition,
        FieldDefinition::Scalar(ScalarField::Gaussian { .. })
    ));
    assert!(f.is_active());

    let rf = restored.field(retired).expect("retired field restored");
    assert!(!rf.is_active(), "retired lifecycle survives");
    assert!(matches!(
        rf.definition,
        FieldDefinition::Scalar(ScalarField::Const(_))
    ));

    let c = restored
        .couplings_for_field(fid)
        .next()
        .expect("coupling restored");
    assert_eq!(c.id, cid);
    assert_eq!(c.selector, NodeSelector::Kind("paper".into()));
    assert_eq!(c.response, CouplingResponse::DampenInside { factor: 0.4 });
    assert_eq!(c.strength, 2.5);
}

#[test]
fn test_snapshot_without_field_layer_loads_empty() {
    // A pre-field-layer snapshot (JSON missing `fields`/`couplings`) must still
    // load, with an empty field layer — the additive `#[serde(default)]` migration.
    let json = r#"{"nodes":[],"edges":[],"import_records":[],"timestamp_secs":0}"#;
    let snapshot: crate::persistence::GraphSnapshot = serde_json::from_str(json).unwrap();
    assert!(snapshot.fields.is_empty());
    assert!(snapshot.couplings.is_empty());

    let restored = Graph::from_snapshot(&snapshot);
    assert_eq!(restored.fields().count(), 0);
    assert_eq!(restored.couplings().count(), 0);
}

#[test]
fn test_snapshot_roundtrips_open_coupling_response() {
    // The open response tail (a non-force family carried by IRI) persists faithfully.
    let mut graph = Graph::new();
    graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let fid = FieldId::from_uuid(Uuid::from_u128(0xF3));
    graph.add_field(Field::new(
        fid,
        FieldDefinition::Scalar(ScalarField::Const(1.0)),
    ));
    let iri = format!("{COUPLING_VOCAB}visual/highlight");
    graph.add_coupling(Coupling::new(
        CouplingId::from_uuid(Uuid::from_u128(0xC2)),
        fid,
        NodeSelector::All,
        CouplingResponse::open(iri.clone()),
        1.0,
    ));

    let restored = Graph::from_snapshot(&graph.to_snapshot());
    let c = restored
        .couplings_for_field(fid)
        .next()
        .expect("open coupling restored");
    assert_eq!(
        c.response,
        CouplingResponse::Open {
            predicate: iri.clone()
        }
    );
    assert_eq!(c.response.predicate(), Some(iri.as_str()));
}
