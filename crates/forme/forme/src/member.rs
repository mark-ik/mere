// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::MemberId;
use crate::graphlet::GraphletId;
use serde::{Deserialize, Serialize};

/// What each member of the tree carries.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct MemberEntry<N: MemberId> {
    /// Display lifecycle. The tree does NOT own transitions —
    /// it receives them from the host via `SetLifecycle`.
    pub lifecycle: Lifecycle,

    /// Why this member is in the tree. Maps to Provenance family
    /// edge sub-kinds. Preserved for reconciliation and undo.
    pub provenance: Provenance<N>,

    /// Which graphlet(s) this member belongs to.
    pub graphlet_membership: Vec<GraphletId>,

    /// Optional taffy layout overrides (min size, flex grow, etc.).
    pub layout_override: Option<LayoutOverride>,
}

impl<N: MemberId> MemberEntry<N> {
    pub fn new(lifecycle: Lifecycle, provenance: Provenance<N>) -> Self {
        Self {
            lifecycle,
            provenance,
            graphlet_membership: Vec::new(),
            layout_override: None,
        }
    }

    pub fn with_graphlet(mut self, id: GraphletId) -> Self {
        self.graphlet_membership.push(id);
        self
    }

    pub fn with_layout_override(mut self, lo: LayoutOverride) -> Self {
        self.layout_override = Some(lo);
        self
    }

    pub fn is_active(&self) -> bool {
        self.lifecycle == Lifecycle::Active
    }

    pub fn is_warm(&self) -> bool {
        self.lifecycle == Lifecycle::Warm
    }

    pub fn is_cold(&self) -> bool {
        self.lifecycle == Lifecycle::Cold
    }

    pub fn is_visible_in_pane(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Active | Lifecycle::Warm)
    }
}

/// Node lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lifecycle {
    /// Open in a pane, rendering, may have focus.
    Active,
    /// Has runtime state, not focused.
    Warm,
    /// In the graph view but not in a pane.
    Cold,
}

/// Why this member is in the tree. Aligned with Provenance family
/// and arrangement edge sub-kinds from `graph_relation_families.md`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub enum Provenance<N: MemberId> {
    /// Opened by following a link/edge from another member.
    /// Maps to Traversal family edge.
    Traversal {
        source: N,
        edge_kind: Option<String>,
    },
    /// Manually added by user action (drag, command palette, import).
    /// Maps to UserGrouped family edge.
    Manual {
        source: Option<N>,
        context: Option<String>,
    },
    /// Present as a graphlet anchor or graph view root.
    Anchor,
    /// Derived by graphlet computation (component, ego, corridor, etc.).
    /// Placed as sibling of its connection point in the topology.
    Derived {
        connection: Option<N>,
        derivation: String,
    },
    /// Agent-inferred (AI enrichment). Carries confidence + decay.
    /// Maps to AgentDerived family edge.
    AgentDerived {
        confidence: f32,
        agent: String,
        source: Option<N>,
    },
    /// Restored from persistence.
    Restored,
}

/// Taffy-compatible layout overrides per member.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutOverride {
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub preferred_split: Option<SplitDirection>,
    /// User-set split proportion (0.0–1.0). When present, overrides flex_grow
    /// for taffy sizing by setting `flex_basis: Percent(ratio * 100)`.
    /// Produced by split-handle drag interactions.
    pub split_ratio: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}
