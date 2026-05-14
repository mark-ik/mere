/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-pane state in a window's FrameLayout.
//!
//! `HostRoot` holds a `HashMap<PaneId, PaneState>` keyed by leaf id.
//! Entries are created lazily on first paint and live as long as the
//! leaf stays in the layout. Closing a leaf (via
//! `FrameLayout::close_leaf`) drops its `PaneState`.
//!
//! This split makes multi-panel windows possible: two orrery leaves
//! each get their own camera + interaction engine + bounds sink; two
//! workbench leaves each get their own tile list. Stateless panels
//! (gloss, apparatus) get `PaneState::Other`.

use std::cell::Cell;
use std::rc::Rc;

use euclid::default::Vector2D;
use gpui::{Bounds, Pixels};
use graph_canvas::camera::CanvasCamera;
use graph_canvas::engine::{InteractionConfig, InteractionEngine};
use graph_tree::{GraphTree, LayoutMode, ProjectionLens};
use mere_frame::PaneContent;
use mere_kernel::graph::NodeKey;

use crate::tiles::TileManager;

pub(crate) enum PaneState {
    Orrery(OrreryPaneState),
    Workbench(WorkbenchPaneState),
    /// Gloss, apparatus, and other panels that derive everything
    /// from app-scope state per paint. No per-instance bits to keep.
    Other,
}

pub(crate) struct OrreryPaneState {
    pub camera: CanvasCamera,
    pub zoom_target: f32,
    pub engine: InteractionEngine<NodeKey>,
    pub bounds_sink: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Default for OrreryPaneState {
    fn default() -> Self {
        Self {
            camera: CanvasCamera::new(Vector2D::zero(), 1.0),
            zoom_target: 1.0,
            engine: InteractionEngine::new(InteractionConfig::default()),
            bounds_sink: Rc::new(Cell::new(None)),
        }
    }
}

pub(crate) struct WorkbenchPaneState {
    pub tiles: TileManager,
    /// Per-workbench **lineage facet** — graph-tree view of which
    /// anchor nodes are in this workbench plus how the user got
    /// from one to another (`Provenance::Traversal`). New-tile
    /// creation populates this in `host_navigation::navigate_to`'s
    /// NewTile branch.
    ///
    /// Per the node-per-tile + lineage facet design (see
    /// `design_docs/mere_docs/implementation_strategy/2026-05-11_node_per_tile_lineage_plan.md`
    /// §5.1): each workbench owns its own tree. Same graph, two
    /// workbenches = two independent lineage views over the same
    /// nodes. Phase 3's `Branch` operation adds graphlets here.
    pub tree: GraphTree<NodeKey>,
}

impl Default for WorkbenchPaneState {
    fn default() -> Self {
        Self {
            tiles: TileManager::new(),
            tree: GraphTree::new(LayoutMode::default(), ProjectionLens::default()),
        }
    }
}

impl PaneState {
    /// Default state for a freshly-summoned leaf of the given kind.
    pub fn for_content(content: &PaneContent) -> Self {
        match content {
            PaneContent::Orrery => Self::Orrery(OrreryPaneState::default()),
            PaneContent::Workbench => Self::Workbench(WorkbenchPaneState::default()),
            // Pinned-tile leaves don't carry per-instance state —
            // the document lives in whatever workbench has the
            // tile open (looked up at render time).
            _ => Self::Other,
        }
    }

    pub fn as_orrery(&self) -> Option<&OrreryPaneState> {
        match self {
            Self::Orrery(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_orrery_mut(&mut self) -> Option<&mut OrreryPaneState> {
        match self {
            Self::Orrery(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_workbench(&self) -> Option<&WorkbenchPaneState> {
        match self {
            Self::Workbench(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_workbench_mut(&mut self) -> Option<&mut WorkbenchPaneState> {
        match self {
            Self::Workbench(s) => Some(s),
            _ => None,
        }
    }
}
