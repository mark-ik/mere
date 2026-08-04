// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Root-canvas projection of one view-local [`forme::FoldRecord`].
//!
//! This is deliberately a proof layer, not a renderer. It makes the source
//! membership and relation accounting explicit before a synthetic summary node
//! is introduced into Canvas paint, hit-testing, or physics.

use std::collections::{BTreeMap, BTreeSet};

use forme::{FoldId, FoldRecord, FOLD_RECORD_VERSION};
use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeFamily, Graph, NodeKey};

use crate::Canvas;

/// The synthetic summary body's radius in Canvas world pixels. It deliberately
/// reads larger than an ordinary node because it stands for several members.
pub(crate) const FOLD_SUMMARY_RADIUS: f32 = 30.0;

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

impl FoldProjection {
    /// The summary lives at the centroid of its members' existing layout
    /// positions. Folding never moves or re-seeds those source members, so an
    /// expand restores their stable placement exactly.
    pub fn summary_center(
        &self,
        mut position_of: impl FnMut(NodeKey) -> Option<PortablePoint>,
    ) -> Option<PortablePoint> {
        let mut total = PortablePoint::zero();
        for &member in &self.members {
            let position = position_of(member)?;
            total.x += position.x;
            total.y += position.y;
        }
        let count = self.members.len() as f32;
        Some(PortablePoint::new(total.x / count, total.y / count))
    }
}

impl Canvas {
    /// Collapse the current multi-selection into one synthetic Canvas summary.
    ///
    /// The returned id is durable view-state identity. This method never adds,
    /// removes, or rewrites a graph node or relation.
    pub fn fold_selected(&mut self, source_scope: impl Into<String>) -> Option<FoldId> {
        if self.fold.is_some() {
            return None;
        }
        let members = self
            .selected
            .iter()
            .filter_map(|&key| self.graph.get_node(key).map(|node| node.id));
        let record = FoldRecord::from_selection(source_scope, members)?;
        let id = record.id;
        self.fold = Some(record);
        self.selected.clear();
        self.selected_edges.clear();
        Some(id)
    }

    /// Expand the active summary, restoring its source members at their unchanged
    /// layout positions and selecting them for immediate orientation.
    pub fn expand_fold(&mut self, id: FoldId) -> bool {
        let Some(record) = self.fold.as_ref() else {
            return false;
        };
        if record.id != id {
            return false;
        }
        let Some(projection) = project_fold(&self.graph, record) else {
            return false;
        };
        self.fold = None;
        self.fold_press = None;
        self.selected = projection.members.into_iter().collect();
        self.selected_edges.clear();
        true
    }

    /// Expand whichever fold Canvas is currently projecting.
    pub fn expand_active_fold(&mut self) -> bool {
        let Some(id) = self.fold.as_ref().map(|record| record.id) else {
            return false;
        };
        self.expand_fold(id)
    }

    /// The current fold's source accounting, if it is still valid against the
    /// live graph. A stale record remains durable state but does not partially
    /// render.
    pub fn active_fold_projection(&self) -> Option<FoldProjection> {
        self.fold
            .as_ref()
            .and_then(|record| project_fold(&self.graph, record))
    }

    /// The durable fold record a host should place in its per-pane view intent.
    /// The record names source members by stable id; it does not serialize a
    /// synthetic Canvas node or any physics state.
    pub fn fold_record(&self) -> Option<&FoldRecord> {
        self.fold.as_ref()
    }

    /// Restore a persisted fold record when it still projects completely
    /// against this graph. A stale record is refused intact so the host can
    /// offer repair rather than silently collapsing a different selection.
    pub fn restore_fold(&mut self, record: Option<FoldRecord>) -> bool {
        if let Some(record) = record {
            if project_fold(&self.graph, &record).is_none() {
                return false;
            }
            self.fold = Some(record);
            self.selected.clear();
            self.selected_edges.clear();
        } else {
            self.fold = None;
        }
        self.fold_press = None;
        true
    }

    pub(crate) fn node_visible_in_canvas(&self, key: NodeKey) -> bool {
        self.node_in_scope(key)
    }

    pub(crate) fn fold_summary_at_screen(&self, cursor: (f32, f32)) -> Option<FoldId> {
        let projection = self.active_fold_projection()?;
        let center = projection.summary_center(|key| {
            self.view
                .position_of(key)
                .map(|position| PortablePoint::new(position.x, position.y))
        })?;
        let (x, y) = self.camera.to_screen(center);
        let radius = FOLD_SUMMARY_RADIUS * self.camera.zoom;
        ((cursor.0 - x).hypot(cursor.1 - y) <= radius).then_some(projection.fold_id)
    }
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
