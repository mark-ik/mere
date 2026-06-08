/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # frame
//!
//! Frame domain layer — defines the savable layout of resizable panes
//! and projects it into a uxtree subtree.
//!
//! See the crate README for the conceptual scope and the relationship
//! to legacy `platen::FrameState` / `shell-state`.
//!
//! Split across submodules to keep each file under the workspace's
//! 600-LOC ceiling: [`layout`] holds the [`FrameLayout`] operations
//! ([`FrameLayout::summon_leaf`], `reparent_leaf`, `close_leaf`, …);
//! [`projection`] holds [`project_frame`] + [`project_frame_with`].

#![doc(html_root_url = "https://docs.rs/frame/0.0.1")]

use serde::{Deserialize, Serialize};

mod layout;
mod projection;

#[cfg(test)]
mod tests;

pub use projection::{project_frame, project_frame_with};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Stable identifier for a saved frame layout.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameId(pub String);

impl FrameId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identifier for an individual pane within a frame layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u64);

/// Stable identifier for a graph at app scope. Every leaf in a
/// `FrameLayout` carries one so the host can resolve "which graph
/// does this panel render?" against the app's `GraphRegistry`.
///
/// Frame layouts persist with serialized graph IDs so a saved
/// arrangement reattaches to the right graphs on next launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphId(pub uuid::Uuid);

impl GraphId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl Default for GraphId {
    fn default() -> Self {
        Self::new()
    }
}

/// Durable session identity. Wraps the runtime/session shape: a
/// session owns a root graph (and may grow sub-graph references),
/// holds the worker manifest, engine profile binding, and policy
/// overrides. v0 of session-persistence maps one `SessionId` 1:1
/// to one root `GraphId`; the type distinction is enforced from
/// day one so later phases (sub-graphs, fork-on-divergence,
/// multi-graph-per-session) don't require a painful retrofit.
///
/// See `design_docs/mere_docs/research/2026-05-11_browser_multiplexer_framing.md`
/// §2 (identity matrix) for the broader identity model and
/// `design_docs/mere_docs/implementation_strategy/2026-05-11_graph_session_manifest_plan.md`
/// for storage / lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub uuid::Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Direction of a split between two child panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitAxis {
    /// Children are arranged side-by-side; ratio applies to width.
    Horizontal,
    /// Children are stacked vertically; ratio applies to height.
    Vertical,
}

/// Identity of a single node within a graph. Mirrors petgraph's
/// `NodeIndex` but kept as an opaque wrapped `u32` here so
/// `frame` doesn't depend on `kernel` / petgraph. The
/// host resolves these against the leaf's `graph_id` to get the
/// real node entity. v0: `usize` index as a `u32` — matches
/// petgraph's `NodeIndex::index()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LeafNodeRef(pub u32);

/// What a leaf pane shows. Extension point: `Custom` carries a
/// host-defined content kind for content not yet promoted to a
/// dedicated mere-domain module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneContent {
    Workbench,
    Orrery,
    Gloss,
    /// The graph's manifest — every primitive (nodes / facets, edges, fields),
    /// examinable. The data-view counterpart to the orrery's space-view (see the
    /// graph-roster + frame-taxonomy design doc).
    Roster,
    Apparatus,
    System,
    /// **Pinned tile** — a single specific tile rendered without a
    /// workbench strip. Per the pane-UX brief §3 frametree
    /// side-by-side rendering. Pairs with the leaf's existing
    /// `graph_id` to fully identify the node; document is looked
    /// up at render time from whichever workbench in the window
    /// has that tile open (or falls back to re-fetch if no live
    /// workbench has it cached).
    Tile(LeafNodeRef),
    Custom(String),
}

impl PaneContent {
    /// Compact tag suitable for tracing fields and accessible names.
    pub fn tag(&self) -> &str {
        match self {
            PaneContent::Workbench => "workbench",
            PaneContent::Orrery => "orrery",
            PaneContent::Gloss => "gloss",
            PaneContent::Roster => "roster",
            PaneContent::Apparatus => "apparatus",
            PaneContent::System => "system",
            PaneContent::Tile(_) => "tile",
            PaneContent::Custom(s) => s.as_str(),
        }
    }
}

/// One node in the layout tree: either a split (two children at a
/// given axis + ratio) or a leaf (one pane showing a content kind
/// bound to a graph).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PaneNode {
    Split {
        axis: SplitAxis,
        /// Fraction of the parent occupied by `first`; `second` takes
        /// `1.0 - ratio`. Clamped by consumers to a sane minimum.
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
    Leaf {
        pane_id: PaneId,
        content: PaneContent,
        /// Which graph this panel renders. The host resolves the
        /// `GraphId` against its `GraphRegistry` to get the live
        /// `Entity<Graph>`. Multiple leaves carrying the same
        /// `graph_id` share a graph; differing IDs in one frame =
        /// multi-graph window.
        ///
        /// `#[serde(default)]` allows pre-`graph_id` layouts saved
        /// to disk to deserialize — they come back as a nil UUID,
        /// which the host stamps with the window's primary graph
        /// on load.
        #[serde(default)]
        graph_id: GraphId,
    },
}

/// One step into a [`PaneNode::Split`] when walking the layout tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SplitChoice {
    First,
    Second,
}

/// Path through the layout tree to a specific split, expressed as
/// `First`/`Second` choices at each branch. Empty path = root.
pub type SplitPath = Vec<SplitChoice>;

/// A complete frame: identity, label, and the layout tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameLayout {
    pub id: FrameId,
    pub label: String,
    pub root: PaneNode,
}

/// Where to insert a new leaf relative to an existing leaf at a
/// `SplitPath`. Used by [`FrameLayout::summon_leaf`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertSide {
    /// New leaf goes left of the existing leaf (horizontal split).
    Left,
    /// New leaf goes right of the existing leaf.
    Right,
    /// New leaf goes above the existing leaf (vertical split).
    Above,
    /// New leaf goes below the existing leaf.
    Below,
}

impl InsertSide {
    pub(crate) fn split_axis(self) -> SplitAxis {
        match self {
            Self::Left | Self::Right => SplitAxis::Horizontal,
            Self::Above | Self::Below => SplitAxis::Vertical,
        }
    }

    /// True when the new leaf goes in `first` position (left / above);
    /// false when it goes in `second` (right / below).
    pub(crate) fn new_is_first(self) -> bool {
        matches!(self, Self::Left | Self::Above)
    }
}
