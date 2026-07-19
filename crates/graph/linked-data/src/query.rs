// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SPARQL query over the graph (the `query` feature).
//!
//! An ephemeral, read-only query path: project the focused graph into an
//! in-memory [`oxrdf::Dataset`] via [`crate::dataset_quads`] (the canonical
//! kernel-to-RDF projection), evaluate a SPARQL query over it with
//! [`spareval`], return the solution rows. The dataset is built per call and
//! dropped after, so there is no second persistence authority: the kernel
//! stays truth, this is a derived view for interop and exploration (the
//! two-natured one-way rule).
//!
//! `spareval` evaluates directly over any [`spareval::QueryableDataset`]; it
//! ships that impl for `&oxrdf::Dataset`, whose internal term interning and
//! (s,p,o,g) indexes are exactly the term-dictionary adapter the petgraph-RDF
//! plan's Phase 3 calls for. Because spareval pins the same `oxrdf` version
//! this crate builds quads with, the projection feeds the evaluator with zero
//! cross-model conversion — unlike the retired copy-into-Oxigraph-`Store`
//! path, which rebuilt every term across crate versions and paid store
//! insertion on top. That path survives only as the parity oracle in this
//! module's tests. No RocksDB anywhere, so this stays wasm-viable.

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

use kernel::graph::Graph;
use oxrdf::{Dataset, Term};
use spareval::{QueryEvaluator, QueryResults};
use spargebra::SparqlParser;

use crate::dataset_quads;

/// The rows of a SPARQL `SELECT` (or the boolean of an `ASK`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRows {
    /// The selected variable names, in result order.
    pub variables: Vec<String>,
    /// One entry per solution; each is the bound term per variable (display
    /// form), or `None` when the variable is unbound in that solution.
    pub rows: Vec<Vec<Option<String>>>,
}

/// Run `query` over `graph` and return the solution rows. The graph is
/// projected into a fresh in-memory dataset per call. Errors (parse,
/// evaluation) are returned as their display string. `CONSTRUCT` / `DESCRIBE`
/// are not supported in this cut.
pub fn sparql(graph: &Graph, query: &str) -> Result<QueryRows, String> {
    let dataset: Dataset = dataset_quads(graph).into_iter().collect();
    let query = SparqlParser::new()
        .parse_query(query)
        .map_err(|e| e.to_string())?;
    let evaluator = QueryEvaluator::new();
    let results = evaluator
        .prepare(&query)
        .execute(&dataset)
        .map_err(|e| e.to_string())?;

    match results {
        QueryResults::Solutions(solutions) => {
            let variables: Vec<String> = solutions
                .variables()
                .iter()
                .map(|v| v.as_str().to_string())
                .collect();
            let mut rows = Vec::new();
            for solution in solutions {
                let solution = solution.map_err(|e| e.to_string())?;
                let row = variables
                    .iter()
                    .map(|var| solution.get(var.as_str()).map(term_to_string))
                    .collect();
                rows.push(row);
            }
            Ok(QueryRows { variables, rows })
        }
        QueryResults::Boolean(value) => Ok(QueryRows {
            variables: vec!["result".to_string()],
            rows: vec![vec![Some(value.to_string())]],
        }),
        QueryResults::Graph(_) => {
            Err("CONSTRUCT / DESCRIBE results are not supported in this cut".to_string())
        }
    }
}

/// A bound term's display form for a result cell: the bare IRI / lexical value
/// (no angle brackets or quotes), `_:id` for a blank node.
fn term_to_string(term: &Term) -> String {
    match term {
        Term::NamedNode(n) => n.as_str().to_string(),
        Term::Literal(l) => l.value().to_string(),
        Term::BlankNode(b) => format!("_:{}", b.as_str()),
        Term::Triple(triple) => triple.to_string(),
    }
}

/// The retired copy-into-Oxigraph-`Store` query path, kept as the parity
/// oracle: an independently implemented SPARQL engine over the same
/// projection, diff-tested against the spareval mainline above. Oxigraph
/// carries its own RDF model (a newer `oxrdf`), so [`baseline::to_ox_quad`]
/// rebuilds each term across the two crate versions.
#[cfg(test)]
mod baseline {
    use super::{QueryRows, XSD_STRING, term_to_string};
    use kernel::graph::Graph;
    use oxigraph::model::{
        BlankNode as OxBlankNode, GraphName as OxGraphName, Literal as OxLiteral,
        NamedNode as OxNamedNode, NamedOrBlankNode as OxNamedOrBlankNode, Quad as OxQuad,
        Term as OxTerm, Triple as OxTriple,
    };
    use oxigraph::sparql::{QueryResults as OxQueryResults, SparqlEvaluator};
    use oxigraph::store::Store;

    use crate::dataset_quads;

    pub(super) fn sparql_store(graph: &Graph, query: &str) -> Result<QueryRows, String> {
        let store = Store::new().map_err(|e| e.to_string())?;
        for quad in dataset_quads(graph) {
            if let Some(oxquad) = to_ox_quad(&quad) {
                store.insert(&oxquad).map_err(|e| e.to_string())?;
            }
        }

        let results = SparqlEvaluator::new()
            .parse_query(query)
            .map_err(|e| e.to_string())?
            .on_store(&store)
            .execute()
            .map_err(|e| e.to_string())?;

        match results {
            OxQueryResults::Solutions(solutions) => {
                let variables: Vec<String> = solutions
                    .variables()
                    .iter()
                    .map(|v| v.as_str().to_string())
                    .collect();
                let mut rows = Vec::new();
                for solution in solutions {
                    let solution = solution.map_err(|e| e.to_string())?;
                    let row = variables
                        .iter()
                        .map(|var| solution.get(var.as_str()).map(ox_term_to_string))
                        .collect();
                    rows.push(row);
                }
                Ok(QueryRows { variables, rows })
            }
            OxQueryResults::Boolean(value) => Ok(QueryRows {
                variables: vec!["result".to_string()],
                rows: vec![vec![Some(value.to_string())]],
            }),
            OxQueryResults::Graph(_) => {
                Err("CONSTRUCT / DESCRIBE results are not supported in this cut".to_string())
            }
        }
    }

    fn to_ox_subject(subject: &oxrdf::NamedOrBlankNode) -> Option<OxNamedOrBlankNode> {
        Some(match subject {
            oxrdf::NamedOrBlankNode::NamedNode(n) => OxNamedNode::new(n.as_str()).ok()?.into(),
            oxrdf::NamedOrBlankNode::BlankNode(b) => OxBlankNode::new(b.as_str()).ok()?.into(),
        })
    }

    fn to_ox_term(term: &oxrdf::Term) -> Option<OxTerm> {
        Some(match term {
            oxrdf::Term::NamedNode(n) => OxNamedNode::new(n.as_str()).ok()?.into(),
            oxrdf::Term::BlankNode(b) => OxBlankNode::new(b.as_str()).ok()?.into(),
            oxrdf::Term::Literal(l) => {
                if let Some(language) = l.language() {
                    OxLiteral::new_language_tagged_literal(l.value(), language)
                        .ok()?
                        .into()
                } else if l.datatype().as_str() == XSD_STRING {
                    OxLiteral::new_simple_literal(l.value()).into()
                } else {
                    OxLiteral::new_typed_literal(
                        l.value(),
                        OxNamedNode::new(l.datatype().as_str()).ok()?,
                    )
                    .into()
                }
            }
            oxrdf::Term::Triple(triple) => OxTriple::new(
                to_ox_subject(&triple.subject)?,
                OxNamedNode::new(triple.predicate.as_str()).ok()?,
                to_ox_term(&triple.object)?,
            )
            .into(),
        })
    }

    fn to_ox_quad(quad: &oxrdf::Quad) -> Option<OxQuad> {
        let subject = to_ox_subject(&quad.subject)?;
        let predicate = OxNamedNode::new(quad.predicate.as_str()).ok()?;
        let object = to_ox_term(&quad.object)?;
        Some(OxQuad::new(
            subject,
            predicate,
            object,
            match &quad.graph_name {
                oxrdf::GraphName::DefaultGraph => OxGraphName::DefaultGraph,
                oxrdf::GraphName::NamedNode(node) => OxNamedNode::new(node.as_str()).ok()?.into(),
                oxrdf::GraphName::BlankNode(node) => OxBlankNode::new(node.as_str()).ok()?.into(),
            },
        ))
    }

    fn ox_term_to_string(term: &OxTerm) -> String {
        match term {
            OxTerm::NamedNode(n) => n.as_str().to_string(),
            OxTerm::Literal(l) => l.value().to_string(),
            OxTerm::BlankNode(b) => format!("_:{}", b.as_str()),
            OxTerm::Triple(triple) => triple.to_string(),
        }
    }

    // Both engines share this crate's display mapping; anchor the assumption.
    const _: fn(&super::Term) -> String = term_to_string;
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::apply::assert_semantic_relation_in_scope;
    use kernel::graph::fixtures::GraphFixtures;
    use kernel::graph::{SemanticStatementSpec, SemanticSubKind};
    use kernel::types::{GraphScope, NodeProperty};

    /// A graph exercising the projection's full surface: curated literals,
    /// recognized and raw statements, named scopes, statement metadata
    /// (reifiers), typed and language-tagged property literals, rdf:type.
    fn rich_graph() -> Graph {
        let mut graph = Graph::new();
        let a = graph.add_node("https://a.test/".to_string(), Default::default());
        let b = graph.add_node("https://b.test/".to_string(), Default::default());
        let c = graph.add_node("https://c.test/".to_string(), Default::default());

        {
            let node = graph.get_node_mut(a).expect("node a");
            node.title = "Article A".to_string();
            node.tags.insert("research".to_string());
        }

        assert_semantic_relation_in_scope(
            &mut graph,
            a,
            b,
            SemanticSubKind::Cites,
            Some("cites".to_string()),
            GraphScope::Source,
        );
        graph.assert_semantic_statement(
            a,
            c,
            SemanticStatementSpec {
                predicate: "https://mere.computer/ns/rel#cites".to_string(),
                recognized_sub_kind: Some(SemanticSubKind::Cites),
                label: Some("also cites".to_string()),
                graph_scope: GraphScope::User,
                provenance_iri: Some("https://people.test/alice".to_string()),
                asserted_at_ms: Some(1_720_000_000_123),
            },
        );
        graph.assert_semantic_statement(
            b,
            c,
            SemanticStatementSpec {
                predicate: "https://example.test/vocab#refutes".to_string(),
                ..Default::default()
            },
        );

        let node = graph.get_node_mut(a).expect("node a");
        let mut published = NodeProperty::new(
            "https://schema.org/datePublished".to_string(),
            "2026-07-04".to_string(),
        )
        .with_graph_scope(GraphScope::User)
        .with_metadata(
            Some("https://people.test/bob".to_string()),
            Some(1_720_000_100_456),
        );
        published.datatype = Some("http://www.w3.org/2001/XMLSchema#date".to_string());
        node.properties.push(published);
        let mut summary = NodeProperty::new(
            "https://schema.org/abstract".to_string(),
            "Un article".to_string(),
        );
        summary.lang = Some("fr".to_string());
        node.properties.push(summary);

        graph
    }

    /// Row order is engine-dependent when the query has no ORDER BY, so
    /// parity compares row multisets.
    fn sorted(mut rows: QueryRows) -> QueryRows {
        rows.rows.sort();
        rows
    }

    const PARITY_QUERIES: &[&str] = &[
        "SELECT ?s ?p ?o WHERE { ?s ?p ?o }",
        "SELECT ?g ?s ?p ?o WHERE { GRAPH ?g { ?s ?p ?o } }",
        "SELECT ?name WHERE { <https://a.test/> <https://schema.org/name> ?name }",
        "SELECT ?t WHERE { GRAPH <https://mere.computer/ns/graph#user> { <https://a.test/> <https://mere.computer/ns/rel#cites> ?t } }",
        "SELECT ?stmt ?prov WHERE { GRAPH ?g { ?stmt <http://www.w3.org/ns/prov#wasAttributedTo> ?prov } }",
        "SELECT ?v WHERE { <https://a.test/> <https://schema.org/abstract> ?v FILTER(lang(?v) = 'fr') }",
        "SELECT (COUNT(?s) AS ?n) WHERE { ?s a <https://mere.computer/ns/core#Node> }",
        "ASK { <https://b.test/> <https://example.test/vocab#refutes> <https://c.test/> }",
        "ASK { <https://b.test/> <https://example.test/vocab#refutes> <https://a.test/> }",
    ];

    /// Phase 3 gate (a): the spareval mainline returns the same solutions as
    /// the retired Oxigraph-Store baseline on every representative query.
    #[test]
    fn spareval_rows_match_store_baseline() {
        let graph = rich_graph();
        for query in PARITY_QUERIES {
            let mainline = sorted(sparql(&graph, query).expect(query));
            let oracle = sorted(baseline::sparql_store(&graph, query).expect(query));
            assert_eq!(mainline, oracle, "row parity for: {query}");
            assert!(
                !mainline.rows.is_empty(),
                "parity query must exercise rows: {query}"
            );
        }
    }

    /// Phase 3 gate (b): the direct evaluation path must not regress against
    /// the store-copy path it replaces. Debug-build wall clock over the
    /// parity battery; generous 3x headroom keeps this a regression tripwire,
    /// not a benchmark.
    #[test]
    fn spareval_is_not_slower_than_store_copy() {
        let graph = rich_graph();
        let battery = |run: &dyn Fn(&str) -> QueryRows| {
            let start = std::time::Instant::now();
            for _ in 0..20 {
                for query in PARITY_QUERIES {
                    run(query);
                }
            }
            start.elapsed()
        };
        let mainline = battery(&|q| sparql(&graph, q).expect(q));
        let oracle = battery(&|q| baseline::sparql_store(&graph, q).expect(q));
        println!("spareval {mainline:?} vs store-copy {oracle:?} over the parity battery x20");
        assert!(
            mainline < oracle * 3,
            "spareval path ({mainline:?}) should not be slower than 3x the store-copy path ({oracle:?})"
        );
    }
}
