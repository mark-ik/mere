// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The kernel's physical node wrapper around chartulary's neutral Container.
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
use crate::types::{ImageRef, ImageRole};

/// A neutral Container in the kernel graph.
///
/// The only physical residue outside Container is the D0 image-reference map.
/// Those small, content-addressed experience handles stay here so paint can
/// resolve them without consulting a metadata facet; pixels remain out of line.
#[derive(Debug, Clone)]
pub struct Node {
    /// The one node substrate. Identity, primary-first addresses, authored
    /// content, media type, title, tags, and nested-graph bearing live here.
    ///
    /// `Deref` keeps the Container capability surface direct.
    pub container: chartulary::Container<Uuid, Address>,

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
            images: BTreeMap::new(),
        }
    }
}
