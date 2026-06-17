/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The tiled-workbench model (S4): platen's canonical tiling state.
//!
//! The orrery and the tiled workbench are two **projections of one arrangement**
//! (the composition spine): the orrery is the [`ProjectionKind::Cartography`]
//! projection (graph members positioned spatially), the tiled workbench the
//! [`ProjectionKind::Tree`] one. This module owns the tiling state (the split tree of
//! tab-stacks, the active tab per stack, the projection mode) and turns it into placed,
//! content-resolved tiles a host renders.
//!
//! The workbench is a **recursive split tree** (see [`tree`]): a leaf is a tab-stack
//! (one or more members, one visible), and a split lays its children along an axis (a
//! `Row` side-by-side, a `Column` top-to-bottom), each with a fractional share. Splits
//! nest, so the tree expresses every variation: horizontal, vertical, and combinations.
//! It is geometry-free; layout is the host's serval/taffy job, reached through
//! [`Workbench::to_tile_tree`] (the pelt surface) and [`Workbench::slot_views`] (the
//! a11y / automation projection).
//!
//! Replaces the legacy `FrameState` / `PaneBinding` frame model (the pre-spine
//! pane-binding workbench), per the 2026-06-04 platen taffy-retarget plan.

use forme::GraphMemberId;
use pelt_core::tile::SplitAxis;

use crate::ProjectionKind;

mod bridge;
mod tree;
use tree::{Branch, Pane, Stack};

/// A geometry-free view of one leaf stack: its members in order and which is active.
/// For projections (a11y, automation) that need the tab grouping without a viewport or
/// a graph to lay anything out. The tree is flattened to its leaves here, in order.
#[derive(Clone, Copy, Debug)]
pub struct SlotView<'a> {
    pub members: &'a [GraphMemberId],
    pub active: usize,
    /// The leaf's fractional share of its immediate parent split (1.0 for a lone root).
    pub weight: f32,
}

/// The host's tiled-workbench composition: the split tree and the projection mode.
/// Cartography (the orrery) is the default; the host flips to Tree for tiles.
#[derive(Clone, Debug)]
pub struct Workbench {
    mode: ProjectionKind,
    root: Option<Pane>,
}

impl Default for Workbench {
    fn default() -> Self {
        Self::new()
    }
}

impl Workbench {
    /// A new workbench: empty, in Cartography (orrery) mode.
    pub fn new() -> Self {
        Self { mode: ProjectionKind::Cartography, root: None }
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

    /// Switch into the tiled (Tree) projection if not already there, returning whether
    /// the mode changed (so the caller can do entry work just once).
    pub fn ensure_tiled(&mut self) -> bool {
        if self.is_tiled() {
            false
        } else {
            self.mode = ProjectionKind::Tree;
            true
        }
    }

    /// Every open member, flattened across the tree left-to-right / top-to-bottom (the
    /// host's reconcile needed set: every tab stays a warm actor, the active one
    /// renders).
    pub fn open_members(&self) -> Vec<GraphMemberId> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            root.collect_members(&mut out);
        }
        out
    }

    /// How many tabs are open across the tree.
    pub fn tile_count(&self) -> usize {
        self.open_members().len()
    }

    /// How many leaf stacks (cells) are open.
    pub fn slot_count(&self) -> usize {
        self.root.as_ref().map_or(0, Pane::leaf_count)
    }

    /// A flattened, in-order view of every leaf stack (members + active tab). The
    /// projection-friendly read of the model for the a11y / automation tree.
    pub fn slot_views(&self) -> impl Iterator<Item = SlotView<'_>> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            collect_slots(root, 1.0, &mut out);
        }
        out.into_iter()
    }

    /// The top-level split's child fractions, in order — a top-level divider drag's
    /// snapshot. Empty when the root is a lone stack (no top-level divider). For nested
    /// dividers use [`split_fractions`](Self::split_fractions).
    pub fn weights(&self) -> Vec<f32> {
        self.split_fractions(&[]).unwrap_or_default()
    }

    /// Set the top-level split's child fractions (clamped, renormalized) — a top-level
    /// divider drag. A no-op when the root is not a split.
    pub fn set_weights(&mut self, weights: &[f32]) {
        self.set_split_fractions(&[], weights);
    }

    /// The child fractions of the split addressed by `path` (the child index taken at
    /// each level from the root), or `None` if `path` does not land on a split. The
    /// per-split divider read (a nested divider carries its split's path).
    pub fn split_fractions(&self, path: &[usize]) -> Option<Vec<f32>> {
        self.root.as_ref().and_then(|r| r.fractions_at(path))
    }

    /// Set the child fractions of the split at `path` (clamped, renormalized). Returns
    /// whether the split was found. The per-split divider write.
    pub fn set_split_fractions(&mut self, path: &[usize], fractions: &[f32]) -> bool {
        self.root.as_mut().is_some_and(|r| r.set_fractions_at(path, fractions))
    }

    /// Whether `member` is open somewhere in the tree.
    pub fn has_tile(&self, member: GraphMemberId) -> bool {
        self.root.as_ref().is_some_and(|r| r.contains(member))
    }

    /// Open `member` as a new top-level column (a fresh single-tile cell appended along
    /// the Row). A no-op if it is already open. Returns whether one was added.
    pub fn open_tile(&mut self, member: GraphMemberId) -> bool {
        if self.has_tile(member) {
            return false;
        }
        self.push_column(Pane::leaf(member));
        true
    }

    /// Open `members` as separate top-level columns (each appended, de-duplicated);
    /// already-open members stay put. Returns how many were newly opened.
    pub fn open_split(&mut self, members: &[GraphMemberId]) -> usize {
        members.iter().filter(|&&m| self.open_tile(m)).count()
    }

    /// Open `members` as one new tab-stack column: gather them into a single appended
    /// cell (first tab active), pulling any already open elsewhere into it so a member
    /// never sits in two cells. A no-op for an empty list.
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
        self.push_column(Pane::Stack(Stack { members: stack, active: 0 }));
    }

    /// Open `member` as a new tab in the cell holding `target`, made the active tab —
    /// "stack into the target cell." Returns `false` if `target` is not open (the caller
    /// falls back to [`open_tile`]). A member open elsewhere is pulled in, not
    /// duplicated. `member == target` just activates it.
    pub fn open_in_slot_of(&mut self, member: GraphMemberId, target: GraphMemberId) -> bool {
        if member == target {
            return self.activate(member);
        }
        if self.has_tile(member) {
            self.detach(member);
        }
        self.root.as_mut().is_some_and(|r| r.stack_into(member, target))
    }

    /// Close the tab showing `member`: drop it from its cell (collapsing an emptied cell
    /// and any single-child split it leaves behind). Returns whether one was removed.
    pub fn close_tile(&mut self, member: GraphMemberId) -> bool {
        if !self.has_tile(member) {
            return false;
        }
        self.detach(member);
        true
    }

    /// Make `member` the active (visible) tab of its cell. Returns whether it was found.
    pub fn activate(&mut self, member: GraphMemberId) -> bool {
        self.root.as_mut().is_some_and(|r| r.activate(member))
    }

    /// Drag-drop `dragged` onto `target`'s cell (stack it there, made active). Within
    /// the same cell it reorders to just after `target`. Returns whether anything moved.
    /// A no-op when `dragged == target` or either is not open.
    pub fn move_to_slot_of(&mut self, dragged: GraphMemberId, target: GraphMemberId) -> bool {
        if dragged == target || !self.has_tile(dragged) || !self.has_tile(target) {
            return false;
        }
        self.detach(dragged);
        self.root.as_mut().is_some_and(|r| r.stack_into(dragged, target))
    }

    /// Split `dragged` out as its own cell beside `target`, along the **Row** axis, on
    /// the left (`after == false`) or right (`after == true`) — the back-compatible
    /// horizontal split. See [`split_beside_axis`](Self::split_beside_axis) for vertical
    /// and nesting.
    pub fn split_beside(&mut self, dragged: GraphMemberId, target: GraphMemberId, after: bool) -> bool {
        self.split_beside_axis(dragged, target, SplitAxis::Row, after)
    }

    /// Split `dragged` out as its own cell beside `target` along `axis` (a `Row` puts it
    /// left/right, a `Column` above/below), on the `after` side. When the split holding
    /// `target` already runs along `axis` the new cell extends it; otherwise it nests a
    /// fresh `axis` split there. Returns whether it moved. A no-op when `dragged ==
    /// target` or either is not open.
    pub fn split_beside_axis(
        &mut self,
        dragged: GraphMemberId,
        target: GraphMemberId,
        axis: SplitAxis,
        after: bool,
    ) -> bool {
        if dragged == target || !self.has_tile(dragged) || !self.has_tile(target) {
            return false;
        }
        self.detach(dragged);
        self.root.as_mut().is_some_and(|r| r.split_beside(dragged, target, axis, after))
    }

    /// Split `dragged` out of its current cell into a fresh cell beside that cell along
    /// `axis`, on the `after` side — a tab dragged onto an edge of its **own** cell.
    /// Anchors on another member of the cell, so a no-op when `dragged` is already alone
    /// in its cell (it is its own cell). Returns whether it moved.
    pub fn split_out(&mut self, dragged: GraphMemberId, axis: SplitAxis, after: bool) -> bool {
        let Some(anchor) = self.cell_sibling(dragged) else {
            return false;
        };
        self.split_beside_axis(dragged, anchor, axis, after)
    }

    /// A different member sharing `member`'s cell, if any (the split-out anchor).
    fn cell_sibling(&self, member: GraphMemberId) -> Option<GraphMemberId> {
        self.root.as_ref().and_then(|r| r.sibling_in_stack(member))
    }

    /// Collapse every open member into a single tab-stack (stack everything). The first
    /// member's tab stays active. A no-op below two members.
    pub fn stack_all(&mut self) {
        let members = self.open_members();
        if members.len() > 1 {
            self.root = Some(Pane::Stack(Stack { members, active: 0 }));
        }
    }

    /// Split every tab into its own top-level column (the inverse of `stack_all`).
    pub fn split_all(&mut self) {
        let members = self.open_members();
        self.root = match members.len() {
            0 => None,
            1 => Some(Pane::leaf(members[0])),
            n => {
                let frac = 1.0 / n as f32;
                Some(Pane::Split {
                    axis: SplitAxis::Row,
                    children: members
                        .into_iter()
                        .map(|m| Branch { fraction: frac, pane: Pane::leaf(m) })
                        .collect(),
                })
            }
        };
    }

    /// Close every cell (the projection mode is left unchanged).
    pub fn clear_tiles(&mut self) {
        self.root = None;
    }

    /// Project this workbench onto pelt's [`TileTree`](pelt_core::tile::TileTree)
    /// contract (V5/V6) — the builder meerkat uses to render the workbench through the
    /// standalone pelt tile surface. The **structure** is the split tree (nested
    /// `Row`/`Column` splits with their fractions, each leaf a stack with its active
    /// tab); the host supplies `tile_for`, resolving each member to its
    /// [`Tile`](pelt_core::tile::Tile). An empty workbench yields `None`. A projection,
    /// never a second authority: the workbench stays the tiling truth, and the surface
    /// is driven entirely through the contract (the host applies tile events back and
    /// re-projects).
    pub fn to_tile_tree(
        &self,
        mut tile_for: impl FnMut(GraphMemberId) -> pelt_core::tile::Tile,
    ) -> Option<pelt_core::tile::TileTree> {
        self.root.as_ref().map(|r| r.to_tile_tree(&mut tile_for))
    }

    /// Append `pane` as a new top-level column (a Row child), wrapping the existing root
    /// in a Row split when it is not already one. Preserves the existing columns'
    /// relative shares; the newcomer takes an equal share of the row.
    fn push_column(&mut self, pane: Pane) {
        match self.root.take() {
            None => self.root = Some(pane),
            Some(Pane::Split { axis: SplitAxis::Row, mut children }) => {
                let frac = 1.0 / (children.len() + 1) as f32;
                for b in &mut children {
                    b.fraction *= 1.0 - frac;
                }
                children.push(Branch { fraction: frac, pane });
                self.root = Some(Pane::Split { axis: SplitAxis::Row, children });
            }
            Some(other) => {
                self.root = Some(Pane::Split {
                    axis: SplitAxis::Row,
                    children: vec![
                        Branch { fraction: 0.5, pane: other },
                        Branch { fraction: 0.5, pane },
                    ],
                });
            }
        }
    }

    /// Remove `member` from its cell, dropping an emptied root.
    fn detach(&mut self, member: GraphMemberId) {
        if let Some(root) = &mut self.root {
            root.remove(member);
            if root.is_empty() {
                self.root = None;
            }
        }
    }
}

/// Walk the tree's leaves in order, pushing a [`SlotView`] per stack with its parent
/// fraction as the weight.
fn collect_slots<'a>(pane: &'a Pane, weight: f32, out: &mut Vec<SlotView<'a>>) {
    match pane {
        Pane::Stack(s) => out.push(SlotView { members: &s.members, active: s.active, weight }),
        Pane::Split { children, .. } => {
            for b in children {
                collect_slots(&b.pane, b.fraction, out);
            }
        }
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
        assert_eq!(wb.slot_count(), 2, "two single-tile columns");
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
        assert_eq!(wb.slot_count(), 3, "each tab back to its own column");
    }

    #[test]
    fn activate_switches_the_visible_tab_in_a_stack() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        wb.open_tile(m(3));
        wb.stack_all();
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
        assert_eq!(wb.slot_count(), 3, "one column per member");
        assert_eq!(wb.open_split(&[m(2), m(4)]), 1, "only the unseen member opens");
        assert_eq!(wb.slot_count(), 4);
    }

    #[test]
    fn open_stack_gathers_into_one_slot_and_rehomes_open_members() {
        let mut wb = Workbench::new();
        wb.open_split(&[m(1), m(2)]); // two single columns
        wb.open_stack(&[m(2), m(3)]); // m(2) is pulled out of its column into the stack
        assert_eq!(wb.slot_count(), 2, "m(1)'s column + the new [2,3] stack");
        assert_eq!(wb.tile_count(), 3, "no member is lost or duplicated");
        assert!(wb.activate(m(3)));
        assert!(wb.has_tile(m(2)) && wb.has_tile(m(3)));
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
        wb.stack_all();
        assert!(wb.close_tile(m(1)), "closing a tab in the stack");
        assert_eq!(wb.tile_count(), 1, "the other tab remains");
        assert_eq!(wb.slot_count(), 1, "the cell survives with one tab");
        assert!(wb.close_tile(m(2)), "closing the last tab");
        assert_eq!(wb.slot_count(), 0, "the emptied cell is removed");
    }

    #[test]
    fn move_to_slot_of_moves_across_and_reorders_within() {
        let mut wb = Workbench::new();
        wb.open_split(&[m(1), m(2), m(3)]);
        assert!(wb.move_to_slot_of(m(1), m(3)));
        assert_eq!(wb.slot_count(), 2, "m(1)'s column emptied + dropped");
        assert_eq!(wb.tile_count(), 3, "no tab lost");
        let stack = wb.slot_views().find(|s| s.members.contains(&m(1))).unwrap();
        assert_eq!(stack.members, &[m(3), m(1)], "m(1) appended to m(3)'s cell");
        assert_eq!(stack.members[stack.active], m(1), "the dragged tab is active");
        assert!(wb.move_to_slot_of(m(3), m(1)));
        let stack = wb.slot_views().find(|s| s.members.contains(&m(3))).unwrap();
        assert_eq!(stack.members, &[m(1), m(3)], "m(3) reordered to after m(1)");
        assert!(!wb.move_to_slot_of(m(1), m(1)));
        assert!(!wb.move_to_slot_of(m(99), m(1)));
    }

    #[test]
    fn open_in_slot_of_stacks_a_new_tab_and_activates_it() {
        let mut wb = Workbench::new();
        wb.open_split(&[m(1), m(2)]);
        assert!(wb.open_in_slot_of(m(3), m(1)));
        assert_eq!(wb.slot_count(), 2, "no new column — it joined m(1)'s");
        let stack = wb.slot_views().find(|s| s.members.contains(&m(3))).unwrap();
        assert_eq!(stack.members, &[m(1), m(3)], "appended to m(1)'s cell");
        assert_eq!(stack.members[stack.active], m(3), "the new tab is active");
        assert!(wb.open_in_slot_of(m(2), m(1)));
        assert_eq!(wb.tile_count(), 3, "m(2) moved, not copied");
        assert_eq!(wb.slot_count(), 1, "m(2)'s old column emptied + dropped");
        assert!(!wb.open_in_slot_of(m(9), m(404)));
    }

    #[test]
    fn split_beside_pulls_a_tab_into_its_own_slot() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        wb.stack_all(); // one stack [1, 2]
        assert_eq!(wb.slot_count(), 1);
        assert!(wb.split_beside(m(2), m(1), true));
        assert_eq!(wb.slot_count(), 2, "m(2) is its own column now");
        assert_eq!(wb.tile_count(), 2);
        let members: Vec<_> = wb.slot_views().map(|s| s.members.to_vec()).collect();
        assert_eq!(members, vec![vec![m(1)], vec![m(2)]]);
        assert!(wb.split_beside(m(1), m(2), false));
        let members: Vec<_> = wb.slot_views().map(|s| s.members.to_vec()).collect();
        assert_eq!(members, vec![vec![m(1)], vec![m(2)]], "m(1) inserted before m(2)");
        assert!(!wb.split_beside(m(1), m(1), true), "self is a no-op");
    }

    #[test]
    fn split_beside_axis_makes_a_vertical_split() {
        let mut wb = Workbench::new();
        wb.open_split(&[m(1), m(2)]); // [1 | 2] (top-level Row)
        // Split m(2) below m(1) along Column: m(1)'s column nests a Column [1 / 2].
        assert!(wb.split_beside_axis(m(2), m(1), SplitAxis::Column, true));
        assert_eq!(wb.slot_count(), 2, "two leaves: 1 and 2");
        assert_eq!(wb.tile_count(), 2);
        // The top level is still a Row of one child (collapsed), nesting the Column.
        let tree = wb.to_tile_tree(actor_tile_for).expect("tree");
        // A single top-level child collapses, so the root is the Column split itself.
        match tree {
            pelt_core::tile::TileTree::Split { axis, children } => {
                assert_eq!(axis, SplitAxis::Column, "a vertical split");
                assert_eq!(children.len(), 2);
            }
            other => panic!("expected a column split, got {other:?}"),
        }
    }

    #[test]
    fn split_out_pulls_a_tab_out_of_its_own_stack() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        wb.stack_all(); // one stack [1, 2]
        assert_eq!(wb.slot_count(), 1);
        assert!(wb.split_out(m(1), SplitAxis::Row, true), "m(1) splits out beside the stack");
        assert_eq!(wb.slot_count(), 2);
        assert_eq!(wb.tile_count(), 2, "no tab lost");
        // A tab alone in its cell has no sibling to anchor on — a no-op.
        let mut wb2 = Workbench::new();
        wb2.open_tile(m(5));
        assert!(!wb2.split_out(m(5), SplitAxis::Column, false), "alone → no-op");
    }

    #[test]
    fn weights_are_fractions_default_equal_and_set_renormalizes() {
        let mut wb = Workbench::new();
        wb.open_split(&[m(1), m(2)]);
        assert_eq!(wb.weights(), vec![0.5, 0.5], "equal fractions by default");
        wb.set_weights(&[3.0, 1.0]);
        let w = wb.weights();
        assert!((w[0] - 0.75).abs() < 1e-5 && (w[1] - 0.25).abs() < 1e-5, "renormalized to sum 1");
        wb.set_weights(&[-1.0, 0.0]);
        assert_eq!(wb.weights(), vec![0.5, 0.5], "clamped + renormalized so neither collapses");
    }

    #[test]
    fn nested_split_fractions_addressed_by_path() {
        let mut wb = Workbench::new();
        wb.open_split(&[m(1), m(2), m(3)]); // [1 | 2 | 3]
        wb.split_beside_axis(m(3), m(2), SplitAxis::Column, true); // [1 | (2 / 3)]
        // The top-level row has two children; child 1 is the nested column.
        assert_eq!(wb.split_fractions(&[]).unwrap().len(), 2);
        assert_eq!(wb.split_fractions(&[1]).unwrap().len(), 2, "the nested column");
        assert!(wb.set_split_fractions(&[1], &[0.7, 0.3]));
        let nested = wb.split_fractions(&[1]).unwrap();
        assert!((nested[0] - 0.7).abs() < 1e-5 && (nested[1] - 0.3).abs() < 1e-5);
    }

    // ── Workbench -> pelt TileTree projection (the V6 surface builder) ──

    use pelt_core::tile::{ContentSource, SplitAxis as Axis, TextureKey, Tile, TileId, TileTree};

    /// A host resolver standing in for meerkat's: a member maps to its actor-texture
    /// lane, keyed (id + texture) by the member's low bits.
    fn actor_tile_for(member: GraphMemberId) -> Tile {
        let key = member.as_u128() as u64;
        Tile {
            id: TileId(key),
            title: String::new(),
            content: ContentSource::ExternalTexture(TextureKey(key)),
            accent: None,
        }
    }

    #[test]
    fn to_tile_tree_empty_is_none() {
        assert!(Workbench::new().to_tile_tree(actor_tile_for).is_none());
    }

    #[test]
    fn to_tile_tree_single_slot_is_a_stack() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        let tree = wb.to_tile_tree(actor_tile_for).expect("one slot");
        assert!(matches!(tree, TileTree::Stack(_)), "a lone slot is the stack itself");
        assert_eq!(tree.tiles().len(), 1);
    }

    #[test]
    fn to_tile_tree_stacked_slot_keeps_active() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        wb.open_tile(m(3));
        wb.stack_all();
        wb.activate(m(3));
        let tree = wb.to_tile_tree(actor_tile_for).expect("tree");
        match tree {
            TileTree::Stack(s) => {
                assert_eq!(s.tabs.len(), 3, "all tabs carried");
                assert_eq!(s.active, 2, "the active tab is preserved");
            }
            other => panic!("expected a stack, got {other:?}"),
        }
    }

    #[test]
    fn to_tile_tree_slots_become_a_weighted_row_split() {
        let mut wb = Workbench::new();
        wb.open_split(&[m(1), m(2)]);
        wb.set_weights(&[3.0, 1.0]);
        let tree = wb.to_tile_tree(actor_tile_for).expect("tree");
        match tree {
            TileTree::Split { axis, children } => {
                assert_eq!(axis, Axis::Row, "slots lay side by side");
                assert_eq!(children.len(), 2);
                assert!((children[0].fraction - 0.75).abs() < 1e-6, "got {}", children[0].fraction);
                assert!((children[1].fraction - 0.25).abs() < 1e-6, "got {}", children[1].fraction);
            }
            other => panic!("expected a row split, got {other:?}"),
        }
    }
}
