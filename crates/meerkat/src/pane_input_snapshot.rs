/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! A small read-only snapshot of per-window selection/view state that pane-data
//! builders can consume without reaching through the whole host context.

use std::collections::{HashMap, HashSet};

use mere::forme::GraphMemberId;
use mere::canvas::NodeState;
use session_runtime::memory_levels::EvictionPolicy;

use super::WindowCtx;

#[derive(Clone, Debug, Default)]
pub(crate) struct PaneInputSnapshot {
    focused_member: Option<GraphMemberId>,
    selected_members: HashSet<GraphMemberId>,
    open_members: HashSet<GraphMemberId>,
    node_states: HashMap<GraphMemberId, NodeState>,
    eviction_policy: EvictionPolicy,
    pending_compose_engram: Option<String>,
}

impl PaneInputSnapshot {
    pub(crate) fn focused_member(&self) -> Option<GraphMemberId> {
        self.focused_member
    }

    pub(crate) fn is_selected(&self, member: GraphMemberId) -> bool {
        self.selected_members.contains(&member)
    }

    pub(crate) fn is_open(&self, member: GraphMemberId) -> bool {
        self.open_members.contains(&member)
    }

    pub(crate) fn node_state(&self, member: GraphMemberId) -> NodeState {
        self.node_states
            .get(&member)
            .copied()
            .unwrap_or(NodeState::Idle)
    }

    pub(crate) fn eviction_policy(&self) -> EvictionPolicy {
        self.eviction_policy
    }

    pub(crate) fn pending_compose_engram(&self) -> Option<&str> {
        self.pending_compose_engram.as_deref()
    }
}

impl WindowCtx<'_> {
    pub(crate) fn pane_input_snapshot(&self) -> PaneInputSnapshot {
        PaneInputSnapshot {
            focused_member: self.focused_member(),
            selected_members: self.orrery().selected_members().into_iter().collect(),
            open_members: self.view.workbench.open_members().into_iter().collect(),
            node_states: self.node_states(),
            eviction_policy: self.shared.presentation.eviction_policy,
            pending_compose_engram: self.shared.presentation.pending_compose_engram.clone(),
        }
    }
}
