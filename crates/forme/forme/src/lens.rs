// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

/// A lens controls which topology drives the visible tree hierarchy.
/// This replaces Navigator's separate section model — sections become
/// lenses over the same GraphTree.
///
/// The underlying membership and topology don't change; the lens
/// controls which edges drive parent-child and how members are grouped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectionLens {
    /// Traversal-first: parent-child from navigation history.
    /// "I opened B from A" -> B is child of A.
    /// Natural tree-style-tabs view. **Default.**
    Traversal,

    /// Arrangement-first: group by graphlet -> frame -> tab group.
    /// The workbench-scope view.
    Arrangement,

    /// Containment-first: group by origin/domain -> url-path -> member.
    /// Derived from Containment family edges (domain, url-path).
    /// Good for origin-based lifecycle management and cleanup.
    Containment,

    /// Semantic-first: group by UserGrouped/AgentDerived relations.
    Semantic,

    /// Recency-first: ordered by last-touched timestamp.
    Recency,

    /// All members: flat with optional graphlet grouping.
    All,
}

impl Default for ProjectionLens {
    fn default() -> Self {
        Self::Traversal
    }
}

impl ProjectionLens {
    /// The relation-family edge tags that drive parent-child for this lens.
    pub fn primary_edge_families(&self) -> &[&str] {
        match self {
            Self::Traversal => &["traversal", "navigation-history"],
            Self::Arrangement => &["frame-member", "tile-member", "tab-neighbor"],
            Self::Containment => &["domain", "url-path", "user-folder"],
            Self::Semantic => &["user-grouped", "agent-derived"],
            Self::Recency => &[],
            Self::All => &[],
        }
    }
}
