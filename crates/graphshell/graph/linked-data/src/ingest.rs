/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! JSON-LD ingest (linked-data plan Phase 2): `application/ld+json` → a
//! [`GraphContribution`].
//!
//! [`from_jsonld`] is a **pure** parse — bytes through `oxjsonld` (sync,
//! wasm-light) to RDF triples, grouped into the contribution. It mirrors
//! [`crate::to_jsonld`]: a resource-valued predicate becomes an edge, `rdf:type`
//! becomes a node type, and the curated literals `schema:name` / `schema:keywords`
//! become a node's title / tags (every other literal is dropped — the kernel has
//! no general property bag). Blank nodes are skolemized to a `urn:mere:bnode:`
//! IRI.
//!
//! [`apply_contribution`] materializes a contribution into a [`Graph`]: it
//! creates a node per subject/object (or reuses one matched by URL) and asserts
//! each edge — a recognized predicate as a typed `Semantic` edge (sub-kind +
//! canonical IRI), an unrecognized one as an **open-predicate** edge via
//! `Graph::assert_semantic_predicate`. It is `not(wasm32)` because `add_node`
//! mints a UUID; a wasm host materializes from the same contribution with
//! `add_node_with_id`.
//!
//! Out of scope here (later): `@type` → node classification (a class-IRI scheme,
//! as in export), CURIE/remote `@context` resolution (bundled-context loader),
//! and full literal fidelity.

use crate::{SCHEMA_KEYWORDS, SCHEMA_NAME};
use oxjsonld::{JsonLdParser, JsonLdRemoteDocument};
use oxrdf::{NamedOrBlankNode, Quad, Term};
use std::collections::BTreeMap;

#[cfg(not(target_arch = "wasm32"))]
use kernel::graph::{EdgeAssertion, Graph, NodeKey, predicate_iri, sub_kind_from_iri};

/// `rdf:type` — the predicate JSON-LD `@type` expands to.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// A node described by an ingested JSON-LD document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeContribution {
    /// The subject IRI (a URL, or a skolemized `urn:` IRI for a blank node).
    pub id: String,
    /// `@type` IRIs (carried for a future class-IRI mapping; not yet applied).
    pub types: Vec<String>,
    /// `schema:name`, if present.
    pub title: Option<String>,
    /// `schema:keywords` values.
    pub tags: Vec<String>,
}

impl NodeContribution {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            types: Vec::new(),
            title: None,
            tags: Vec::new(),
        }
    }
}

/// A predicate edge: `subject —predicate→ object`, all IRIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeContribution {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

/// The result of parsing a JSON-LD document: the nodes it describes and the
/// predicate edges between them. Every IRI referenced by an edge also appears as
/// a node, so the contribution is self-contained.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphContribution {
    pub nodes: Vec<NodeContribution>,
    pub edges: Vec<EdgeContribution>,
}

/// JSON-LD ingest failure.
#[derive(Debug)]
pub enum IngestError {
    /// The bytes were not valid JSON-LD (oxjsonld parse/expansion error).
    Parse(String),
}

impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IngestError::Parse(msg) => write!(f, "JSON-LD parse error: {msg}"),
        }
    }
}

impl std::error::Error for IngestError {}

/// Skolemize a blank-node id (document-scoped) to a stable Mere IRI.
fn skolemize(blank_id: &str) -> String {
    format!("urn:mere:bnode:{blank_id}")
}

/// The IRI for a subject term (skolemizing a blank node).
fn subject_iri(subject: &NamedOrBlankNode) -> String {
    match subject {
        NamedOrBlankNode::NamedNode(node) => node.as_str().to_string(),
        NamedOrBlankNode::BlankNode(node) => skolemize(node.as_str()),
    }
}

/// Route a resource-valued object: `rdf:type` adds a node type, anything else is
/// an edge.
fn route_resource(
    nodes: &mut BTreeMap<String, NodeContribution>,
    edges: &mut Vec<EdgeContribution>,
    subject: &str,
    predicate: &str,
    object: String,
) {
    if predicate == RDF_TYPE {
        nodes
            .get_mut(subject)
            .expect("subject inserted before routing")
            .types
            .push(object);
    } else {
        edges.push(EdgeContribution {
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object,
        });
    }
}

/// Parse an `application/ld+json` document into a [`GraphContribution`]. Pure: no
/// graph, no kernel mutation. Expects inline-`@context` or expanded JSON-LD; a
/// remote `@context` is not fetched (a bundled-context loader is a later step).
pub fn from_jsonld(bytes: &[u8]) -> Result<GraphContribution, IngestError> {
    collect_contribution(JsonLdParser::new().for_slice(bytes))
}

/// Like [`from_jsonld`], but a remote `@context` is resolved from `contexts`
/// instead of the network. A context URL absent from the cache is an error, so
/// ingest never makes a request. The cache is the seam for bundling schema.org /
/// Dublin Core / ActivityStreams; populating it (the asset-weight decision) is a
/// separate step.
pub fn from_jsonld_with_contexts(
    bytes: &[u8],
    contexts: ContextCache,
) -> Result<GraphContribution, IngestError> {
    let quads = JsonLdParser::new()
        .for_slice(bytes)
        .with_load_document_callback(move |url, _options| {
            contexts
                .get(url)
                .map(|document| JsonLdRemoteDocument {
                    document: document.to_vec(),
                    document_url: url.to_string(),
                })
                .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                    format!("refused remote @context (not in the bundled cache): {url}").into()
                })
        });
    collect_contribution(quads)
}

/// Group a stream of RDF quads into a [`GraphContribution`] — shared by the
/// network-free and bundled-context parsers.
fn collect_contribution<E: std::fmt::Display>(
    quads: impl Iterator<Item = Result<Quad, E>>,
) -> Result<GraphContribution, IngestError> {
    let mut nodes: BTreeMap<String, NodeContribution> = BTreeMap::new();
    let mut edges: Vec<EdgeContribution> = Vec::new();

    for quad in quads {
        let quad = quad.map_err(|err| IngestError::Parse(err.to_string()))?;
        let subject = subject_iri(&quad.subject);
        let predicate = quad.predicate.as_str();
        nodes
            .entry(subject.clone())
            .or_insert_with(|| NodeContribution::new(&subject));

        match &quad.object {
            Term::Literal(literal) => {
                let node = nodes.get_mut(&subject).expect("subject just inserted");
                match predicate {
                    SCHEMA_NAME => node.title = Some(literal.value().to_string()),
                    SCHEMA_KEYWORDS => node.tags.push(literal.value().to_string()),
                    // Uncurated literal: dropped (no general property bag).
                    _ => {}
                }
            }
            Term::NamedNode(object) => {
                route_resource(&mut nodes, &mut edges, &subject, predicate, object.as_str().to_string())
            }
            Term::BlankNode(object) => {
                route_resource(&mut nodes, &mut edges, &subject, predicate, skolemize(object.as_str()))
            }
        }
    }

    // Make the contribution self-contained: every edge endpoint is a node.
    for edge in &edges {
        nodes
            .entry(edge.object.clone())
            .or_insert_with(|| NodeContribution::new(&edge.object));
    }

    let nodes = nodes
        .into_values()
        .map(|mut node| {
            node.types.sort();
            node.types.dedup();
            node.tags.sort();
            node.tags.dedup();
            node
        })
        .collect();

    Ok(GraphContribution { nodes, edges })
}

/// A bundled JSON-LD `@context` cache for offline ingest: a map of context URL to
/// its document bytes. [`from_jsonld_with_contexts`] serves these and refuses any
/// URL not present, so a document's remote `@context` never hits the network.
#[derive(Clone, Default)]
pub struct ContextCache {
    documents: std::collections::HashMap<String, Vec<u8>>,
}

impl ContextCache {
    /// An empty cache: every remote `@context` is refused.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bundle a context document under its URL (builder style).
    pub fn with(mut self, url: impl Into<String>, document: impl Into<Vec<u8>>) -> Self {
        self.documents.insert(url.into(), document.into());
        self
    }

    fn get(&self, url: &str) -> Option<&[u8]> {
        self.documents.get(url).map(Vec::as_slice)
    }
}

/// What [`apply_contribution`] did.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyOutcome {
    pub nodes_created: usize,
    pub edges_asserted: usize,
    /// Edges whose subject or object node could not be resolved (should be zero
    /// for a self-contained contribution).
    pub edges_skipped: usize,
}

/// Materialize a [`GraphContribution`] into `graph`: a node per subject/object
/// (reused if matched by URL, else created), and one `Semantic` edge per edge —
/// recognized predicate → typed sub-kind + canonical IRI; unrecognized → open
/// predicate via [`Graph::assert_semantic_predicate`]. Curated literals
/// (`title` / `tags`) are written onto the node; `@type` is not yet mapped.
///
/// `not(wasm32)`: `add_node` mints a UUID. A wasm host materializes from the same
/// contribution using `add_node_with_id` with a host-provided UUID.
#[cfg(not(target_arch = "wasm32"))]
pub fn apply_contribution(graph: &mut Graph, contribution: &GraphContribution) -> ApplyOutcome {
    use std::collections::HashMap;

    let mut outcome = ApplyOutcome::default();
    let mut key_for: HashMap<&str, NodeKey> = HashMap::new();

    for node in &contribution.nodes {
        let key = graph
            .get_node_by_url(&node.id)
            .map(|(key, _)| key)
            .unwrap_or_else(|| {
                outcome.nodes_created += 1;
                graph.add_node(node.id.clone(), Default::default())
            });
        if node.title.is_some() || !node.tags.is_empty() {
            if let Some(target) = graph.get_node_mut(key) {
                if let Some(title) = &node.title {
                    target.title = title.clone();
                }
                for tag in &node.tags {
                    target.tags.insert(tag.clone());
                }
            }
        }
        key_for.insert(node.id.as_str(), key);
    }

    for edge in &contribution.edges {
        let (Some(&from), Some(&to)) = (
            key_for.get(edge.subject.as_str()),
            key_for.get(edge.object.as_str()),
        ) else {
            outcome.edges_skipped += 1;
            continue;
        };
        let asserted = if let Some(sub_kind) = sub_kind_from_iri(&edge.predicate) {
            // Recognized: typed Semantic edge + its canonical predicate IRI.
            graph
                .assert_relation(
                    from,
                    to,
                    EdgeAssertion::Semantic {
                        sub_kind,
                        label: None,
                        decay_progress: None,
                    },
                )
                .inspect(|&key| {
                    if let Some(payload) = graph.get_edge_mut(key) {
                        payload.set_semantic_predicate(Some(predicate_iri(sub_kind).to_string()));
                    }
                })
                .is_some()
        } else {
            // Unrecognized: an open-predicate Semantic edge (raw IRI).
            graph
                .assert_semantic_predicate(from, to, edge.predicate.clone())
                .is_some()
        };
        if asserted {
            outcome.edges_asserted += 1;
        }
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expanded JSON-LD (full IRIs, no `@context`) — also the shape
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
        }));
        assert!(contribution.edges.contains(&EdgeContribution {
            subject: "https://a.test/".into(),
            predicate: "https://schema.org/citation".into(),
            object: "https://c.test/".into(),
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
        let cites = graph.get_edge(graph.find_edge_key(a, b).expect("a→b")).unwrap();
        assert!(cites.has_relation(RelationSelector::Semantic(SemanticSubKind::Cites)));
        assert_eq!(
            cites.semantic_data().and_then(|d| d.predicate.as_deref()),
            Some("https://mere.computer/ns/rel#cites")
        );

        // Raw predicate → open-predicate Semantic edge (no sub-kinds).
        let citation = graph.get_edge(graph.find_edge_key(a, c).expect("a→c")).unwrap();
        assert!(citation.has_relation(RelationSelector::Family(EdgeFamily::Semantic)));
        assert!(citation.semantic_data().is_some_and(|d| d.sub_kinds.is_empty()));
        assert_eq!(
            citation.semantic_data().and_then(|d| d.predicate.as_deref()),
            Some("https://schema.org/citation")
        );
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
        assert!(matches!(from_jsonld(REMOTE_DOC), Err(IngestError::Parse(_))));
    }
}
