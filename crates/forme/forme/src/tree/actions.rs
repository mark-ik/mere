// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::MemberId;
use crate::layout::LayoutMode;
use crate::lens::ProjectionLens;
use crate::member::{LayoutOverride, Lifecycle, MemberEntry, Provenance};
use crate::nav::{FocusCycleRegion, FocusDirection, NavAction, NavResult, TreeIntent};

use super::GraphTree;

impl<N: MemberId> GraphTree<N> {
    // ---------------------------------------------------------------
    // Navigation — apply()
    // ---------------------------------------------------------------

    /// Apply a navigation action. Returns intents for the host.
    pub fn apply(&mut self, action: NavAction<N>) -> NavResult<N> {
        match action {
            NavAction::Select(member) => self.apply_select(member),
            NavAction::Activate(member) => self.apply_activate(member),
            NavAction::Dismiss(member) => self.apply_dismiss(member),
            NavAction::ToggleExpand(member) => self.apply_toggle_expand(member),
            NavAction::Reveal(member) => self.apply_reveal(member),
            NavAction::Attach { member, provenance } => self.apply_attach(member, provenance),
            NavAction::Detach { member, recursive } => self.apply_detach(member, recursive),
            NavAction::Reparent { member, new_parent } => self.apply_reparent(member, new_parent),
            NavAction::Reorder { parent, new_order } => self.apply_reorder(parent, new_order),
            NavAction::SetLifecycle(member, lifecycle) => {
                self.apply_set_lifecycle(member, lifecycle)
            }
            NavAction::SetLayoutMode(mode) => self.apply_set_layout_mode(mode),
            NavAction::SetLens(lens) => self.apply_set_lens(lens),
            NavAction::CycleFocus(direction) => self.apply_cycle_focus(direction),
            NavAction::CycleFocusRegion(region) => self.apply_cycle_focus_region(region),
            NavAction::SetLayoutOverride(member, layout_override) => {
                self.apply_set_layout_override(member, layout_override)
            }
        }
    }

    // ---------------------------------------------------------------
    // Action implementations
    // ---------------------------------------------------------------

    fn apply_select(&mut self, member: N) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }
        self.active = Some(member.clone());
        NavResult::session(vec![TreeIntent::SelectionChanged(member)])
    }

    fn apply_activate(&mut self, member: N) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }
        if let Some(entry) = self.members.get_mut(&member) {
            entry.lifecycle = Lifecycle::Active;
        }
        self.active = Some(member.clone());
        NavResult::session(vec![
            TreeIntent::RequestActivation(member.clone()),
            TreeIntent::SelectionChanged(member),
        ])
    }

    fn apply_dismiss(&mut self, member: N) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }
        if let Some(entry) = self.members.get_mut(&member) {
            entry.lifecycle = Lifecycle::Cold;
        }
        // If the dismissed member was active, clear selection
        if self.active.as_ref() == Some(&member) {
            self.active = None;
        }
        NavResult::session(vec![TreeIntent::RequestDismissal(member)])
    }

    fn apply_toggle_expand(&mut self, member: N) -> NavResult<N> {
        if self.expanded.contains(&member) {
            self.expanded.remove(&member);
        } else {
            self.expanded.insert(member);
        }
        NavResult::session(Vec::new())
    }

    fn apply_reveal(&mut self, member: N) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }
        // Expand all ancestors
        let ancestors = self.topology.ancestors(&member);
        for ancestor in ancestors {
            self.expanded.insert(ancestor);
        }
        self.scroll_anchor = Some(member);
        NavResult::session(Vec::new())
    }

    fn apply_attach(&mut self, member: N, provenance: Provenance<N>) -> NavResult<N> {
        if self.contains(&member) {
            return NavResult::empty();
        }

        // Determine placement from provenance.
        // If the requested parent/sibling doesn't exist in the topology,
        // the topology method returns false and we fall back to root placement.
        let placed = match &provenance {
            Provenance::Traversal { source, .. } => {
                self.topology.attach_child(member.clone(), source)
            }
            Provenance::Manual {
                source: Some(source),
                ..
            } => self.topology.attach_sibling(member.clone(), source),
            Provenance::Derived {
                connection: Some(conn),
                ..
            } => self.topology.attach_sibling(member.clone(), conn),
            Provenance::AgentDerived {
                source: Some(source),
                ..
            } => self.topology.attach_sibling(member.clone(), source),
            _ => {
                // Anchor, Restored, Manual without source, Derived without connection
                self.topology.attach_root(member.clone())
            }
        };

        // If provenance-guided placement failed (e.g. source not in topology),
        // fall back to root placement so the member is always reachable.
        if !placed {
            self.topology.attach_root(member.clone());
        }

        let entry = MemberEntry::new(Lifecycle::Cold, provenance);
        self.members.insert(member.clone(), entry);

        NavResult::structural(vec![TreeIntent::MemberAttached(member)])
    }

    fn apply_detach(&mut self, member: N, recursive: bool) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }

        let mut intents = Vec::new();

        if recursive {
            let detached = self.topology.detach(&member);
            for node in &detached {
                self.members.remove(node);
                self.expanded.remove(node);
                intents.push(TreeIntent::MemberDetached(node.clone()));
            }
        } else {
            // Non-recursive: remove only this member, reparenting its children
            // to its parent (or promoting them to roots).
            let children: Vec<N> = self.topology.children_of(&member).to_vec();
            let parent = self.topology.parent_of(&member).cloned();

            // Remove only this single node from the topology. We can't use
            // topology.detach() here because it removes the entire subtree,
            // which would orphan grandchildren still in the members map.
            self.topology.detach_single(&member);
            self.members.remove(&member);
            self.expanded.remove(&member);

            // Re-attach children (with their full subtrees intact) to the
            // detached member's parent, or promote them to roots.
            // After detach_single, the children have no parent pointer and
            // aren't in insertion_order as children — we need to re-link them.
            for child in children {
                if let Some(ref p) = parent {
                    // Re-establish parent/child link directly.
                    self.topology.reattach_child(child, p);
                } else {
                    // Promote to root.
                    self.topology.promote_to_root(&child);
                }
            }

            intents.push(TreeIntent::MemberDetached(member.clone()));
        }

        // Clear active if it was detached
        if let Some(ref active) = self.active {
            if !self.contains(active) {
                self.active = None;
            }
        }

        NavResult::structural(intents)
    }

    fn apply_reparent(&mut self, member: N, new_parent: N) -> NavResult<N> {
        if !self.contains(&member) || !self.contains(&new_parent) {
            return NavResult::empty();
        }
        if self.topology.reparent(&member, &new_parent) {
            NavResult::structural(Vec::new())
        } else {
            // Rejected (cycle or self-reparent)
            NavResult::empty()
        }
    }

    fn apply_reorder(&mut self, parent: N, new_order: Vec<N>) -> NavResult<N> {
        if !self.contains(&parent) {
            return NavResult::empty();
        }
        self.topology.reorder_children(&parent, new_order);
        NavResult::structural(Vec::new())
    }

    fn apply_set_lifecycle(&mut self, member: N, lifecycle: Lifecycle) -> NavResult<N> {
        if let Some(entry) = self.members.get_mut(&member) {
            entry.lifecycle = lifecycle;
            NavResult::session(Vec::new())
        } else {
            NavResult::empty()
        }
    }

    fn apply_set_layout_override(
        &mut self,
        member: N,
        layout_override: LayoutOverride,
    ) -> NavResult<N> {
        if let Some(entry) = self.members.get_mut(&member) {
            entry.layout_override = Some(layout_override);
            NavResult::session(Vec::new())
        } else {
            NavResult::empty()
        }
    }

    fn apply_set_layout_mode(&mut self, mode: LayoutMode) -> NavResult<N> {
        self.layout_mode = mode;
        NavResult::session(vec![TreeIntent::LayoutModeChanged(mode)])
    }

    fn apply_set_lens(&mut self, lens: ProjectionLens) -> NavResult<N> {
        self.active_lens = lens.clone();
        NavResult::session(vec![TreeIntent::LensChanged(lens)])
    }

    fn apply_cycle_focus(&mut self, direction: FocusDirection) -> NavResult<N> {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return NavResult::empty();
        }

        let current_idx = self
            .active
            .as_ref()
            .and_then(|a| rows.iter().position(|r| r.member == *a));

        let next_idx = match (current_idx, direction) {
            (Some(idx), FocusDirection::Next) => (idx + 1) % rows.len(),
            (Some(idx), FocusDirection::Previous) => {
                if idx == 0 {
                    rows.len() - 1
                } else {
                    idx - 1
                }
            }
            (None, _) => 0,
        };

        let member = rows[next_idx].member.clone();
        self.active = Some(member.clone());
        NavResult::session(vec![TreeIntent::SelectionChanged(member)])
    }

    fn apply_cycle_focus_region(&mut self, region: FocusCycleRegion) -> NavResult<N> {
        let candidates: Vec<N> = match region {
            FocusCycleRegion::Roots => self.topology.roots().to_vec(),
            FocusCycleRegion::Branches => self
                .members
                .keys()
                .filter(|m| self.topology.has_children(m))
                .cloned()
                .collect(),
            FocusCycleRegion::Leaves => self
                .members
                .keys()
                .filter(|m| !self.topology.has_children(m))
                .cloned()
                .collect(),
        };

        if candidates.is_empty() {
            return NavResult::empty();
        }

        let current_idx = self
            .active
            .as_ref()
            .and_then(|a| candidates.iter().position(|c| c == a));

        let next_idx = match current_idx {
            Some(idx) => (idx + 1) % candidates.len(),
            None => 0,
        };

        let member = candidates[next_idx].clone();
        self.active = Some(member.clone());
        NavResult::session(vec![TreeIntent::SelectionChanged(member)])
    }
}
