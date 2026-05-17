// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structural parity diagnostics for migration safety.
//!
//! During the egui_tiles → GraphTree migration, both systems run in
//! parallel. Parity diagnostics compare the two views and surface
//! structural drift — not just membership-set differences.
//!
//! This module is designed to be consumed by the host's diagnostic
//! channel infrastructure. It produces a `ParityReport` that the
//! host can log, display, or gate on.

use std::collections::{HashMap, HashSet};

use crate::MemberId;
use crate::member::Lifecycle;
use crate::tree::GraphTree;

/// A snapshot of external (e.g. egui_tiles) tree state, flattened
/// into the fields that parity checks need. The host constructs this
/// from whatever the legacy tree exposes.
#[derive(Clone, Debug)]
pub struct ExternalTreeSnapshot<N: MemberId> {
    /// All member IDs present in the external tree.
    pub members: HashSet<N>,

    /// Parent → children relationships. Missing key = root.
    pub children: HashMap<N, Vec<N>>,

    /// The currently active/focused member, if any.
    pub active: Option<N>,

    /// Members that are visually displayed (open panes / visible tabs).
    pub visible: HashSet<N>,

    /// Members whose children are currently expanded/visible in the tree.
    /// Leave empty if the external tree has no expansion concept.
    pub expanded: HashSet<N>,

    /// Ordered sequence of visible member IDs as the external tree renders them.
    /// Leave empty if the external tree has no canonical ordering.
    pub visible_order: Vec<N>,
}

/// Individual divergence found during parity comparison.
#[derive(Clone, Debug)]
pub enum ParityDivergence<N: MemberId> {
    /// Member exists in GraphTree but not in external tree.
    MissingFromExternal(N),
    /// Member exists in external tree but not in GraphTree.
    MissingFromGraphTree(N),
    /// Parent/child relationship differs.
    TopologyMismatch {
        member: N,
        graph_tree_parent: Option<N>,
        external_parent: Option<N>,
    },
    /// Active member disagrees.
    ActiveMismatch {
        graph_tree: Option<N>,
        external: Option<N>,
    },
    /// Visible set differs (member visible in one but not the other).
    VisibilityMismatch {
        member: N,
        in_graph_tree: bool,
        in_external: bool,
    },
    /// Expansion state differs (expanded in one but not the other).
    ExpansionMismatch {
        member: N,
        in_graph_tree: bool,
        in_external: bool,
    },
    /// Visible member ordering differs.
    VisibleOrderMismatch {
        /// GraphTree's rendering order for visible members.
        graph_tree_order: Vec<N>,
        /// External tree's rendering order for visible members.
        external_order: Vec<N>,
    },
}

/// Full parity comparison result.
#[derive(Clone, Debug)]
pub struct ParityReport<N: MemberId> {
    /// All divergences found.
    pub divergences: Vec<ParityDivergence<N>>,

    /// Summary counts for quick triage.
    pub graph_tree_only: usize,
    pub external_only: usize,
    pub topology_mismatches: usize,
    pub visibility_mismatches: usize,
    pub expansion_mismatches: usize,
    pub active_matches: bool,
    pub visible_order_matches: bool,
}

impl<N: MemberId> ParityReport<N> {
    /// True when no divergences were found.
    pub fn is_clean(&self) -> bool {
        self.divergences.is_empty()
    }

    /// True when membership sets match (ignoring topology).
    pub fn membership_matches(&self) -> bool {
        self.graph_tree_only == 0 && self.external_only == 0
    }

    /// True when membership AND topology both match.
    pub fn structural_match(&self) -> bool {
        self.membership_matches() && self.topology_mismatches == 0
    }

    /// True when all axes match (full structural + behavioral parity).
    pub fn full_match(&self) -> bool {
        self.structural_match()
            && self.active_matches
            && self.visibility_mismatches == 0
            && self.expansion_mismatches == 0
            && self.visible_order_matches
    }
}

/// Compare a `GraphTree` against an external tree snapshot.
///
/// Checks membership, topology (parent/child), active member,
/// and visible set. Returns a `ParityReport` with all divergences.
///
/// During the transition phase, divergences are expected (e.g. Cold
/// members exist in GraphTree but not in egui_tiles). The report
/// lets the host decide which divergences are acceptable.
pub fn compare<N: MemberId>(
    tree: &GraphTree<N>,
    external: &ExternalTreeSnapshot<N>,
) -> ParityReport<N> {
    let mut divergences = Vec::new();
    let mut graph_tree_only = 0usize;
    let mut external_only = 0usize;
    let mut topology_mismatches = 0usize;
    let mut visibility_mismatches = 0usize;
    let mut expansion_mismatches = 0usize;

    let gt_members: HashSet<N> = tree.members().map(|(id, _)| id.clone()).collect();

    // --- Membership ---

    for id in &gt_members {
        if !external.members.contains(id) {
            // Cold members are expected to be missing from external
            let entry = tree.get(id);
            let is_cold = entry.is_some_and(|e| e.lifecycle == Lifecycle::Cold);
            if !is_cold {
                divergences.push(ParityDivergence::MissingFromExternal(id.clone()));
                graph_tree_only += 1;
            }
        }
    }

    for id in &external.members {
        if !gt_members.contains(id) {
            divergences.push(ParityDivergence::MissingFromGraphTree(id.clone()));
            external_only += 1;
        }
    }

    // --- Topology (only for members in both, only when external has topology data) ---

    let shared: HashSet<&N> = gt_members.intersection(&external.members).collect();

    if !external.children.is_empty() {
        for id in &shared {
            let gt_parent = tree.topology().parent_of(id).cloned();

            // Derive external parent by scanning children map
            let ext_parent = external
                .children
                .iter()
                .find(|(_, children)| children.contains(id))
                .map(|(parent, _)| parent.clone());

            if gt_parent != ext_parent {
                divergences.push(ParityDivergence::TopologyMismatch {
                    member: (*id).clone(),
                    graph_tree_parent: gt_parent,
                    external_parent: ext_parent,
                });
                topology_mismatches += 1;
            }
        }
    }

    // --- Active member (only when external reports an active member) ---

    let active_matches = external.active.is_none() || tree.active().cloned() == external.active;
    if !active_matches {
        divergences.push(ParityDivergence::ActiveMismatch {
            graph_tree: tree.active().cloned(),
            external: external.active.clone(),
        });
    }

    // --- Visibility ---

    let gt_visible: HashSet<N> = tree
        .members()
        .filter(|(_, e)| e.is_visible_in_pane())
        .map(|(id, _)| id.clone())
        .collect();

    for id in gt_visible.difference(&external.visible) {
        divergences.push(ParityDivergence::VisibilityMismatch {
            member: id.clone(),
            in_graph_tree: true,
            in_external: false,
        });
        visibility_mismatches += 1;
    }

    for id in external.visible.difference(&gt_visible) {
        if gt_members.contains(id) {
            divergences.push(ParityDivergence::VisibilityMismatch {
                member: id.clone(),
                in_graph_tree: false,
                in_external: true,
            });
            visibility_mismatches += 1;
        }
    }

    // --- Expansion state (only when external has expansion data) ---

    if !external.expanded.is_empty() {
        let gt_expanded: HashSet<N> = tree.expanded_members().cloned().collect();

        for id in gt_expanded.difference(&external.expanded) {
            if external.members.contains(id) {
                divergences.push(ParityDivergence::ExpansionMismatch {
                    member: id.clone(),
                    in_graph_tree: true,
                    in_external: false,
                });
                expansion_mismatches += 1;
            }
        }
        for id in external.expanded.difference(&gt_expanded) {
            if gt_members.contains(id) {
                divergences.push(ParityDivergence::ExpansionMismatch {
                    member: id.clone(),
                    in_graph_tree: false,
                    in_external: true,
                });
                expansion_mismatches += 1;
            }
        }
    }

    // --- Visible order (only when external provides an ordering) ---

    let visible_order_matches = if external.visible_order.is_empty() {
        true // external doesn't have ordering data — skip comparison
    } else {
        let gt_order: Vec<N> = tree
            .visible_rows()
            .into_iter()
            .map(|row| row.member.clone())
            .collect();
        let matches = gt_order == external.visible_order;
        if !matches {
            divergences.push(ParityDivergence::VisibleOrderMismatch {
                graph_tree_order: gt_order,
                external_order: external.visible_order.clone(),
            });
        }
        matches
    };

    ParityReport {
        divergences,
        graph_tree_only,
        external_only,
        topology_mismatches,
        visibility_mismatches,
        expansion_mismatches,
        active_matches,
        visible_order_matches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutMode;
    use crate::lens::ProjectionLens;
    use crate::member::Provenance;
    use crate::nav::NavAction;

    fn build_tree() -> GraphTree<u64> {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Warm));
        tree.apply(NavAction::Activate(1));
        tree
    }

    fn matching_snapshot() -> ExternalTreeSnapshot<u64> {
        let mut children = HashMap::new();
        children.insert(1, vec![2, 3]);
        ExternalTreeSnapshot {
            members: HashSet::from([1, 2, 3]),
            children,
            active: Some(1),
            visible: HashSet::from([1, 2]),
            expanded: HashSet::new(), // leave empty → expansion check skipped
            visible_order: Vec::new(), // leave empty → order check skipped
        }
    }

    #[test]
    fn clean_parity_report() {
        let tree = build_tree();
        let snapshot = matching_snapshot();
        let report = compare(&tree, &snapshot);
        assert!(report.is_clean());
        assert!(report.membership_matches());
        assert!(report.structural_match());
        assert!(report.active_matches);
        assert!(report.visible_order_matches);
        assert_eq!(report.expansion_mismatches, 0);
    }

    #[test]
    fn missing_member_in_external() {
        let tree = build_tree();
        let mut snapshot = matching_snapshot();
        snapshot.members.remove(&2);
        snapshot.visible.remove(&2);

        let report = compare(&tree, &snapshot);
        assert!(!report.membership_matches());
        assert_eq!(report.graph_tree_only, 1);
    }

    #[test]
    fn cold_members_not_flagged_as_divergence() {
        let tree = build_tree();
        // Member 3 is Cold — should NOT be flagged even if missing from external
        let mut snapshot = matching_snapshot();
        snapshot.members.remove(&3);

        let report = compare(&tree, &snapshot);
        // 3 is Cold, so not flagged as MissingFromExternal
        assert_eq!(report.graph_tree_only, 0);
    }

    #[test]
    fn topology_mismatch_detected() {
        let tree = build_tree();
        let mut snapshot = matching_snapshot();
        // Make 2 a root in external (parent = None) instead of child of 1
        snapshot.children.get_mut(&1).unwrap().retain(|&x| x != 2);

        let report = compare(&tree, &snapshot);
        assert!(!report.structural_match());
        assert_eq!(report.topology_mismatches, 1);
    }

    #[test]
    fn active_mismatch_detected() {
        let tree = build_tree();
        let mut snapshot = matching_snapshot();
        snapshot.active = Some(2);

        let report = compare(&tree, &snapshot);
        assert!(!report.active_matches);
    }

    #[test]
    fn visibility_mismatch_detected() {
        let tree = build_tree();
        let mut snapshot = matching_snapshot();
        snapshot.visible.remove(&2);

        let report = compare(&tree, &snapshot);
        assert_eq!(report.visibility_mismatches, 1);
    }

    #[test]
    fn active_none_in_external_not_flagged() {
        // When external.active = None, the active comparison is skipped
        // (transition phase: tile tree doesn't always report active).
        let tree = build_tree(); // GraphTree has active = Some(1)
        let mut snapshot = matching_snapshot();
        snapshot.active = None;

        let report = compare(&tree, &snapshot);
        assert!(
            report.active_matches,
            "None external active should not flag mismatch"
        );
    }

    #[test]
    fn expansion_mismatch_detected() {
        let mut tree = build_tree();
        // Expand node 1 in GraphTree.
        tree.apply(NavAction::ToggleExpand(1));

        let mut snapshot = matching_snapshot();
        // External says node 1 is NOT expanded.
        snapshot.expanded = HashSet::from([99u64]); // non-empty so check runs; 99 not in GT

        let report = compare(&tree, &snapshot);
        // GT has 1 expanded, external has only 99 → mismatch for 1
        assert!(report.expansion_mismatches > 0);
    }

    #[test]
    fn expansion_check_skipped_when_external_expanded_empty() {
        let mut tree = build_tree();
        tree.apply(NavAction::ToggleExpand(1));

        let snapshot = matching_snapshot(); // expanded is empty → skip check
        let report = compare(&tree, &snapshot);
        assert_eq!(report.expansion_mismatches, 0);
    }

    #[test]
    fn visible_order_mismatch_detected() {
        let tree = build_tree(); // visible order: [1, 2] (1 active+expanded→children shown)
        let mut snapshot = matching_snapshot();
        // External gives order [2, 1] — reversed.
        snapshot.visible_order = vec![2, 1];

        let report = compare(&tree, &snapshot);
        assert!(!report.visible_order_matches);
    }

    #[test]
    fn visible_order_check_skipped_when_external_order_empty() {
        let tree = build_tree();
        let snapshot = matching_snapshot(); // visible_order is empty → skip
        let report = compare(&tree, &snapshot);
        assert!(report.visible_order_matches);
    }
}
