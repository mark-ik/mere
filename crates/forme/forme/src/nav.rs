// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::MemberId;
use crate::graphlet::GraphletId;
use crate::layout::LayoutMode;
use crate::lens::ProjectionLens;
use crate::member::{LayoutOverride, Lifecycle, Provenance};

/// Navigation actions — the verbs of tree interaction.
/// Aligned with NAVIGATOR.md §6 + lens switching + arrangement edges.
#[derive(Clone, Debug)]
pub enum NavAction<N: MemberId> {
    /// Set focus to a member without activating it.
    Select(N),
    /// Activate a member (lifecycle → Active, gains focus).
    Activate(N),
    /// Dismiss a member (lifecycle → Cold, removed from pane).
    Dismiss(N),
    /// Toggle expansion of a member's children in tree view.
    ToggleExpand(N),
    /// Ensure a member is visible (expand ancestors, scroll).
    Reveal(N),

    /// Attach a new member with placement derived from provenance.
    /// Traversal → child of source. Manual → sibling of connection.
    /// Derived → sibling of connection or child of anchor.
    Attach {
        member: N,
        provenance: Provenance<N>,
    },

    /// Detach a member (and optionally its subtree) from the tree.
    Detach { member: N, recursive: bool },

    /// Move a member to be a child of a new parent.
    Reparent { member: N, new_parent: N },

    /// Reorder children of a parent node.
    Reorder { parent: N, new_order: Vec<N> },

    /// Set a member's lifecycle state.
    SetLifecycle(N, Lifecycle),

    /// Switch layout mode.
    SetLayoutMode(LayoutMode),

    /// Switch projection lens.
    SetLens(ProjectionLens),

    /// Cycle focus to next/previous member.
    CycleFocus(FocusDirection),

    /// Cycle focus within a specific region.
    CycleFocusRegion(FocusCycleRegion),

    /// Update a member's layout override (split ratio, flex, direction).
    SetLayoutOverride(N, LayoutOverride),
}

/// Direction for focus cycling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDirection {
    Next,
    Previous,
}

/// Region constraint for focus cycling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusCycleRegion {
    /// Cycle among root nodes only.
    Roots,
    /// Cycle among branches (nodes with children).
    Branches,
    /// Cycle among leaves (nodes without children).
    Leaves,
}

/// Result of applying a navigation action.
#[derive(Clone, Debug)]
pub struct NavResult<N: MemberId> {
    /// Intents emitted for the host to act on.
    pub intents: Vec<TreeIntent<N>>,
    /// Whether the tree's topology changed.
    pub structure_changed: bool,
    /// Whether session state changed (focus, expansion, scroll).
    pub session_changed: bool,
}

impl<N: MemberId> NavResult<N> {
    pub fn empty() -> Self {
        Self {
            intents: Vec::new(),
            structure_changed: false,
            session_changed: false,
        }
    }

    pub fn session(intents: Vec<TreeIntent<N>>) -> Self {
        Self {
            intents,
            structure_changed: false,
            session_changed: true,
        }
    }

    pub fn structural(intents: Vec<TreeIntent<N>>) -> Self {
        Self {
            intents,
            structure_changed: true,
            session_changed: true,
        }
    }
}

/// Intents emitted by the tree for the host to handle.
/// The tree doesn't own activation or rendering — it requests them.
#[derive(Clone, Debug)]
pub enum TreeIntent<N: MemberId> {
    /// Active selection changed.
    SelectionChanged(N),
    /// Request the host to activate (render, give resources to) a member.
    RequestActivation(N),
    /// Request the host to dismiss (deactivate, free resources from) a member.
    RequestDismissal(N),
    /// Request the host to focus a member's content.
    RequestFocus(N),
    /// A graphlet needs reconciliation with its spec.
    ReconciliationNeeded {
        graphlet: GraphletId,
        reason: String,
    },
    /// The projection lens changed — host may update edge visibility.
    LensChanged(ProjectionLens),
    /// The layout mode changed.
    LayoutModeChanged(LayoutMode),
    /// A member was attached to the tree.
    MemberAttached(N),
    /// A member was detached from the tree.
    MemberDetached(N),
}
