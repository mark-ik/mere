/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pane geometry for a window: the content band, where each pane (orrery,
//! workbench, comms, roster, gloss, apparatus) sits on screen, hit-testing a
//! window point to a pane / scrying tile / gloss node / frame divider, and the
//! divider drag. These are the spatial queries `render` and `input` resolve
//! against; the pane lifecycle (open / close / toggle / sync) stays in
//! `frame_ops`. Factored out to keep files under the 600-LOC ceiling.

use forme::GraphMemberId;
use frame::{GraphId, PaneContent, PaneId, SplitAxis, SplitChoice};

use super::{FALLBACK_TOOLBAR_H, WindowCtx, frame_view};

impl WindowCtx<'_> {
    /// The graph + screen rect of the Orrery pane under window point `(x, y)`, if the
    /// cursor is over one — for routing hover / wheel to the orrery that pane
    /// resolves to, so a second graph-pane pans / zooms independently of the focused
    /// one. `None` off every Orrery leaf (over the workbench / a utility pane / the
    /// toolbar). (Window composition P2 — per-pane input.)
    pub(super) fn orrery_pane_at(&self, x: f32, y: f32) -> Option<(GraphId, [f32; 4])> {
        self.laid_leaves()
            .into_iter()
            .find(|l| {
                matches!(l.content, PaneContent::Orrery)
                    && x >= l.rect[0]
                    && x < l.rect[2]
                    && y >= l.rect[1]
                    && y < l.rect[3]
            })
            .map(|l| (l.graph_id, l.rect))
    }

    /// The content band (below the toolbar, inside the shellbar carve) in window
    /// coords — the same band `render` lays the panes out in. It must match render's
    /// `band_after_shellbar`: the input hit-rects (pane leaves, dividers, the orrery
    /// origin) and the a11y bounds all derive from this, so any divergence shifts
    /// every content click off from what's drawn (a left-docked shellbar carves the
    /// band's left edge, so a band starting at x=0 would offset clicks by the strip
    /// width). (Shellbar F2.1.)
    pub(super) fn content_band(&self) -> [f32; 4] {
        let th = self.view.toolbar_h.max(FALLBACK_TOOLBAR_H) as f32;
        // A slim (leaf) window carves no shellbar, so the band is the whole area below
        // the toolbar; the carve must match render's band exactly. (MW3 step 4.)
        if self.view.kind.is_slim() {
            return [0.0, th, self.view.width as f32, self.view.height as f32];
        }
        super::shellbar::band_after_shellbar(
            self.shared.presentation.shellbar_edge,
            self.view.width as f32,
            self.view.height as f32,
            th,
        )
    }

    /// Map a window point to the orrery pane's local space (its rect origin at 0,0) —
    /// the coordinate space the orrery's pointer + camera operate in (it rasterizes
    /// into its own texture and `render` composites that at the orrery leaf's origin).
    /// Subtracts the *leaf* origin, so it stays correct under a shellbar carve or a
    /// frame split, not just the toolbar inset. (Cursor-offset fix.)
    pub(super) fn orrery_point(&self, x: f32, y: f32) -> (f32, f32) {
        let rect = self.orrery_leaf_rect();
        (x - rect[0], y - rect[1])
    }

    /// The laid-out content panes (leaf rects) for the current frame layout.
    pub(super) fn laid_leaves(&self) -> Vec<frame_view::LaidLeaf> {
        frame_view::leaf_rects(&self.view.frame_layout, self.content_band(), self.view.maximized_pane)
    }

    /// The *focused* orrery pane's screen rect: the Orrery leaf bound to
    /// `focused_graph` (so input coords + the primary drive follow focus), falling
    /// back to the first Orrery leaf, then the whole band when none is laid out
    /// (e.g. another pane is maximized). The orrery is always present. (Window
    /// composition — pane-as-unit: the focused pane is the primary one.)
    pub(super) fn orrery_leaf_rect(&self) -> [f32; 4] {
        let band = self.content_band();
        let gid = self.view.focused_graph;
        let leaves = self.laid_leaves();
        leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Orrery) && l.graph_id == gid)
            .or_else(|| leaves.iter().find(|l| matches!(l.content, PaneContent::Orrery)))
            .map(|l| l.rect)
            .unwrap_or(band)
    }

    /// The tiled-workbench pane's screen rect, if the workbench pane is open.
    pub(super) fn workbench_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Workbench))
            .map(|l| l.rect)
    }

    /// The roster pane's screen rect, if the roster is open.
    pub(super) fn roster_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Roster))
            .map(|l| l.rect)
    }

    /// The gloss pane's screen rect, if open.
    pub(super) fn gloss_leaf_rect(&self) -> Option<[f32; 4]> {
        self.laid_leaves()
            .into_iter()
            .find(|l| matches!(l.content, PaneContent::Gloss))
            .map(|l| l.rect)
    }

    /// If window point `(x, y)` falls on a live scrying surface, the member +
    /// **surface-local** `(x, y)` to forward into its WebView. Scans the surfaces
    /// last-first so the most recently composited (topmost) pane wins on overlap.
    /// (Scrying X2; multi-tile.)
    pub(super) fn scrying_at(&self, x: f32, y: f32) -> Option<(GraphMemberId, i32, i32)> {
        self.view
            .scrying_rects
            .iter()
            .rev()
            .find(|(_, r)| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
            .map(|(member, r)| (*member, (x - r[0]) as i32, (y - r[1]) as i32))
    }

    /// The node whose gloss minimap square contains window point `(x, y)`, if any.
    pub(super) fn gloss_node_at(&self, x: f32, y: f32) -> Option<GraphMemberId> {
        self.view.gloss_node_rects
            .iter()
            .find(|(_, r)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
            .map(|(member, _)| *member)
    }

    /// The node whose gloss "recent" row contains window point `(x, y)`, if any.
    pub(super) fn gloss_recent_at(&self, x: f32, y: f32) -> Option<GraphMemberId> {
        self.view.gloss_recent_rects
            .iter()
            .find(|(_, r)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
            .map(|(member, _)| *member)
    }

    /// The pane (leaf) under window point `(x, y)`, if any.
    pub(super) fn pane_at(&self, x: f32, y: f32) -> Option<PaneId> {
        self.laid_leaves()
            .into_iter()
            .find(|l| x >= l.rect[0] && x < l.rect[2] && y >= l.rect[1] && y < l.rect[3])
            .map(|l| l.pane_id)
    }

    /// The id of the open leaf whose content equals `content`, if any.
    pub(super) fn pane_of_content(&self, content: &PaneContent) -> Option<PaneId> {
        self.view.frame_layout
            .iter_leaves()
            .find(|(_, c, _)| **c == *content)
            .map(|(id, _, _)| id)
    }

    /// The frame divider gutter under window point `(x, y)`, as its split path +
    /// the split's (parent) rect + axis — for starting a divider drag.
    pub(super) fn frame_divider_at(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(Vec<SplitChoice>, [f32; 4], SplitAxis)> {
        let band = self.content_band();
        frame_view::divider_rects(&self.view.frame_layout, band, self.view.maximized_pane)
            .into_iter()
            .find(|d| x >= d.rect[0] && x < d.rect[2] && y >= d.rect[1] && y < d.rect[3])
            .map(|d| (d.path, d.parent, d.axis))
    }

    /// Drive an in-progress frame-divider drag from the current cursor: map the
    /// pointer's position within the split's parent rect to a new ratio.
    pub(super) fn drag_frame_divider(&mut self) {
        let Some((path, parent, axis)) = self.view.frame_divider_drag.clone() else {
            return;
        };
        let (cx, cy) = self.view.cursor;
        let ratio = match axis {
            SplitAxis::Horizontal => {
                (cx - parent[0]) / (parent[2] - parent[0] - frame_view::DIVIDER).max(1.0)
            }
            SplitAxis::Vertical => {
                (cy - parent[1]) / (parent[3] - parent[1] - frame_view::DIVIDER).max(1.0)
            }
        };
        // `set_split_ratio` clamps to a sane minimum so a pane can't collapse.
        self.view.frame_layout.set_split_ratio(&path, ratio);
        self.view.request_redraw();
    }
}
