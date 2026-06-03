/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # linked-data
//!
//! JSON-LD bridge between the Mere graph [`kernel`] and linked data (linked-data
//! plan, `design_docs/mere_docs/implementation_strategy/2026-05-22_linked_data_ingest_export_plan.md`).
//!
//! **Export.** [`to_jsonld`] emits *expanded* JSON-LD (full IRIs, no `@context`);
//! [`to_jsonld_compact`] emits the *compacted* form (a `@graph` under an inline
//! `@context`). The statements-over-schema cut shows in both: a node's `Semantic`
//! edges become RDF predicates — a recognized [`kernel::graph::SemanticSubKind`]
//! uses its canonical Mere IRI ([`kernel::graph::predicate_iri`]) or short term,
//! while an open predicate passes through verbatim. The other edge families
//! (Traversal, Containment, …) are Mere's *experience* layer and are not exported.
//! Curated literals only (no general property bag): `title` → `schema:name`,
//! `tags` → `schema:keywords`; `@id` is the node's URL, or a skolemized
//! `urn:uuid:` IRI when it has none.
//!
//! **Ingest.** [`from_jsonld`] parses a document into a [`GraphContribution`]
//! (via `oxjsonld`); [`from_jsonld_with_contexts`] resolves a remote `@context`
//! from a bundled [`ContextCache`] rather than the network. [`apply_contribution`]
//! materializes a contribution into a graph (recognized predicate → typed
//! sub-kind, raw → open-predicate edge). Export ↔ ingest round-trips.
//!
//! Later: `@type` ↔ classification mapping, real bundled context assets, and the
//! host-side `application/ld+json` dispatch.

#![doc(html_root_url = "https://docs.rs/linked-data/0.0.1")]

use kernel::graph::{Graph, Node, NodeKey, SemanticData, predicate_iri, sub_kind_from_iri};
use kernel::types::ClassificationScheme;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// JSON-LD ingest (Phase 2): `application/ld+json` → a graph contribution.
pub mod ingest;

pub use ingest::{
    ContextCache, EdgeContribution, GraphContribution, IngestError, NodeContribution, from_html,
    from_html_with_contexts, from_jsonld, from_jsonld_with_contexts, is_bundled_context,
    referenced_context_urls,
};
#[cfg(not(target_arch = "wasm32"))]
pub use ingest::{ApplyOutcome, apply_contribution};

/// `schema:name` — the curated mapping target for a node's title.
pub(crate) const SCHEMA_NAME: &str = "https://schema.org/name";
/// `schema:keywords` — the curated mapping target for a node's tags.
pub(crate) const SCHEMA_KEYWORDS: &str = "https://schema.org/keywords";

/// Export the whole graph as expanded JSON-LD: a [`Value::Array`] of node
/// objects, one per graph node, in node-insertion order. Deterministic (tags and
/// edge targets are sorted), so the output is safe to pin in a golden test.
pub fn to_jsonld(graph: &Graph) -> Value {
    Value::Array(
        graph
            .nodes()
            .map(|(key, node)| node_object(graph, key, node))
            .collect(),
    )
}

/// Pretty-printed [`to_jsonld`], for goldens and human inspection.
pub fn to_jsonld_string(graph: &Graph) -> String {
    serde_json::to_string_pretty(&to_jsonld(graph))
        .expect("a JSON-LD document of strings is always serializable")
}

/// A node's `@id`: its primary address URL, or a skolemized `urn:uuid:` IRI when
/// the node has no dereferenceable address.
fn node_id(node: &Node) -> String {
    let url = node.primary_address().as_url_str();
    if url.is_empty() {
        format!("urn:uuid:{}", node.id)
    } else {
        url.to_string()
    }
}

/// The RDF predicate IRIs a `Semantic` edge contributes. An explicit open
/// predicate (raw or canonical) wins and is emitted verbatim; otherwise each
/// recognized sub-kind maps to its canonical Mere IRI.
fn edge_predicates(semantic: &SemanticData) -> Vec<String> {
    if let Some(predicate) = &semantic.predicate {
        vec![predicate.clone()]
    } else {
        semantic
            .sub_kinds
            .iter()
            .map(|&sub_kind| predicate_iri(sub_kind).to_string())
            .collect()
    }
}

/// Insert an `@type` array — the node's `rdf:type` classification IRIs (what
/// ingest writes from JSON-LD `@type`), sorted — when present. Shared by the
/// expanded and compacted exports.
fn insert_types(obj: &mut Map<String, Value>, node: &Node) {
    let mut types: Vec<String> = node
        .classifications
        .iter()
        .filter(|c| matches!(&c.scheme, ClassificationScheme::Custom(s) if s == "rdf:type"))
        .map(|c| c.value.clone())
        .collect();
    types.sort();
    types.dedup();
    if !types.is_empty() {
        obj.insert(
            "@type".to_string(),
            Value::Array(types.into_iter().map(Value::String).collect()),
        );
    }
}

fn node_object(graph: &Graph, key: NodeKey, node: &Node) -> Value {
    let url = node.primary_address().as_url_str();
    let mut obj = Map::new();
    obj.insert("@id".to_string(), Value::String(node_id(node)));
    insert_types(&mut obj, node);

    // Curated literals. Skip a title that is only the URL fallback (`add_node`
    // seeds `title = url` for an untitled node) — that is not a real name.
    if !node.title.is_empty() && node.title != url {
        obj.insert(SCHEMA_NAME.to_string(), json!([{ "@value": node.title }]));
    }
    if !node.tags.is_empty() {
        let mut tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
        tags.sort_unstable();
        obj.insert(
            SCHEMA_KEYWORDS.to_string(),
            Value::Array(tags.into_iter().map(|t| json!({ "@value": t })).collect()),
        );
    }

    // Semantic edges → predicate IRIs → target `@id`s. Grouped by predicate and
    // sorted (keys via `BTreeMap`, targets explicitly) for a stable document.
    let mut by_predicate: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for target in graph.out_neighbors(key) {
        let Some(edge_key) = graph.find_edge_key(key, target) else {
            continue;
        };
        let Some(semantic) = graph.get_edge(edge_key).and_then(|p| p.semantic_data()) else {
            continue;
        };
        let Some(target_node) = graph.get_node(target) else {
            continue;
        };
        let target_id = node_id(target_node);
        for predicate in edge_predicates(semantic) {
            by_predicate
                .entry(predicate)
                .or_default()
                .push(target_id.clone());
        }
    }
    for (predicate, mut targets) in by_predicate {
        targets.sort_unstable();
        obj.insert(
            predicate,
            Value::Array(targets.into_iter().map(|id| json!({ "@id": id })).collect()),
        );
    }

    // Open literal properties → `@value` entries, merged into the predicate's
    // array (expanded values are always arrays).
    let mut props: Vec<(&str, &str)> = node
        .properties
        .iter()
        .map(|p| (p.predicate.as_str(), p.value.as_str()))
        .collect();
    props.sort_unstable();
    for (predicate, value) in props {
        obj.entry(predicate.to_string())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("expanded predicate value is an array")
            .push(json!({ "@value": value }));
    }

    Value::Object(obj)
}

/// Export the whole graph as **compacted** JSON-LD: `{"@context": {…}, "@graph":
/// […]}`. A recognized relation and the curated literals are emitted as short
/// terms backed by an inline `@context` (term → IRI); an open / raw predicate
/// keeps its full IRI as the key (the open tail stays explicit). This is the
/// curated kernel-vocabulary context's first consumer, and it round-trips through
/// [`from_jsonld`], which expands the inline context. Deterministic, like
/// [`to_jsonld`].
pub fn to_jsonld_compact(graph: &Graph) -> Value {
    let mut context = Map::new();
    let graph_nodes: Vec<Value> = graph
        .nodes()
        .map(|(key, node)| compact_node_object(graph, key, node, &mut context))
        .collect();
    json!({ "@context": Value::Object(context), "@graph": Value::Array(graph_nodes) })
}

/// The short term for a recognized predicate IRI: its fragment or last path
/// segment (`…/rel#cites` → `cites`, `schema.org/name` → `name`).
fn term_for(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}

fn compact_node_object(
    graph: &Graph,
    key: NodeKey,
    node: &Node,
    context: &mut Map<String, Value>,
) -> Value {
    let url = node.primary_address().as_url_str();
    let mut obj = Map::new();
    obj.insert("@id".to_string(), Value::String(node_id(node)));
    insert_types(&mut obj, node);

    if !node.title.is_empty() && node.title != url {
        context
            .entry("name".to_string())
            .or_insert_with(|| json!(SCHEMA_NAME));
        obj.insert("name".to_string(), json!(node.title));
    }
    if !node.tags.is_empty() {
        context
            .entry("keywords".to_string())
            .or_insert_with(|| json!(SCHEMA_KEYWORDS));
        let mut tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
        tags.sort_unstable();
        obj.insert("keywords".to_string(), json!(tags));
    }

    // Group targets by the key we emit: a short term for a recognized predicate
    // (registered in the inline context), the full IRI for a raw one.
    let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for target in graph.out_neighbors(key) {
        let Some(edge_key) = graph.find_edge_key(key, target) else {
            continue;
        };
        let Some(semantic) = graph.get_edge(edge_key).and_then(|p| p.semantic_data()) else {
            continue;
        };
        let Some(target_node) = graph.get_node(target) else {
            continue;
        };
        let target_id = node_id(target_node);
        for predicate in edge_predicates(semantic) {
            let emit_key = if sub_kind_from_iri(&predicate).is_some() {
                let term = term_for(&predicate).to_string();
                context
                    .entry(term.clone())
                    .or_insert_with(|| json!(predicate));
                term
            } else {
                predicate
            };
            by_key.entry(emit_key).or_default().push(target_id.clone());
        }
    }
    for (emit_key, mut targets) in by_key {
        targets.sort_unstable();
        let refs: Vec<Value> = targets.into_iter().map(|id| json!({ "@id": id })).collect();
        let value = if refs.len() == 1 {
            refs.into_iter().next().expect("length checked")
        } else {
            Value::Array(refs)
        };
        obj.insert(emit_key, value);
    }

    // Open literal properties: a full-IRI key → literal value (scalar, or array
    // for repeats). Not short-termed — the open tail stays explicit.
    let mut props: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for property in &node.properties {
        props
            .entry(property.predicate.clone())
            .or_default()
            .push(property.value.clone());
    }
    for (predicate, mut values) in props {
        values.sort();
        let value = if values.len() == 1 {
            Value::String(values.into_iter().next().expect("length checked"))
        } else {
            Value::Array(values.into_iter().map(Value::String).collect())
        };
        obj.insert(predicate, value);
    }

    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::{
        EdgeContribution, GraphContribution, apply_contribution, from_jsonld, to_jsonld,
        to_jsonld_compact,
    };
    use kernel::graph::{EdgeAssertion, Graph, SemanticSubKind};
    use serde_json::json;

    /// A graph with one recognized edge (A cites B, canonical IRI), one raw
    /// open-predicate edge (A → C, `schema:citation`), and curated literals on A.
    fn seed() -> Graph {
        let mut graph = Graph::new();
        let a = graph.add_node("https://a.test/".to_string(), Default::default());
        let b = graph.add_node("https://b.test/".to_string(), Default::default());
        let c = graph.add_node("https://c.test/".to_string(), Default::default());

        graph.assert_relation(
            a,
            b,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
        );
        let e = graph
            .assert_relation(
                a,
                c,
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Cites,
                    label: None,
                    decay_progress: None,
                },
            )
            .expect("edge");
        graph
            .get_edge_mut(e)
            .expect("payload")
            .set_semantic_predicate(Some("https://schema.org/citation".to_string()));

        let node_a = graph.get_node_mut(a).expect("node a");
        node_a.title = "Article A".to_string();
        node_a.tags.insert("research".to_string());
        graph
    }

    #[test]
    fn empty_graph_exports_empty_array() {
        assert_eq!(to_jsonld(&Graph::new()), json!([]));
    }

    #[test]
    fn properties_round_trip_via_the_property_bag() {
        // A non-curated literal survives ingest → graph property bag → export.
        let doc = br#"{"@id":"https://a.test/","https://schema.org/datePublished":[{"@value":"2026-06-02"}]}"#;
        let contribution = from_jsonld(doc).expect("parse");
        let node = contribution
            .nodes
            .iter()
            .find(|n| n.id == "https://a.test/")
            .expect("node a");
        assert_eq!(
            node.properties,
            vec![(
                "https://schema.org/datePublished".to_string(),
                "2026-06-02".to_string()
            )]
        );

        let mut graph = Graph::new();
        apply_contribution(&mut graph, &contribution);
        let exported = to_jsonld(&graph);
        let a = exported
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["@id"] == json!("https://a.test/"))
            .expect("exported node a");
        assert_eq!(
            a["https://schema.org/datePublished"],
            json!([{ "@value": "2026-06-02" }])
        );
    }

    #[test]
    fn type_round_trips_via_rdf_type_classification() {
        // @type → node types on ingest, applied as an `rdf:type` classification,
        // re-exported as @type.
        let doc = br#"{"@id":"https://a.test/","@type":["https://schema.org/Article"]}"#;
        let contribution = from_jsonld(doc).expect("parse");
        let node = contribution
            .nodes
            .iter()
            .find(|n| n.id == "https://a.test/")
            .expect("node a");
        assert_eq!(node.types, vec!["https://schema.org/Article".to_string()]);

        let mut graph = Graph::new();
        apply_contribution(&mut graph, &contribution);
        let exported = to_jsonld(&graph);
        let a = exported
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["@id"] == json!("https://a.test/"))
            .expect("exported node a");
        assert_eq!(a["@type"], json!(["https://schema.org/Article"]));
    }

    #[test]
    fn exports_recognized_and_raw_predicates_with_literals() {
        assert_eq!(
            to_jsonld(&seed()),
            json!([
                {
                    "@id": "https://a.test/",
                    "https://schema.org/name": [{ "@value": "Article A" }],
                    "https://schema.org/keywords": [{ "@value": "research" }],
                    "https://mere.computer/ns/rel#cites": [{ "@id": "https://b.test/" }],
                    "https://schema.org/citation": [{ "@id": "https://c.test/" }]
                },
                { "@id": "https://b.test/" },
                { "@id": "https://c.test/" }
            ])
        );
    }

    #[test]
    fn compact_export_uses_terms_and_keeps_raw_iris() {
        let compact = to_jsonld_compact(&seed());
        // A recognized relation + the curated literals are short terms backed by
        // the inline context.
        assert_eq!(
            compact["@context"]["cites"],
            json!("https://mere.computer/ns/rel#cites")
        );
        assert_eq!(
            compact["@context"]["name"],
            json!("https://schema.org/name")
        );
        let a = compact["@graph"]
            .as_array()
            .expect("@graph array")
            .iter()
            .find(|n| n["@id"] == json!("https://a.test/"))
            .expect("node a");
        assert_eq!(a["name"], json!("Article A"));
        assert_eq!(a["cites"], json!({ "@id": "https://b.test/" }));
        // The raw predicate keeps its full IRI as the key, not a context term.
        assert_eq!(
            a["https://schema.org/citation"],
            json!({ "@id": "https://c.test/" })
        );
        assert!(compact["@context"].get("citation").is_none());
    }

    #[test]
    fn expanded_export_round_trips_through_ingest() {
        let doc = serde_json::to_vec(&to_jsonld(&seed())).expect("serialize");
        assert_round_trip(&from_jsonld(&doc).expect("round-trip parse"));
    }

    #[test]
    fn compact_export_round_trips_through_ingest() {
        let doc = serde_json::to_vec(&to_jsonld_compact(&seed())).expect("serialize");
        assert_round_trip(&from_jsonld(&doc).expect("round-trip parse"));
    }

    /// Both export forms must ingest back to the same logical content: A's curated
    /// literals, the recognized `cites` edge (canonical IRI), and the raw
    /// `schema:citation` edge.
    fn assert_round_trip(contribution: &GraphContribution) {
        let a = contribution
            .nodes
            .iter()
            .find(|n| n.id == "https://a.test/")
            .expect("node a");
        assert_eq!(a.title.as_deref(), Some("Article A"));
        assert_eq!(a.tags, vec!["research".to_string()]);
        assert!(contribution.edges.contains(&EdgeContribution {
            subject: "https://a.test/".into(),
            predicate: "https://mere.computer/ns/rel#cites".into(),
            object: "https://b.test/".into(),
        }));
        assert!(contribution.edges.contains(&EdgeContribution {
            subject: "https://a.test/".into(),
            predicate: "https://schema.org/citation".into(),
            object: "https://c.test/".into(),
        }));
    }
}
