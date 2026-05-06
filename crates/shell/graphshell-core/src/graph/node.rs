/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Webpage `Node` and `NodeLifecycle` — the durable entity that
//! anchors each web page (or addressable artifact) in the graph.
//!
//! Extracted from `graph/mod.rs` per the 2026-04-30 renderer plan §6.4
//! decomposition target. The history-projection types (`NodeNavigationMemory`,
//! `NodeHistoryProjection`, etc.) remain in `graph/mod.rs` for now — they
//! are the natural next decomposition target (as `graph/history.rs`).

use std::collections::HashSet;

use euclid::default::{Point2D, Vector2D};
use rkyv::{Archive, Deserialize, Serialize};
use uuid::Uuid;

use super::identity::{Point2DAsTuple, UuidAsBytes, Vector2DAsTuple};
use super::{
    NodeHistoryBranchProjection, NodeHistoryProjection, NodeHistorySemanticSummary,
    NodeNavigationMemory,
};
use crate::address::{Address, address_from_url, cached_host_from_url};
use crate::types::{
    FrameLayoutHint, NodeClassification, NodeImportProvenance, NodeTagPresentationState,
};

/// A webpage node in the graph
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
pub struct Node {
    /// Stable node identity.
    #[rkyv(with = UuidAsBytes)]
    pub id: Uuid,

    /// Cached hostname derived from the node's address for UI label rendering.
    pub cached_host: Option<String>,

    /// Page title (or URL if no title)
    pub title: String,

    /// Transient projected position in graph space.
    ///
    /// Render and physics code may move this continuously between reducer
    /// commits. `pub(crate)` so `impl Graph` in `graph/mod.rs` can reach
    /// it directly; external callers use the [`Node::projected_position`]
    /// accessor.
    #[rkyv(with = Point2DAsTuple)]
    pub(crate) position: Point2D<f32>,

    /// Durable committed position used for snapshots and reducer-authored
    /// moves. `pub(crate)` for the same reason as `position`.
    #[rkyv(with = Point2DAsTuple)]
    pub(crate) committed_position: Point2D<f32>,

    /// Velocity for physics simulation
    #[rkyv(with = Vector2DAsTuple)]
    pub velocity: Vector2D<f32>,

    /// Canonical durable semantic tags for this node.
    pub tags: HashSet<String>,

    /// Presentation-only metadata for ordering and icon overrides.
    pub tag_presentation: NodeTagPresentationState,

    /// Derived external import provenance for this node.
    pub import_provenance: Vec<NodeImportProvenance>,

    /// Durable provenance-bearing classification records for this node.
    ///
    /// Spec: `graph_enrichment_plan.md §Core Data Model` — carries scheme, value,
    /// label, confidence, provenance, and status for each classification.
    pub classifications: Vec<NodeClassification>,

    /// Whether this node's position is pinned (doesn't move with physics)
    pub is_pinned: bool,

    /// Timestamp of last visit
    #[rkyv(with = rkyv::with::AsUnixTime)]
    pub last_visited: std::time::SystemTime,

    /// Owner-scoped persisted navigation memory for this node's mapped webview.
    pub navigation_memory: NodeNavigationMemory,

    /// Optional thumbnail bytes (PNG), persisted in snapshots.
    pub thumbnail_png: Option<Vec<u8>>,

    /// Thumbnail width in pixels (valid when `thumbnail_png` is `Some`).
    pub thumbnail_width: u32,

    /// Thumbnail height in pixels (valid when `thumbnail_png` is `Some`).
    pub thumbnail_height: u32,

    /// Optional favicon pixel data (RGBA8), persisted in snapshots.
    pub favicon_rgba: Option<Vec<u8>>,

    /// Favicon width in pixels (valid when `favicon_rgba` is `Some`).
    pub favicon_width: u32,

    /// Favicon height in pixels (valid when `favicon_rgba` is `Some`).
    pub favicon_height: u32,

    /// Last known scroll offset for higher-fidelity cold restore.
    pub session_scroll: Option<(f32, f32)>,

    /// Optional best-effort form draft payload (feature-guarded by caller policy).
    pub session_form_draft: Option<String>,

    /// Optional declared or sniffed MIME type; drives renderer selection.
    ///
    /// Set at node creation time from URL extension sniffing; may be updated by
    /// WAL entry `UpdateNodeMimeHint` when content-byte detection or a
    /// Content-Type header provides a more precise value.
    pub mime_hint: Option<String>,

    /// User-set viewer override that takes precedence over all MIME/address-based
    /// selection.  Stored in graph data and survives persistence/sync.
    /// `None` means "use automatic viewer selection".
    pub viewer_override: Option<String>,

    /// Per-node "compatibility mode" toggle. When `true`, verso routes
    /// web-managed content for this node through `WebEnginePreference::Wry`
    /// (platform WebView) regardless of the app-level default web backend.
    /// Middlenet-routed content (feeds, gemini, etc.) is unaffected because
    /// compat mode only shifts the web-engine preference, not the lane
    /// decision.
    ///
    /// Distinct from `viewer_override`: `viewer_override` pins a specific
    /// viewer id; `compat_mode` only biases the web-engine preference and
    /// lets verso's routing policy still apply.
    pub compat_mode: bool,

    /// Typed address — carries both the URL scheme classification and the raw
    /// URL string (or clip id for clip routes). Use `address.address_kind()` to
    /// get the scheme classification and `address.as_url_str()` to get the URL.
    pub address: Address,

    /// Durable split arrangement annotations for frame-anchor nodes.
    pub frame_layout_hints: Vec<FrameLayoutHint>,

    /// Durable opt-out for split-offer affordances on frame-anchor nodes.
    pub frame_split_offer_suppressed: bool,

    /// Webview lifecycle state
    pub lifecycle: NodeLifecycle,
}

/// Lifecycle state for webview management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub enum NodeLifecycle {
    /// Active webview (visible, rendering)
    Active,

    /// Warm webview (kept alive in memory but not currently visible in a pane)
    Warm,

    /// Cold (metadata only, no process)
    Cold,

    /// Tombstoned node retained for history/identity continuity but not live rendering/runtime.
    Tombstone,
}

impl Node {
    pub fn projected_position(&self) -> Point2D<f32> {
        self.position
    }

    pub fn committed_position(&self) -> Point2D<f32> {
        self.committed_position
    }

    /// Returns the node's raw URL string.
    pub fn url(&self) -> &str {
        self.address.as_url_str()
    }

    pub fn history_projection(&self) -> NodeHistoryProjection {
        self.navigation_memory.projection()
    }

    pub fn history_entries(&self) -> Vec<String> {
        self.history_projection().entries
    }

    pub fn history_index(&self) -> usize {
        self.history_projection().current_index
    }

    pub fn current_history_url(&self) -> Option<String> {
        self.navigation_memory.current_url()
    }

    pub fn history_branch_projection(&self) -> NodeHistoryBranchProjection {
        self.navigation_memory.branch_projection()
    }

    pub fn history_semantic_summary(&self) -> NodeHistorySemanticSummary {
        self.navigation_memory.semantic_summary()
    }

    pub fn replace_history_state(&mut self, entries: Vec<String>, current_index: usize) {
        self.navigation_memory
            .replace_linear_history(entries, current_index);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn test_stub(url: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            cached_host: cached_host_from_url(url),
            title: url.to_string(),
            position: Point2D::new(0.0, 0.0),
            committed_position: Point2D::new(0.0, 0.0),
            velocity: Vector2D::new(0.0, 0.0),
            tags: HashSet::new(),
            tag_presentation: NodeTagPresentationState::default(),
            import_provenance: Vec::new(),
            classifications: Vec::new(),
            is_pinned: false,
            last_visited: std::time::SystemTime::now(),
            navigation_memory: NodeNavigationMemory::empty(),
            thumbnail_png: None,
            thumbnail_width: 0,
            thumbnail_height: 0,
            favicon_rgba: None,
            favicon_width: 0,
            favicon_height: 0,
            session_scroll: None,
            session_form_draft: None,
            mime_hint: None,
            viewer_override: None,
            compat_mode: false,
            address: address_from_url(url),
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
            lifecycle: NodeLifecycle::Cold,
        }
    }
}
