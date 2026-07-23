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
//! `browser_node_state`), per the mere/merecat boundary pass plan
//! slice C: the graph library holds graph facts; what the browser
//! knows about a node rides beside the graph, keyed by node id.

use std::collections::HashSet;

use rkyv::{Archive, Deserialize, Serialize};
use uuid::Uuid;

use super::identity::{LogIdAsString, UuidAsBytes};
use crate::address::{Address, AddressClaim, address_from_url, cached_host_from_url};
use crate::types::{
    FrameLayoutHint, NodeClassification, NodeDerivation, NodeImportProvenance, NodeProperty,
    NodeTagPresentationState,
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
    #[rkyv(with = rkyv::with::AsUnixTime)]
    pub last_visited: std::time::SystemTime,

    /// The app-launch session number this node was last navigated in, stamped by
    /// [`super::Graph::navigate_node`] from [`super::Graph::current_session`]. `0`
    /// means "never stamped" (a node from before this field existed, or never
    /// re-visited since boot) — by-sessions eviction treats that as undated, never
    /// evicted (mirrors the by-time policy's "never drop what we cannot date").
    /// (Alembic B5.)
    pub last_session_visited: u64,

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

    /// Optional declared or sniffed MIME type — content classification
    /// (what kind of content this node holds), consumed by mere-domain
    /// surfaces (roster bucketing, note-format detection) and, host-side,
    /// viewer selection. Set at node creation from URL extension sniffing;
    /// may be updated by `SetNodeMimeHint` when content-byte detection or
    /// a Content-Type header provides a more precise value.
    ///
    /// Deliberately kernel-side where scroll/viewer/compat state is not:
    /// mime is a fact about the content; those were facts about the
    /// browser's handling of it (now the host's `BrowserNodeState`).
    pub mime_hint: Option<String>,

    /// Inline authored content body — a knot note's djot source, for nodes whose
    /// content is authored in place rather than fetched. `None` for fetched / remote
    /// nodes (their content lives in the durable content cache). Mutable (the live note
    /// editor writes it) and persisted with the node, so it travels on snapshot / sync /
    /// fork, unlike the local content cache. (Djot editor reframe, slice 3 — the inline
    /// `Node` body.)
    pub body: Option<String>,

    /// Address claims attached to this node — Primary + zero-or-more Aliases.
    ///
    /// Per the [node identity + duplicates brief](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/research/2026-05-18_node_identity_and_duplicates_brief.md):
    /// identity is `id: Uuid` (above); addresses are properties of the node.
    /// Exactly one claim must have role `AddressRole::Primary`; the rest are
    /// `Alias` (mirrors, cross-protocol pairs, user-declared aliases).
    ///
    /// Use [`Node::primary_address`] for the canonical retrieval target;
    /// iterate `addresses` for aliases.
    pub addresses: Vec<AddressClaim>,

    /// Durable split arrangement annotations for frame-anchor nodes.
    pub frame_layout_hints: Vec<FrameLayoutHint>,

    /// Durable opt-out for split-offer affordances on frame-anchor nodes.
    pub frame_split_offer_suppressed: bool,

    /// The nested graph this node BEARS, by log identity — structural
    /// containment (chartulary `GraphBearing`; the one-node ruling's
    /// containment tier). `None` for the ordinary node. A denizen's inner
    /// world hangs here; the residency facet keeps only agency (subject +
    /// kind). Graph truth: persists, journals attributed, and is deliberately
    /// NOT carried by a cross-graph copy (a fork's copy is un-resided rather
    /// than sharing one world; the slot-convention world move is the
    /// follow-on that makes forked worlds real copies).
    #[rkyv(with = rkyv::with::Map<LogIdAsString>)]
    pub nested: Option<codicil::LogId>,
}

impl Node {
    /// Returns the node's canonical retrieval address (the Primary claim).
    ///
    /// Panics if the per-node invariant (exactly one Primary claim) is
    /// violated — which the constructors guarantee.
    pub fn primary_address(&self) -> &Address {
        self.addresses
            .iter()
            .find(|c| c.is_primary())
            .map(|c| &c.address)
            .expect("Node invariant violated: no Primary AddressClaim")
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
            id: Uuid::new_v4(),
            cached_host: cached_host_from_url(url),
            title: url.to_string(),
            tags: HashSet::new(),
            tag_presentation: NodeTagPresentationState::default(),
            import_provenance: Vec::new(),
            classifications: Vec::new(),
            derivations: Vec::new(),
            properties: Vec::new(),
            is_pinned: false,
            last_visited: std::time::SystemTime::now(),
            last_session_visited: 0,
            thumbnail_png: None,
            thumbnail_width: 0,
            thumbnail_height: 0,
            favicon_rgba: None,
            favicon_width: 0,
            favicon_height: 0,
            mime_hint: None,
            body: None,
            addresses: vec![AddressClaim::primary(address_from_url(url))],
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
            nested: None,
        }
    }
}
