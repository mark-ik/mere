/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The tiled-workbench model (S4): platen's canonical tiling state.
//!
//! The orrery and the tiled workbench are two **projections of one arrangement**
//! (the composition spine): the orrery is the [`ProjectionKind::Cartography`]
//! projection (graph members positioned spatially), the tiled workbench the
//! [`ProjectionKind::Tree`] one. This module owns the tiling state (which tiles are
//! open, how they group into tab-stacks, the active tab per stack, the projection
//! mode) and turns it into placed, content-resolved slots a host renders.
//!
//! A workbench is a list of slots laid side by side. A slot holds one or more graph
//! members: one member is a plain tile, several are a **tab-stack** (a tab strip,
//! one tab visible at a time). The model is geometry-free: it owns the slots, the
//! grouping, and the active tab; layout is the host's serval/taffy job (see
//! [`platen-view`](https://crates.io/crates/platen-view)), reading the structure
//! via [`Workbench::slot_views`].
//!
//! Replaces the legacy `FrameState` / `PaneBinding` frame model (the pre-spine
//! pane-binding workbench), per the 2026-06-04 platen taffy-retarget plan.

use forme::GraphMemberId;

use crate::ProjectionKind;

/// One workbench slot: a stack of one or more graph members sharing a column,
/// with `active` the index of the visible tab. A single member is a plain tile.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Slot {
    members: Vec<GraphMemberId>,
    active: usize,
}

impl Slot {
    fn single(member: GraphMemberId) -> Self {
        Self { members: vec![member], active: 0 }
    }

    /// The visible tab's member (the active one, or the first as a fallback).
    fn active_member(&self) -> Option<GraphMemberId> {
        self.members.get(self.active).or_else(|| self.members.first()).copied()
    }
}

/// A geometry-free view of one slot's structure: its members in order and which is
/// active. For projections (a11y, automation) that need the tab grouping without a
/// viewport or a graph to lay anything out.
#[derive(Clone, Copy, Debug)]
pub struct SlotView<'a> {
    pub members: &'a [GraphMemberId],
    pub active: usize,
}

/// The host's tiled-workbench composition: the slots and the projection mode.
/// Cartography (the orrery) is the default; the host flips to Tree for tiles.
#[derive(Clone, Debug)]
pub struct Workbench {
    mode: ProjectionKind,
    slots: Vec<Slot>,
}

impl Default for Workbench {
    fn default() -> Self {
        Self::new()
    }
}

impl Workbench {
    /// A new workbench: no slots, in Cartography (orrery) mode.
    pub fn new() -> Self {
        Self { mode: ProjectionKind::Cartography, slots: Vec::new() }
    }

    /// The current projection mode.
    pub fn mode(&self) -> ProjectionKind {
        self.mode
    }

    /// Whether the workbench is in the tiled (Tree) projection.
    pub fn is_tiled(&self) -> bool {
        matches!(self.mode, ProjectionKind::Tree)
    }

    /// Switch between the orrery (Cartography) and the tiled workbench (Tree),
    /// returning the new mode.
    pub fn toggle_mode(&mut self) -> ProjectionKind {
        self.mode = match self.mode {
            ProjectionKind::Cartography => ProjectionKind::Tree,
            ProjectionKind::Tree => ProjectionKind::Cartography,
        };
        self.mode
    }

    /// Set the projection mode explicitly.
    pub fn set_mode(&mut self, mode: ProjectionKind) {
        self.mode = mode;
    }

    /// Switch into the tiled (Tree) projection if not already there, returning
    /// whether the mode changed (so the caller can do entry work just once).
    pub fn ensure_tiled(&mut self) -> bool {
        if self.is_tiled() {
            false
        } else {
            self.mode = ProjectionKind::Tree;
            true
        }
    }

    /// Every open member, flattened across all slots (the host's reconcile needed
    /// set: every tab stays a warm actor, the active one is what renders).
    pub fn open_members(&self) -> Vec<GraphMemberId> {
        self.slots.iter().flat_map(|s| s.members.iter().copied()).collect()
    }

    /// How many tabs are open across all slots.
    pub fn tile_count(&self) -> usize {
        self.slots.iter().map(|s| s.members.len()).sum()
    }

    /// How many slots (side-by-side columns) are open.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// A geometry-free structural view of every slot, in order (each slot's members
    /// and active index). The projection-friendly read of the model, used by the
    /// a11y / automation tree without needing a viewport or a graph.
    pub fn slot_views(&self) -> impl Iterator<Item = SlotView<'_>> {
        self.slots.iter().map(|s| SlotView { members: &s.members, active: s.active })
    }

    /// Whether `member` is open in some slot.
    pub fn has_tile(&self, member: GraphMemberId) -> bool {
        self.slots.iter().any(|s| s.members.contains(&member))
    }

    /// Open `member` as a new single-tile slot (appended). A no-op if it is
    /// already open somewhere. Returns whether a new slot was added.
    pub fn open_tile(&mut self, member: GraphMemberId) -> bool {
        if self.has_tile(member) {
            return false;
        }
        self.slots.push(Slot::single(member));
        true
    }

    /// Open `members` as separate single-tile slots (each appended, de-duplicated);
    /// already-open members stay where they are. Returns how many were newly opened.
    pub fn open_split(&mut self, members: &[GraphMemberId]) -> usize {
        members.iter().filter(|&&m| self.open_tile(m)).count()
    }

    /// Open `members` as one tab-stack slot: gather them into a single new slot
    /// (first tab active), pulling any that were already open elsewhere into it so
    /// a member never sits in two slots. A no-op for an empty list.
    pub fn open_stack(&mut self, members: &[GraphMemberId]) {
        let mut stack: Vec<GraphMemberId> = Vec::new();
        for &m in members {
            if !stack.contains(&m) {
                stack.push(m);
            }
        }
        if stack.is_empty() {
            return;
        }
        for &m in &stack {
            self.detach(m);
        }
        self.slots.push(Slot { members: stack, active: 0 });
    }

    /// Close the tab showing `member`: drop it from its slot (removing the slot if
    /// it empties, clamping the active index otherwise). Returns whether one was
    /// removed.
    pub fn close_tile(&mut self, member: GraphMemberId) -> bool {
        if !self.has_tile(member) {
            return false;
        }
        self.detach(member);
        true
    }

    /// Remove `member` from whatever slot holds it, dropping the slot if it empties
    /// and clamping the active index otherwise. Shared by `close_tile` (a user
    /// close) and `open_stack` (re-homing a member into a new stack).
    fn detach(&mut self, member: GraphMemberId) {
        let Some(i) = self.slots.iter().position(|s| s.members.contains(&member)) else {
            return;
        };
        let slot = &mut self.slots[i];
        slot.members.retain(|m| *m != member);
        if slot.members.is_empty() {
            self.slots.remove(i);
        } else if slot.active >= slot.members.len() {
            slot.active = slot.members.len() - 1;
        }
    }

    /// Make `member` the active (visible) tab of its slot. Returns whether it was
    /// found.
    pub fn activate(&mut self, member: GraphMemberId) -> bool {
        for slot in &mut self.slots {
            if let Some(pos) = slot.members.iter().position(|m| *m == member) {
                slot.active = pos;
                return true;
            }
        }
        false
    }

    /// Collapse every open member into a single tab-stack (stack everything). The
    /// first member's tab stays active. A no-op below two members.
    pub fn stack_all(&mut self) {
        let members = self.open_members();
        if members.len() > 1 {
            self.slots = vec![Slot { members, active: 0 }];
        }
    }

    /// Split every tab into its own single-tile slot (the inverse of `stack_all`).
    pub fn split_all(&mut self) {
        let members = self.open_members();
        self.slots = members.into_iter().map(Slot::single).collect();
    }

    /// Close every slot (the projection mode is left unchanged).
    pub fn clear_tiles(&mut self) {
        self.slots.clear();
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn m(n: u128) -> GraphMemberId {
        Uuid::from_u128(n)
    }

    #[test]
    fn new_workbench_is_cartography_and_empty() {
        let wb = Workbench::new();
        assert_eq!(wb.mode(), ProjectionKind::Cartography);
        assert!(!wb.is_tiled());
        assert_eq!(wb.tile_count(), 0);
        assert_eq!(wb.slot_count(), 0);
    }

    #[test]
    fn open_tile_appends_single_slots_and_dedups() {
        let mut wb = Workbench::new();
        assert!(wb.open_tile(m(1)), "first open is new");
        assert!(wb.open_tile(m(2)), "a distinct member is new");
        assert!(!wb.open_tile(m(1)), "re-opening an open member is a no-op");
        assert_eq!(wb.open_members(), vec![m(1), m(2)]);
        assert_eq!(wb.slot_count(), 2, "two single-tile slots");
        assert_eq!(wb.tile_count(), 2);
    }

    #[test]
    fn stack_all_then_split_all_separates() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        wb.open_tile(m(3));
        wb.stack_all();
        assert_eq!(wb.slot_count(), 1, "all three collapse into one stack");
        assert_eq!(wb.tile_count(), 3, "all three tabs are still open");
        wb.split_all();
        assert_eq!(wb.slot_count(), 3, "each tab back to its own slot");
    }

    #[test]
    fn activate_switches_the_visible_tab_in_a_stack() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        wb.open_tile(m(3));
        wb.stack_all();
        // One stacked slot, first tab active by default (read structurally).
        {
            let view = wb.slot_views().next().unwrap();
            assert_eq!(view.members, &[m(1), m(2), m(3)], "three stacked tabs");
            assert_eq!(view.active, 0, "first tab active by default");
        }
        assert!(wb.activate(m(3)), "activating a member in the stack succeeds");
        assert_eq!(wb.slot_views().next().unwrap().active, 2, "the active tab switched");
    }

    #[test]
    fn open_split_opens_each_as_its_own_slot_and_dedups() {
        let mut wb = Workbench::new();
        assert_eq!(wb.open_split(&[m(1), m(2), m(3)]), 3, "all three are new");
        assert_eq!(wb.slot_count(), 3, "one slot per member");
        assert_eq!(wb.open_split(&[m(2), m(4)]), 1, "only the unseen member opens");
        assert_eq!(wb.slot_count(), 4);
    }

    #[test]
    fn open_stack_gathers_into_one_slot_and_rehomes_open_members() {
        let mut wb = Workbench::new();
        wb.open_split(&[m(1), m(2)]); // two single slots
        wb.open_stack(&[m(2), m(3)]); // m(2) is pulled out of its slot into the stack
        assert_eq!(wb.slot_count(), 2, "m(1)'s slot + the new [2,3] stack");
        assert_eq!(wb.tile_count(), 3, "no member is lost or duplicated");
        // The stack is the appended slot; activating m(3) proves they share it.
        assert!(wb.activate(m(3)));
        assert!(wb.has_tile(m(2)) && wb.has_tile(m(3)));
        // De-dup within the input: a repeated member appears once.
        let mut wb2 = Workbench::new();
        wb2.open_stack(&[m(7), m(7), m(8)]);
        assert_eq!(wb2.slot_count(), 1);
        assert_eq!(wb2.tile_count(), 2, "the repeat collapses");
    }

    #[test]
    fn ensure_tiled_switches_once() {
        let mut wb = Workbench::new();
        assert!(wb.ensure_tiled(), "first call flips Cartography → Tree");
        assert!(wb.is_tiled());
        assert!(!wb.ensure_tiled(), "already tiled → no change");
    }

    #[test]
    fn close_tab_drops_it_and_removes_an_empty_slot() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        wb.stack_all(); // one stack of [1, 2]
        assert!(wb.close_tile(m(1)), "closing a tab in the stack");
        assert_eq!(wb.tile_count(), 1, "the other tab remains");
        assert_eq!(wb.slot_count(), 1, "the slot survives with one tab");
        assert!(wb.close_tile(m(2)), "closing the last tab");
        assert_eq!(wb.slot_count(), 0, "the emptied slot is removed");
    }
}
