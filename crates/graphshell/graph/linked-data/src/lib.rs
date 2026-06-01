/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # linked-data
//!
//! JSON-LD bridge between the Mere graph [`kernel`] and linked data (linked-data
//! plan, `design_docs/mere_docs/implementation_strategy/2026-05-22_linked_data_ingest_export_plan.md`).
//!
//! Phase 1 — **export**. [`to_jsonld`] serializes a graph as *expanded* JSON-LD
//! (an array of node objects, full IRIs, no `@context`). The
//! statements-over-schema cut shows here: a node's `Semantic` edges become RDF
//! predicates — a recognized [`kernel::graph::SemanticSubKind`] uses its
//! canonical Mere IRI ([`kernel::graph::predicate_iri`]), and an open predicate
//! (a raw / CURIE IRI stamped on the edge) passes through verbatim. The other
//! edge families (Traversal, Containment, Arrangement, …) are Mere's *experience*
//! layer and are not exported.
//!
//! Curated literals only — the kernel has no general property bag: `title` →
//! `schema:name`, `tags` → `schema:keywords`. A node's `@id` is its primary
//! address URL, or a skolemized `urn:uuid:` IRI when it has none.
//!
//! Out of scope for Phase 1 (later phases): ingest (`from_jsonld`), `@type`
//! mapping from classifications, and bundled `@context`s.

#![doc(html_root_url = "https://docs.rs/linked-data/0.0.1")]

use kernel::graph::{Graph, Node, NodeKey, SemanticData, predicate_iri};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// JSON-LD ingest (Phase 2): `application/ld+json` → a graph contribution.
pub mod ingest;

pub use ingest::{EdgeContribution, GraphContribution, IngestError, NodeContribution, from_jsonld};
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

fn node_object(graph: &Graph, key: NodeKey, node: &Node) -> Value {
    let url = node.primary_address().as_url_str();
    let mut obj = Map::new();
    obj.insert("@id".to_string(), Value::String(node_id(node)));

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

    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::to_jsonld;
    use kernel::graph::{EdgeAssertion, Graph, SemanticSubKind};
    use serde_json::json;

    #[test]
    fn empty_graph_exports_empty_array() {
        assert_eq!(to_jsonld(&Graph::new()), json!([]));
    }

    #[test]
    fn exports_recognized_and_raw_predicates_with_literals() {
        let mut graph = Graph::new();
        let a = graph.add_node("https://a.test/".to_string(), Default::default());
        let b = graph.add_node("https://b.test/".to_string(), Default::default());
        let c = graph.add_node("https://c.test/".to_string(), Default::default());

        // A cites B: recognized sub-kind, no explicit predicate → canonical IRI.
        graph.assert_relation(
            a,
            b,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
        );
        // A → C with a raw (non-Mere) predicate stamped → emitted verbatim.
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

        // A real title + a tag on A; B and C stay untitled (title == url).
        {
            let node_a = graph.get_node_mut(a).expect("node a");
            node_a.title = "Article A".to_string();
            node_a.tags.insert("research".to_string());
        }

        assert_eq!(
            to_jsonld(&graph),
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
}
