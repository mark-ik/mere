// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Webpage `Node` — the durable entity that anchors each web page
//! (or addressable artifact) in the graph.
//!
//! Extracted from `graph/mod.rs` per the 2026-04-30 renderer plan §6.4
//! decomposition target. The history-projection types (`NodeNavigationMemory`,
//! `NodeHistoryProjection`, etc.) remain in `graph/mod.rs` for now — they
//! are the natural next decomposition target (as `graph/history.rs`).
//!
//! Browser-runtime state (scroll/form restore, viewer override, compat
//! mode, webview lifecycle) left this struct on 2026-07-09 for the
//! host-owned `BrowserNodeState` sidecar (session-runtime
//! `browser_node_state`), per the mere/turnstone boundary pass plan
//! slice C: the graph library holds graph facts; what the browser
//! knows about a node rides beside the graph, keyed by node id.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

use uuid::Uuid;

use crate::address::{Address, address_from_url};
use crate::types::{
    FrameLayoutHint, ImageRef, ImageRole, NodeClassification, NodeDerivation, NodeImportProvenance,
    NodeProperty, NodeTagPresentationState,
};

/// A webpage node in the graph
#[derive(Debug, Clone)]
pub struct Node {
    /// The one node substrate. Identity, primary-first addresses, authored
    /// content, media type, title, tags, and nested-graph bearing live here.
    ///
    /// `Deref` keeps the field-level read surface source-compatible while the
    /// optional remainder below dissolves into facets rung by rung.
    pub container: chartulary::Container<Uuid, Address>,

    /// Presentation-only metadata for ordering and icon overrides.
    pub tag_presentation: NodeTagPresentationState,

    /// Derived external import provenance for this node.
    pub import_provenance: Vec<NodeImportProvenance>,

    /// Durable provenance-bearing classification records for this node.
    ///
    /// Spec: `graph_enrichment_plan.md §Core Data Model` — carries scheme, value,
    /// label, confidence, provenance, and status for each classification.
    pub classifications: Vec<NodeClassification>,

    /// Cross-graph derivation provenance: records that this node was copied /
    /// forked from a node in another graph (tear-out brief §7.5). Empty for a
    /// natively-minted node. The node-anchored analog of a `Provenance` edge,
    /// for derivations whose source lives in a different graph.
    pub derivations: Vec<NodeDerivation>,

    /// Open literal properties: non-curated literal statements an ingest
    /// preserves (`title` / `tags` are the curated fast-paths), including
    /// datatype and language-tag metadata when present.
    pub properties: Vec<NodeProperty>,

    /// Whether this node's position is pinned (doesn't move with physics)
    pub is_pinned: bool,

    /// Timestamp of last visit
    pub last_visited: std::time::SystemTime,

    /// The app-launch session number this node was last navigated in, stamped by
    /// [`super::Graph::navigate_node`] from [`super::Graph::current_session`]. `0`
    /// means "never stamped" (a node from before this field existed, or never
    /// re-visited since boot) — by-sessions eviction treats that as undated, never
    /// evicted (mirrors the by-time policy's "never drop what we cannot date").
    /// (Alembic B5.)
    pub last_session_visited: u64,

    /// Content-addressed preview imagery, keyed by role. The node carries
    /// ~40-byte [`ImageRef`] handles; the pixels live in the durable blob
    /// store under `content/image/<hex>` (see
    /// `session-runtime::image_store`).
    ///
    /// This is the node-image externalization plan's phase 2. Inline
    /// `thumbnail_png` / `favicon_rgba` bytes used to ride here and dominated
    /// the graph: at 50k nodes they were 64% of live heap, and the lane-D gate
    /// measured them at 18x the snapshot size and 3.8x its load time. A
    /// preview is *experience*, not truth, so it belongs in a bounded,
    /// collectable cache rather than in every node forever.
    ///
    /// Read through [`Node::image`] / [`Node::favicon`] / [`Node::preview`]
    /// rather than indexing the map, so call sites name a role.
    pub images: BTreeMap<ImageRole, ImageRef>,

    /// Durable split arrangement annotations for frame-anchor nodes.
    pub frame_layout_hints: Vec<FrameLayoutHint>,

    /// Durable opt-out for split-offer affordances on frame-anchor nodes.
    pub frame_split_offer_suppressed: bool,

}

impl Deref for Node {
    type Target = chartulary::Container<Uuid, Address>;

    fn deref(&self) -> &Self::Target {
        &self.container
    }
}

impl DerefMut for Node {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.container
    }
}

impl Node {
    /// The image reference held for `role`, if any.
    pub fn image(&self, role: ImageRole) -> Option<&ImageRef> {
        self.images.get(&role)
    }

    /// The favicon reference, if one has been stored.
    pub fn favicon(&self) -> Option<&ImageRef> {
        self.image(ImageRole::Favicon)
    }

    /// The preview reference, if one has been stored. This is the role the
    /// legacy inline `thumbnail_png` migrates into.
    pub fn preview(&self) -> Option<&ImageRef> {
        self.image(ImageRole::Preview)
    }

    /// Attach (or replace) the reference for `role`, returning any prior one.
    /// The pixels must already be in the blob store; the node only carries
    /// the handle.
    pub fn set_image(&mut self, role: ImageRole, image: ImageRef) -> Option<ImageRef> {
        self.images.insert(role, image)
    }

    /// Drop the reference for `role`, returning it. The blob itself is left
    /// for the orphan-GC pass, per the manifest-deletion doctrine: dropping a
    /// reference is not a delete.
    pub fn clear_image(&mut self, role: ImageRole) -> Option<ImageRef> {
        self.images.remove(&role)
    }

    /// Returns the node's canonical retrieval address (the first address).
    pub fn primary_address(&self) -> &Address {
        self.addresses
            .first()
            .expect("Node invariant violated: no primary address")
    }

    /// Returns the canonical retrieval URL string. Convenience over
    /// [`Node::primary_address`].
    pub fn url(&self) -> &str {
        self.primary_address().as_url_str()
    }

    // Per-node navigation history moved to the graph-level shared visit space
    // (`Graph.nav` / `SharedNavigationMemory`); read it via the `Graph::node_history_*`
    // / `Graph::node_current_url` methods. (The (b) anchor design, 2026-06-06.)

    #[cfg(not(target_arch = "wasm32"))]
    pub fn test_stub(url: &str) -> Self {
        Self {
            container: chartulary::Container::with_identity(Uuid::new_v4())
                .with_address_record(address_from_url(url))
                .with_title(url),
            tag_presentation: NodeTagPresentationState::default(),
            import_provenance: Vec::new(),
            classifications: Vec::new(),
            derivations: Vec::new(),
            properties: Vec::new(),
            is_pinned: false,
            last_visited: std::time::SystemTime::now(),
            last_session_visited: 0,
            images: BTreeMap::new(),
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
        }
    }
}
