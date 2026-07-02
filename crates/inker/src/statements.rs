/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Statements-over-schema ingest for knot documents (knot design doc §10.5
//! Phase 4; linked-data plan Phase 0).
//!
//! A knot inline link can carry an open *predicate* in its `rel`
//! (`[Topic](mere://node/topic){rel=cites}`). This module turns those links into
//! kernel `Semantic` edges, in two layers:
//!
//! - [`link_statements`] is a **pure** walk of a document's blocks, collecting
//!   every predicate-bearing inline link as a [`LinkStatement`] — no graph, no
//!   kernel mutation. (Plain navigation links, with no `rel`, are not
//!   statements and are excluded.)
//! - [`apply_link_statements`] resolves each `rel` against Mere's relation
//!   vocabulary and, for a recognized relation whose target node already exists,
//!   asserts a `Semantic` edge and stamps the canonical predicate IRI through
//!   `EdgePayload::set_semantic_predicate`.
//!
//! Scope is deliberately narrow, matching the linked-data plan's deferrals:
//!
//! - **No node creation.** `Graph::add_node` is wasm-gated (it mints a UUID) and
//!   positioning is a layout concern, so the host seeds nodes (e.g. by following
//!   the link) and this module only *upgrades* existing edges. A statement whose
//!   target is absent is returned in [`StatementOutcome::pending_targets`].
//! - **Recognized relations only.** A predicate outside Mere's vocabulary (a raw
//!   IRI / CURIE like `schema:citation`) needs a raw-predicate `Semantic` edge,
//!   which the kernel does not yet hold standalone — that pairs with JSON-LD
//!   ingest (linked-data plan Phase 2). Such statements are returned in
//!   [`StatementOutcome::unrecognized`], never dropped (statements-over-schema:
//!   never lose a predicate).

use crate::{Block, EngineDocument, InlineSpan};
use kernel::graph::{
    EdgeAssertion, Graph, NodeKey, REL_VOCAB, SemanticSubKind, predicate_iri, sub_kind_from_iri,
};

/// A predicate-bearing inline link extracted from a knot document: the link's
/// target URL plus the verbatim `rel` value (a bare slug like `cites`, a full
/// Mere IRI, or a raw CURIE). The `rel` is resolved against the relation
/// vocabulary at apply time, not here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkStatement {
    pub target_url: String,
    pub rel: String,
}

/// Walk a document's blocks and collect every predicate-bearing inline link, in
/// document order. Pure: no graph, no kernel mutation.
pub fn link_statements(doc: &EngineDocument) -> Vec<LinkStatement> {
    let mut out = Vec::new();
    for block in &doc.blocks {
        collect_block(block, &mut out);
    }
    out
}

fn collect_block(block: &Block, out: &mut Vec<LinkStatement>) {
    match block {
        Block::Heading { spans, .. } | Block::Paragraph { spans } => {
            for span in spans {
                collect_span(span, out);
            }
        }
        Block::Quote { blocks } => {
            for inner in blocks {
                collect_block(inner, out);
            }
        }
        Block::List { items, .. } => {
            for item in items {
                for inner in item {
                    collect_block(inner, out);
                }
            }
        }
        Block::Table { header, rows, .. } => {
            for cell in header.iter().chain(rows.iter().flatten()) {
                for span in cell {
                    collect_span(span, out);
                }
            }
        }
        // Feed blocks carry navigation URLs (article / source), not `rel`
        // statements; the remaining variants hold no inline links.
        Block::FeedHeader { .. }
        | Block::FeedEntry { .. }
        | Block::CodeBlock { .. }
        | Block::Image { .. }
        | Block::Preformatted { .. }
        | Block::Rule
        | Block::MetadataRow { .. }
        | Block::Badge { .. } => {}
    }
}

fn collect_span(span: &InlineSpan, out: &mut Vec<LinkStatement>) {
    match span {
        InlineSpan::Link {
            url,
            predicate,
            spans,
            ..
        } => {
            if let Some(rel) = predicate {
                out.push(LinkStatement {
                    target_url: url.clone(),
                    rel: rel.clone(),
                });
            }
            for inner in spans {
                collect_span(inner, out);
            }
        }
        InlineSpan::Emphasis(inner) | InlineSpan::Strong(inner) => {
            for s in inner {
                collect_span(s, out);
            }
        }
        InlineSpan::Text(_)
        | InlineSpan::Code(_)
        | InlineSpan::LineBreak
        | InlineSpan::SoftBreak => {}
    }
}

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

/// Apply a knot document's predicate-bearing links to `graph` as `Semantic`
/// edges from `source`. For each statement whose `rel` resolves to a recognized
/// relation and whose target node already exists, asserts
/// `EdgeAssertion::Semantic { sub_kind, .. }` and stamps the canonical predicate
/// IRI via `EdgePayload::set_semantic_predicate`. See [`StatementOutcome`] and
/// the module docs for the (intentional) deferrals.
pub fn apply_link_statements(
    graph: &mut Graph,
    source: NodeKey,
    doc: &EngineDocument,
) -> StatementOutcome {
    let mut outcome = StatementOutcome::default();
    for stmt in link_statements(doc) {
        let Some((sub_kind, predicate)) = resolve_rel(&stmt.rel) else {
            outcome.unrecognized.push(stmt);
            continue;
        };
        // Edge only to a target already in the graph; the immutable lookup ends
        // before the mutation below (`NodeKey` is `Copy`).
        let Some(target) = graph.get_node_by_url(&stmt.target_url).map(|(key, _)| key) else {
            outcome.pending_targets.push(stmt.target_url);
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
    use kernel::graph::fixtures::GraphFixtures;
    use crate::{DocumentProvenance, DocumentTrustState};
    use kernel::graph::RelationSelector;

    fn doc(blocks: Vec<Block>) -> EngineDocument {
        EngineDocument {
            address: "knot:test".to_string(),
            title: None,
            content_type: "text/x-knot".to_string(),
            lang: None,
            provenance: DocumentProvenance::default(),
            trust: DocumentTrustState::default(),
            diagnostics: Vec::new(),
            blocks,
        }
    }

    /// A one-paragraph document with a single link carrying `rel`.
    fn link_doc(url: &str, rel: Option<&str>) -> EngineDocument {
        doc(vec![Block::Paragraph {
            spans: vec![InlineSpan::Link {
                url: url.to_string(),
                title: None,
                spans: vec![InlineSpan::Text("label".to_string())],
                predicate: rel.map(str::to_string),
            }],
        }])
    }

    #[test]
    fn link_statements_extracts_predicate_links_only() {
        let document = doc(vec![Block::Paragraph {
            spans: vec![
                InlineSpan::Link {
                    url: "https://plain.test/".to_string(),
                    title: None,
                    spans: vec![InlineSpan::Text("plain".to_string())],
                    predicate: None,
                },
                InlineSpan::Link {
                    url: "mere://node/topic".to_string(),
                    title: None,
                    spans: vec![InlineSpan::Text("Topic".to_string())],
                    predicate: Some("cites".to_string()),
                },
            ],
        }]);
        assert_eq!(
            link_statements(&document),
            vec![LinkStatement {
                target_url: "mere://node/topic".to_string(),
                rel: "cites".to_string(),
            }]
        );
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

        let outcome = apply_link_statements(&mut graph, source, &link_doc("mere://node/topic", Some("cites")));

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

        let outcome =
            apply_link_statements(&mut graph, source, &link_doc("mere://node/absent", Some("cites")));

        assert_eq!(outcome.edges_asserted, 0);
        assert_eq!(outcome.pending_targets, vec!["mere://node/absent".to_string()]);
    }

    #[test]
    fn apply_reports_unrecognized_raw_predicate() {
        let mut graph = Graph::new();
        let source = graph.add_node("knot:test".to_string(), Default::default());
        let _target = graph.add_node("mere://node/topic".to_string(), Default::default());

        let outcome = apply_link_statements(
            &mut graph,
            source,
            &link_doc("mere://node/topic", Some("schema:citation")),
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
