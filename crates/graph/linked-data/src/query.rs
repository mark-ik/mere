/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! SPARQL query over the graph (the `query` feature).
//!
//! An ephemeral, read-only query path: project the focused graph into an
//! in-memory Oxigraph [`Store`] via [`crate::node_quads`] (the canonical
//! kernel-to-RDF projection), run a SPARQL query, return the solution rows. The
//! store is built per call and dropped after, so there is no second persistence
//! authority: the kernel stays truth, this is a derived view for interop and
//! exploration (the two-natured one-way rule). `Store::new()` is in-memory and
//! pulls no RocksDB, so this stays wasm-viable.
//!
//! Oxigraph carries its own RDF model (a newer `oxrdf`) than the `oxrdf` this
//! crate builds quads with, so [`to_ox_quad`] converts across the two by value
//! (both are the same RDF model at different crate versions); no shared-type
//! version pin is needed.

use kernel::graph::Graph;
use oxigraph::model::{
    BlankNode as OxBlankNode, GraphName as OxGraphName, Literal as OxLiteral,
    NamedNode as OxNamedNode, NamedOrBlankNode as OxNamedOrBlankNode, Quad as OxQuad, Term as OxTerm,
};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use crate::node_quads;

/// The rows of a SPARQL `SELECT` (or the boolean of an `ASK`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRows {
    /// The selected variable names, in result order.
    pub variables: Vec<String>,
    /// One entry per solution; each is the bound term per variable (display
    /// form), or `None` when the variable is unbound in that solution.
    pub rows: Vec<Vec<Option<String>>>,
}

/// Run `query` over `graph` and return the solution rows. The graph is projected
/// into a fresh in-memory store per call. Errors (parse, evaluation, storage)
/// are returned as their display string. `CONSTRUCT` / `DESCRIBE` are not
/// supported in this cut.
pub fn sparql(graph: &Graph, query: &str) -> Result<QueryRows, String> {
    let store = Store::new().map_err(|e| e.to_string())?;
    for (key, node) in graph.nodes() {
        for quad in node_quads(graph, key, node) {
            if let Some(oxquad) = to_ox_quad(&quad) {
                store.insert(&oxquad).map_err(|e| e.to_string())?;
            }
        }
    }

    let results = SparqlEvaluator::new()
        .parse_query(query)
        .map_err(|e| e.to_string())?
        .on_store(&store)
        .execute()
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

/// Convert one of this crate's `oxrdf` quads into an Oxigraph-model quad. Both
/// sides are the same RDF model at different crate versions, so this is a
/// field-by-field rebuild. `node_quads` emits only simple literals, named nodes,
/// and the default graph; a term that cannot be rebuilt (a malformed IRI) is
/// dropped.
fn to_ox_quad(quad: &oxrdf::Quad) -> Option<OxQuad> {
    let subject: OxNamedOrBlankNode = match &quad.subject {
        oxrdf::NamedOrBlankNode::NamedNode(n) => OxNamedNode::new(n.as_str()).ok()?.into(),
        oxrdf::NamedOrBlankNode::BlankNode(b) => OxBlankNode::new(b.as_str()).ok()?.into(),
    };
    let predicate = OxNamedNode::new(quad.predicate.as_str()).ok()?;
    let object: OxTerm = match &quad.object {
        oxrdf::Term::NamedNode(n) => OxNamedNode::new(n.as_str()).ok()?.into(),
        oxrdf::Term::BlankNode(b) => OxBlankNode::new(b.as_str()).ok()?.into(),
        oxrdf::Term::Literal(l) => OxLiteral::new_simple_literal(l.value()).into(),
    };
    Some(OxQuad::new(
        subject,
        predicate,
        object,
        OxGraphName::DefaultGraph,
    ))
}

/// A bound term's display form for a result cell: the bare IRI / lexical value
/// (no angle brackets or quotes), `_:id` for a blank node.
fn term_to_string(term: &OxTerm) -> String {
    match term {
        OxTerm::NamedNode(n) => n.as_str().to_string(),
        OxTerm::Literal(l) => l.value().to_string(),
        OxTerm::BlankNode(b) => format!("_:{}", b.as_str()),
    }
}
