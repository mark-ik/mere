// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Frame-layout helpers for the spatial-chrome path: walking a
//! [`FrameLayout`] tree to compute per-leaf bounds, splitter-drag
//! state, and projection of the tree into a substrate scene.
//!
//! Lifted from the legacy gpui host (`mere-host::panes`) and
//! retyped against `kurbo` so the new xilem-side host can consume
//! the same geometry. The structural shape — nested split tree
//! with horizontal/vertical axes, ratio clamping, leaf identity
//! by `PaneId` — is preserved verbatim.

use std::collections::HashMap;

use kurbo::{Point, Size};
use mere_frame::{
    FrameLayout, GraphId, PaneContent, PaneId, PaneNode, SplitAxis, SplitChoice, SplitPath,
};
use mere_renderer_registry::{NodeContentKind, NodeIdentity, Placement};
use mere_spatial_prototype::{SubstrateNode, SubstrateScene};

use crate::MereHostApp;

/// State for an in-progress splitter drag. The path identifies which
/// split in the layout tree the user grabbed; the cursor + ratio
/// snapshots let pixel deltas map to ratio deltas at any nesting
/// depth, using [`compute_container_size`] for scaling.
///
/// kurbo-typed sibling of the legacy `mere_host::panes::SplitterDrag`.
/// Lives alongside the layout walkers because the drag handler needs
/// the same `(path, axis, start_ratio)` snapshot to compute the new
/// ratio from a cursor delta.
#[derive(Clone, Debug)]
pub struct SplitterDrag {
    pub path: SplitPath,
    pub axis: SplitAxis,
    pub start_cursor: Point,
    pub start_ratio: f32,
}

/// Pixel-space placement + size for one leaf in a `FrameLayout`,
/// produced by [`walk_leaves`]. The placement is a translation to
/// the leaf's origin within the viewport; the size is the leaf's
/// rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct LeafBounds {
    pub pane_id: PaneId,
    pub content: PaneContent,
    pub graph_id: GraphId,
    pub placement: Placement,
    pub size: Size,
}

/// Walk the layout tree from the viewport down to each leaf,
/// multiplying dimensions by each split's ratio along the way.
/// Returns the pixel-space size of the container at `path` (so
/// that pixel deltas from a splitter drag translate to ratio
/// deltas at any nesting depth).
///
/// If `path` points into a Leaf rather than a Split, returns the
/// leaf's size — the caller (typically a splitter drag handler)
/// should only call this on paths that name a Split.
pub fn compute_container_size(
    layout: &FrameLayout,
    path: &[SplitChoice],
    viewport_size: Size,
) -> Size {
    let mut size = viewport_size;
    let mut node = &layout.root;
    for step in path {
        let PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } = node
        else {
            return size;
        };
        let (next_size, next_node) = match step {
            SplitChoice::First => (
                match axis {
                    SplitAxis::Horizontal => Size::new(size.width * *ratio as f64, size.height),
                    SplitAxis::Vertical => Size::new(size.width, size.height * *ratio as f64),
                },
                first.as_ref(),
            ),
            SplitChoice::Second => (
                match axis {
                    SplitAxis::Horizontal => {
                        Size::new(size.width * (1.0 - *ratio) as f64, size.height)
                    }
                    SplitAxis::Vertical => {
                        Size::new(size.width, size.height * (1.0 - *ratio) as f64)
                    }
                },
                second.as_ref(),
            ),
        };
        size = next_size;
        node = next_node;
    }
    size
}

/// Walk every leaf in `layout`, accumulating per-leaf
/// `(placement, size)` from the viewport's origin. Returns one
/// [`LeafBounds`] entry per leaf in depth-first order
/// (first-child before second-child). Origin is implicitly
/// `(0, 0)`; consumers wanting a window-local offset can add it
/// to each placement.
pub fn walk_leaves(layout: &FrameLayout, viewport_size: Size) -> Vec<LeafBounds> {
    let mut out = Vec::new();
    walk_leaves_inner(&layout.root, Point::ZERO, viewport_size, &mut out);
    out
}

fn walk_leaves_inner(node: &PaneNode, origin: Point, size: Size, out: &mut Vec<LeafBounds>) {
    match node {
        PaneNode::Leaf {
            pane_id,
            content,
            graph_id,
        } => {
            out.push(LeafBounds {
                pane_id: *pane_id,
                content: content.clone(),
                graph_id: *graph_id,
                placement: Placement::translate(origin.x, origin.y),
                size,
            });
        }
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => match axis {
            SplitAxis::Horizontal => {
                let first_w = size.width * *ratio as f64;
                let first_size = Size::new(first_w, size.height);
                let second_size = Size::new(size.width - first_w, size.height);
                walk_leaves_inner(first, origin, first_size, out);
                let second_origin = Point::new(origin.x + first_w, origin.y);
                walk_leaves_inner(second, second_origin, second_size, out);
            }
            SplitAxis::Vertical => {
                let first_h = size.height * *ratio as f64;
                let first_size = Size::new(size.width, first_h);
                let second_size = Size::new(size.width, size.height - first_h);
                walk_leaves_inner(first, origin, first_size, out);
                let second_origin = Point::new(origin.x, origin.y + first_h);
                walk_leaves_inner(second, second_origin, second_size, out);
            }
        },
    }
}

/// v0 default mapping from a [`PaneContent`] kind to the
/// [`NodeContentKind`] the substrate's renderer registry uses to
/// route the pane's body to a registered renderer. Hosts wanting
/// finer-grained routing pass their own closure to
/// [`MereHostApp::sync_scene_from_frame_layout_with`].
pub fn default_content_kind_for(content: &PaneContent) -> NodeContentKind {
    match content {
        PaneContent::Orrery => NodeContentKind::GraphView,
        PaneContent::Tile(_) => NodeContentKind::DocumentTile,
        // Workbench / Gloss / Apparatus / System / Custom default
        // to Panel — they're tile-strip-plus-body or document-body
        // shaped and a single Panel renderer can host any of them
        // until each gets its own dedicated renderer.
        _ => NodeContentKind::Panel,
    }
}

impl MereHostApp {
    /// Project a [`FrameLayout`] into a `SubstrateScene` — one
    /// substrate node per leaf, placed at split-computed bounds
    /// within `viewport_size`. Content kinds come from
    /// [`default_content_kind_for`]; for custom mapping, call
    /// [`Self::sync_scene_from_frame_layout_with`].
    ///
    /// Identity stability: leaves that were in the scene before
    /// sync keep the same `NodeIdentity` (mapped by `PaneId`), so
    /// producer handles + accessibility tree ids stay stable when
    /// only split ratios change.
    pub fn sync_scene_from_frame_layout(&mut self, layout: &FrameLayout, viewport_size: Size) {
        self.sync_scene_from_frame_layout_with(layout, viewport_size, default_content_kind_for);
    }

    /// Same as [`Self::sync_scene_from_frame_layout`] with a
    /// caller-supplied `content -> kind` mapping. Useful when the
    /// host has registered specialised renderers per pane content
    /// (workbench, orrery, gloss, apparatus, system) and wants
    /// each leaf routed to the right one.
    pub fn sync_scene_from_frame_layout_with<F>(
        &mut self,
        layout: &FrameLayout,
        viewport_size: Size,
        kind_for_content: F,
    ) where
        F: Fn(&PaneContent) -> NodeContentKind,
    {
        let leaves = walk_leaves(layout, viewport_size);
        let mut new_scene = SubstrateScene::new();
        let mut new_map = HashMap::with_capacity(leaves.len());
        for leaf in &leaves {
            let identity = self
                .pane_identity_map
                .get(&leaf.pane_id)
                .copied()
                .unwrap_or_else(NodeIdentity::next);
            new_scene.insert(SubstrateNode {
                identity,
                placement: leaf.placement,
                size: leaf.size,
                lod: mere_renderer_registry::LodLevel::FullPane,
                content_kind: kind_for_content(&leaf.content),
                renderer_pin: None,
            });
            new_map.insert(leaf.pane_id, identity);
        }
        self.scene = new_scene;
        self.pane_identity_map = new_map;
    }

    /// Substrate identity assigned to `pane_id`, if the pane is
    /// currently in the scene (i.e. covered by the last
    /// `sync_scene_from_frame_layout`). Sibling of
    /// [`Self::identity_for_tile`] for the pane-level identity
    /// space.
    pub fn identity_for_pane(&self, pane_id: PaneId) -> Option<NodeIdentity> {
        self.pane_identity_map.get(&pane_id).copied()
    }

    /// Open-order index of the leaf with `pane_id` within the
    /// last-synced layout's depth-first traversal, or `None` if
    /// the pane isn't currently in the scene.
    pub fn pane_index_for(&self, pane_id: PaneId) -> Option<usize> {
        // `pane_identity_map` is HashMap-backed and unordered;
        // for an open-order index, the host walks its own
        // FrameLayout. This stub returns None so callers wire
        // through their layout-side accessor when needed.
        let _ = pane_id;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere_frame::{FrameId, PaneContent, PaneId, PaneNode, SplitAxis};

    fn graph_id(seed: u128) -> GraphId {
        GraphId::from_uuid(uuid::Uuid::from_u128(seed))
    }

    /// 3-leaf fixture: workbench on the left (60%), orrery top-right
    /// (30% height of the right column), apparatus bottom-right.
    fn fixture_three_pane() -> FrameLayout {
        let g = graph_id(0xc01);
        FrameLayout {
            id: FrameId::new("reading"),
            label: "Reading".to_string(),
            root: PaneNode::Split {
                axis: SplitAxis::Horizontal,
                ratio: 0.6,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(1),
                    content: PaneContent::Workbench,
                    graph_id: g,
                }),
                second: Box::new(PaneNode::Split {
                    axis: SplitAxis::Vertical,
                    ratio: 0.3,
                    first: Box::new(PaneNode::Leaf {
                        pane_id: PaneId(2),
                        content: PaneContent::Orrery,
                        graph_id: g,
                    }),
                    second: Box::new(PaneNode::Leaf {
                        pane_id: PaneId(3),
                        content: PaneContent::Apparatus,
                        graph_id: g,
                    }),
                }),
            },
        }
    }

    /// Helper: f32 ratios stored on `PaneNode::Split` deliver into
    /// f64 layout math via a cast that picks up tiny rounding
    /// artifacts (0.6_f32 ≠ 0.6_f64 exactly). Tests use a 1e-3
    /// tolerance — pane bounds are layout pixels, sub-pixel
    /// precision is moot.
    fn approx_eq(a: Size, b: Size) -> bool {
        (a.width - b.width).abs() < 1e-3 && (a.height - b.height).abs() < 1e-3
    }

    #[test]
    fn compute_container_size_root_returns_viewport() {
        let layout = fixture_three_pane();
        let s = compute_container_size(&layout, &[], Size::new(1000.0, 800.0));
        assert_eq!(s, Size::new(1000.0, 800.0));
    }

    #[test]
    fn compute_container_size_walks_into_horizontal_split() {
        let layout = fixture_three_pane();
        // Root is Horizontal 0.6; Second child gets 40% width.
        let s = compute_container_size(&layout, &[SplitChoice::Second], Size::new(1000.0, 800.0));
        assert!(approx_eq(s, Size::new(400.0, 800.0)), "got {:?}", s);
    }

    #[test]
    fn compute_container_size_walks_into_nested_vertical_split() {
        let layout = fixture_three_pane();
        // Second child is a Vertical 0.3 split inside the right column.
        // Walking into First gets 30% of the column's height.
        let s = compute_container_size(
            &layout,
            &[SplitChoice::Second, SplitChoice::First],
            Size::new(1000.0, 800.0),
        );
        assert!(approx_eq(s, Size::new(400.0, 240.0)), "got {:?}", s);
    }

    #[test]
    fn walk_leaves_emits_one_entry_per_leaf_with_split_computed_bounds() {
        let layout = fixture_three_pane();
        let leaves = walk_leaves(&layout, Size::new(1000.0, 800.0));
        assert_eq!(leaves.len(), 3);

        // Workbench: left 60%, full height.
        assert_eq!(leaves[0].pane_id, PaneId(1));
        let t0 = leaves[0].placement.transform.translation();
        assert!(t0.x.abs() < 1e-3 && t0.y.abs() < 1e-3);
        assert!(approx_eq(leaves[0].size, Size::new(600.0, 800.0)));

        // Orrery: right 40%, top 30%. Origin (600, 0).
        assert_eq!(leaves[1].pane_id, PaneId(2));
        let t1 = leaves[1].placement.transform.translation();
        assert!((t1.x - 600.0).abs() < 1e-3 && t1.y.abs() < 1e-3);
        assert!(approx_eq(leaves[1].size, Size::new(400.0, 240.0)));

        // Apparatus: right 40%, bottom 70%. Origin (600, 240).
        assert_eq!(leaves[2].pane_id, PaneId(3));
        let t2 = leaves[2].placement.transform.translation();
        assert!((t2.x - 600.0).abs() < 1e-3 && (t2.y - 240.0).abs() < 1e-3);
        assert!(approx_eq(leaves[2].size, Size::new(400.0, 560.0)));
    }

    #[test]
    fn default_content_kind_routes_orrery_to_graph_view() {
        assert_eq!(
            default_content_kind_for(&PaneContent::Orrery),
            NodeContentKind::GraphView
        );
        assert_eq!(
            default_content_kind_for(&PaneContent::Workbench),
            NodeContentKind::Panel
        );
        assert_eq!(
            default_content_kind_for(&PaneContent::Tile(mere_frame::LeafNodeRef(7))),
            NodeContentKind::DocumentTile
        );
    }

    #[test]
    fn sync_scene_from_frame_layout_produces_one_node_per_leaf() {
        let layout = fixture_three_pane();
        let mut app = MereHostApp::new();
        app.sync_scene_from_frame_layout(&layout, Size::new(1000.0, 800.0));

        assert_eq!(app.scene.len(), 3);
        assert!(app.identity_for_pane(PaneId(1)).is_some());
        assert!(app.identity_for_pane(PaneId(2)).is_some());
        assert!(app.identity_for_pane(PaneId(3)).is_some());
    }

    #[test]
    fn sync_scene_from_frame_layout_preserves_pane_identity_across_calls() {
        let layout = fixture_three_pane();
        let mut app = MereHostApp::new();
        app.sync_scene_from_frame_layout(&layout, Size::new(1000.0, 800.0));
        let id_a_first = app.identity_for_pane(PaneId(1)).unwrap();
        let id_b_first = app.identity_for_pane(PaneId(2)).unwrap();

        // Re-sync with a different viewport size — pane identities
        // should not churn, because pane_ids are stable.
        app.sync_scene_from_frame_layout(&layout, Size::new(800.0, 600.0));
        assert_eq!(app.identity_for_pane(PaneId(1)), Some(id_a_first));
        assert_eq!(app.identity_for_pane(PaneId(2)), Some(id_b_first));
    }

    #[test]
    fn sync_scene_from_frame_layout_uses_default_content_kinds_for_each_leaf() {
        let layout = fixture_three_pane();
        let mut app = MereHostApp::new();
        app.sync_scene_from_frame_layout(&layout, Size::new(1000.0, 800.0));

        // Workbench (Panel), Orrery (GraphView), Apparatus (Panel).
        let kinds: Vec<NodeContentKind> = app.scene.iter().map(|n| n.content_kind).collect();
        assert!(kinds.contains(&NodeContentKind::Panel));
        assert!(kinds.contains(&NodeContentKind::GraphView));
    }

    #[test]
    fn sync_scene_from_frame_layout_with_routes_via_custom_mapping() {
        let layout = fixture_three_pane();
        let mut app = MereHostApp::new();
        app.sync_scene_from_frame_layout_with(&layout, Size::new(1000.0, 800.0), |content| {
            match content {
                PaneContent::Workbench => NodeContentKind::CustomCanvas,
                _ => NodeContentKind::Panel,
            }
        });
        // Verify workbench leaf got CustomCanvas via the override.
        let workbench_identity = app.identity_for_pane(PaneId(1)).unwrap();
        let workbench_node = app
            .scene
            .iter()
            .find(|n| n.identity == workbench_identity)
            .unwrap();
        assert_eq!(workbench_node.content_kind, NodeContentKind::CustomCanvas);
    }

    #[test]
    fn splitter_drag_stores_start_state() {
        let drag = SplitterDrag {
            path: vec![SplitChoice::First],
            axis: SplitAxis::Horizontal,
            start_cursor: Point::new(100.0, 50.0),
            start_ratio: 0.6,
        };
        assert_eq!(drag.path.len(), 1);
        assert_eq!(drag.start_cursor, Point::new(100.0, 50.0));
    }
}
