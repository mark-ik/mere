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
#[cfg(not(target_arch = "wasm32"))]
use kernel::types::{
    ClassificationProvenance, ClassificationScheme, ClassificationStatus, NodeClassification,
    NodeProperty,
};

/// `rdf:type` — the predicate JSON-LD `@type` expands to.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// A node described by an ingested JSON-LD document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeContribution {
    /// The subject IRI (a URL, or a skolemized `urn:` IRI for a blank node).
    pub id: String,
    /// `@type` IRIs — applied as `rdf:type` classifications by
    /// [`apply_contribution`].
    pub types: Vec<String>,
    /// `schema:name`, if present.
    pub title: Option<String>,
    /// `schema:keywords` values.
    pub tags: Vec<String>,
    /// Non-curated literal properties: `(predicate IRI, value)` pairs — every
    /// literal beyond `schema:name` / `schema:keywords`.
    pub properties: Vec<(String, String)>,
}

impl NodeContribution {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            types: Vec::new(),
            title: None,
            tags: Vec::new(),
            properties: Vec::new(),
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

/// A stable per-document namespace (FNV-1a over the bytes, hex) that scopes a
/// blank node's skolemized IRI to its source document. oxjsonld already assigns
/// each blank a unique label per parse, so this is defensive scoping rather than
/// strict collision-avoidance; full re-ingest idempotency for blanks would need
/// RDF canonicalization (URDNA2015), which is out of scope.
fn doc_namespace(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Skolemize a blank-node id to a Mere IRI, scoped to its source document via
/// `namespace`.
fn skolemize(namespace: &str, blank_id: &str) -> String {
    format!("urn:mere:bnode:{namespace}:{blank_id}")
}

/// Canonicalize a schema.org IRI to its `https` form. The schema.org `@context`
/// uses `@vocab: http://schema.org/`, so a document referencing it expands terms
/// to `http://schema.org/…`; Mere keys on (and stores) `https://schema.org/…`, so
/// the two flavors of schema.org document unify.
fn normalize_schema_org(iri: &str) -> String {
    match iri.strip_prefix("http://schema.org/") {
        Some(rest) => format!("https://schema.org/{rest}"),
        None => iri.to_string(),
    }
}

/// Literal predicates promoted to a node's title, across the curated
/// vocabularies (schema.org, Dublin Core, ActivityStreams). Last one seen wins.
const TITLE_PREDICATES: &[&str] = &[
    SCHEMA_NAME,                                  // schema:name
    "http://purl.org/dc/terms/title",             // dcterms:title
    "http://purl.org/dc/elements/1.1/title",      // dc:title
    "https://www.w3.org/ns/activitystreams#name", // as:name
];

/// Literal predicates promoted to a node's tags.
const TAGS_PREDICATES: &[&str] = &[
    SCHEMA_KEYWORDS,                          // schema:keywords
    "http://purl.org/dc/terms/subject",       // dcterms:subject
    "http://purl.org/dc/elements/1.1/subject", // dc:subject
];

fn is_title_predicate(predicate: &str) -> bool {
    TITLE_PREDICATES.contains(&predicate)
}

fn is_tags_predicate(predicate: &str) -> bool {
    TAGS_PREDICATES.contains(&predicate)
}

/// The IRI for a subject term (skolemizing a blank node within `namespace`).
fn subject_iri(subject: &NamedOrBlankNode, namespace: &str) -> String {
    match subject {
        NamedOrBlankNode::NamedNode(node) => node.as_str().to_string(),
        NamedOrBlankNode::BlankNode(node) => skolemize(namespace, node.as_str()),
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
    collect_contribution(JsonLdParser::new().for_slice(bytes), &doc_namespace(bytes))
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
    let namespace = doc_namespace(bytes);
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
    collect_contribution(quads, &namespace)
}

/// Extract every `<script type="application/ld+json">` block from an HTML body
/// and parse each as JSON-LD, returning the contributions that parsed (parse
/// failures and other `<script>` types are skipped). A lightweight tag scan, not
/// a full HTML parser — enough to harvest the structured data most pages embed.
/// A host pairs this with rendering: a page both displays and contributes its
/// linked data.
/// Scan a JSON-LD document for the remote `@context` URLs it references — the
/// string entries of any `@context`, top-level or nested, including inside
/// `@graph`. A host fetches these (minus the ones the bundled packs already
/// cover, see [`is_bundled_context`]) and folds them into a [`ContextCache`]
/// before ingest, so an unbundled vocabulary resolves. Pure and network-free;
/// returns `[]` for non-JSON input.
pub fn referenced_context_urls(body: &[u8]) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let mut urls = Vec::new();
    collect_context_urls(&value, &mut urls);
    urls.sort();
    urls.dedup();
    urls
}

/// Walk every object for an `@context`, collecting its remote string references.
fn collect_context_urls(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(ctx) = map.get("@context") {
                collect_context_strings(ctx, out);
            }
            for (key, child) in map {
                if key != "@context" {
                    collect_context_urls(child, out);
                }
            }
        },
        serde_json::Value::Array(items) => items.iter().for_each(|i| collect_context_urls(i, out)),
        _ => {},
    }
}

/// A `@context` value is a URL string, an array (URL strings + inline objects), or
/// an inline object. Collect only the remote (`http(s)`) string references.
fn collect_context_strings(ctx: &serde_json::Value, out: &mut Vec<String>) {
    match ctx {
        serde_json::Value::String(s) if s.starts_with("http://") || s.starts_with("https://") => {
            out.push(s.clone());
        },
        serde_json::Value::Array(items) => {
            items.iter().for_each(|i| collect_context_strings(i, out))
        },
        _ => {},
    }
}

pub fn from_html(html: &str) -> Vec<GraphContribution> {
    from_html_with_contexts(html, ContextCache::new())
}

/// Like [`from_html`], but each embedded document is parsed with `contexts`, so a
/// `<script>` block referencing a remote `@context` (e.g. schema.org) resolves
/// from the bundled cache rather than being skipped.
pub fn from_html_with_contexts(html: &str, contexts: ContextCache) -> Vec<GraphContribution> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(rel) = lower[pos..].find("<script") {
        let tag_start = pos + rel;
        let Some(gt) = lower[tag_start..].find('>') else {
            break;
        };
        let open_tag = &lower[tag_start..tag_start + gt];
        let content_start = tag_start + gt + 1;
        let Some(close_rel) = lower[content_start..].find("</script>") else {
            break;
        };
        let content_end = content_start + close_rel;
        if open_tag.contains("application/ld+json") {
            let block = html[content_start..content_end].as_bytes();
            if let Ok(contribution) = from_jsonld_with_contexts(block, contexts.clone()) {
                out.push(contribution);
            }
        }
        pos = content_end + "</script>".len();
    }
    out
}

/// Group a stream of RDF quads into a [`GraphContribution`] — shared by the
/// network-free and bundled-context parsers. `namespace` scopes blank-node
/// skolemization to the source document.
fn collect_contribution<E: std::fmt::Display>(
    quads: impl Iterator<Item = Result<Quad, E>>,
    namespace: &str,
) -> Result<GraphContribution, IngestError> {
    let mut nodes: BTreeMap<String, NodeContribution> = BTreeMap::new();
    let mut edges: Vec<EdgeContribution> = Vec::new();

    for quad in quads {
        let quad = quad.map_err(|err| IngestError::Parse(err.to_string()))?;
        let subject = subject_iri(&quad.subject, namespace);
        let predicate_norm = normalize_schema_org(quad.predicate.as_str());
        let predicate = predicate_norm.as_str();
        nodes
            .entry(subject.clone())
            .or_insert_with(|| NodeContribution::new(&subject));

        match &quad.object {
            Term::Literal(literal) => {
                let node = nodes.get_mut(&subject).expect("subject just inserted");
                let value = literal.value().to_string();
                if is_title_predicate(predicate) {
                    node.title = Some(value);
                } else if is_tags_predicate(predicate) {
                    node.tags.push(value);
                } else {
                    // Any other literal goes into the open property bag.
                    node.properties.push((predicate.to_string(), value));
                }
            }
            Term::NamedNode(object) => {
                route_resource(&mut nodes, &mut edges, &subject, predicate, object.as_str().to_string())
            }
            Term::BlankNode(object) => route_resource(
                &mut nodes,
                &mut edges,
                &subject,
                predicate,
                skolemize(namespace, object.as_str()),
            ),
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
            node.properties.sort();
            node.properties.dedup();
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

/// The URL Mere's curated context is served at.
const MERE_CONTEXT_URL: &str = "https://mere.computer/ns/context";

/// Mere's curated context (the "minimal" pack): `schema:name` / `schema:keywords`
/// plus the `mere:` relation prefix.
const MERE_MINIMAL_CONTEXT: &[u8] = br#"{"@context":{"name":"https://schema.org/name","keywords":"https://schema.org/keywords","mere":"https://mere.computer/ns/rel#"}}"#;

/// The vendored schema.org context (see `assets/NOTICE`; CC BY-SA 3.0).
#[cfg(feature = "bundled-contexts")]
const SCHEMA_ORG_CONTEXT: &[u8] = include_bytes!("../assets/schema.org.context.jsonld");

/// The `@context` URLs a document may reference for schema.org.
#[cfg(feature = "bundled-contexts")]
const SCHEMA_ORG_URLS: &[&str] = &[
    "https://schema.org/",
    "https://schema.org",
    "http://schema.org/",
    "http://schema.org",
];

/// The vendored ActivityStreams 2.0 context (see `assets/NOTICE`; W3C license).
#[cfg(feature = "bundled-contexts")]
const ACTIVITYSTREAMS_CONTEXT: &[u8] = include_bytes!("../assets/activitystreams.context.jsonld");

#[cfg(feature = "bundled-contexts")]
const ACTIVITYSTREAMS_URLS: &[&str] = &[
    "https://www.w3.org/ns/activitystreams",
    "http://www.w3.org/ns/activitystreams",
];

/// A small Mere-authored Dublin Core context (MPL; not a third-party asset).
#[cfg(feature = "bundled-contexts")]
const DUBLIN_CORE_CONTEXT: &[u8] = include_bytes!("../assets/dublin-core.context.jsonld");

#[cfg(feature = "bundled-contexts")]
const DUBLIN_CORE_URLS: &[&str] = &["http://purl.org/dc/terms/", "http://purl.org/dc/elements/1.1/"];

/// Whether `url` is a remote `@context` the bundled packs already cover, so a host
/// need not fetch it before ingest. Always `false` when `bundled-contexts` is off
/// (then `full()` equals `minimal()`, so every remote context must be fetched).
pub fn is_bundled_context(url: &str) -> bool {
    #[cfg(feature = "bundled-contexts")]
    {
        SCHEMA_ORG_URLS.contains(&url)
            || ACTIVITYSTREAMS_URLS.contains(&url)
            || DUBLIN_CORE_URLS.contains(&url)
    }
    #[cfg(not(feature = "bundled-contexts"))]
    {
        let _ = url;
        false
    }
}

impl ContextCache {
    /// An empty cache (the **none** preset): every remote `@context` is refused.
    pub fn new() -> Self {
        Self::default()
    }

    /// The **minimal** preset: only Mere's curated context (`schema:name` /
    /// `schema:keywords` + the `mere:` relation prefix), served at
    /// `https://mere.computer/ns/context`. Tiny; resolves Mere-authored documents
    /// but not arbitrary schema.org pages — those need [`full`](Self::full).
    pub fn minimal() -> Self {
        Self::new().with(MERE_CONTEXT_URL, MERE_MINIMAL_CONTEXT)
    }

    /// The **full** preset (the host default): the minimal context plus the
    /// vendored standard vocabularies (schema.org, ActivityStreams 2.0, Dublin
    /// Core; see `assets/NOTICE`), so an arbitrary remote `@context` for any of
    /// them resolves offline. Each pack is an independent module registered
    /// here; with the `bundled-contexts` feature off this equals
    /// [`minimal`](Self::minimal). A user can also step down to `minimal` /
    /// `new`, or bring their own via [`with`](Self::with).
    pub fn full() -> Self {
        #[allow(unused_mut)]
        let mut cache = Self::minimal();
        #[cfg(feature = "bundled-contexts")]
        {
            cache = cache
                .register(SCHEMA_ORG_URLS, SCHEMA_ORG_CONTEXT)
                .register(ACTIVITYSTREAMS_URLS, ACTIVITYSTREAMS_CONTEXT)
                .register(DUBLIN_CORE_URLS, DUBLIN_CORE_CONTEXT);
        }
        cache
    }

    /// Register `document` as the context served for each of `urls`.
    #[cfg(feature = "bundled-contexts")]
    fn register(mut self, urls: &[&str], document: &'static [u8]) -> Self {
        for url in urls {
            self = self.with(*url, document);
        }
        self
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

    /// An ingested `@type` IRI as a classification under the `rdf:type` scheme
    /// (the full type IRI is the value — lossless).
    fn rdf_type_classification(type_iri: &str) -> NodeClassification {
        NodeClassification {
            scheme: ClassificationScheme::Custom("rdf:type".to_string()),
            value: type_iri.to_string(),
            label: None,
            confidence: 1.0,
            provenance: ClassificationProvenance::Imported,
            status: ClassificationStatus::Imported,
            primary: false,
        }
    }

    let mut outcome = ApplyOutcome::default();
    let mut key_for: HashMap<&str, NodeKey> = HashMap::new();

    for node in &contribution.nodes {
        let key = graph
            .get_node_by_url(&node.id)
            .map(|(key, _)| key)
            .unwrap_or_else(|| {
                outcome.nodes_created += 1;
                // The `@id` is the node's identity here, so mint a deterministic
                // UUIDv5 from it: two hosts ingesting the same document agree on
                // node ids, so a federated merge needs no reconciliation.
                graph.add_node_with_id(
                    Graph::node_namespace_id(&node.id),
                    node.id.clone(),
                    Default::default(),
                )
            });
        if node.title.is_some() || !node.tags.is_empty() || !node.properties.is_empty() {
            if let Some(target) = graph.get_node_mut(key) {
                if let Some(title) = &node.title {
                    target.title = title.clone();
                }
                for tag in &node.tags {
                    target.tags.insert(tag.clone());
                }
                for (predicate, value) in &node.properties {
                    let property = NodeProperty {
                        predicate: predicate.clone(),
                        value: value.clone(),
                    };
                    if !target.properties.contains(&property) {
                        target.properties.push(property);
                    }
                }
            }
        }
        // `@type` IRIs become `rdf:type` classifications (kernel dedups them).
        for type_iri in &node.types {
            graph.add_node_classification(key, rdf_type_classification(type_iri));
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
        assert_eq!(contributions.len(), 1, "only the ld+json script is harvested");
        let nodes = &contributions[0].nodes;
        assert!(nodes.iter().any(|n| n.id == "https://a.test/" && n.title.as_deref() == Some("Paper A")));
        assert!(contributions[0].edges.iter().any(|e| e.predicate.ends_with("#cites")));
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
        assert_eq!(prefix(&a), prefix(&blank(doc_a)), "namespace is content-stable");
        assert_ne!(prefix(&a), prefix(&blank(doc_b)), "different doc, different namespace");
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
        assert!(
            node.properties
                .iter()
                .any(|(p, v)| p == "https://schema.org/datePublished" && v == "2026-06-02")
        );
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
        let doc = br#"{"@context":{"name":"https://schema.org/name"},"@id":"https://x.test/","name":"X"}"#;
        let contribution = from_jsonld(doc).expect("parsed");
        let mut g1 = Graph::new();
        let mut g2 = Graph::new();
        apply_contribution(&mut g1, &contribution);
        apply_contribution(&mut g2, &contribution);
        let id1 = g1.get_node_by_url("https://x.test/").expect("node in g1").1.id;
        let id2 = g2.get_node_by_url("https://x.test/").expect("node in g2").1.id;
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
        assert_eq!(urls.len(), 2, "inline object entries are not URL references");
    }

    #[test]
    fn referenced_context_urls_ignores_inline_and_nonjson() {
        let inline = br#"{"@context":{"name":"https://schema.org/name"},"@id":"a"}"#;
        assert!(referenced_context_urls(inline).is_empty(), "inline context has no remote ref");
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
