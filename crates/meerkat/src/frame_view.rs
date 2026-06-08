/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Geometry for the frame tree (F1). The `frame` crate is geometry-free — it
//! owns the split *tree* (axis + ratio), not pixels — so meerkat projects a
//! [`FrameLayout`] onto the content band here: each leaf's screen rect, the
//! draggable divider gutters, and a maximize override that gives one leaf the
//! whole band. Rendering + input walk these results; nothing else in the host
//! needs to know the tree shape.

use frame::{FrameLayout, PaneContent, PaneId, PaneNode, SplitAxis, SplitChoice};

/// Width (px) of the gutter reserved between split siblings — the draggable
/// frame divider (distinct from the workbench tile tree's slot dividers).
pub const DIVIDER: f32 = 6.0;

/// A laid-out leaf: its pane id, content, and screen rect `[x0, y0, x1, y1]`.
/// (Split paths come from [`pane_path`] / [`divider_rects`] when an op needs one.)
pub struct LaidLeaf {
    pub pane_id: PaneId,
    pub content: PaneContent,
    pub rect: [f32; 4],
}

/// A laid-out divider: the split path to its `Split` node, the gutter rect, the
/// split's own (parent) rect, and its axis — so a drag maps the cursor position
/// within the parent rect to a new ratio via `set_split_ratio`.
pub struct LaidDivider {
    pub path: Vec<SplitChoice>,
    pub rect: [f32; 4],
    pub parent: [f32; 4],
    pub axis: SplitAxis,
}

/// Lay every leaf out within `band` (`[x0, y0, x1, y1]`). When `maximized` names a
/// leaf present in the layout, that leaf fills the whole band and the rest are
/// omitted (the maximize override).
pub fn leaf_rects(layout: &FrameLayout, band: [f32; 4], maximized: Option<PaneId>) -> Vec<LaidLeaf> {
    if let Some(pid) = maximized {
        if let Some((content, _path)) = find_leaf(&layout.root, pid, &mut Vec::new()) {
            return vec![LaidLeaf { pane_id: pid, content, rect: band }];
        }
    }
    let mut out = Vec::new();
    walk_leaves(&layout.root, band, &mut out);
    out
}

/// Lay every divider gutter out within `band`. Empty when a leaf is maximized
/// (no visible splits then).
pub fn divider_rects(
    layout: &FrameLayout,
    band: [f32; 4],
    maximized: Option<PaneId>,
) -> Vec<LaidDivider> {
    if maximized.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk_dividers(&layout.root, band, &mut Vec::new(), &mut out);
    out
}

fn walk_leaves(node: &PaneNode, rect: [f32; 4], out: &mut Vec<LaidLeaf>) {
    match node {
        PaneNode::Leaf { pane_id, content, .. } => {
            out.push(LaidLeaf { pane_id: *pane_id, content: content.clone(), rect })
        },
        PaneNode::Split { axis, ratio, first, second } => {
            let (r1, r2) = split_rect(rect, *axis, *ratio);
            walk_leaves(first, r1, out);
            walk_leaves(second, r2, out);
        },
    }
}

fn walk_dividers(
    node: &PaneNode,
    rect: [f32; 4],
    path: &mut Vec<SplitChoice>,
    out: &mut Vec<LaidDivider>,
) {
    if let PaneNode::Split { axis, ratio, first, second } = node {
        out.push(LaidDivider {
            path: path.clone(),
            rect: gutter_rect(rect, *axis, *ratio),
            parent: rect,
            axis: *axis,
        });
        let (r1, r2) = split_rect(rect, *axis, *ratio);
        path.push(SplitChoice::First);
        walk_dividers(first, r1, path, out);
        path.pop();
        path.push(SplitChoice::Second);
        walk_dividers(second, r2, path, out);
        path.pop();
    }
}

/// Split `rect` by `axis` + `ratio`, reserving the [`DIVIDER`] gutter between the
/// two child rects. `first` takes `ratio` of the usable extent.
fn split_rect(rect: [f32; 4], axis: SplitAxis, ratio: f32) -> ([f32; 4], [f32; 4]) {
    let [x0, y0, x1, y1] = rect;
    match axis {
        SplitAxis::Horizontal => {
            let usable = (x1 - x0 - DIVIDER).max(0.0);
            let cut = x0 + usable * ratio;
            ([x0, y0, cut, y1], [cut + DIVIDER, y0, x1, y1])
        },
        SplitAxis::Vertical => {
            let usable = (y1 - y0 - DIVIDER).max(0.0);
            let cut = y0 + usable * ratio;
            ([x0, y0, x1, cut], [x0, cut + DIVIDER, x1, y1])
        },
    }
}

/// The gutter rect between this split's two children.
fn gutter_rect(rect: [f32; 4], axis: SplitAxis, ratio: f32) -> [f32; 4] {
    let [x0, y0, x1, y1] = rect;
    match axis {
        SplitAxis::Horizontal => {
            let usable = (x1 - x0 - DIVIDER).max(0.0);
            let cut = x0 + usable * ratio;
            [cut, y0, cut + DIVIDER, y1]
        },
        SplitAxis::Vertical => {
            let usable = (y1 - y0 - DIVIDER).max(0.0);
            let cut = y0 + usable * ratio;
            [x0, cut, x1, cut + DIVIDER]
        },
    }
}

/// The split path to leaf `pane`, if present (for close / maximize / divider ops).
pub fn pane_path(layout: &FrameLayout, pane: PaneId) -> Option<Vec<SplitChoice>> {
    find_leaf(&layout.root, pane, &mut Vec::new()).map(|(_, path)| path)
}

/// Find leaf `pid`, returning its content + split path.
fn find_leaf(
    node: &PaneNode,
    pid: PaneId,
    path: &mut Vec<SplitChoice>,
) -> Option<(PaneContent, Vec<SplitChoice>)> {
    match node {
        PaneNode::Leaf { pane_id, content, .. } if *pane_id == pid => Some((content.clone(), path.clone())),
        PaneNode::Leaf { .. } => None,
        PaneNode::Split { first, second, .. } => {
            path.push(SplitChoice::First);
            if let Some(hit) = find_leaf(first, pid, path) {
                return Some(hit);
            }
            path.pop();
            path.push(SplitChoice::Second);
            if let Some(hit) = find_leaf(second, pid, path) {
                return Some(hit);
            }
            path.pop();
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use frame::{FrameId, GraphId};

    use super::*;

    fn leaf(id: u64, content: PaneContent) -> PaneNode {
        PaneNode::Leaf { pane_id: PaneId(id), content, graph_id: GraphId::default() }
    }

    fn layout(root: PaneNode) -> FrameLayout {
        FrameLayout { id: FrameId::new("t"), label: "t".into(), root }
    }

    #[test]
    fn single_leaf_fills_the_band() {
        let l = layout(leaf(0, PaneContent::Workbench));
        let leaves = leaf_rects(&l, [0.0, 0.0, 800.0, 600.0], None);
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].rect, [0.0, 0.0, 800.0, 600.0]);
        assert!(divider_rects(&l, [0.0, 0.0, 800.0, 600.0], None).is_empty());
    }

    #[test]
    fn horizontal_split_places_children_with_a_gutter() {
        let l = layout(PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(leaf(0, PaneContent::Workbench)),
            second: Box::new(leaf(1, PaneContent::Roster)),
        });
        let leaves = leaf_rects(&l, [0.0, 0.0, 806.0, 600.0], None);
        assert_eq!(leaves.len(), 2);
        // 806 - 6 gutter = 800 usable; 0.5 each => first [0,400], second [406,806].
        assert_eq!(leaves[0].rect, [0.0, 0.0, 400.0, 600.0]);
        assert_eq!(leaves[1].rect, [406.0, 0.0, 806.0, 600.0]);
        let dividers = divider_rects(&l, [0.0, 0.0, 806.0, 600.0], None);
        assert_eq!(dividers.len(), 1);
        assert_eq!(dividers[0].rect, [400.0, 0.0, 406.0, 600.0]);
    }

    #[test]
    fn maximize_gives_one_leaf_the_whole_band() {
        let l = layout(PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(leaf(0, PaneContent::Workbench)),
            second: Box::new(leaf(1, PaneContent::Roster)),
        });
        let leaves = leaf_rects(&l, [0.0, 0.0, 806.0, 600.0], Some(PaneId(1)));
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].pane_id, PaneId(1));
        assert_eq!(leaves[0].rect, [0.0, 0.0, 806.0, 600.0]);
        // An unknown maximized id falls back to the normal layout.
        assert_eq!(leaf_rects(&l, [0.0, 0.0, 806.0, 600.0], Some(PaneId(9))).len(), 2);
    }
}
