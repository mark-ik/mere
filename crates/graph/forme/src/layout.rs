// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::MemberId;
use crate::Rect;
use crate::graphlet::GraphletId;
use crate::member::{Lifecycle, SplitDirection};
use crate::topology::TreeRow;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How the tree's spatial layout is computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutMode {
    /// Tree-style tabs + single focused content pane. **Default.**
    /// The tree IS the navigation; one member is active at a time.
    TreeStyleTabs,
    /// Flat tab bar: warm/active members as tabs, topology-ordered.
    FlatTabs,
    /// Split panes: active members get taffy-computed rects.
    /// Supports min/max, flex grow/shrink, nested splits.
    SplitPanes,
}

impl Default for LayoutMode {
    fn default() -> Self {
        Self::TreeStyleTabs
    }
}

/// Result of layout computation.
#[derive(Clone, Debug)]
pub struct LayoutResult<N: MemberId> {
    /// Pane rectangles (SplitPanes mode only).
    pub pane_rects: HashMap<N, Rect>,
    /// Draggable split boundaries between adjacent sibling panes.
    pub split_boundaries: Vec<SplitBoundary<N>>,
    /// Tab ordering (FlatTabs mode; also populated in other modes).
    pub tab_order: Vec<TabEntry<N>>,
    /// Tree rows (always populated — powers the sidebar in every mode).
    pub tree_rows: Vec<OwnedTreeRow<N>>,
    /// Currently active member.
    pub active: Option<N>,
}

/// A draggable boundary between two adjacent sibling panes.
///
/// Produced by `compute_layout()` in SplitPanes mode. The host renders a
/// drag handle at `axis_position` and converts mouse deltas into updated
/// `split_ratio` values via `NavAction::SetLayoutOverride`.
#[derive(Clone, Debug)]
pub struct SplitBoundary<N: MemberId> {
    /// The member on the "before" side (left or top).
    pub before: N,
    /// The member on the "after" side (right or bottom).
    pub after: N,
    /// Split direction of the parent container.
    pub direction: SplitDirection,
    /// Position along the split axis where the boundary falls (px).
    /// For Horizontal splits: x coordinate. For Vertical: y coordinate.
    pub axis_position: f32,
    /// Start of the boundary line on the cross-axis (px).
    pub cross_start: f32,
    /// End of the boundary line on the cross-axis (px).
    pub cross_end: f32,
    /// Total extent of the parent container along the split axis (px).
    /// Used to convert drag deltas into split_ratio changes.
    pub container_extent: f32,
}

/// Tab bar entry.
#[derive(Clone, Debug)]
pub struct TabEntry<N: MemberId> {
    pub member: N,
    pub lifecycle: Lifecycle,
    pub is_anchor: bool,
    pub depth: usize,
    pub graphlet_id: Option<GraphletId>,
}

/// Owned version of `TreeRow` for storage in `LayoutResult`.
#[derive(Clone, Debug)]
pub struct OwnedTreeRow<N: MemberId> {
    pub member: N,
    pub depth: usize,
    pub is_expanded: bool,
    pub has_children: bool,
    pub is_last_sibling: bool,
    pub graphlet_id: Option<GraphletId>,
}

impl<'a, N: MemberId> From<TreeRow<'a, N>> for OwnedTreeRow<N> {
    fn from(row: TreeRow<'a, N>) -> Self {
        Self {
            member: row.member.clone(),
            depth: row.depth,
            is_expanded: row.is_expanded,
            has_children: row.has_children,
            is_last_sibling: row.is_last_sibling,
            graphlet_id: row.graphlet_id,
        }
    }
}

/// Framework adapter trait. Each framework implements this once.
/// The tree does the heavy lifting; the adapter just paints.
pub trait GraphTreeRenderer<N: MemberId> {
    type Ctx;
    type Out;

    fn render_tree_tabs(
        &mut self,
        tree: &crate::tree::GraphTree<N>,
        rows: &[OwnedTreeRow<N>],
        ctx: &mut Self::Ctx,
    ) -> Self::Out;

    fn render_flat_tabs(
        &mut self,
        tree: &crate::tree::GraphTree<N>,
        tabs: &[TabEntry<N>],
        ctx: &mut Self::Ctx,
    ) -> Self::Out;

    fn render_pane_chrome(
        &mut self,
        tree: &crate::tree::GraphTree<N>,
        rects: &HashMap<N, Rect>,
        ctx: &mut Self::Ctx,
    ) -> Self::Out;
}
