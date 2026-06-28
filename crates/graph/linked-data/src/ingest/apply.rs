/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#[cfg(not(target_arch = "wasm32"))]
use kernel::graph::{
    EdgeAssertion, Graph, NodeKey, ProvenanceSubKind, SemanticSubKind, predicate_iri,
    sub_kind_from_iri,
};
#[cfg(not(target_arch = "wasm32"))]
use kernel::types::{
    ClassificationProvenance, ClassificationScheme, ClassificationStatus, NodeClassification,
    NodeDerivation, NodeProperty,
};

use super::GraphContribution;

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
            let semantic_ok = graph
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
                .is_some();
            // A harvested hyperlink also records derivation provenance on the
            // target: it was `ExtractedFrom` the source page (capture plan C3).
            // Recorded as a node derivation (like cross-graph `CopiedFrom`), so it
            // feeds the provenance trail without polluting the link graph's
            // out-edges — a channel distinct from the `Hyperlink` semantic edge.
            if sub_kind == SemanticSubKind::Hyperlink {
                if let Some(source_node) = graph.get_node(from).map(|n| n.id.to_string()) {
                    graph.record_derivation(
                        to,
                        NodeDerivation {
                            sub_kind: ProvenanceSubKind::ExtractedFrom,
                            source_node,
                            source_graph: None,
                        },
                    );
                }
            }
            semantic_ok
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

