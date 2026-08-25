// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! JSON-LD ingest (linked-data plan Phase 2): `application/ld+json` → a
//! [`GraphContribution`].
//!
//! [`from_jsonld`] is a **pure** parse — bytes through `oxjsonld` (sync,
//! wasm-light) to RDF triples, grouped into the contribution. It mirrors
//! [`crate::to_jsonld`]: a resource-valued predicate becomes an edge, `rdf:type`
//! becomes a node type, and the curated literals `schema:name` / `schema:keywords`
//! become a node's title / tags; every other literal lands in the node property
//! bag, carrying datatype and language-tag metadata when present. Blank nodes are
//! skolemized to a `urn:mere:bnode:` IRI.
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
//! and named-graph / statement metadata fidelity.

use crate::{SCHEMA_KEYWORDS, SCHEMA_NAME};
use kernel::types::{GraphScope, NodeProperty};
use oxjsonld::{JsonLdParser, JsonLdRemoteDocument};
use oxrdf::{NamedOrBlankNode, Quad, Term};
use std::collections::BTreeMap;

/// `rdf:type` — the predicate JSON-LD `@type` expands to.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

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
    /// Non-curated literal properties: every literal beyond `schema:name` /
    /// `schema:keywords`, including datatype/lang fidelity when present.
    pub properties: Vec<NodeProperty>,
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

/// A predicate edge: `subject —predicate→ object`, all IRIs. Statement
/// metadata (fact handle, label, provenance, assertion time) arrives via an
/// RDF 1.2 reifier over the triple; absent for plain edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeContribution {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub graph_scope: GraphScope,
    /// The fact handle a `urn:mere:statement:<id>` reifier carried; preserving
    /// it through apply is what makes the dataset round trip id-stable.
    pub statement_id: Option<String>,
    pub label: Option<String>,
    pub provenance_iri: Option<String>,
    pub asserted_at_ms: Option<u64>,
}

/// The result of parsing a JSON-LD document: the nodes it describes and the
/// predicate edges between them. Every IRI referenced by an edge also appears as
/// a node, so the contribution is self-contained.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphContribution {
    pub nodes: Vec<NodeContribution>,
    pub edges: Vec<EdgeContribution>,
}

/// RDF ingest failure (JSON-LD, N-Quads, or TriG).
#[derive(Debug)]
pub enum IngestError {
    /// The bytes were not valid for their RDF syntax (oxjsonld / oxttl
    /// parse/expansion error).
    Parse(String),
}

impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IngestError::Parse(msg) => write!(f, "RDF parse error: {msg}"),
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
    SCHEMA_KEYWORDS,                           // schema:keywords
    "http://purl.org/dc/terms/subject",        // dcterms:subject
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
    graph_scope: GraphScope,
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
            graph_scope,
            statement_id: None,
            label: None,
            provenance_iri: None,
            asserted_at_ms: None,
        });
    }
}

fn scope_from_graph_name(graph_name: &oxrdf::GraphName, namespace: &str) -> GraphScope {
    match graph_name {
        oxrdf::GraphName::DefaultGraph => GraphScope::Default,
        oxrdf::GraphName::NamedNode(node) => match node.as_str() {
            "https://mere.computer/ns/graph#source" => GraphScope::Source,
            "https://mere.computer/ns/graph#user" => GraphScope::User,
            "https://mere.computer/ns/graph#agent" => GraphScope::Agent,
            "https://mere.computer/ns/graph#moot" => GraphScope::Moot,
            iri => GraphScope::Custom(iri.to_string()),
        },
        oxrdf::GraphName::BlankNode(node) => {
            GraphScope::Custom(skolemize(namespace, node.as_str()))
        }
    }
}

/// Parse an `application/ld+json` document into a [`GraphContribution`]. Pure: no
/// graph, no kernel mutation. Expects inline-`@context` or expanded JSON-LD; a
/// remote `@context` is not fetched (a bundled-context loader is a later step).
pub fn from_jsonld(bytes: &[u8]) -> Result<GraphContribution, IngestError> {
    collect_contribution(JsonLdParser::new().for_slice(bytes), &doc_namespace(bytes))
}

/// Ingest a set of RDF quads directly — the dataset path (`dataset_quads` /
/// Turtle / N-Quads), and the half of the Phase 2 round-trip gate that JSON-LD
/// cannot carry: RDF 1.2 reifier metadata (triple terms) is recognized here
/// and attached to the matching edge / property contributions.
pub fn from_quads(
    quads: impl IntoIterator<Item = Quad>,
    namespace: &str,
) -> Result<GraphContribution, IngestError> {
    collect_contribution(
        quads.into_iter().map(Ok::<Quad, std::convert::Infallible>),
        namespace,
    )
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
        }
        serde_json::Value::Array(items) => items.iter().for_each(|i| collect_context_urls(i, out)),
        _ => {}
    }
}

/// A `@context` value is a URL string, an array (URL strings + inline objects), or
/// an inline object. Collect only the remote (`http(s)`) string references.
fn collect_context_strings(ctx: &serde_json::Value, out: &mut Vec<String>) {
    match ctx {
        serde_json::Value::String(s) if s.starts_with("http://") || s.starts_with("https://") => {
            out.push(s.clone());
        }
        serde_json::Value::Array(items) => {
            items.iter().for_each(|i| collect_context_strings(i, out))
        }
        _ => {}
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

/// The reifier-IRI prefix `dataset_quads` mints fact handles under.
const STATEMENT_REIFIER_PREFIX: &str = "urn:mere:statement:";
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const PROV_WAS_ATTRIBUTED_TO: &str = "http://www.w3.org/ns/prov#wasAttributedTo";
const PROV_GENERATED_AT_TIME: &str = "http://www.w3.org/ns/prov#generatedAtTime";

/// One lifted RDF 1.2 reifier: the fact it names + the metadata its other
/// quads carried.
struct ReifiedStatement {
    subject: String,
    predicate: String,
    object: ReifiedObject,
    graph_scope: GraphScope,
    label: Option<String>,
    provenance_iri: Option<String>,
    asserted_at_ms: Option<u64>,
}

enum ReifiedObject {
    Resource(String),
    Literal {
        value: String,
        datatype: String,
        lang: Option<String>,
    },
}

/// Parse an `xsd:dateTime` lexical back to unix ms — the inverse of the
/// export's `prov:generatedAtTime` formatting.
fn parse_xsd_datetime_ms(lexical: &str) -> Option<u64> {
    let parsed =
        time::OffsetDateTime::parse(lexical, &time::format_description::well_known::Rfc3339)
            .ok()?;
    u64::try_from(parsed.unix_timestamp_nanos() / 1_000_000).ok()
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

    // Pass A: materialize the stream and lift out RDF 1.2 reifiers. A subject
    // that `rdf:reifies` a triple term is statement METADATA, not a node: its
    // reified (s, p, o, g) names the fact, its other quads carry the fact's
    // label / provenance / assertion time, and a `urn:mere:statement:<id>`
    // IRI carries the fact handle itself. Everything else flows to pass B.
    let mut plain: Vec<Quad> = Vec::new();
    let mut reified: std::collections::HashMap<String, ReifiedStatement> =
        std::collections::HashMap::new();
    let mut reifier_meta: Vec<Quad> = Vec::new();
    for quad in quads {
        let quad = quad.map_err(|err| IngestError::Parse(err.to_string()))?;
        if quad.predicate.as_str() == RDF_REIFIES
            && let Term::Triple(triple) = &quad.object
        {
            let reifier = subject_iri(&quad.subject, namespace);
            let object = match &triple.object {
                Term::NamedNode(node) => ReifiedObject::Resource(node.as_str().to_string()),
                Term::BlankNode(node) => {
                    ReifiedObject::Resource(skolemize(namespace, node.as_str()))
                }
                Term::Literal(literal) => ReifiedObject::Literal {
                    value: literal.value().to_string(),
                    datatype: literal.datatype().as_str().to_string(),
                    lang: literal.language().map(str::to_string),
                },
                Term::Triple(_) => continue, // nested triple terms: out of profile
            };
            reified.insert(
                reifier,
                ReifiedStatement {
                    subject: subject_iri(&triple.subject.clone().into(), namespace),
                    predicate: normalize_schema_org(triple.predicate.as_str()),
                    object,
                    graph_scope: scope_from_graph_name(&quad.graph_name, namespace),
                    label: None,
                    provenance_iri: None,
                    asserted_at_ms: None,
                },
            );
            continue;
        }
        plain.push(quad);
    }
    if !reified.is_empty() {
        // Split off the reifier-subject metadata quads; the rest stay plain.
        let mut rest = Vec::with_capacity(plain.len());
        for quad in plain {
            if reified.contains_key(&subject_iri(&quad.subject, namespace)) {
                reifier_meta.push(quad);
            } else {
                rest.push(quad);
            }
        }
        plain = rest;
        for quad in &reifier_meta {
            let reifier = subject_iri(&quad.subject, namespace);
            let Some(statement) = reified.get_mut(&reifier) else {
                continue;
            };
            match (quad.predicate.as_str(), &quad.object) {
                (RDFS_LABEL, Term::Literal(literal)) => {
                    statement.label = Some(literal.value().to_string());
                }
                (PROV_WAS_ATTRIBUTED_TO, Term::NamedNode(agent)) => {
                    statement.provenance_iri = Some(agent.as_str().to_string());
                }
                (PROV_GENERATED_AT_TIME, Term::Literal(literal)) => {
                    statement.asserted_at_ms = parse_xsd_datetime_ms(literal.value());
                }
                _ => {}
            }
        }
    }

    for quad in plain {
        let subject = subject_iri(&quad.subject, namespace);
        let predicate_norm = normalize_schema_org(quad.predicate.as_str());
        let predicate = predicate_norm.as_str();
        let graph_scope = scope_from_graph_name(&quad.graph_name, namespace);
        nodes
            .entry(subject.clone())
            .or_insert_with(|| NodeContribution::new(&subject));

        match &quad.object {
            Term::Literal(literal) => {
                let node = nodes.get_mut(&subject).expect("subject just inserted");
                let value = literal.value().to_string();
                if graph_scope == GraphScope::Default && is_title_predicate(predicate) {
                    node.title = Some(value);
                } else if graph_scope == GraphScope::Default && is_tags_predicate(predicate) {
                    node.tags.push(value);
                } else {
                    // Any other literal goes into the open property bag.
                    let lang = literal.language().map(str::to_string);
                    let datatype = if lang.is_some() {
                        None
                    } else {
                        let datatype = literal.datatype().as_str();
                        (datatype != XSD_STRING).then(|| normalize_schema_org(datatype))
                    };
                    let mut property = NodeProperty::new(predicate.to_string(), value)
                        .with_graph_scope(graph_scope.clone());
                    property.datatype = datatype;
                    property.lang = lang;
                    node.properties.push(property);
                }
            }
            Term::NamedNode(object) => route_resource(
                &mut nodes,
                &mut edges,
                &subject,
                predicate,
                object.as_str().to_string(),
                graph_scope,
            ),
            Term::BlankNode(object) => route_resource(
                &mut nodes,
                &mut edges,
                &subject,
                predicate,
                skolemize(namespace, object.as_str()),
                graph_scope,
            ),
            Term::Triple(_) => {}
        }
    }

    // Attach the lifted reifier metadata to the matching contributions:
    // resource-object statements to edges (by subject + predicate + object +
    // scope), literal-object statements to node properties (by predicate +
    // value + typing + scope). A `urn:mere:statement:<id>` reifier also hands
    // over the fact id, keeping the round trip id-stable; a foreign reifier
    // leaves the id to the kernel's minter.
    //
    // Both matches are indexed up front rather than linear-scanned per reifier,
    // which is what turns this pass from O(R x E) into O(R + E). Each key maps
    // to the FIRST position that carries it, preserving `find`'s semantics when
    // a document repeats a triple.
    type EdgeMatchKey = (String, String, String, GraphScope);
    type PropertyMatchKey = (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        GraphScope,
    );
    let mut edge_index: std::collections::HashMap<EdgeMatchKey, usize> =
        std::collections::HashMap::new();
    let mut property_index: std::collections::HashMap<PropertyMatchKey, usize> =
        std::collections::HashMap::new();
    if !reified.is_empty() {
        for (position, edge) in edges.iter().enumerate() {
            edge_index
                .entry((
                    edge.subject.clone(),
                    edge.predicate.clone(),
                    edge.object.clone(),
                    edge.graph_scope.clone(),
                ))
                .or_insert(position);
        }
        for (subject, node) in &nodes {
            for (position, property) in node.properties.iter().enumerate() {
                property_index
                    .entry((
                        subject.clone(),
                        property.predicate.clone(),
                        property.value.clone(),
                        property.datatype.clone(),
                        property.lang.clone(),
                        property.graph_scope.clone(),
                    ))
                    .or_insert(position);
            }
        }
    }
    for (reifier, statement) in reified {
        let statement_id = reifier
            .strip_prefix(STATEMENT_REIFIER_PREFIX)
            .map(str::to_string);
        match &statement.object {
            ReifiedObject::Resource(object) => {
                let match_key = (
                    statement.subject.clone(),
                    statement.predicate.clone(),
                    object.clone(),
                    statement.graph_scope.clone(),
                );
                if let Some(&position) = edge_index.get(&match_key) {
                    let edge = &mut edges[position];
                    edge.statement_id = statement_id;
                    edge.label = statement.label;
                    edge.provenance_iri = statement.provenance_iri;
                    edge.asserted_at_ms = statement.asserted_at_ms;
                } else {
                    // A reifier without its base triple still names a fact:
                    // materialize the edge (RDF 1.2 allows unasserted
                    // reification, but the Mere profile projects asserted
                    // facts, so ingest treats it as one).
                    nodes
                        .entry(statement.subject.clone())
                        .or_insert_with(|| NodeContribution::new(&statement.subject));
                    // The materialized edge joins the index, so a later reifier
                    // naming the same fact attaches to it instead of pushing a
                    // duplicate — what the old linear scan over `edges` did.
                    edge_index.insert(match_key, edges.len());
                    edges.push(EdgeContribution {
                        subject: statement.subject,
                        predicate: statement.predicate,
                        object: object.clone(),
                        graph_scope: statement.graph_scope,
                        statement_id,
                        label: statement.label,
                        provenance_iri: statement.provenance_iri,
                        asserted_at_ms: statement.asserted_at_ms,
                    });
                }
            }
            ReifiedObject::Literal {
                value,
                datatype,
                lang,
            } => {
                let wanted_datatype = if lang.is_some() {
                    None
                } else {
                    (datatype != XSD_STRING).then(|| normalize_schema_org(datatype))
                };
                let match_key = (
                    statement.subject.clone(),
                    statement.predicate.clone(),
                    value.clone(),
                    wanted_datatype,
                    lang.clone(),
                    statement.graph_scope.clone(),
                );
                let Some(&position) = property_index.get(&match_key) else {
                    continue;
                };
                let Some(node) = nodes.get_mut(&statement.subject) else {
                    continue;
                };
                let property = &mut node.properties[position];
                if let Some(id) = statement_id {
                    property.statement_id = id;
                }
                property.provenance_iri = statement.provenance_iri;
                property.asserted_at_ms = statement.asserted_at_ms;
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
            node.properties.sort_by(|a, b| {
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
            node.properties.dedup_by(|a, b| a.content_eq(b));
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
const DUBLIN_CORE_URLS: &[&str] = &[
    "http://purl.org/dc/terms/",
    "http://purl.org/dc/elements/1.1/",
];

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

mod apply;
pub use apply::ApplyOutcome;
#[cfg(not(target_arch = "wasm32"))]
pub use apply::apply_contribution;
#[cfg(test)]
mod tests;
