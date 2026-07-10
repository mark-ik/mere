/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Statements-over-schema ingest (linked-data plan Phase 0) — the apply half.
//!
//! Inker's pure `link_statements` walk extracts predicate-bearing inline
//! links from a knot document; this module resolves each `rel` against Mere's
//! relation vocabulary and asserts `Semantic` edges. Relocated here from
//! inker (2026-07-10, inker-adoption plan): the walk is portable document
//! machinery, the apply is graph-kernel mutation, and this crate is the
//! statements' Phase-0 home beside the JSON-LD ingest it pairs with.
//!
//! Scope is deliberately narrow, matching the linked-data plan's deferrals:
//!
//! - **No node creation.** The host seeds nodes (e.g. by following the link)
//!   and this module only *upgrades* existing edges. A statement whose target
//!   is absent is returned in [`StatementOutcome::pending_targets`].
//! - **Recognized relations only.** A predicate outside Mere's vocabulary (a
//!   raw IRI / CURIE like `schema:citation`) needs a raw-predicate `Semantic`
//!   edge, which the kernel does not yet hold standalone — that pairs with
//!   JSON-LD ingest (linked-data plan Phase 2). Such statements are returned
//!   in [`StatementOutcome::unrecognized`], never dropped
//!   (statements-over-schema: never lose a predicate).

use inker::LinkStatement;
use kernel::graph::{
    EdgeAssertion, Graph, NodeKey, REL_VOCAB, SemanticSubKind, predicate_iri, sub_kind_from_iri,
};

/// Resolve a knot `rel` to a recognized Mere relation: its [`SemanticSubKind`]
/// and canonical predicate IRI ([`predicate_iri`]). Accepts both a bare slug
/// (`cites`, `depends-on`) and a full Mere vocabulary IRI. Returns `None` for any
/// predicate outside Mere's vocabulary (a raw / CURIE IRI), which is deferred to
/// JSON-LD ingest (linked-data plan Phase 2).
pub fn resolve_rel(rel: &str) -> Option<(SemanticSubKind, &'static str)> {
    if let Some(sub_kind) = sub_kind_from_iri(rel) {
        return Some((sub_kind, predicate_iri(sub_kind)));
    }
    // A bare slug: normalize to the Mere vocabulary IRI, then recognize.
    let iri = format!("{REL_VOCAB}{rel}");
    sub_kind_from_iri(&iri).map(|sub_kind| (sub_kind, predicate_iri(sub_kind)))
}

/// What [`apply_link_statements`] did. Everything not edged is reported, never
/// silently dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatementOutcome {
    /// Number of `Semantic` edges asserted (each with its predicate stamped).
    pub edges_asserted: usize,
    /// Target URLs not yet present in the graph — the host follows the link
    /// first (creating the node), then re-applies.
    pub pending_targets: Vec<String>,
    /// Statements whose `rel` is outside Mere's vocabulary — a raw predicate
    /// awaiting the raw-IRI `Semantic` edge path (linked-data plan Phase 2).
    pub unrecognized: Vec<LinkStatement>,
}

/// Apply predicate-bearing link statements to `graph` as `Semantic` edges from
/// `source`. For each statement whose `rel` resolves to a recognized relation
/// and whose target node already exists, asserts
/// `EdgeAssertion::Semantic { sub_kind, .. }` and stamps the canonical predicate
/// IRI via `EdgePayload::set_semantic_predicate`. Compose with inker's walk:
/// `apply_link_statements(graph, source, &inker::link_statements(&doc))`. See
/// [`StatementOutcome`] and the module docs for the (intentional) deferrals.
pub fn apply_link_statements(
    graph: &mut Graph,
    source: NodeKey,
    statements: &[LinkStatement],
) -> StatementOutcome {
    let mut outcome = StatementOutcome::default();
    for stmt in statements {
        let Some((sub_kind, predicate)) = resolve_rel(&stmt.rel) else {
            outcome.unrecognized.push(stmt.clone());
            continue;
        };
        // Edge only to a target already in the graph; the immutable lookup ends
        // before the mutation below (`NodeKey` is `Copy`).
        let Some(target) = graph.get_node_by_url(&stmt.target_url).map(|(key, _)| key) else {
            outcome.pending_targets.push(stmt.target_url.clone());
            continue;
        };
        let edge = kernel::graph::apply::assert_relation(
            graph,
            source,
            target,
            EdgeAssertion::Semantic {
                sub_kind,
                label: None,
                decay_progress: None,
            },
        );
        if let Some(key) = edge {
            let _ = kernel::graph::apply::apply_graph_delta(
                graph,
                kernel::graph::apply::GraphDelta::SetEdgeSemanticPredicate {
                    edge: key,
                    predicate: Some(predicate.to_string()),
                },
            );
            outcome.edges_asserted += 1;
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::RelationSelector;
    // Extension trait supplying the test-only `Graph::add_node` (the fixtures
    // feature); production node creation is the host's job.
    use kernel::graph::fixtures::GraphFixtures;

    fn statement(url: &str, rel: &str) -> LinkStatement {
        LinkStatement {
            target_url: url.to_string(),
            rel: rel.to_string(),
        }
    }

    #[test]
    fn resolve_rel_recognizes_bare_slug_and_full_iri() {
        let (sk, iri) = resolve_rel("cites").expect("bare slug recognized");
        assert_eq!(sk, SemanticSubKind::Cites);
        assert_eq!(iri, "https://mere.computer/ns/rel#cites");
        // The full canonical IRI resolves to the same relation.
        assert_eq!(
            resolve_rel("https://mere.computer/ns/rel#cites"),
            Some((SemanticSubKind::Cites, "https://mere.computer/ns/rel#cites"))
        );
        // A multi-word slug (hyphenated) also resolves.
        assert_eq!(
            resolve_rel("depends-on").map(|(sk, _)| sk),
            Some(SemanticSubKind::DependsOn)
        );
        // A raw / CURIE predicate is not in Mere's vocabulary.
        assert_eq!(resolve_rel("schema:citation"), None);
    }

    #[test]
    fn apply_edges_recognized_statement_to_existing_target() {
        let mut graph = Graph::new();
        let source = graph.add_node("knot:test".to_string(), Default::default());
        let target = graph.add_node("mere://node/topic".to_string(), Default::default());

        let outcome = apply_link_statements(
            &mut graph,
            source,
            &[statement("mere://node/topic", "cites")],
        );

        assert_eq!(outcome.edges_asserted, 1);
        assert!(outcome.pending_targets.is_empty());
        assert!(outcome.unrecognized.is_empty());

        let key = graph.find_edge_key(source, target).expect("edge created");
        let payload = graph.get_edge(key).expect("edge payload");
        assert!(payload.has_relation(RelationSelector::Semantic(SemanticSubKind::Cites)));
        assert_eq!(
            payload.semantic_data().and_then(|d| d.predicate.as_deref()),
            Some("https://mere.computer/ns/rel#cites")
        );
    }

    #[test]
    fn apply_reports_pending_when_target_absent() {
        let mut graph = Graph::new();
        let source = graph.add_node("knot:test".to_string(), Default::default());

        let outcome = apply_link_statements(
            &mut graph,
            source,
            &[statement("mere://node/absent", "cites")],
        );

        assert_eq!(outcome.edges_asserted, 0);
        assert_eq!(
            outcome.pending_targets,
            vec!["mere://node/absent".to_string()]
        );
    }

    #[test]
    fn apply_reports_unrecognized_raw_predicate() {
        let mut graph = Graph::new();
        let source = graph.add_node("knot:test".to_string(), Default::default());
        let _target = graph.add_node("mere://node/topic".to_string(), Default::default());

        let outcome = apply_link_statements(
            &mut graph,
            source,
            &[statement("mere://node/topic", "schema:citation")],
        );

        assert_eq!(outcome.edges_asserted, 0);
        assert_eq!(
            outcome.unrecognized,
            vec![LinkStatement {
                target_url: "mere://node/topic".to_string(),
                rel: "schema:citation".to_string(),
            }]
        );
    }
}
