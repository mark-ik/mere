// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use kernel::graph::Graph;
use kernel::types::GraphScope;

/// [`crate::to_jsonld`] emits. One recognized predicate (`rel#cites`) and one
/// raw predicate (`schema:citation`).
const SAMPLE: &[u8] = br#"[
  {
    "@id": "https://a.test/",
    "@type": ["https://schema.org/Article"],
    "https://schema.org/name": [{"@value": "Article A"}],
    "https://schema.org/keywords": [{"@value": "research"}],
    "https://mere.computer/ns/rel#cites": [{"@id": "https://b.test/"}],
    "https://schema.org/citation": [{"@id": "https://c.test/"}]
  }
]"#;

#[test]
fn from_html_harvests_embedded_jsonld_scripts() {
    let html = r#"<!doctype html><html><head>
      <title>A page</title>
      <script type="application/ld+json">
        {"@context":{"name":"https://schema.org/name","cites":"https://mere.computer/ns/rel#cites"},
         "@id":"https://a.test/","name":"Paper A","cites":{"@id":"https://b.test/"}}
      </script>
      <script type="text/javascript">console.log("ignored");</script>
    </head><body>body</body></html>"#;
    let contributions = from_html(html);
    assert_eq!(
        contributions.len(),
        1,
        "only the ld+json script is harvested"
    );
    let nodes = &contributions[0].nodes;
    assert!(
        nodes
            .iter()
            .any(|n| n.id == "https://a.test/" && n.title.as_deref() == Some("Paper A"))
    );
    assert!(
        contributions[0]
            .edges
            .iter()
            .any(|e| e.predicate.ends_with("#cites"))
    );
}

#[test]
fn blank_nodes_skolemize_under_a_document_namespace() {
    // A blank node becomes `urn:mere:bnode:<doc-namespace>:<label>`. The
    // namespace is stable per document content; oxjsonld assigns the label
    // fresh per parse (so blanks are not idempotent across re-ingests without
    // canonicalization), but distinct documents get distinct namespaces.
    let blank = |doc: &[u8]| {
        from_jsonld(doc)
            .unwrap()
            .edges
            .iter()
            .find(|e| e.predicate.ends_with("#cites"))
            .expect("cites edge")
            .object
            .clone()
    };
    let doc_a = br#"{"@context":{"cites":"https://mere.computer/ns/rel#cites"},"@id":"https://a.test/","cites":{"@type":"https://schema.org/Thing"}}"#;
    let doc_b = br#"{"@context":{"cites":"https://mere.computer/ns/rel#cites"},"@id":"https://b.test/","cites":{"@type":"https://schema.org/Thing"}}"#;
    let a = blank(doc_a);
    assert!(a.starts_with("urn:mere:bnode:"), "skolemized: {a}");
    // Everything before the final `:` is the `urn:mere:bnode:<namespace>`
    // prefix; the label after it is what oxjsonld varies per parse.
    let prefix = |iri: &str| iri.rsplit_once(':').map(|(p, _)| p.to_string()).unwrap();
    assert_eq!(
        prefix(&a),
        prefix(&blank(doc_a)),
        "namespace is content-stable"
    );
    assert_ne!(
        prefix(&a),
        prefix(&blank(doc_b)),
        "different doc, different namespace"
    );
}

#[test]
fn from_jsonld_parses_nodes_literals_types_and_edges() {
    let contribution = from_jsonld(SAMPLE).expect("valid JSON-LD");

    assert_eq!(
        contribution.nodes,
        vec![
            NodeContribution {
                id: "https://a.test/".into(),
                types: vec!["https://schema.org/Article".into()],
                title: Some("Article A".into()),
                tags: vec!["research".into()],
                properties: vec![],
            },
            NodeContribution::new("https://b.test/"),
            NodeContribution::new("https://c.test/"),
        ]
    );

    assert_eq!(contribution.edges.len(), 2);
    assert!(contribution.edges.contains(&EdgeContribution {
        subject: "https://a.test/".into(),
        predicate: "https://mere.computer/ns/rel#cites".into(),
        object: "https://b.test/".into(),
        graph_scope: GraphScope::Default,
        statement_id: None,
        label: None,
        provenance_iri: None,
        asserted_at_ms: None,
    }));
    assert!(contribution.edges.contains(&EdgeContribution {
        subject: "https://a.test/".into(),
        predicate: "https://schema.org/citation".into(),
        object: "https://c.test/".into(),
        graph_scope: GraphScope::Default,
        statement_id: None,
        label: None,
        provenance_iri: None,
        asserted_at_ms: None,
    }));
}

#[test]
fn apply_materializes_recognized_and_raw_edges() {
    use kernel::graph::{EdgeFamily, Graph, RelationSelector, SemanticSubKind};

    let contribution = from_jsonld(SAMPLE).expect("valid JSON-LD");
    let mut graph = Graph::new();
    let outcome = apply_contribution(&mut graph, &contribution);

    assert_eq!(outcome.nodes_created, 3);
    assert_eq!(outcome.edges_asserted, 2);
    assert_eq!(outcome.edges_skipped, 0);

    // Curated literals landed on the subject node.
    let (a, node_a) = graph.get_node_by_url("https://a.test/").expect("node a");
    assert_eq!(node_a.title, "Article A");
    assert!(node_a.tags.contains("research"));
    let (b, _) = graph.get_node_by_url("https://b.test/").expect("node b");
    let (c, _) = graph.get_node_by_url("https://c.test/").expect("node c");

    // Recognized predicate → typed Semantic edge with canonical IRI.
    let cites = graph
        .get_edge(graph.find_edge_key(a, b).expect("a→b"))
        .unwrap();
    assert!(cites.has_relation(RelationSelector::Semantic(SemanticSubKind::Cites)));
    assert_eq!(
        cites.semantic_data().and_then(|d| d.predicate.as_deref()),
        Some("https://mere.computer/ns/rel#cites")
    );

    // Raw predicate → open-predicate Semantic edge (no sub-kinds).
    let citation = graph
        .get_edge(graph.find_edge_key(a, c).expect("a→c"))
        .unwrap();
    assert!(citation.has_relation(RelationSelector::Family(EdgeFamily::Semantic)));
    assert!(
        citation
            .semantic_data()
            .is_some_and(|d| d.sub_kinds.is_empty())
    );
    assert_eq!(
        citation
            .semantic_data()
            .and_then(|d| d.predicate.as_deref()),
        Some("https://schema.org/citation")
    );
}

#[test]
fn from_jsonld_preserves_typed_and_language_tagged_literals() {
    let doc = br#"{
      "@id":"https://a.test/",
      "https://schema.org/datePublished":[{"@value":"2026-06-02","@type":"http://www.w3.org/2001/XMLSchema#date"}],
      "https://schema.org/headline":[{"@value":"Bonjour","@language":"fr"}]
    }"#;
    let contribution = from_jsonld(doc).expect("valid JSON-LD");
    let node = contribution
        .nodes
        .iter()
        .find(|n| n.id == "https://a.test/")
        .expect("node a");
    assert_eq!(node.properties.len(), 2);
    assert_eq!(
        node.properties[0].predicate,
        "https://schema.org/datePublished"
    );
    assert_eq!(node.properties[0].value, "2026-06-02");
    assert_eq!(
        node.properties[0].datatype.as_deref(),
        Some("http://www.w3.org/2001/XMLSchema#date")
    );
    assert_eq!(node.properties[0].lang, None);
    assert_eq!(node.properties[0].graph_scope, GraphScope::Default);
    assert!(!node.properties[0].statement_id.is_empty());
    assert_eq!(node.properties[0].provenance_iri, None);
    assert_eq!(node.properties[0].asserted_at_ms, None);

    assert_eq!(node.properties[1].predicate, "https://schema.org/headline");
    assert_eq!(node.properties[1].value, "Bonjour");
    assert_eq!(node.properties[1].datatype, None);
    assert_eq!(node.properties[1].lang.as_deref(), Some("fr"));
    assert_eq!(node.properties[1].graph_scope, GraphScope::Default);
    assert!(!node.properties[1].statement_id.is_empty());
    assert_eq!(node.properties[1].provenance_iri, None);
    assert_eq!(node.properties[1].asserted_at_ms, None);
}

#[test]
fn a_harvested_hyperlink_records_extracted_from_provenance_on_the_target() {
    use kernel::graph::{Graph, ProvenanceSubKind};

    // The shape a materialize / crawl contribution carries: a source page
    // linking to a target (capture plan C3).
    let contribution = GraphContribution {
        nodes: vec![
            NodeContribution::new("https://src.test/"),
            NodeContribution::new("https://dst.test/"),
        ],
        edges: vec![EdgeContribution {
            subject: "https://src.test/".to_string(),
            predicate: "https://mere.computer/ns/rel#hyperlink".to_string(),
            object: "https://dst.test/".to_string(),
            graph_scope: GraphScope::Default,
            statement_id: None,
            label: None,
            provenance_iri: None,
            asserted_at_ms: None,
        }],
    };
    let mut graph = Graph::new();
    apply_contribution(&mut graph, &contribution);

    let (src_key, src) = graph
        .get_node_by_url("https://src.test/")
        .expect("source node");
    let (dst_key, _dst) = graph
        .get_node_by_url("https://dst.test/")
        .expect("target node");

    // The target names the page it was extracted from; the source carries none.
    assert!(
        graph.node_derivations(src_key).unwrap().is_empty(),
        "the source page is not derived"
    );
    let derivations = graph.node_derivations(dst_key).unwrap();
    assert_eq!(derivations.len(), 1, "the target carries one derivation");
    let d = &derivations[0];
    assert_eq!(d.sub_kind, ProvenanceSubKind::ExtractedFrom);
    assert_eq!(
        d.source_node,
        src.id.to_string(),
        "the derivation names the source node"
    );
    assert_eq!(d.source_graph, None, "same-graph derivation");
}

const REMOTE_DOC: &[u8] = br#"{
  "@context": "https://ctx.test/v1",
  "@id": "https://a.test/",
  "name": "Article A",
  "cites": {"@id": "https://b.test/"}
}"#;

const BUNDLED_CONTEXT: &[u8] = br#"{
  "@context": {
    "name": "https://schema.org/name",
    "cites": "https://mere.computer/ns/rel#cites"
  }
}"#;

#[test]
fn context_presets_resolve_mere_docs() {
    let doc = br#"{"@context":"https://mere.computer/ns/context","@id":"https://a.test/","name":"A","mere:cites":{"@id":"https://b.test/"}}"#;
    // minimal + full resolve the Mere context; none refuses the remote URL.
    let resolved = from_jsonld_with_contexts(doc, ContextCache::minimal()).expect("minimal");
    assert!(
        resolved
            .nodes
            .iter()
            .any(|n| n.id == "https://a.test/" && n.title.as_deref() == Some("A"))
    );
    assert!(
        resolved
            .edges
            .iter()
            .any(|e| e.predicate == "https://mere.computer/ns/rel#cites")
    );
    assert!(from_jsonld_with_contexts(doc, ContextCache::full()).is_ok());
    assert!(from_jsonld_with_contexts(doc, ContextCache::new()).is_err());
}

#[cfg(feature = "bundled-contexts")]
#[test]
fn full_pack_resolves_a_schema_org_remote_context() {
    // A page referencing schema.org's remote context resolves offline, and
    // schema.org's http `@vocab` is normalized to https.
    let doc = br#"{"@context":"https://schema.org/","@id":"https://a.test/","name":"Article A","datePublished":"2026-06-02"}"#;
    let contribution =
        from_jsonld_with_contexts(doc, ContextCache::full()).expect("schema.org resolved offline");
    let node = contribution
        .nodes
        .iter()
        .find(|n| n.id == "https://a.test/")
        .expect("node a");
    assert_eq!(node.title.as_deref(), Some("Article A"));
    assert!(node.properties.iter().any(|property| {
        property.predicate == "https://schema.org/datePublished"
            && property.value == "2026-06-02"
            && property.datatype.as_deref() == Some("https://schema.org/Date")
            && property.lang.is_none()
    }));
}

#[cfg(feature = "bundled-contexts")]
#[test]
fn full_pack_resolves_activitystreams() {
    // A fediverse-style object: as:name -> title, as:inReplyTo (an @id term)
    // -> a Semantic edge.
    let doc = br#"{"@context":"https://www.w3.org/ns/activitystreams","@id":"https://x.test/n1","name":"Hello","inReplyTo":"https://y.test/n0"}"#;
    let c = from_jsonld_with_contexts(doc, ContextCache::full()).expect("AS2 resolved offline");
    let node = c
        .nodes
        .iter()
        .find(|n| n.id == "https://x.test/n1")
        .expect("node n1");
    assert_eq!(node.title.as_deref(), Some("Hello"));
    assert!(
        c.edges
            .iter()
            .any(|e| e.object == "https://y.test/n0" && e.predicate.contains("inReplyTo"))
    );
}

#[test]
fn ingested_node_ids_are_deterministic_across_graphs() {
    // Two hosts ingesting the same document mint the same node ids (federation
    // identity), each derived from its `@id`.
    let doc =
        br#"{"@context":{"name":"https://schema.org/name"},"@id":"https://x.test/","name":"X"}"#;
    let contribution = from_jsonld(doc).expect("parsed");
    let mut g1 = Graph::new();
    let mut g2 = Graph::new();
    apply_contribution(&mut g1, &contribution);
    apply_contribution(&mut g2, &contribution);
    let id1 = g1
        .get_node_by_url("https://x.test/")
        .expect("node in g1")
        .1
        .id;
    let id2 = g2
        .get_node_by_url("https://x.test/")
        .expect("node in g2")
        .1
        .id;
    assert_eq!(id1, id2, "same @id yields the same node id on every host");
    assert_eq!(id1, Graph::node_namespace_id("https://x.test/"));
}

#[test]
fn dublin_core_literals_are_recognized() {
    // dcterms:title -> title, dcterms:subject -> tag. Recognition works on an
    // inline prefix, with no remote context fetched.
    let doc = br#"{"@context":{"dcterms":"http://purl.org/dc/terms/"},"@id":"https://d.test/","dcterms:title":"Doc","dcterms:subject":"alpha"}"#;
    let c = from_jsonld(doc).expect("DC parsed");
    let node = c
        .nodes
        .iter()
        .find(|n| n.id == "https://d.test/")
        .expect("node d");
    assert_eq!(node.title.as_deref(), Some("Doc"));
    assert!(node.tags.iter().any(|t| t == "alpha"));
}

#[test]
fn referenced_context_urls_finds_remote_refs() {
    let doc = br#"{"@context":["https://schema.org/",{"x":"https://ex/#x"}],"@graph":[{"@context":"https://www.w3.org/ns/activitystreams","@id":"a"}]}"#;
    let urls = referenced_context_urls(doc);
    assert!(urls.contains(&"https://schema.org/".to_string()));
    assert!(urls.contains(&"https://www.w3.org/ns/activitystreams".to_string()));
    assert_eq!(
        urls.len(),
        2,
        "inline object entries are not URL references"
    );
}

#[test]
fn referenced_context_urls_ignores_inline_and_nonjson() {
    let inline = br#"{"@context":{"name":"https://schema.org/name"},"@id":"a"}"#;
    assert!(
        referenced_context_urls(inline).is_empty(),
        "inline context has no remote ref"
    );
    assert!(referenced_context_urls(b"not json at all").is_empty());
}

#[cfg(feature = "bundled-contexts")]
#[test]
fn is_bundled_context_covers_the_packs() {
    assert!(is_bundled_context("https://schema.org/"));
    assert!(is_bundled_context("https://www.w3.org/ns/activitystreams"));
    assert!(is_bundled_context("http://purl.org/dc/terms/"));
    assert!(!is_bundled_context("https://example.com/ctx"));
}

#[test]
fn bundled_context_expands_a_remote_context() {
    let cache = ContextCache::new().with("https://ctx.test/v1", BUNDLED_CONTEXT);
    let contribution = from_jsonld_with_contexts(REMOTE_DOC, cache).expect("context resolved");

    // `name` expands to schema:name (a title); `cites` to the Mere predicate
    // (an edge) — both via the bundled context, no network.
    let a = contribution
        .nodes
        .iter()
        .find(|n| n.id == "https://a.test/")
        .expect("node a");
    assert_eq!(a.title.as_deref(), Some("Article A"));
    assert!(contribution.edges.contains(&EdgeContribution {
        subject: "https://a.test/".into(),
        predicate: "https://mere.computer/ns/rel#cites".into(),
        object: "https://b.test/".into(),
        graph_scope: GraphScope::Default,
        statement_id: None,
        label: None,
        provenance_iri: None,
        asserted_at_ms: None,
    }));
}

#[test]
fn unbundled_remote_context_is_refused() {
    // Empty cache → the remote @context cannot be resolved → ingest errors.
    assert!(matches!(
        from_jsonld_with_contexts(REMOTE_DOC, ContextCache::new()),
        Err(IngestError::Parse(_))
    ));
    // The network-free parser refuses a remote @context outright.
    assert!(matches!(
        from_jsonld(REMOTE_DOC),
        Err(IngestError::Parse(_))
    ));
}
