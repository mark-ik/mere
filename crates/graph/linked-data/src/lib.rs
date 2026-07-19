// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

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
//! Curated literals stay on their fast paths (`title` → `schema:name`, `tags` →
//! `schema:keywords`), and the open node property bag projects as RDF literals
//! with datatype / language-tag fidelity. `@id` is the node's URL, or a
//! skolemized `urn:uuid:` IRI when it has none.
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
use kernel::types::{GraphScope, NodeProperty};
use oxrdf::{GraphName, Literal, NamedNode, Quad, Term, Triple};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// JSON-LD ingest (Phase 2): `application/ld+json` → a graph contribution.
pub mod ingest;

/// Standard-vocabulary alignment for Mere's recognized relations: the 3-category
/// (exact / approximate / Mere-only) mapping projected as `owl:equivalentProperty`
/// / `rdfs:subPropertyOf` quads.
pub mod vocab;

/// Turtle-family file I/O (N-Quads / TriG) for the RDF projection, via `oxttl`.
pub mod serialize;

/// Statements-over-schema ingest (Phase 0): knot `rel` links → `Semantic`
/// edges. The apply half of inker's pure `link_statements` walk; relocated
/// here 2026-07-10 (inker-adoption plan) so inker stays kernel-free.
pub mod statements;

/// SPARQL query over the graph via spareval evaluating directly over the
/// projected dataset (the `query` feature). Consumes [`dataset_quads`] as its
/// projection.
#[cfg(feature = "query")]
pub mod query;

#[cfg(not(target_arch = "wasm32"))]
pub use ingest::{ApplyOutcome, apply_contribution};
pub use ingest::{
    ContextCache, EdgeContribution, GraphContribution, IngestError, NodeContribution, from_html,
    from_html_with_contexts, from_jsonld, from_jsonld_with_contexts, from_quads, is_bundled_context,
    referenced_context_urls,
};
pub use serialize::{from_nquads, from_trig, to_nquads, to_trig};
pub use statements::{StatementOutcome, apply_link_statements, resolve_rel};
pub use vocab::{Alignment, alignment, vocabulary_alignment_quads};

/// `schema:name` — the curated mapping target for a node's title.
pub(crate) const SCHEMA_NAME: &str = "https://schema.org/name";
/// `schema:keywords` — the curated mapping target for a node's tags.
pub(crate) const SCHEMA_KEYWORDS: &str = "https://schema.org/keywords";
/// `rdf:type` — the predicate JSON-LD `@type` expands to.
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const PROV_WAS_ATTRIBUTED_TO: &str = "http://www.w3.org/ns/prov#wasAttributedTo";
const PROV_GENERATED_AT_TIME: &str = "http://www.w3.org/ns/prov#generatedAtTime";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
const GRAPH_SCOPE_SOURCE: &str = "https://mere.computer/ns/graph#source";
const GRAPH_SCOPE_USER: &str = "https://mere.computer/ns/graph#user";
const GRAPH_SCOPE_AGENT: &str = "https://mere.computer/ns/graph#agent";
const GRAPH_SCOPE_MOOT: &str = "https://mere.computer/ns/graph#moot";

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
    if !semantic.statements().is_empty() {
        semantic
            .statements()
            .iter()
            .map(|statement| statement.predicate.clone())
            .collect()
    } else if let Some(predicate) = &semantic.predicate {
        vec![predicate.clone()]
    } else {
        semantic
            .sub_kinds
            .iter()
            .map(|&sub_kind| predicate_iri(sub_kind).to_string())
            .collect()
    }
}

fn graph_name_for_scope(scope: &GraphScope) -> Option<GraphName> {
    match scope {
        GraphScope::Default => Some(GraphName::DefaultGraph),
        GraphScope::Source => NamedNode::new(GRAPH_SCOPE_SOURCE).ok().map(GraphName::from),
        GraphScope::User => NamedNode::new(GRAPH_SCOPE_USER).ok().map(GraphName::from),
        GraphScope::Agent => NamedNode::new(GRAPH_SCOPE_AGENT).ok().map(GraphName::from),
        GraphScope::Moot => NamedNode::new(GRAPH_SCOPE_MOOT).ok().map(GraphName::from),
        GraphScope::Custom(iri) => NamedNode::new(iri.as_str()).ok().map(GraphName::from),
    }
}

/// Push a quad `subject —predicate→ object` in the given graph scope, skipping
/// it when either the predicate or the graph name is not a valid IRI.
fn push_quad(
    quads: &mut Vec<Quad>,
    subject: &NamedNode,
    predicate: &str,
    object: Term,
    graph_scope: &GraphScope,
) {
    if let (Ok(predicate), Some(graph_name)) =
        (NamedNode::new(predicate), graph_name_for_scope(graph_scope))
    {
        quads.push(Quad::new(subject.clone(), predicate, object, graph_name));
    }
}

fn property_literal(property: &NodeProperty) -> Literal {
    if let Some(lang) = property.lang.as_deref()
        && let Ok(literal) = Literal::new_language_tagged_literal(property.value.clone(), lang)
    {
        return literal;
    }
    if let Some(datatype) = property.datatype.as_deref()
        && datatype != XSD_STRING
        && let Ok(datatype) = NamedNode::new(datatype)
    {
        return Literal::new_typed_literal(property.value.clone(), datatype);
    }
    Literal::new_simple_literal(property.value.clone())
}

fn statement_reifier_id(statement_id: &str) -> String {
    format!("urn:mere:statement:{statement_id}")
}

fn asserted_at_literal(asserted_at_ms: u64) -> Option<Literal> {
    let timestamp =
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(asserted_at_ms) * 1_000_000).ok()?;
    let lexical = timestamp.format(&Rfc3339).ok()?;
    let datatype = NamedNode::new(XSD_DATETIME).ok()?;
    Some(Literal::new_typed_literal(lexical, datatype))
}

fn push_statement_metadata_quads(
    quads: &mut Vec<Quad>,
    subject: &NamedNode,
    statement_id: &str,
    predicate: &str,
    object: Term,
    graph_scope: &GraphScope,
    label: Option<&str>,
    provenance_iri: Option<&str>,
    asserted_at_ms: Option<u64>,
) {
    let Ok(reifier) = NamedNode::new(statement_reifier_id(statement_id)) else {
        return;
    };
    let Ok(predicate_node) = NamedNode::new(predicate) else {
        return;
    };
    let Some(graph_name) = graph_name_for_scope(graph_scope) else {
        return;
    };
    let Ok(rdf_reifies) = NamedNode::new(RDF_REIFIES) else {
        return;
    };

    quads.push(Quad::new(
        reifier.clone(),
        rdf_reifies,
        Term::from(Triple::new(subject.clone(), predicate_node, object)),
        graph_name.clone(),
    ));

    if let Some(label) = label
        && let Ok(predicate) = NamedNode::new(RDFS_LABEL)
    {
        quads.push(Quad::new(
            reifier.clone(),
            predicate,
            Term::from(Literal::new_simple_literal(label)),
            graph_name.clone(),
        ));
    }
    if let Some(provenance_iri) = provenance_iri
        && let (Ok(predicate), Ok(agent)) = (
            NamedNode::new(PROV_WAS_ATTRIBUTED_TO),
            NamedNode::new(provenance_iri),
        )
    {
        quads.push(Quad::new(
            reifier.clone(),
            predicate,
            Term::from(agent),
            graph_name.clone(),
        ));
    }
    if let Some(asserted_at_ms) = asserted_at_ms
        && let (Ok(predicate), Some(literal)) = (
            NamedNode::new(PROV_GENERATED_AT_TIME),
            asserted_at_literal(asserted_at_ms),
        )
    {
        quads.push(Quad::new(
            reifier,
            predicate,
            Term::from(literal),
            graph_name,
        ));
    }
}

fn literal_json_value(literal: &Literal) -> Value {
    let mut value = Map::new();
    value.insert(
        "@value".to_string(),
        Value::String(literal.value().to_string()),
    );
    if let Some(language) = literal.language() {
        value.insert("@language".to_string(), Value::String(language.to_string()));
    } else {
        let datatype = literal.datatype().as_str();
        if datatype != XSD_STRING {
            value.insert("@type".to_string(), Value::String(datatype.to_string()));
        }
    }
    Value::Object(value)
}

fn compact_literal_json_value(literal: &Literal) -> Value {
    if literal.language().is_none() && literal.datatype().as_str() == XSD_STRING {
        Value::String(literal.value().to_string())
    } else {
        literal_json_value(literal)
    }
}

/// The direct RDF quads for one node: its `@id` subject carrying `rdf:type`,
/// the curated literals (`schema:name` / `schema:keywords`), the recognized and
/// raw `Semantic` edges, and the open literal properties, with graph scope taken
/// from semantic statements / node properties.
fn node_direct_quads(graph: &Graph, key: NodeKey, node: &Node) -> Vec<Quad> {
    let mut quads = Vec::new();
    let Ok(subject) = NamedNode::new(node_id(node)) else {
        return quads;
    };

    // `rdf:type` from the node's `rdf:type` classifications, sorted + deduped.
    let mut types: Vec<&str> = node
        .classifications
        .iter()
        .filter(|c| matches!(&c.scheme, ClassificationScheme::Custom(s) if s == "rdf:type"))
        .map(|c| c.value.as_str())
        .collect();
    types.sort_unstable();
    types.dedup();
    for ty in types {
        if let Ok(ty) = NamedNode::new(ty) {
            push_quad(
                &mut quads,
                &subject,
                RDF_TYPE,
                ty.into(),
                &GraphScope::Default,
            );
        }
    }

    // Curated literals. Skip a title that is only the URL fallback (`add_node`
    // seeds `title = url` for an untitled node), which is not a real name.
    let url = node.primary_address().as_url_str();
    if !node.title.is_empty() && node.title != url {
        push_quad(
            &mut quads,
            &subject,
            SCHEMA_NAME,
            Literal::new_simple_literal(node.title.as_str()).into(),
            &GraphScope::Default,
        );
    }
    let mut tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
    tags.sort_unstable();
    for tag in tags {
        push_quad(
            &mut quads,
            &subject,
            SCHEMA_KEYWORDS,
            Literal::new_simple_literal(tag).into(),
            &GraphScope::Default,
        );
    }

    // Semantic edges -> predicate IRIs -> target `@id`s, sorted by
    // (predicate, target, graph) for a stable dataset.
    let mut edges: Vec<(String, String, GraphScope)> = Vec::new();
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
        if !semantic.statements().is_empty() {
            for statement in semantic.statements() {
                edges.push((
                    statement.predicate.clone(),
                    target_id.clone(),
                    statement.graph_scope.clone(),
                ));
            }
        } else {
            for predicate in edge_predicates(semantic) {
                edges.push((predicate, target_id.clone(), GraphScope::Default));
            }
        }
    }
    edges.sort();
    edges.dedup();
    for (predicate, target_id, graph_scope) in edges {
        if let Ok(target) = NamedNode::new(target_id) {
            push_quad(
                &mut quads,
                &subject,
                &predicate,
                target.into(),
                &graph_scope,
            );
        }
    }

    // Open literal properties, sorted by full literal identity.
    let mut props: Vec<&NodeProperty> = node.properties.iter().collect();
    props.sort_by(|a, b| {
        (
            a.predicate.as_str(),
            a.value.as_str(),
            a.datatype.as_deref(),
            a.lang.as_deref(),
            &a.graph_scope,
        )
            .cmp(&(
                b.predicate.as_str(),
                b.value.as_str(),
                b.datatype.as_deref(),
                b.lang.as_deref(),
                &b.graph_scope,
            ))
    });
    for property in props {
        push_quad(
            &mut quads,
            &subject,
            property.predicate.as_str(),
            property_literal(property).into(),
            &property.graph_scope,
        );
    }

    quads
}

/// Dataset-only metadata quads for one node. These are reifier nodes that carry
/// statement handles plus provenance / time / label metadata without polluting
/// the node-subject JSON-LD shaper.
fn node_metadata_quads(graph: &Graph, key: NodeKey, node: &Node) -> Vec<Quad> {
    let mut quads = Vec::new();
    let Ok(subject) = NamedNode::new(node_id(node)) else {
        return quads;
    };

    let mut semantic_statements = Vec::new();
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
        let Ok(target_id) = NamedNode::new(node_id(target_node)) else {
            continue;
        };
        for statement in semantic.statements() {
            semantic_statements.push((
                statement.predicate.clone(),
                target_id.clone(),
                statement.statement_id.clone(),
                statement.graph_scope.clone(),
                statement.label.clone(),
                statement.provenance_iri.clone(),
                statement.asserted_at_ms,
            ));
        }
    }
    semantic_statements
        .sort_by(|a, b| (&a.0, a.1.as_str(), &a.3, &a.2).cmp(&(&b.0, b.1.as_str(), &b.3, &b.2)));
    for (predicate, target, statement_id, graph_scope, label, provenance_iri, asserted_at_ms) in
        semantic_statements
    {
        push_statement_metadata_quads(
            &mut quads,
            &subject,
            &statement_id,
            &predicate,
            target.into(),
            &graph_scope,
            label.as_deref(),
            provenance_iri.as_deref(),
            asserted_at_ms,
        );
    }

    let mut properties: Vec<&NodeProperty> = node.properties.iter().collect();
    properties.sort_by(|a, b| {
        (
            a.predicate.as_str(),
            a.value.as_str(),
            a.datatype.as_deref(),
            a.lang.as_deref(),
            &a.graph_scope,
            a.statement_id.as_str(),
        )
            .cmp(&(
                b.predicate.as_str(),
                b.value.as_str(),
                b.datatype.as_deref(),
                b.lang.as_deref(),
                &b.graph_scope,
                b.statement_id.as_str(),
            ))
    });
    for property in properties {
        push_statement_metadata_quads(
            &mut quads,
            &subject,
            property.statement_id.as_str(),
            property.predicate.as_str(),
            property_literal(property).into(),
            &property.graph_scope,
            None,
            property.provenance_iri.as_deref(),
            property.asserted_at_ms,
        );
    }

    quads
}

fn node_dataset_quads(graph: &Graph, key: NodeKey, node: &Node) -> Vec<Quad> {
    let mut quads = node_direct_quads(graph, key, node);
    quads.extend(node_metadata_quads(graph, key, node));
    quads
}

/// The RDF quads for one node, restricted to the default graph. This remains
/// the JSON-LD shaper's input until named-graph JSON-LD export lands.
pub fn node_quads(graph: &Graph, key: NodeKey, node: &Node) -> Vec<Quad> {
    node_direct_quads(graph, key, node)
        .into_iter()
        .filter(|quad| matches!(quad.graph_name, GraphName::DefaultGraph))
        .collect()
}

/// The RDF dataset quads for the whole graph, including named-graph scoped
/// semantic statements and node properties plus dataset-only reifier metadata.
pub fn dataset_quads(graph: &Graph) -> Vec<Quad> {
    graph
        .nodes()
        .flat_map(|(key, node)| node_dataset_quads(graph, key, node))
        .collect()
}

fn node_object(graph: &Graph, key: NodeKey, node: &Node) -> Value {
    let mut obj = Map::new();
    obj.insert("@id".to_string(), Value::String(node_id(node)));

    // Render the node's quads as expanded JSON-LD: `@type` collects the `rdf:type`
    // objects; every other predicate is an array of `{@id}` / `{@value}` entries,
    // grouped and key-sorted (`BTreeMap`) for a stable document. The quads already
    // arrive in a deterministic order, so per-predicate entry order is stable.
    let mut types: Vec<String> = Vec::new();
    let mut by_predicate: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for quad in node_quads(graph, key, node) {
        let predicate = quad.predicate.as_str();
        if predicate == RDF_TYPE {
            if let Term::NamedNode(object) = &quad.object {
                types.push(object.as_str().to_string());
            }
            continue;
        }
        let entry = match &quad.object {
            Term::NamedNode(object) => json!({ "@id": object.as_str() }),
            Term::Literal(object) => literal_json_value(object),
            _ => continue,
        };
        by_predicate
            .entry(predicate.to_string())
            .or_default()
            .push(entry);
    }

    if !types.is_empty() {
        obj.insert(
            "@type".to_string(),
            Value::Array(types.into_iter().map(Value::String).collect()),
        );
    }
    for (predicate, values) in by_predicate {
        obj.insert(predicate, Value::Array(values));
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
    let mut obj = Map::new();
    obj.insert("@id".to_string(), Value::String(node_id(node)));

    // Render the node's quads as compacted JSON-LD. `@type` collects the
    // `rdf:type` objects; the curated literals become the `name` (scalar) and
    // `keywords` (array) short terms; a recognized relation becomes a short term
    // registered in the inline context; a raw predicate keeps its full IRI as the
    // key. Edge and open-property values collapse to a scalar when single.
    let mut types: Vec<String> = Vec::new();
    let mut name: Option<String> = None;
    let mut keywords: Vec<String> = Vec::new();
    let mut by_key: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for quad in node_quads(graph, key, node) {
        let predicate = quad.predicate.as_str();
        if predicate == RDF_TYPE {
            if let Term::NamedNode(object) = &quad.object {
                types.push(object.as_str().to_string());
            }
            continue;
        }
        if predicate == SCHEMA_NAME {
            if let Term::Literal(object) = &quad.object {
                name = Some(object.value().to_string());
            }
            continue;
        }
        if predicate == SCHEMA_KEYWORDS {
            if let Term::Literal(object) = &quad.object {
                keywords.push(object.value().to_string());
            }
            continue;
        }
        let (emit_key, value) = match &quad.object {
            Term::NamedNode(object) => {
                let emit_key = if sub_kind_from_iri(predicate).is_some() {
                    let term = term_for(predicate).to_string();
                    context
                        .entry(term.clone())
                        .or_insert_with(|| json!(predicate));
                    term
                } else {
                    predicate.to_string()
                };
                (emit_key, json!({ "@id": object.as_str() }))
            }
            Term::Literal(object) => (predicate.to_string(), compact_literal_json_value(object)),
            _ => continue,
        };
        by_key.entry(emit_key).or_default().push(value);
    }

    if !types.is_empty() {
        obj.insert(
            "@type".to_string(),
            Value::Array(types.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(name) = name {
        context
            .entry("name".to_string())
            .or_insert_with(|| json!(SCHEMA_NAME));
        obj.insert("name".to_string(), Value::String(name));
    }
    if !keywords.is_empty() {
        context
            .entry("keywords".to_string())
            .or_insert_with(|| json!(SCHEMA_KEYWORDS));
        obj.insert(
            "keywords".to_string(),
            Value::Array(keywords.into_iter().map(Value::String).collect()),
        );
    }
    for (emit_key, mut values) in by_key {
        let value = if values.len() == 1 {
            values.pop().expect("length checked")
        } else {
            Value::Array(values)
        };
        obj.insert(emit_key, value);
    }

    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::{
        EdgeContribution, GraphContribution, RDF_REIFIES, apply_contribution, from_jsonld,
        node_quads, to_jsonld, to_jsonld_compact,
    };
    use kernel::graph::fixtures::GraphFixtures;
    use kernel::graph::{EdgeAssertion, Graph, SemanticSubKind};
    use kernel::types::{GraphScope, NodeProperty};
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

    fn assert_property_fields(
        property: &NodeProperty,
        predicate: &str,
        value: &str,
        datatype: Option<&str>,
        lang: Option<&str>,
        graph_scope: GraphScope,
    ) {
        assert_eq!(property.predicate, predicate);
        assert_eq!(property.value, value);
        assert_eq!(property.datatype.as_deref(), datatype);
        assert_eq!(property.lang.as_deref(), lang);
        assert_eq!(property.graph_scope, graph_scope);
        assert!(!property.statement_id.is_empty());
        assert_eq!(property.provenance_iri, None);
        assert_eq!(property.asserted_at_ms, None);
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
        assert_eq!(node.properties.len(), 1);
        assert_property_fields(
            &node.properties[0],
            "https://schema.org/datePublished",
            "2026-06-02",
            None,
            None,
            GraphScope::Default,
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
    fn typed_and_language_tagged_properties_round_trip_via_the_property_bag() {
        let doc = br#"{
            "@id":"https://a.test/",
            "https://schema.org/datePublished":[{"@value":"2026-06-02","@type":"http://www.w3.org/2001/XMLSchema#date"}],
            "https://schema.org/headline":[{"@value":"Bonjour","@language":"fr"}]
        }"#;
        let contribution = from_jsonld(doc).expect("parse");
        let node = contribution
            .nodes
            .iter()
            .find(|n| n.id == "https://a.test/")
            .expect("node a");
        assert_eq!(node.properties.len(), 2);
        assert_property_fields(
            &node.properties[0],
            "https://schema.org/datePublished",
            "2026-06-02",
            Some("http://www.w3.org/2001/XMLSchema#date"),
            None,
            GraphScope::Default,
        );
        assert_property_fields(
            &node.properties[1],
            "https://schema.org/headline",
            "Bonjour",
            None,
            Some("fr"),
            GraphScope::Default,
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
            json!([{
                "@value": "2026-06-02",
                "@type": "http://www.w3.org/2001/XMLSchema#date"
            }])
        );
        assert_eq!(
            a["https://schema.org/headline"],
            json!([{
                "@value": "Bonjour",
                "@language": "fr"
            }])
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
    fn exports_multiple_statements_on_one_node_pair() {
        let mut graph = Graph::new();
        let a = graph.add_node("https://a.test/".to_string(), Default::default());
        let b = graph.add_node("https://b.test/".to_string(), Default::default());

        graph.assert_relation(
            a,
            b,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
        );
        graph.assert_semantic_predicate(a, b, "https://schema.org/citation".to_string());

        assert_eq!(
            to_jsonld(&graph),
            json!([
                {
                    "@id": "https://a.test/",
                    "https://mere.computer/ns/rel#cites": [{ "@id": "https://b.test/" }],
                    "https://schema.org/citation": [{ "@id": "https://b.test/" }]
                },
                { "@id": "https://b.test/" }
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
    /// THE Phase 2 losslessness gate (petgraph-RDF plan): a graph exercising
    /// the full profile construct matrix — typed + language-tagged literals,
    /// named graph scopes, statement metadata (label / provenance / assertion
    /// time via RDF 1.2 reifiers), two differently-scoped statements on one
    /// pair, a recognized (CiTO-mapped) and a raw predicate, `rdf:type`,
    /// curated title/tags — projects to quads, re-ingests through the quad
    /// path into a FRESH graph, and projects identically: a normalized
    /// (sorted N-Quads) dataset compare, id-stable through the reifier
    /// handles. "Lossless under the profile" as a checked property.
    #[test]
    fn dataset_round_trip_is_lossless_under_the_profile() {
        use kernel::graph::SemanticStatementSpec;

        let mut graph = Graph::new();
        let a = graph.add_node("https://a.test/".to_string(), Default::default());
        let b = graph.add_node("https://b.test/".to_string(), Default::default());
        let c = graph.add_node("https://c.test/".to_string(), Default::default());

        // Curated fast-path literals.
        graph.get_node_mut(a).expect("a").title = "Article A".to_string();
        graph.get_node_mut(a).expect("a").tags =
            std::collections::HashSet::from(["research".to_string()]);

        // A recognized predicate with full statement metadata, plus a second
        // statement on the SAME pair in a different named graph.
        graph
            .assert_semantic_statement(
                a,
                b,
                SemanticStatementSpec {
                    predicate: "https://mere.computer/ns/rel#cites".to_string(),
                    recognized_sub_kind: Some(SemanticSubKind::Cites),
                    label: Some("cited in the intro".to_string()),
                    graph_scope: GraphScope::User,
                    provenance_iri: Some("https://persona.test/mark".to_string()),
                    asserted_at_ms: Some(1_720_000_000_000),
                    ..Default::default()
                },
            )
            .expect("cites statement");
        graph
            .assert_semantic_statement(
                a,
                b,
                SemanticStatementSpec {
                    predicate: "https://mere.computer/ns/rel#cites".to_string(),
                    recognized_sub_kind: Some(SemanticSubKind::Cites),
                    graph_scope: GraphScope::Source,
                    ..Default::default()
                },
            )
            .expect("source-scoped statement");
        // A raw (unrecognized) predicate.
        graph
            .assert_semantic_statement(
                a,
                c,
                SemanticStatementSpec {
                    predicate: "https://example.test/vocab#inspiredBy".to_string(),
                    asserted_at_ms: Some(1_720_000_100_000),
                    ..Default::default()
                },
            )
            .expect("raw predicate statement");

        // Typed + language-tagged + scoped literals with metadata.
        let node_a = graph.get_node_mut(a).expect("a");
        let mut published = NodeProperty::new(
            "https://schema.org/datePublished".to_string(),
            "2026-07-04".to_string(),
        )
        .with_graph_scope(GraphScope::User);
        published.datatype = Some("http://www.w3.org/2001/XMLSchema#date".to_string());
        published.provenance_iri = Some("https://persona.test/mark".to_string());
        published.asserted_at_ms = Some(1_720_000_200_000);
        node_a.properties.push(published);
        let mut greeting = NodeProperty::new(
            "https://schema.org/description".to_string(),
            "bonjour".to_string(),
        );
        greeting.lang = Some("fr".to_string());
        node_a.properties.push(greeting);

        // rdf:type via classification (through the delta, the public write).
        let _ = kernel::graph::apply::apply_graph_delta(
            &mut graph,
            kernel::graph::apply::GraphDelta::AddNodeClassification {
                key: a,
                classification: kernel::types::NodeClassification {
                    scheme: kernel::types::ClassificationScheme::Custom("rdf:type".to_string()),
                    value: "https://schema.org/Article".to_string(),
                    label: None,
                    confidence: 1.0,
                    provenance: kernel::types::ClassificationProvenance::Imported,
                    status: kernel::types::ClassificationStatus::Imported,
                    primary: false,
                },
            },
        );

        let normalized = |graph: &Graph| -> Vec<String> {
            let mut lines: Vec<String> = crate::dataset_quads(graph)
                .iter()
                .map(|quad| format!("{quad} ."))
                .collect();
            lines.sort();
            lines
        };
        let exported = normalized(&graph);

        let contribution = crate::ingest::from_quads(crate::dataset_quads(&graph), "gate")
            .expect("quad ingest");
        let mut reimported = Graph::new();
        let outcome = crate::ingest::apply_contribution(&mut reimported, &contribution);
        assert!(outcome.edges_skipped == 0, "self-contained contribution");

        let reexported = normalized(&reimported);
        assert_eq!(
            exported, reexported,
            "RDF -> kernel -> RDF is byte-stable as a normalized dataset"
        );
    }

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

    #[cfg(feature = "query")]
    #[test]
    fn sparql_selects_a_literal_and_an_edge_over_node_quads() {
        let graph = seed();

        // A curated literal: A's schema:name.
        let names = crate::query::sparql(
            &graph,
            "SELECT ?name WHERE { <https://a.test/> <https://schema.org/name> ?name }",
        )
        .expect("name query");
        assert_eq!(names.variables, vec!["name".to_string()]);
        assert_eq!(names.rows, vec![vec![Some("Article A".to_string())]]);

        // A recognized semantic edge: A cites B (canonical Mere IRI).
        let cites = crate::query::sparql(
            &graph,
            "SELECT ?t WHERE { <https://a.test/> <https://mere.computer/ns/rel#cites> ?t }",
        )
        .expect("edge query");
        assert_eq!(cites.rows, vec![vec![Some("https://b.test/".to_string())]]);
    }

    #[cfg(feature = "query")]
    #[test]
    fn sparql_graph_clause_sees_scoped_statements_and_properties() {
        let mut graph = Graph::new();
        let a = graph.add_node("https://a.test/".to_string(), Default::default());
        let b = graph.add_node("https://b.test/".to_string(), Default::default());

        kernel::graph::apply::assert_semantic_relation_in_scope(
            &mut graph,
            a,
            b,
            SemanticSubKind::Cites,
            None,
            GraphScope::Source,
        );
        graph.get_node_mut(a).expect("node a").properties.push(
            NodeProperty::new(
                "https://schema.org/datePublished".to_string(),
                "2026-07-04".to_string(),
            )
            .with_graph_scope(GraphScope::User),
        );

        let scoped_edge = crate::query::sparql(
            &graph,
            "SELECT ?t WHERE { GRAPH <https://mere.computer/ns/graph#source> { <https://a.test/> <https://mere.computer/ns/rel#cites> ?t } }",
        )
        .expect("scoped edge query");
        assert_eq!(
            scoped_edge.rows,
            vec![vec![Some("https://b.test/".to_string())]]
        );

        let scoped_property = crate::query::sparql(
            &graph,
            "SELECT ?v WHERE { GRAPH <https://mere.computer/ns/graph#user> { <https://a.test/> <https://schema.org/datePublished> ?v } }",
        )
        .expect("scoped property query");
        assert_eq!(
            scoped_property.rows,
            vec![vec![Some("2026-07-04".to_string())]]
        );
    }

    #[cfg(feature = "query")]
    #[test]
    fn sparql_exposes_reifier_metadata_but_node_quads_stay_clean() {
        let mut graph = Graph::new();
        let a = graph.add_node("https://a.test/".to_string(), Default::default());
        let b = graph.add_node("https://b.test/".to_string(), Default::default());

        kernel::graph::apply::assert_semantic_relation_in_scope(
            &mut graph,
            a,
            b,
            SemanticSubKind::Cites,
            Some("cites".to_string()),
            GraphScope::Source,
        );

        let edge = graph.find_edge_key(a, b).expect("semantic edge");
        let payload = graph.get_edge_mut(edge).expect("payload");
        let statement = payload
            .semantic
            .as_mut()
            .and_then(|semantic| semantic.statements.iter_mut().next())
            .expect("semantic statement");
        statement.statement_id = "stmt-edge-1".to_string();
        statement.provenance_iri = Some("https://people.test/alice".to_string());
        statement.asserted_at_ms = Some(1_720_000_000_123);

        let mut property = NodeProperty::new(
            "https://schema.org/datePublished".to_string(),
            "2026-07-04".to_string(),
        )
        .with_graph_scope(GraphScope::User)
        .with_metadata(
            Some("https://people.test/bob".to_string()),
            Some(1_720_000_100_456),
        );
        property.statement_id = "stmt-prop-1".to_string();
        graph
            .get_node_mut(a)
            .expect("node a")
            .properties
            .push(property);

        let (_, node_a) = graph.get_node_by_url("https://a.test/").expect("node a");
        assert!(
            node_quads(&graph, a, node_a)
                .iter()
                .all(|quad| quad.predicate.as_str() != RDF_REIFIES)
        );

        let edge_metadata = crate::query::sparql(
            &graph,
            "SELECT ?prov ?label WHERE {
                GRAPH <https://mere.computer/ns/graph#source> {
                    <urn:mere:statement:stmt-edge-1> <http://www.w3.org/ns/prov#wasAttributedTo> ?prov ;
                                                     <http://www.w3.org/2000/01/rdf-schema#label> ?label .
                }
            }",
        )
        .expect("edge metadata query");
        assert_eq!(
            edge_metadata.rows,
            vec![vec![
                Some("https://people.test/alice".to_string()),
                Some("cites".to_string()),
            ]]
        );

        let property_metadata = crate::query::sparql(
            &graph,
            "SELECT ?prov WHERE {
                GRAPH <https://mere.computer/ns/graph#user> {
                    <urn:mere:statement:stmt-prop-1> <http://www.w3.org/ns/prov#wasAttributedTo> ?prov .
                }
            }",
        )
        .expect("property metadata query");
        assert_eq!(
            property_metadata.rows,
            vec![vec![Some("https://people.test/bob".to_string())]]
        );
    }
}
