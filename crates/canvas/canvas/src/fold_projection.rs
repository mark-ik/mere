// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Root-canvas projection of one view-local [`forme::FoldRecord`].
//!
//! This is deliberately a proof layer, not a renderer. It makes the source
//! membership and relation accounting explicit before a synthetic summary node
//! is introduced into Canvas paint, hit-testing, or physics.

use std::collections::{BTreeMap, BTreeSet};

use forme::{FOLD_RECORD_VERSION, FoldId, FoldRecord};
use kernel::graph::{EdgeFamily, Graph, NodeKey};

/// Whether a boundary cell enters or leaves a folded summary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FoldBoundaryDirection {
    Incoming,
    Outgoing,
}

/// Cells crossing a fold boundary, bundled only by their outside endpoint,
/// direction, and relation family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoldBoundaryBundle {
    pub outside: NodeKey,
    pub direction: FoldBoundaryDirection,
    pub family: EdgeFamily,
    pub count: usize,
}

/// The source accounting a Canvas summary node will render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldProjection {
    pub fold_id: FoldId,
    /// Source nodes replaced by the synthetic summary in this projection.
    pub members: BTreeSet<NodeKey>,
    /// Source nodes still individually visible beside the summary.
    pub visible_members: Vec<NodeKey>,
    /// Relation cells with both endpoints inside the fold.
    pub internal_relation_count: usize,
    /// Relation cells crossing the boundary, exact after family bundling.
    pub boundary_bundles: Vec<FoldBoundaryBundle>,
}

/// Project one record against its current source graph.
///
/// A stale or unsupported record is refused rather than folding a partial
/// member set. The caller can keep the record for inspection and offer an
/// explicit repair or removal action.
pub fn project_fold(graph: &Graph, fold: &FoldRecord) -> Option<FoldProjection> {
    if fold.version != FOLD_RECORD_VERSION || fold.members.len() < 2 {
        return None;
    }
    let members: BTreeSet<NodeKey> = fold
        .members
        .iter()
        .map(|member| graph.get_node_key_by_id(*member))
        .collect::<Option<_>>()?;
    if members.len() != fold.members.len() {
        return None;
    }

    let mut bundles: BTreeMap<(NodeKey, FoldBoundaryDirection, EdgeFamily), usize> =
        BTreeMap::new();
    let mut internal_relation_count = 0;
    for relation in graph.relations() {
        let from_folded = members.contains(&relation.from);
        let to_folded = members.contains(&relation.to);
        match (from_folded, to_folded) {
            (true, true) => internal_relation_count += 1,
            (true, false) => {
                *bundles
                    .entry((
                        relation.to,
                        FoldBoundaryDirection::Outgoing,
                        relation.kind.family(),
                    ))
                    .or_default() += 1;
            }
            (false, true) => {
                *bundles
                    .entry((
                        relation.from,
                        FoldBoundaryDirection::Incoming,
                        relation.kind.family(),
                    ))
                    .or_default() += 1;
            }
            (false, false) => {}
        }
    }

    let visible_members = graph
        .nodes()
        .filter_map(|(key, _)| (!members.contains(&key)).then_some(key))
        .collect();
    Some(FoldProjection {
        fold_id: fold.id,
        members,
        visible_members,
        internal_relation_count,
        boundary_bundles: bundles
            .into_iter()
            .map(|((outside, direction, family), count)| FoldBoundaryBundle {
                outside,
                direction,
                family,
                count,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use euclid::default::Point2D;
    use kernel::graph::{
        ContainmentSubKind, EdgeAssertion, SemanticSubKind, fixtures::GraphFixtures,
    };

    use super::*;

    #[test]
    fn fold_projects_members_and_bundles_only_boundary_cells_by_family() {
        let mut graph = Graph::new();
        let a = graph.add_node("https://a".into(), Point2D::new(0.0, 0.0));
        let b = graph.add_node("https://b".into(), Point2D::new(1.0, 0.0));
        let outside = graph.add_node("https://outside".into(), Point2D::new(2.0, 0.0));
        graph.assert_relation(
            a,
            b,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
        graph.assert_relation(
            a,
            outside,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
        );
        graph.assert_relation(
            b,
            outside,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Quotes,
                label: None,
                decay_progress: None,
            },
        );
        graph.assert_relation(
            outside,
            b,
            EdgeAssertion::Containment {
                sub_kind: ContainmentSubKind::Domain,
            },
        );
        let fold = FoldRecord::from_selection(
            "graph:fixture",
            [graph.get_node(a).unwrap().id, graph.get_node(b).unwrap().id],
        )
        .expect("two source members");

        let projection = project_fold(&graph, &fold).expect("current record projects");
        assert_eq!(projection.members, BTreeSet::from([a, b]));
        assert_eq!(projection.visible_members, vec![outside]);
        assert_eq!(projection.internal_relation_count, 1);
        assert_eq!(
            projection.boundary_bundles,
            vec![
                FoldBoundaryBundle {
                    outside,
                    direction: FoldBoundaryDirection::Incoming,
                    family: EdgeFamily::Containment,
                    count: 1,
                },
                FoldBoundaryBundle {
                    outside,
                    direction: FoldBoundaryDirection::Outgoing,
                    family: EdgeFamily::Semantic,
                    count: 2,
                },
            ],
            "two semantic cells bundle, while a different family and direction remain distinct"
        );
        assert_eq!(graph.nodes().count(), 3, "the source graph stays intact");
        assert_eq!(
            graph.relations().count(),
            4,
            "projection does not remove cells"
        );
    }

    #[test]
    fn stale_member_refuses_partial_fold() {
        let mut graph = Graph::new();
        let node = graph.add_node("https://only-live".into(), Point2D::new(0.0, 0.0));
        let fold = FoldRecord::from_selection(
            "graph:fixture",
            [graph.get_node(node).unwrap().id, uuid::Uuid::from_u128(99)],
        )
        .expect("two ids in the durable record");
        assert!(project_fold(&graph, &fold).is_none());
    }
}
