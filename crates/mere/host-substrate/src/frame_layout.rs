// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Frame-layout helpers for the spatial-chrome path: walking a
//! [`FrameLayout`] tree to compute per-leaf bounds, splitter-drag
//! state, and projection of the tree into a substrate scene.
//!
//! Lifted from the legacy gpui host (`host::panes`) and
//! retyped against `kurbo` so the new xilem-side host can consume
//! the same geometry. The structural shape — nested split tree
//! with horizontal/vertical axes, ratio clamping, leaf identity
//! by `PaneId` — is preserved verbatim.

use std::collections::HashMap;

use kurbo::{Point, Size};
use frame::{
    FrameLayout, GraphId, PaneContent, PaneId, PaneNode, SplitAxis, SplitChoice, SplitPath,
};
use register_renderer::{NodeContentKind, NodeIdentity, Placement};
use spatial_substrate::{SubstrateNode, SubstrateScene};

use crate::HostApp;

/// State for an in-progress splitter drag. The path identifies which
/// split in the layout tree the user grabbed; the cursor + ratio
/// snapshots let pixel deltas map to ratio deltas at any nesting
/// depth, using [`compute_container_size`] for scaling.
///
/// kurbo-typed sibling of the legacy `host::panes::SplitterDrag`.
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

/// Pixel-space bounds for one splitter in a `FrameLayout`, produced
/// by [`walk_splitters`]. A splitter is the narrow chrome region
/// between a Split's two children — vertical strip for a horizontal
/// split (children laid left/right), horizontal strip for a vertical
/// split (children laid top/bottom). The strip sits *between* the
/// children with zero overlap on either; thickness comes from
/// [`SPLITTER_THICKNESS`].
#[derive(Clone, Debug, PartialEq)]
pub struct SplitterBounds {
    pub path: SplitPath,
    pub axis: SplitAxis,
    pub placement: Placement,
    pub size: Size,
}

/// Visual + hit-test thickness for the splitter chrome between
/// sibling panes. Matches the legacy gpui host's value so the
/// xilem-side path lands with the same affordance density.
pub const SPLITTER_THICKNESS: f64 = 4.0;

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

/// Walk every Split in `layout`, accumulating per-split splitter
/// bounds. The splitter sits centered on the boundary between
/// the Split's two children — a vertical strip of [`SPLITTER_THICKNESS`]
/// width for horizontal splits, a horizontal strip for vertical
/// splits. Returns one [`SplitterBounds`] per Split in depth-first
/// order. Visually overlays the pane edges; pane backgrounds paint
/// behind, splitter chrome paints on top.
pub fn walk_splitters(layout: &FrameLayout, viewport_size: Size) -> Vec<SplitterBounds> {
    let mut out = Vec::new();
    walk_splitters_inner(
        &layout.root,
        Point::ZERO,
        viewport_size,
        &mut Vec::new(),
        &mut out,
    );
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

fn walk_splitters_inner(
    node: &PaneNode,
    origin: Point,
    size: Size,
    path: &mut Vec<SplitChoice>,
    out: &mut Vec<SplitterBounds>,
) {
    let PaneNode::Split {
        axis,
        ratio,
        first,
        second,
    } = node
    else {
        return;
    };
    // Emit the splitter for THIS Split, centered on the boundary.
    let half = SPLITTER_THICKNESS / 2.0;
    match axis {
        SplitAxis::Horizontal => {
            let first_w = size.width * *ratio as f64;
            let splitter_origin = Point::new(origin.x + first_w - half, origin.y);
            let splitter_size = Size::new(SPLITTER_THICKNESS, size.height);
            out.push(SplitterBounds {
                path: path.clone(),
                axis: *axis,
                placement: Placement::translate(splitter_origin.x, splitter_origin.y),
                size: splitter_size,
            });
            // Recurse into children with their split-shares.
            let first_size = Size::new(first_w, size.height);
            let second_size = Size::new(size.width - first_w, size.height);
            path.push(SplitChoice::First);
            walk_splitters_inner(first, origin, first_size, path, out);
            path.pop();
            let second_origin = Point::new(origin.x + first_w, origin.y);
            path.push(SplitChoice::Second);
            walk_splitters_inner(second, second_origin, second_size, path, out);
            path.pop();
        }
        SplitAxis::Vertical => {
            let first_h = size.height * *ratio as f64;
            let splitter_origin = Point::new(origin.x, origin.y + first_h - half);
            let splitter_size = Size::new(size.width, SPLITTER_THICKNESS);
            out.push(SplitterBounds {
                path: path.clone(),
                axis: *axis,
                placement: Placement::translate(splitter_origin.x, splitter_origin.y),
                size: splitter_size,
            });
            let first_size = Size::new(size.width, first_h);
            let second_size = Size::new(size.width, size.height - first_h);
            path.push(SplitChoice::First);
            walk_splitters_inner(first, origin, first_size, path, out);
            path.pop();
            let second_origin = Point::new(origin.x, origin.y + first_h);
            path.push(SplitChoice::Second);
            walk_splitters_inner(second, second_origin, second_size, path, out);
            path.pop();
        }
    }
}

/// v0 default mapping from a [`PaneContent`] kind to the
/// [`NodeContentKind`] the substrate's renderer registry uses to
/// route the pane's body to a registered renderer. Hosts wanting
/// finer-grained routing pass their own closure to
/// [`HostApp::sync_scene_from_frame_layout_with`].
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

impl HostApp {
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
        let splitters = walk_splitters(layout, viewport_size);
        let mut new_scene = SubstrateScene::new();
        let mut new_pane_map = HashMap::with_capacity(leaves.len());
        let mut new_splitter_map = HashMap::with_capacity(splitters.len());
        // Insert leaves first so the splitter chrome paints over
        // their edges. Hit-test runs in reverse insertion order;
        // splitters added last hit first, which is what we want
        // (the 4px boundary should grab clicks even when the user's
        // cursor is also inside the abutting pane bounds).
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
                lod: register_renderer::LodLevel::FullPane,
                content_kind: kind_for_content(&leaf.content),
                renderer_pin: None,
            });
            new_pane_map.insert(leaf.pane_id, identity);
        }
        for splitter in &splitters {
            let identity = self
                .splitter_identity_map
                .get(&splitter.path)
                .copied()
                .unwrap_or_else(NodeIdentity::next);
            new_scene.insert(SubstrateNode {
                identity,
                placement: splitter.placement,
                size: splitter.size,
                lod: register_renderer::LodLevel::FullPane,
                content_kind: NodeContentKind::Splitter,
                renderer_pin: None,
            });
            new_splitter_map.insert(splitter.path.clone(), identity);
        }
        self.scene = new_scene;
        self.pane_identity_map = new_pane_map;
        self.splitter_identity_map = new_splitter_map;
    }

    /// Substrate identity assigned to `pane_id`, if the pane is
    /// currently in the scene (i.e. covered by the last
    /// `sync_scene_from_frame_layout`). Sibling of
    /// [`Self::identity_for_tile`] for the pane-level identity
    /// space.
    pub fn identity_for_pane(&self, pane_id: PaneId) -> Option<NodeIdentity> {
        self.pane_identity_map.get(&pane_id).copied()
    }

    /// Substrate identity assigned to the splitter at `path` in the
    /// last-synced layout, if any.
    pub fn splitter_identity_for(&self, path: &frame::SplitPath) -> Option<NodeIdentity> {
        self.splitter_identity_map.get(path).copied()
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
    use frame::{FrameId, PaneContent, PaneId, PaneNode, SplitAxis};

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
            default_content_kind_for(&PaneContent::Tile(frame::LeafNodeRef(7))),
            NodeContentKind::DocumentTile
        );
    }

    #[test]
    fn sync_scene_from_frame_layout_produces_one_node_per_leaf() {
        let layout = fixture_three_pane();
        let mut app = HostApp::new();
        app.sync_scene_from_frame_layout(&layout, Size::new(1000.0, 800.0));

        // 3 leaves + 2 splitters = 5 substrate nodes total.
        assert_eq!(app.scene.len(), 5);
        assert!(app.identity_for_pane(PaneId(1)).is_some());
        assert!(app.identity_for_pane(PaneId(2)).is_some());
        assert!(app.identity_for_pane(PaneId(3)).is_some());
    }

    #[test]
    fn sync_scene_from_frame_layout_preserves_pane_identity_across_calls() {
        let layout = fixture_three_pane();
        let mut app = HostApp::new();
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
        let mut app = HostApp::new();
        app.sync_scene_from_frame_layout(&layout, Size::new(1000.0, 800.0));

        // Workbench (Panel), Orrery (GraphView), Apparatus (Panel).
        let kinds: Vec<NodeContentKind> = app.scene.iter().map(|n| n.content_kind).collect();
        assert!(kinds.contains(&NodeContentKind::Panel));
        assert!(kinds.contains(&NodeContentKind::GraphView));
    }

    #[test]
    fn sync_scene_from_frame_layout_with_routes_via_custom_mapping() {
        let layout = fixture_three_pane();
        let mut app = HostApp::new();
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

    #[test]
    fn walk_splitters_emits_one_entry_per_split() {
        let layout = fixture_three_pane();
        let splitters = walk_splitters(&layout, Size::new(1000.0, 800.0));
        // Two Splits in the fixture: the root Horizontal and the
        // nested Vertical in the second child.
        assert_eq!(splitters.len(), 2);
    }

    #[test]
    fn walk_splitters_root_horizontal_is_vertical_strip_at_boundary() {
        let layout = fixture_three_pane();
        let splitters = walk_splitters(&layout, Size::new(1000.0, 800.0));
        // First emitted is the root split (depth-first).
        let s = &splitters[0];
        assert_eq!(s.path, Vec::<SplitChoice>::new());
        assert_eq!(s.axis, SplitAxis::Horizontal);
        // Centered on x = 600 with thickness 4 → origin x = 598.
        let t = s.placement.transform.translation();
        assert!((t.x - 598.0).abs() < 1e-3);
        assert!(t.y.abs() < 1e-3);
        assert!((s.size.width - SPLITTER_THICKNESS).abs() < 1e-3);
        assert!((s.size.height - 800.0).abs() < 1e-3);
    }

    #[test]
    fn walk_splitters_nested_vertical_is_horizontal_strip() {
        let layout = fixture_three_pane();
        let splitters = walk_splitters(&layout, Size::new(1000.0, 800.0));
        // Second emitted is the nested vertical split in the right column.
        let s = &splitters[1];
        assert_eq!(s.path, vec![SplitChoice::Second]);
        assert_eq!(s.axis, SplitAxis::Vertical);
        // Right column origin x = 600, width = 400. Vertical 0.3 split
        // → first child height 240; splitter centered at y = 240,
        // origin y = 238, height = 4, width = column width (400).
        let t = s.placement.transform.translation();
        assert!((t.x - 600.0).abs() < 1e-3);
        assert!((t.y - 238.0).abs() < 1e-3);
        assert!((s.size.width - 400.0).abs() < 1e-3);
        assert!((s.size.height - SPLITTER_THICKNESS).abs() < 1e-3);
    }

    #[test]
    fn sync_scene_from_frame_layout_includes_splitter_nodes() {
        let layout = fixture_three_pane();
        let mut app = HostApp::new();
        app.sync_scene_from_frame_layout(&layout, Size::new(1000.0, 800.0));
        // 3 panes + 2 splitters = 5 substrate nodes.
        assert_eq!(app.scene.len(), 5);
        // All splitter paths are queryable through the identity map.
        let root_path: Vec<SplitChoice> = Vec::new();
        let nested_path = vec![SplitChoice::Second];
        assert!(app.splitter_identity_for(&root_path).is_some());
        assert!(app.splitter_identity_for(&nested_path).is_some());
    }
}
