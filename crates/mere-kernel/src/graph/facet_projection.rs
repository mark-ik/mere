/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! PMEST facet projection — computes a queryable facet map from a node + graph context.
//!
//! Spec: `faceted_filter_surface_spec.md §3`
//!
//! The projection is **pure** (no mutations). It derives canonical facet values
//! from durable node data plus graph-structural context (edge degrees, etc.).
//!
//! Several facet keys are intentionally derived projections rather than stored
//! node fields: `domain`, `edge_kinds`, `in_degree`, `out_degree`,
//! `frame_memberships`, `udc_classes`.

use std::collections::HashSet;

use petgraph::Direction;
use petgraph::visit::EdgeRef;

use super::filter::{FacetProjection, FacetScalar, FacetValue, facet_keys};
use super::{ArrangementSubKind, EdgeFamily, Graph, NodeKey};

/// Compute the PMEST facet projection for a single node.
///
/// Returns a [`FacetProjection`] (`HashMap<String, FacetValue>`) ready for
/// evaluation by [`FacetExpr::evaluate`].
///
/// Graph-structural facets (degrees, edge kinds, frame memberships) are derived
/// from `graph` at call time. The projection reflects the current graph state
/// and must be recomputed when graph structure changes.
pub fn facet_projection_for_node(graph: &Graph, key: NodeKey) -> Option<FacetProjection> {
    let node = graph.get_node(key)?;
    let mut proj: FacetProjection = std::collections::HashMap::new();

    // --- Personality ---

    proj.insert(
        facet_keys::ADDRESS_KIND.to_string(),
        FacetValue::Scalar(FacetScalar::Text(format!(
            "{:?}",
            node.address.address_kind()
        ))),
    );

    if let Ok(url) = url::Url::parse(node.url()) {
        if let Some(host) = url.host_str() {
            proj.insert(
                facet_keys::DOMAIN.to_string(),
                FacetValue::Scalar(FacetScalar::Text(host.to_string())),
            );
        }
    }

    proj.insert(
        facet_keys::TITLE.to_string(),
        FacetValue::Scalar(FacetScalar::Text(node.title.clone())),
    );

    proj.insert(
        facet_keys::ADDRESS.to_string(),
        FacetValue::Scalar(FacetScalar::Text(node.url().to_string())),
    );

    // --- Matter ---

    if let Some(mime) = &node.mime_hint {
        proj.insert(
            facet_keys::MIME_HINT.to_string(),
            FacetValue::Scalar(FacetScalar::Text(mime.clone())),
        );
    }

    // --- Energy (edge-derived) ---

    let out_edges: Vec<_> = graph.inner.edges(key).collect();
    let in_edges: Vec<_> = graph
        .inner
        .edges_directed(key, Direction::Incoming)
        .collect();

    let out_degree = out_edges.len();
    let in_degree = in_edges.len();

    let mut edge_kind_labels: HashSet<&'static str> = HashSet::new();
    for e in out_edges.iter().chain(in_edges.iter()) {
        for family in e.weight().families() {
            edge_kind_labels.insert(edge_family_label(*family));
        }
    }

    proj.insert(
        facet_keys::IN_DEGREE.to_string(),
        FacetValue::Scalar(FacetScalar::Number(in_degree as f64)),
    );
    proj.insert(
        facet_keys::OUT_DEGREE.to_string(),
        FacetValue::Scalar(FacetScalar::Number(out_degree as f64)),
    );

    if !edge_kind_labels.is_empty() {
        proj.insert(
            facet_keys::EDGE_KINDS.to_string(),
            FacetValue::Collection(
                edge_kind_labels
                    .into_iter()
                    .map(|s| FacetScalar::Text(s.to_string()))
                    .collect(),
            ),
        );
    }

    // Traversal count from outgoing TraversalDerived edges
    let traversal_count: usize = graph
        .inner
        .edges(key)
        .filter_map(|e| e.weight().traversal.as_ref())
        .map(|t| t.metrics.total_navigations as usize)
        .sum();
    proj.insert(
        facet_keys::TRAVERSAL_COUNT.to_string(),
        FacetValue::Scalar(FacetScalar::Number(traversal_count as f64)),
    );

    // --- Space ---

    // Frame memberships: nodes that are target of an ArrangementRelation(FrameMember) edge
    // directed TO this node (i.e., this node is a frame member).
    // Represented as the source node keys (frame anchors).
    let frame_memberships: Vec<FacetScalar> = graph
        .inner
        .edges_directed(key, Direction::Incoming)
        .filter(|e| {
            e.weight()
                .arrangement
                .as_ref()
                .is_some_and(|a| a.sub_kinds.contains(&ArrangementSubKind::FrameMember))
        })
        .map(|e| FacetScalar::Text(format!("{:?}", e.source())))
        .collect();

    if !frame_memberships.is_empty() {
        proj.insert(
            facet_keys::FRAME_MEMBERSHIPS.to_string(),
            FacetValue::Collection(frame_memberships),
        );
    }

    // UDC classes: semantic tags + durable NodeClassification values (Stage A schema).
    // Tags already use "udc:"-prefixed values; classification values carry the same
    // scheme-prefixed format (e.g. "udc:519.6") so they slot directly into the same
    // collection facet.
    {
        let mut udc_values: Vec<FacetScalar> = node
            .tags
            .iter()
            .map(|t| FacetScalar::Text(t.clone()))
            .collect();
        for c in &node.classifications {
            udc_values.push(FacetScalar::Text(c.value.clone()));
        }
        if !udc_values.is_empty() {
            proj.insert(
                facet_keys::UDC_CLASSES.to_string(),
                FacetValue::Collection(udc_values),
            );
        }
    }

    // --- Time ---

    // last_visited as milliseconds since epoch (best available time source on Node)
    if let Ok(duration) = node.last_visited.duration_since(std::time::UNIX_EPOCH) {
        proj.insert(
            facet_keys::LAST_TRAVERSAL.to_string(),
            FacetValue::Scalar(FacetScalar::Number(duration.as_millis() as f64)),
        );
    }

    proj.insert(
        facet_keys::LIFECYCLE.to_string(),
        FacetValue::Scalar(FacetScalar::Text(format!("{:?}", node.lifecycle))),
    );

    Some(proj)
}

fn edge_family_label(family: EdgeFamily) -> &'static str {
    match family {
        EdgeFamily::Semantic => "Semantic",
        EdgeFamily::Traversal => "Traversal",
        EdgeFamily::Containment => "Containment",
        EdgeFamily::Arrangement => "Arrangement",
        EdgeFamily::Imported => "Imported",
        EdgeFamily::Provenance => "Provenance",
    }
}

#[cfg(test)]
mod tests {
    use euclid::default::Point2D;

    use super::*;
    use crate::graph::Graph;
    use crate::graph::apply::{GraphDelta, GraphDeltaResult, apply_graph_delta};
    use crate::graph::filter::facet_keys;

    fn build_node(graph: &mut Graph, url: &str) -> NodeKey {
        let GraphDeltaResult::NodeAdded(key) = apply_graph_delta(
            graph,
            GraphDelta::AddNode {
                id: None,
                url: url.to_string(),
                position: Point2D::new(0.0, 0.0),
            },
        ) else {
            panic!("expected NodeAdded");
        };
        key
    }

    #[test]
    fn projection_includes_address_and_domain() {
        let mut graph = Graph::new();
        let key = build_node(&mut graph, "https://example.com/page");
        let proj = facet_projection_for_node(&graph, key).unwrap();

        assert!(proj.contains_key(facet_keys::ADDRESS));
        assert!(proj.contains_key(facet_keys::DOMAIN));
        let domain = &proj[facet_keys::DOMAIN];
        assert_eq!(
            *domain,
            FacetValue::Scalar(FacetScalar::Text("example.com".to_string()))
        );
    }

    #[test]
    fn projection_computes_in_out_degree() {
        let mut graph = Graph::new();
        let a = build_node(&mut graph, "https://a.test/");
        let b = build_node(&mut graph, "https://b.test/");
        apply_graph_delta(
            &mut graph,
            GraphDelta::AssertRelation {
                from: a,
                to: b,
                assertion: crate::graph::EdgeAssertion::Semantic {
                    sub_kind: crate::graph::SemanticSubKind::Hyperlink,
                    label: None,
                    decay_progress: None,
                },
            },
        );

        let proj_a = facet_projection_for_node(&graph, a).unwrap();
        let proj_b = facet_projection_for_node(&graph, b).unwrap();

        assert_eq!(
            proj_a[facet_keys::OUT_DEGREE],
            FacetValue::Scalar(FacetScalar::Number(1.0))
        );
        assert_eq!(
            proj_b[facet_keys::IN_DEGREE],
            FacetValue::Scalar(FacetScalar::Number(1.0))
        );
    }

    #[test]
    fn projection_includes_lifecycle() {
        let mut graph = Graph::new();
        let key = build_node(&mut graph, "https://example.com/");
        let proj = facet_projection_for_node(&graph, key).unwrap();
        assert!(proj.contains_key(facet_keys::LIFECYCLE));
    }

    #[test]
    fn projection_udc_classes_includes_classification_values() {
        use crate::graph::{
            ClassificationProvenance, ClassificationScheme, ClassificationStatus,
            NodeClassification,
        };
        let mut graph = Graph::new();
        let key = build_node(&mut graph, "https://example.com/");
        graph.add_node_classification(
            key,
            NodeClassification {
                scheme: ClassificationScheme::Udc,
                value: "udc:519.6".to_string(),
                label: Some("Computational mathematics".to_string()),
                confidence: 1.0,
                provenance: ClassificationProvenance::UserAuthored,
                status: ClassificationStatus::Accepted,
                primary: true,
            },
        );
        let proj = facet_projection_for_node(&graph, key).unwrap();
        let udc = proj.get(facet_keys::UDC_CLASSES).unwrap();
        let FacetValue::Collection(items) = udc else {
            panic!("expected Collection");
        };
        assert!(
            items.contains(&FacetScalar::Text("udc:519.6".to_string())),
            "classification value must appear in udc_classes facet"
        );
    }

    #[test]
    fn projection_udc_classes_merges_tags_and_classifications() {
        use crate::graph::{
            ClassificationProvenance, ClassificationScheme, ClassificationStatus,
            NodeClassification,
        };
        let mut graph = Graph::new();
        let key = build_node(&mut graph, "https://example.com/");
        // add a tag directly
        graph.insert_node_tag(key, "udc:51".to_string());
        // add a classification
        graph.add_node_classification(
            key,
            NodeClassification {
                scheme: ClassificationScheme::Udc,
                value: "udc:519.6".to_string(),
                label: None,
                confidence: 0.9,
                provenance: ClassificationProvenance::RegistryDerived,
                status: ClassificationStatus::Suggested,
                primary: false,
            },
        );
        let proj = facet_projection_for_node(&graph, key).unwrap();
        let FacetValue::Collection(items) = &proj[facet_keys::UDC_CLASSES] else {
            panic!("expected Collection");
        };
        assert!(items.contains(&FacetScalar::Text("udc:51".to_string())));
        assert!(items.contains(&FacetScalar::Text("udc:519.6".to_string())));
    }
}
