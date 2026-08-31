// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The tiled layout model (S4): platen's canonical tiling state.
//!
//! The orrery and the tiled workbench are two **projections of one arrangement**
//! (the composition spine): the orrery is the [`ProjectionKind::Cartography`]
//! projection (graph members positioned spatially), the tiled layout the
//! [`ProjectionKind::Tree`] one. This module owns the tiling state (the split tree of
//! tab-stacks, the active tab per stack, the projection mode) and turns it into placed,
//! content-resolved tiles a host renders.
//!
//! The layout is a **recursive split tree** (see [`tree`]): a leaf is a tab-stack
//! (one or more members, one visible), and a split lays its children along an axis (a
//! `Row` side-by-side, a `Column` top-to-bottom), each with a fractional share. Splits
//! nest, so the tree expresses every variation: horizontal, vertical, and combinations.
//! It is geometry-free; layout is the host's genet/taffy job, reached through
//! [`TileLayout::to_tile_tree`] (the Genet tile surface) and [`TileLayout::slot_views`] (the
//! a11y / automation projection).
//!
//! Replaces the legacy `FrameState` / `PaneBinding` frame model (the pre-spine
//! pane-binding workbench), per the 2026-06-04 platen taffy-retarget plan.

use forme::GraphMemberId;
use workbench::SplitAxis;

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

/// The host's tiled composition: the split tree and the projection mode.
/// Cartography (the orrery) is the default; the host flips to Tree for tiles.
#[derive(Clone, Debug)]
pub struct TileLayout {
    mode: ProjectionKind,
    root: Option<Pane>,
}

impl Default for TileLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl TileLayout {
    /// A new tile layout: empty, in Cartography (orrery) mode.
    pub fn new() -> Self {
        Self {
            mode: ProjectionKind::Cartography,
            root: None,
        }
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
        self.root
            .as_mut()
            .is_some_and(|r| r.set_fractions_at(path, fractions))
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
        self.push_column(Pane::Stack(Stack {
            members: stack,
            active: 0,
        }));
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
        self.root
            .as_mut()
            .is_some_and(|r| r.stack_into(member, target))
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
        self.root
            .as_mut()
            .is_some_and(|r| r.stack_into(dragged, target))
    }

    /// Split `dragged` out as its own cell beside `target`, along the **Row** axis, on
    /// the left (`after == false`) or right (`after == true`) — the back-compatible
    /// horizontal split. See [`split_beside_axis`](Self::split_beside_axis) for vertical
    /// and nesting.
    pub fn split_beside(
        &mut self,
        dragged: GraphMemberId,
        target: GraphMemberId,
        after: bool,
    ) -> bool {
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
        self.root
            .as_mut()
            .is_some_and(|r| r.split_beside(dragged, target, axis, after))
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
                        .map(|m| Branch {
                            fraction: frac,
                            pane: Pane::leaf(m),
                        })
                        .collect(),
                })
            }
        };
    }

    /// Close every cell (the projection mode is left unchanged).
    pub fn clear_tiles(&mut self) {
        self.root = None;
    }

    /// Project this tile layout onto Genet's [`TileTree`](workbench::TileTree)
    /// contract (V5/V6) — the builder meerkat uses to render the workbench through the
    /// Genet tile surface. The **structure** is the split tree (nested
    /// `Row`/`Column` splits with their fractions, each leaf a stack with its active
    /// tab); the host supplies `tile_for`, resolving each member to its
    /// [`Tile`](workbench::Tile). An empty layout yields `None`. A projection,
    /// never a second authority: the layout stays the tiling truth, and the surface
    /// is driven entirely through the contract (the host applies tile events back and
    /// re-projects).
    pub fn to_tile_tree(
        &self,
        mut tile_for: impl FnMut(GraphMemberId) -> workbench::Tile,
    ) -> Option<workbench::TileTree> {
        self.root.as_ref().map(|r| r.to_tile_tree(&mut tile_for))
    }

    /// Append `pane` as a new top-level column (a Row child), wrapping the existing root
    /// in a Row split when it is not already one. Preserves the existing columns'
    /// relative shares; the newcomer takes an equal share of the row.
    fn push_column(&mut self, pane: Pane) {
        match self.root.take() {
            None => self.root = Some(pane),
            Some(Pane::Split {
                axis: SplitAxis::Row,
                mut children,
            }) => {
                let frac = 1.0 / (children.len() + 1) as f32;
                for b in &mut children {
                    b.fraction *= 1.0 - frac;
                }
                children.push(Branch {
                    fraction: frac,
                    pane,
                });
                self.root = Some(Pane::Split {
                    axis: SplitAxis::Row,
                    children,
                });
            }
            Some(other) => {
                self.root = Some(Pane::Split {
                    axis: SplitAxis::Row,
                    children: vec![
                        Branch {
                            fraction: 0.5,
                            pane: other,
                        },
                        Branch {
                            fraction: 0.5,
                            pane,
                        },
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

/// Compatibility name for the pre-projection-authoring tiled layout API.
///
/// `Workbench` will be reused by the future projection-authoring component. New code should
/// name this geometry-free split/tab state [`TileLayout`].
pub type Workbench = TileLayout;

/// Walk the tree's leaves in order, pushing a [`SlotView`] per stack with its parent
/// fraction as the weight.
fn collect_slots<'a>(pane: &'a Pane, weight: f32, out: &mut Vec<SlotView<'a>>) {
    match pane {
        Pane::Stack(s) => out.push(SlotView {
            members: &s.members,
            active: s.active,
            weight,
        }),
        Pane::Split { children, .. } => {
            for b in children {
                collect_slots(&b.pane, b.fraction, out);
            }
        }
    }
}

#[cfg(test)]
mod tests;
