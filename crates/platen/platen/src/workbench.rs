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
//! one tab visible at a time). Geometry comes from [`project_tree`] + [`layout_plan`]:
//! one tile-intent per slot yields the side-by-side rects, then the slot carves its
//! own strip off the top and resolves its members to URLs. (One rect per slot, not
//! platen's `Tabs` slot, so the tab set + active index stay the model's.)
//!
//! Replaces the legacy `FrameState` / `PaneBinding` frame model (the pre-spine
//! pane-binding workbench), per the 2026-06-04 platen taffy-retarget plan.

use forme::{Arrangement, GraphMemberId};
use kernel::geometry::{PortablePoint, PortableRect, PortableSize};
use kernel::graph::Graph;

use crate::{layout_plan, project_tree, LaidOutSlot, LayoutConfig, ProjectionKind};

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

/// One tab within a placed slot: its graph member + resolved URL.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedTab {
    pub member: GraphMemberId,
    /// The node's URL, or `None` if the member has left the graph.
    pub url: Option<String>,
}

/// A placed workbench slot: where its active tab's content renders, its tab
/// strip, and the tabs themselves (one for a plain tile, several for a stack).
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedSlot {
    /// Where the active tab's content renders (below the strip).
    pub content: PortableRect,
    /// The tab-strip band across the slot's top.
    pub strip: PortableRect,
    /// The slot's tabs, in order.
    pub tabs: Vec<PlacedTab>,
    /// Index into `tabs` of the visible tab.
    pub active: usize,
}

impl PlacedSlot {
    /// The active tab, if the slot has any.
    pub fn active_tab(&self) -> Option<&PlacedTab> {
        self.tabs.get(self.active)
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

    /// Collapse every open member into a single tab-stack (group everything). The
    /// first member's tab stays active. A no-op below two members.
    pub fn group_all(&mut self) {
        let members = self.open_members();
        if members.len() > 1 {
            self.slots = vec![Slot { members, active: 0 }];
        }
    }

    /// Split every tab into its own single-tile slot (the inverse of `group_all`).
    pub fn split_all(&mut self) {
        let members = self.open_members();
        self.slots = members.into_iter().map(Slot::single).collect();
    }

    /// Close every slot (the projection mode is left unchanged).
    pub fn clear_tiles(&mut self) {
        self.slots.clear();
    }

    /// The forme arrangement: one root-attached tile-intent per slot (the slot's
    /// active member), so `project_tree` yields one split slot per column. The
    /// stack's tabs are ours, carved off the slot rect, not platen's `Tabs`.
    fn arrangement(&self) -> Arrangement {
        let mut arrangement = Arrangement::new();
        for slot in &self.slots {
            arrangement.add_tile_intent(slot.active_member());
        }
        arrangement
    }

    /// Lay the slots out side by side inside `viewport` and resolve each tab's URL
    /// via `graph`. Each slot carves a `tab_strip_height` strip off its top for the
    /// tab strip; its active tab's content renders below. Band-relative rects.
    pub fn laid_out_slots(
        &self,
        viewport: PortableSize,
        config: &LayoutConfig,
        graph: &Graph,
    ) -> Vec<PlacedSlot> {
        let plan = project_tree(&self.arrangement());
        let laid = layout_plan(&plan, viewport, config);
        laid
            .slots
            .into_iter()
            .zip(self.slots.iter())
            .map(|(laid_slot, slot)| {
                let rect = slot_rect(&laid_slot);
                let strip_h = config.tab_strip_height.min(rect.size.height);
                let strip = PortableRect::new(
                    rect.origin,
                    PortableSize::new(rect.size.width, strip_h),
                );
                let content = PortableRect::new(
                    PortablePoint::new(rect.origin.x, rect.origin.y + strip_h),
                    PortableSize::new(rect.size.width, (rect.size.height - strip_h).max(1.0)),
                );
                let tabs = slot
                    .members
                    .iter()
                    .map(|&member| PlacedTab {
                        member,
                        url: graph.get_node_by_id(member).map(|(_, node)| node.url().to_string()),
                    })
                    .collect();
                PlacedSlot { content, strip, tabs, active: slot.active }
            })
            .collect()
    }
}

/// The rect of a laid-out slot (a plain Tile, since our arrangement never stacks).
fn slot_rect(slot: &LaidOutSlot) -> PortableRect {
    match slot {
        LaidOutSlot::Tile { rect, .. } => *rect,
        LaidOutSlot::Tabs { rect, .. } => *rect,
    }
}

#[cfg(test)]
mod tests {
    use kernel::geometry::PortablePoint;
    use uuid::Uuid;

    use super::*;

    fn m(n: u128) -> GraphMemberId {
        Uuid::from_u128(n)
    }

    fn band() -> PortableSize {
        PortableSize::new(1000.0, 600.0)
    }

    /// A graph with one node per `(uuid, url)`, positioned at the origin.
    fn graph_with(nodes: &[(Uuid, &str)]) -> Graph {
        let mut graph = Graph::new();
        for (id, url) in nodes {
            graph.add_node_with_id(*id, (*url).to_string(), PortablePoint::new(0.0, 0.0));
        }
        graph
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
    fn group_all_stacks_then_split_all_separates() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        wb.open_tile(m(3));
        wb.group_all();
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
        wb.group_all();
        let graph = graph_with(&[
            (m(1), "https://a.example"),
            (m(2), "https://b.example"),
            (m(3), "https://c.example"),
        ]);
        let slots = wb.laid_out_slots(band(), &LayoutConfig::default(), &graph);
        assert_eq!(slots.len(), 1, "one stacked slot");
        assert_eq!(slots[0].tabs.len(), 3, "three tabs");
        assert_eq!(slots[0].active, 0, "first tab active by default");
        assert_eq!(slots[0].active_tab().unwrap().member, m(1));

        assert!(wb.activate(m(3)), "activating a member in the stack succeeds");
        let slots = wb.laid_out_slots(band(), &LayoutConfig::default(), &graph);
        assert_eq!(slots[0].active, 2, "the active tab switched");
        assert_eq!(slots[0].active_tab().unwrap().member, m(3));
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
        wb.group_all(); // one stack of [1, 2]
        assert!(wb.close_tile(m(1)), "closing a tab in the stack");
        assert_eq!(wb.tile_count(), 1, "the other tab remains");
        assert_eq!(wb.slot_count(), 1, "the slot survives with one tab");
        assert!(wb.close_tile(m(2)), "closing the last tab");
        assert_eq!(wb.slot_count(), 0, "the emptied slot is removed");
    }

    #[test]
    fn slots_split_the_band_with_a_strip_carved_off_each() {
        let graph = graph_with(&[(m(1), "https://a.example"), (m(2), "https://b.example")]);
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        let cfg = LayoutConfig { gap: 10.0, tab_strip_height: 28.0 };
        let slots = wb.laid_out_slots(PortableSize::new(810.0, 600.0), &cfg, &graph);

        assert_eq!(slots.len(), 2, "one slot per open tile");
        // (810 - 10 gap) / 2 = 400 wide each; the second begins at 410.
        assert_eq!(slots[0].strip.origin.x, 0.0);
        assert_eq!(slots[0].strip.size, PortableSize::new(400.0, 28.0), "strip across the top");
        assert_eq!(slots[0].content.origin, PortablePoint::new(0.0, 28.0), "content below the strip");
        assert_eq!(slots[0].content.size, PortableSize::new(400.0, 572.0));
        assert_eq!(slots[0].tabs[0].url.as_deref(), Some("https://a.example"));
        assert_eq!(slots[1].strip.origin.x, 410.0);
        assert_eq!(slots[1].tabs[0].url.as_deref(), Some("https://b.example"));
    }

    #[test]
    fn no_slots_lays_out_nothing() {
        let wb = Workbench::new();
        assert!(wb.laid_out_slots(band(), &LayoutConfig::default(), &graph_with(&[])).is_empty());
    }
}
