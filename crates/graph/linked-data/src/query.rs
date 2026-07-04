/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! SPARQL query over the graph (the `query` feature).
//!
//! An ephemeral, read-only query path: project the focused graph into an
//! in-memory Oxigraph [`Store`] via [`crate::dataset_quads`] (the canonical
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

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

use kernel::graph::Graph;
use oxigraph::model::{
    BlankNode as OxBlankNode, GraphName as OxGraphName, Literal as OxLiteral,
    NamedNode as OxNamedNode, NamedOrBlankNode as OxNamedOrBlankNode, Quad as OxQuad,
    Term as OxTerm, Triple as OxTriple,
};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

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

/// Run `query` over `graph` and return the solution rows. The graph is projected
/// into a fresh in-memory store per call. Errors (parse, evaluation, storage)
/// are returned as their display string. `CONSTRUCT` / `DESCRIBE` are not
/// supported in this cut.
pub fn sparql(graph: &Graph, query: &str) -> Result<QueryRows, String> {
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

/// Convert one of this crate's `oxrdf` quads into an Oxigraph-model quad. Both
/// sides are the same RDF model at different crate versions, so this is a
/// field-by-field rebuild. `dataset_quads` may carry named graphs, literals, and
/// RDF 1.2 triple terms; a term that cannot be rebuilt is dropped.
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

/// A bound term's display form for a result cell: the bare IRI / lexical value
/// (no angle brackets or quotes), `_:id` for a blank node.
fn term_to_string(term: &OxTerm) -> String {
    match term {
        OxTerm::NamedNode(n) => n.as_str().to_string(),
        OxTerm::Literal(l) => l.value().to_string(),
        OxTerm::BlankNode(b) => format!("_:{}", b.as_str()),
        OxTerm::Triple(triple) => triple.to_string(),
    }
}
