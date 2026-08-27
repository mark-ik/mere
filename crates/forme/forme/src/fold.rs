// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! View-local graph folds.
//!
//! A fold is curation, not a source-graph group. It records the source members
//! a projection replaces with one summary object; a renderer owns that summary
//! object's geometry and its boundary-relation routing.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::GraphMemberId;

/// First durable wire shape for a [`FoldRecord`]. Bump only with an explicit
/// migration; a persisted fold must never silently acquire new semantics.
pub const FOLD_RECORD_VERSION: u16 = 1;

/// Stable identity of one local fold, distinct from every source graph member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FoldId(Uuid);

impl FoldId {
    /// Mint a new fold identity for one curation action.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Rebuild a persisted fold id or create deterministic fixture data.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for FoldId {
    fn default() -> Self {
        Self::new()
    }
}

/// How relations crossing a fold boundary read in the current projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoldBoundaryPolicy {
    /// Internal cells disappear. Boundary cells attach to the summary and
    /// bundle only with cells of the same relation family.
    #[default]
    BundleByRelationFamily,
}

/// One durable, view-local fold.
///
/// `members` are normalized into UUID order so serialization, undo comparisons,
/// and boundary calculations agree. `source_scope` is opaque to forme: it lets
/// a host reject a fold from a different graph, history cursor, or lens without
/// giving this data crate ownership of those source concepts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldRecord {
    pub version: u16,
    pub id: FoldId,
    pub source_scope: String,
    pub members: Vec<GraphMemberId>,
    pub boundary_policy: FoldBoundaryPolicy,
}

impl FoldRecord {
    /// Build a fold from at least two source members. A one-member fold carries
    /// no grouping meaning and is rejected rather than becoming a disguised
    /// per-node visibility flag.
    pub fn from_selection(
        source_scope: impl Into<String>,
        members: impl IntoIterator<Item = GraphMemberId>,
    ) -> Option<Self> {
        let mut members: Vec<_> = members.into_iter().collect();
        members.sort();
        members.dedup();
        (members.len() >= 2).then(|| Self {
            version: FOLD_RECORD_VERSION,
            id: FoldId::new(),
            source_scope: source_scope.into(),
            members,
            boundary_policy: FoldBoundaryPolicy::BundleByRelationFamily,
        })
    }

    /// Whether this fold replaces `member` in its source projection.
    pub fn contains(&self, member: GraphMemberId) -> bool {
        self.members.binary_search(&member).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_fold_canonicalizes_members_without_becoming_graph_truth() {
        let fold = FoldRecord::from_selection(
            "graph:local",
            [Uuid::from_u128(3), Uuid::from_u128(1), Uuid::from_u128(3)],
        )
        .expect("two distinct members form a summary");
        assert_eq!(fold.version, FOLD_RECORD_VERSION);
        assert_eq!(fold.members, vec![Uuid::from_u128(1), Uuid::from_u128(3)]);
        assert!(fold.contains(Uuid::from_u128(1)));
        assert!(!fold.contains(Uuid::from_u128(2)));
        assert_eq!(
            fold.boundary_policy,
            FoldBoundaryPolicy::BundleByRelationFamily
        );
    }

    #[test]
    fn one_member_cannot_be_folded() {
        assert!(FoldRecord::from_selection("graph:local", [Uuid::from_u128(1)]).is_none());
    }

    #[test]
    fn fold_record_round_trips_through_json() {
        let fold =
            FoldRecord::from_selection("graph:local", [Uuid::from_u128(1), Uuid::from_u128(2)])
                .expect("two members");
        let json = serde_json::to_string(&fold).expect("serialize fold");
        let restored: FoldRecord = serde_json::from_str(&json).expect("deserialize fold");
        assert_eq!(restored, fold);
    }
}
