/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Projecting a chartulary graph into RDF.

use std::collections::BTreeMap;
use std::fmt::Display;

use chartulary::{Addressed, Graph, Identified, Labeled, NodeKey, Predicated};
use serde_json::{json, Map, Value};

/// `schema:name` — the curated mapping target for a node's title.
pub const SCHEMA_NAME: &str = "https://schema.org/name";
/// `schema:keywords` — the curated mapping target for a node's tags.
pub const SCHEMA_KEYWORDS: &str = "https://schema.org/keywords";

/// An RDF term: an IRI or a literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    /// A named node (an IRI).
    Iri(String),
    /// A literal value, optionally typed or language-tagged.
    Literal {
        /// The lexical value.
        value: String,
        /// An `xsd:`-style datatype IRI, if typed.
        datatype: Option<String>,
        /// A BCP-47 language tag, if language-tagged.
        lang: Option<String>,
    },
}

impl Term {
    fn plain(value: &str) -> Self {
        Term::Literal {
            value: value.to_string(),
            datatype: None,
            lang: None,
        }
    }
}

/// An RDF triple in the default graph: subject IRI, predicate IRI, object term.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quad {
    /// The subject IRI.
    pub subject: String,
    /// The predicate IRI.
    pub predicate: String,
    /// The object term.
    pub object: Term,
}

/// The IRI a node projects to: its primary address, or a skolem `urn:chart:` IRI
/// when it has none.
fn subject_iri<N>(node: &N) -> String
where
    N: Identified + Addressed,
    N::Id: Display,
{
    match node.primary_address() {
        Some(address) => address.as_str().to_string(),
        None => format!("urn:chart:{}", node.id()),
    }
}

/// The `(predicate, object)` statements a single node contributes: its curated
/// literals (name, keywords) and its semantic-ring edges. App-private edges (whose
/// [`Predicated::predicate`] is `None`) contribute nothing, so only the shared
/// semantic ring reaches RDF.
fn statements<N, E>(graph: &Graph<N, E>, key: NodeKey) -> (String, Vec<(String, Term)>)
where
    N: Identified + Addressed + Labeled,
    N::Id: Display,
    E: Predicated,
{
    let node = graph.node(key).expect("valid key");
    let subject = subject_iri(node);
    let mut out = Vec::new();

    if let Some(title) = node.title() {
        out.push((SCHEMA_NAME.to_string(), Term::plain(title)));
    }
    for tag in node.tags() {
        out.push((SCHEMA_KEYWORDS.to_string(), Term::plain(tag)));
    }
    for (_, target, edge) in graph.out_edges(key) {
        if let Some(predicate) = edge.predicate() {
            let object = subject_iri(graph.node(target).expect("valid target"));
            out.push((predicate.to_string(), Term::Iri(object)));
        }
    }

    (subject, out)
}

/// Project the whole graph to RDF quads (default graph). Every node's curated
/// literals and semantic-ring edges become triples; app-private relations do not.
pub fn to_quads<N, E>(graph: &Graph<N, E>) -> Vec<Quad>
where
    N: Identified + Addressed + Labeled,
    N::Id: Display,
    E: Predicated,
{
    let keys: Vec<NodeKey> = graph.nodes().map(|(key, _)| key).collect();
    let mut quads = Vec::new();
    for key in keys {
        let (subject, node_statements) = statements(graph, key);
        for (predicate, object) in node_statements {
            quads.push(Quad {
                subject: subject.clone(),
                predicate,
                object,
            });
        }
    }
    quads
}

/// Project the whole graph to **expanded JSON-LD**: an array of node objects, each
/// with an `@id` and its predicates. Every node appears (with at least its `@id`),
/// even one with no statements. Deterministic (predicates are key-sorted).
pub fn to_jsonld<N, E>(graph: &Graph<N, E>) -> Value
where
    N: Identified + Addressed + Labeled,
    N::Id: Display,
    E: Predicated,
{
    let keys: Vec<NodeKey> = graph.nodes().map(|(key, _)| key).collect();
    let mut array = Vec::new();
    for key in keys {
        let (subject, node_statements) = statements(graph, key);
        let mut object = Map::new();
        object.insert("@id".to_string(), Value::String(subject));

        let mut by_predicate: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (predicate, term) in node_statements {
            let entry = match term {
                Term::Iri(iri) => json!({ "@id": iri }),
                Term::Literal {
                    value,
                    datatype,
                    lang,
                } => {
                    let mut literal = Map::new();
                    literal.insert("@value".to_string(), Value::String(value));
                    if let Some(lang) = lang {
                        literal.insert("@language".to_string(), Value::String(lang));
                    } else if let Some(datatype) = datatype {
                        literal.insert("@type".to_string(), Value::String(datatype));
                    }
                    Value::Object(literal)
                }
            };
            by_predicate.entry(predicate).or_default().push(entry);
        }
        for (predicate, values) in by_predicate {
            object.insert(predicate, Value::Array(values));
        }
        array.push(Value::Object(object));
    }
    Value::Array(array)
}

/// Project the whole graph to an **N-Quads** document (one triple per line, default
/// graph).
pub fn to_nquads<N, E>(graph: &Graph<N, E>) -> String
where
    N: Identified + Addressed + Labeled,
    N::Id: Display,
    E: Predicated,
{
    let mut lines: Vec<String> = to_quads(graph)
        .iter()
        .map(|quad| {
            format!(
                "{} {} {} .",
                angle(&quad.subject),
                angle(&quad.predicate),
                object_nq(&quad.object)
            )
        })
        .collect();
    lines.sort();
    lines.join("\n")
}

fn angle(iri: &str) -> String {
    format!("<{iri}>")
}

fn object_nq(term: &Term) -> String {
    match term {
        Term::Iri(iri) => angle(iri),
        Term::Literal {
            value,
            datatype,
            lang,
        } => {
            let escaped = value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n");
            let mut out = format!("\"{escaped}\"");
            if let Some(lang) = lang {
                out.push_str(&format!("@{lang}"));
            } else if let Some(datatype) = datatype {
                out.push_str(&format!("^^{}", angle(datatype)));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chartulary::{Container, Graph, Recognized, Relation, RelationClass};

    fn seed() -> Graph<Container, Relation> {
        let mut graph = Graph::new();
        let a = graph.insert(
            Container::new("a")
                .with_address("https://a.example/")
                .with_title("Paper A")
                .with_tag("research"),
        );
        let b = graph.insert(Container::new("b").with_address("https://b.example/"));
        // c has no address: it must skolemize.
        let c = graph.insert(Container::new("c"));
        // A semantic-ring edge (projects) and an app-private edge (does not).
        graph.connect(a, b, Relation::new(RelationClass::recognized(Recognized::Cites)));
        graph.connect(a, c, Relation::new(RelationClass::app("woodshed", 0)));
        graph
    }

    #[test]
    fn semantic_edges_project_and_literals_come_from_labels() {
        let quads = to_quads(&seed());
        assert!(quads.contains(&Quad {
            subject: "https://a.example/".into(),
            predicate: "urn:chart:rel:cites".into(),
            object: Term::Iri("https://b.example/".into()),
        }));
        assert!(quads.contains(&Quad {
            subject: "https://a.example/".into(),
            predicate: SCHEMA_NAME.into(),
            object: Term::plain("Paper A"),
        }));
        assert!(quads.contains(&Quad {
            subject: "https://a.example/".into(),
            predicate: SCHEMA_KEYWORDS.into(),
            object: Term::plain("research"),
        }));
    }

    #[test]
    fn app_private_edges_do_not_project() {
        let quads = to_quads(&seed());
        // The a -> c edge is an app family; no triple points at c's skolem IRI.
        assert!(
            !quads.iter().any(|q| matches!(&q.object, Term::Iri(iri) if iri == "urn:chart:c")),
            "app-family edges stay out of RDF"
        );
    }

    #[test]
    fn a_node_without_an_address_skolemizes() {
        let json = to_jsonld(&seed());
        let ids: Vec<&str> = json
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|obj| obj["@id"].as_str())
            .collect();
        assert!(ids.contains(&"urn:chart:c"), "c has no address, so a urn: id");
        assert!(ids.contains(&"https://a.example/"));
    }

    #[test]
    fn jsonld_shapes_the_expanded_form() {
        let json = to_jsonld(&seed());
        let a = json
            .as_array()
            .unwrap()
            .iter()
            .find(|obj| obj["@id"] == json!("https://a.example/"))
            .expect("node a");
        assert_eq!(a["https://schema.org/name"], json!([{ "@value": "Paper A" }]));
        assert_eq!(
            a["urn:chart:rel:cites"],
            json!([{ "@id": "https://b.example/" }])
        );
    }

    #[test]
    fn nquads_emits_the_cites_triple() {
        let doc = to_nquads(&seed());
        assert!(doc.contains("<https://a.example/> <urn:chart:rel:cites> <https://b.example/> ."));
        assert!(
            doc.contains("<https://a.example/> <https://schema.org/name> \"Paper A\" ."),
            "got: {doc}"
        );
    }
}
