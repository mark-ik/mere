// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Turtle-family file I/O for the RDF projection (petgraph-RDF plan, Phase 2's
//! "Turtle / N-Quads I/O via `oxttl`").
//!
//! [`dataset_quads`](crate::dataset_quads) is an in-memory projection; this
//! module makes it a file a standard tool can read, and reads one back. Both
//! serializations carry named graphs (the scope graphs plus the vocabulary
//! graph), so **N-Quads** and **TriG** are the faithful full-dataset formats;
//! plain Turtle/N-Triples would flatten the scopes and is not offered. N-Quads
//! is the canonical line-based interchange the plan names; TriG is its
//! Turtle-family, graph-grouped, human-readable sibling.
//!
//! Every serialization is self-describing: it appends
//! [`vocabulary_alignment_quads`](crate::vocabulary_alignment_quads) so a
//! consumer can bridge Mere's `rel#` IRIs onto standard vocabularies from the
//! file alone. On the way back in, that alignment graph is dropped (it is
//! re-derivable schema, not instance content), so `graph -> file -> contribution
//! -> graph -> file` is byte-stable under the same normalized-dataset compare as
//! the in-memory round-trip gate. RDF 1.2 triple terms (the reifier metadata)
//! ride through both formats via oxttl's `rdf-12` feature.

use kernel::graph::Graph;
use oxttl::{NQuadsParser, NQuadsSerializer, TriGParser, TriGSerializer};

use crate::ingest::{GraphContribution, IngestError, from_quads};
use crate::vocab::is_vocabulary_quad;
use crate::{dataset_quads, vocabulary_alignment_quads};

/// The whole graph as **N-Quads**: instance quads (scoped) plus the vocabulary
/// alignment graph. Deterministic (the projection is sorted), so it is safe to
/// pin in a golden test.
pub fn to_nquads(graph: &Graph) -> String {
    let mut serializer = NQuadsSerializer::new().for_writer(Vec::<u8>::new());
    for quad in dataset_quads(graph)
        .iter()
        .chain(vocabulary_alignment_quads().iter())
    {
        serializer
            .serialize_quad(quad)
            .expect("writing N-Quads to an in-memory Vec is infallible");
    }
    String::from_utf8(serializer.finish()).expect("oxttl emits valid UTF-8")
}

/// The whole graph as **TriG** (Turtle with named graphs): the same content as
/// [`to_nquads`], grouped by graph and abbreviated. `Err` only on the rare
/// serializer I/O fault (the writer is an in-memory `Vec`, so effectively never).
pub fn to_trig(graph: &Graph) -> Result<String, String> {
    let mut serializer = TriGSerializer::new().for_writer(Vec::<u8>::new());
    for quad in dataset_quads(graph)
        .iter()
        .chain(vocabulary_alignment_quads().iter())
    {
        serializer.serialize_quad(quad).map_err(|e| e.to_string())?;
    }
    let bytes = serializer.finish().map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Parse **N-Quads** into a graph contribution, dropping the re-derivable
/// vocabulary-alignment graph. `namespace` scopes any skolemized blank nodes, as
/// in [`from_quads`].
pub fn from_nquads(text: &str, namespace: &str) -> Result<GraphContribution, IngestError> {
    let mut quads = Vec::new();
    for quad in NQuadsParser::new().for_slice(text.as_bytes()) {
        let quad = quad.map_err(|e| IngestError::Parse(e.to_string()))?;
        if !is_vocabulary_quad(&quad) {
            quads.push(quad);
        }
    }
    from_quads(quads, namespace)
}

/// Parse **TriG** into a graph contribution, dropping the re-derivable
/// vocabulary-alignment graph.
pub fn from_trig(text: &str, namespace: &str) -> Result<GraphContribution, IngestError> {
    let mut quads = Vec::new();
    for quad in TriGParser::new().for_slice(text.as_bytes()) {
        let quad = quad.map_err(|e| IngestError::Parse(e.to_string()))?;
        if !is_vocabulary_quad(&quad) {
            quads.push(quad);
        }
    }
    from_quads(quads, namespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::apply_contribution;
    use kernel::graph::apply::assert_semantic_relation_in_scope;
    use kernel::graph::fixtures::GraphFixtures;
    use kernel::graph::{SemanticStatementSpec, SemanticSubKind};
    use kernel::types::{GraphScope, NodeProperty};

    /// A graph exercising the projection surface that file I/O must preserve:
    /// curated literals, a recognized statement with reifier metadata (an RDF 1.2
    /// triple term), a raw predicate, a scoped + typed property, rdf:type.
    fn rich_graph() -> Graph {
        let mut graph = Graph::new();
        let a = graph.add_node("https://a.test/".to_string(), Default::default());
        let b = graph.add_node("https://b.test/".to_string(), Default::default());
        let c = graph.add_node("https://c.test/".to_string(), Default::default());

        graph.get_node_mut(a).expect("a").title = "Article A".to_string();
        graph.get_node_mut(a).expect("a").tags =
            std::collections::HashSet::from(["research".to_string()]);

        assert_semantic_relation_in_scope(
            &mut graph,
            a,
            b,
            SemanticSubKind::Cites,
            Some("cited in the intro".to_string()),
            GraphScope::Source,
        );
        // Attach reifier metadata so a triple term must survive file I/O.
        let edge = graph.find_edge_key(a, b).expect("edge");
        let statement = graph
            .get_edge_mut(edge)
            .and_then(|p| p.semantic.as_mut())
            .and_then(|s| s.statements.iter_mut().next())
            .expect("statement");
        statement.provenance_iri = Some("https://persona.test/mark".to_string());
        statement.asserted_at_ms = Some(1_720_000_000_000);

        graph.assert_semantic_statement(
            a,
            c,
            SemanticStatementSpec {
                predicate: "https://example.test/vocab#inspiredBy".to_string(),
                ..Default::default()
            },
        );

        let node = graph.get_node_mut(a).expect("a");
        let mut published = NodeProperty::new(
            "https://schema.org/datePublished".to_string(),
            "2026-07-04".to_string(),
        )
        .with_graph_scope(GraphScope::User);
        published.datatype = Some("http://www.w3.org/2001/XMLSchema#date".to_string());
        node.properties.push(published);

        graph
    }

    fn sorted_dataset(graph: &Graph) -> Vec<String> {
        let mut lines: Vec<String> = crate::dataset_quads(graph)
            .iter()
            .map(|quad| format!("{quad} ."))
            .collect();
        lines.sort();
        lines
    }

    #[test]
    fn nquads_round_trip_is_lossless_and_drops_vocabulary() {
        let graph = rich_graph();
        let text = to_nquads(&graph);

        // Self-describing: the file carries the vocabulary alignment.
        assert!(
            text.contains("http://purl.org/spar/cito/cites"),
            "N-Quads file publishes the vocabulary alignment"
        );

        let contribution = from_nquads(&text, "gate").expect("parse N-Quads");
        let mut reimported = Graph::new();
        let outcome = apply_contribution(&mut reimported, &contribution);
        assert_eq!(outcome.edges_skipped, 0, "self-contained contribution");

        assert_eq!(
            sorted_dataset(&graph),
            sorted_dataset(&reimported),
            "graph -> N-Quads -> contribution -> graph is lossless under the profile"
        );
    }

    #[test]
    fn trig_round_trip_matches_nquads() {
        let graph = rich_graph();
        let text = to_trig(&graph).expect("serialize TriG");

        let contribution = from_trig(&text, "gate").expect("parse TriG");
        let mut reimported = Graph::new();
        apply_contribution(&mut reimported, &contribution);

        assert_eq!(
            sorted_dataset(&graph),
            sorted_dataset(&reimported),
            "TriG carries the same dataset as N-Quads"
        );
    }
}
