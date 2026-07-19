// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Snapshot-size gate (petgraph-RDF plan, the common-case-bloat risk).
//!
//! The projection profile added per-statement `graph_scope`, `provenance_iri`,
//! `asserted_at_ms`, and `label` to node properties and semantic statements. The
//! plan's guard is that these must not tax the 99% case — a human-asserted, now,
//! default-graph statement with no provenance. This is the "before / after"
//! tripwire that keeps that guard honest.
//!
//! A [`NodeProperty`] is the record that carries the profile metadata *without*
//! the heavy `PersistedEdge` wrapper (traversal / arrangement / containment
//! payloads) that would otherwise swamp the signal, so the property lane is
//! where the metadata cost is measured cleanly: an unpopulated common-case
//! property is pinned under a tight ceiling, and populating the metadata must
//! cost strictly more (pay-per-use). A separate, coarse ceiling on a
//! semantic-statement chain guards the edge lane from gross regression.
//!
//! Size is the compact-JSON snapshot — the actual on-disk form
//! (`store::save_graph`), minus the pretty-printer's whitespace so the number
//! reflects field/key cost, not formatting.

use super::super::*;
use crate::graph::SemanticStatementSpec;
use crate::types::{GraphScope, NodeProperty};
use euclid::default::Point2D;

/// Compact-JSON byte length of a graph's snapshot (the on-disk persistence form).
fn snapshot_len(graph: &Graph) -> usize {
    let snapshot = graph.to_snapshot();
    serde_json::to_vec(&snapshot)
        .expect("a graph snapshot is always serializable")
        .len()
}

/// `count` nodes with sequential test URLs, no edges, no properties.
fn bare_nodes(count: usize) -> (Graph, Vec<NodeKey>) {
    let mut graph = Graph::new();
    let keys = (0..count)
        .map(|i| graph.add_node(format!("https://n{i}.test/"), Point2D::new(0.0, 0.0)))
        .collect();
    (graph, keys)
}

#[test]
fn common_case_property_metadata_stays_cheap_in_the_snapshot() {
    const N: usize = 50;

    // Baseline: N isolated nodes, no properties.
    let (bare, _) = bare_nodes(N);
    let bare_len = snapshot_len(&bare);

    // Common case: one simple default-scope property per node, no metadata.
    let (mut plain, keys) = bare_nodes(N);
    for &key in &keys {
        plain.get_node_mut(key).expect("node").properties.push(
            NodeProperty::new(
                "https://schema.org/datePublished".to_string(),
                "2026-07-04".to_string(),
            ),
        );
    }
    let plain_len = snapshot_len(&plain);

    // Fully annotated: the same property, now scoped + typed + attributed + timed.
    let (mut rich, keys) = bare_nodes(N);
    for &key in &keys {
        let mut property = NodeProperty::new(
            "https://schema.org/datePublished".to_string(),
            "2026-07-04".to_string(),
        )
        .with_graph_scope(GraphScope::User)
        .with_metadata(
            Some("https://persona.test/some-long-agent-iri".to_string()),
            Some(1_720_000_000_000),
        );
        property.datatype = Some("http://www.w3.org/2001/XMLSchema#date".to_string());
        rich.get_node_mut(key).expect("node").properties.push(property);
    }
    let rich_len = snapshot_len(&rich);

    let per_plain = (plain_len - bare_len) as f64 / N as f64;
    let per_rich = (rich_len - bare_len) as f64 / N as f64;
    println!(
        "snapshot-size gate (property): bare={bare_len} plain={plain_len} rich={rich_len} \
         (common-case {per_plain:.0} B/property, annotated {per_rich:.0} B/property)"
    );

    // A common-case property carries only its id, predicate IRI, value, and the
    // (empty) metadata fields. Ceiling = measured (~200 B) with headroom; trips
    // if a metadata field becomes mandatory or gains fixed overhead.
    assert!(
        per_plain < 280.0,
        "common-case property snapshot cost regressed: {per_plain:.0} B/property (bare={bare_len}, plain={plain_len})"
    );

    // Metadata is pay-per-use: populating the optional fields costs strictly more
    // than leaving them empty. If this fails, the common case is paying for
    // metadata it does not carry.
    assert!(
        rich_len > plain_len,
        "annotated snapshot ({rich_len}) must exceed the common-case one ({plain_len})"
    );
}

#[test]
fn common_case_semantic_statement_stays_cheap_in_the_snapshot() {
    const N: usize = 50;
    let statements = N - 1;

    let (bare, _) = bare_nodes(N);
    let bare_len = snapshot_len(&bare);

    // A chain of simple default-scope `cites` statements, no metadata.
    let (mut plain, keys) = bare_nodes(N);
    for pair in keys.windows(2) {
        plain
            .assert_semantic_statement(
                pair[0],
                pair[1],
                SemanticStatementSpec {
                    predicate: predicate_iri(SemanticSubKind::Cites).to_string(),
                    recognized_sub_kind: Some(SemanticSubKind::Cites),
                    ..Default::default()
                },
            )
            .expect("statement");
    }
    let plain_len = snapshot_len(&plain);

    let per_statement = (plain_len - bare_len) as f64 / statements as f64;
    println!(
        "snapshot-size gate (statement): bare={bare_len} plain={plain_len} \
         (common-case {per_statement:.0} B/statement, incl. edge wrapper)"
    );

    // Coarse: the whole `PersistedEdge` wrapper (defaulted edge-family payloads)
    // dominates here, so this ceiling guards the edge lane from gross regression
    // rather than isolating statement-metadata cost. Ceiling = measured (~590 B)
    // with headroom.
    assert!(
        per_statement < 760.0,
        "common-case statement snapshot cost regressed: {per_statement:.0} B/statement (bare={bare_len}, plain={plain_len})"
    );
}
