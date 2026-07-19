// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Node-property setters and per-node query accessors.
//!
//! All of Graph's property mutators for individual nodes: title,
//! thumbnail, favicon, mime hint, pinned, frame-layout hints, tags,
//! classifications, tag-icon override, position (committed +
//! projected), plus the per-node query accessors that sit alongside
//! them (`node_tags`, `node_classifications`, `frame_layout_hints`,
//! `node_projected_position`, `node_committed_position`,
//! `projected_centroid`). Browser-runtime setters (session scroll,
//! form draft, viewer override, compat mode, lifecycle) left with the
//! `BrowserNodeState` sidecar (boundary pass slice C, 2026-07-09).
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel
//! decomposition pass. Lifecycle ops (`new`, `add_node`,
//! `add_node_with_id`, `remove_node`, `update_node_url`,
//! `recompute_cached_hosts`) stay in `mod.rs` since they own Graph's
//! construction shape; everything property-setter-shaped lives here.

use std::collections::HashSet;
use std::time::{Duration, UNIX_EPOCH};

use super::Graph;
use super::identity::NodeKey;
use super::node::Node;
use crate::types::{
    ClassificationProvenance, ClassificationScheme, ClassificationStatus, FrameLayoutHint,
    NodeClassification, NodeImportProvenance, NodeProperty, NodeTagPresentationState,
};

impl Graph {
    pub(crate) fn set_node_title(&mut self, key: NodeKey, title: String) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.title == title {
            return false;
        }
        node.title = title;
        true
    }

    pub(crate) fn set_node_thumbnail(
        &mut self,
        key: NodeKey,
        png_bytes: Vec<u8>,
        width: u32,
        height: u32,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.thumbnail_png.as_ref() == Some(&png_bytes)
            && node.thumbnail_width == width
            && node.thumbnail_height == height
        {
            return false;
        }
        node.thumbnail_png = Some(png_bytes);
        node.thumbnail_width = width;
        node.thumbnail_height = height;
        true
    }

    pub(crate) fn set_node_favicon(
        &mut self,
        key: NodeKey,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.favicon_rgba.as_ref() == Some(&rgba)
            && node.favicon_width == width
            && node.favicon_height == height
        {
            return false;
        }
        node.favicon_rgba = Some(rgba);
        node.favicon_width = width;
        node.favicon_height = height;
        true
    }

    pub(crate) fn set_node_mime_hint(&mut self, key: NodeKey, mime_hint: Option<String>) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.mime_hint == mime_hint {
            return false;
        }
        node.mime_hint = mime_hint;
        true
    }

    pub(crate) fn set_node_pinned(&mut self, key: NodeKey, is_pinned: bool) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.is_pinned == is_pinned {
            return false;
        }
        node.is_pinned = is_pinned;
        true
    }

    pub(crate) fn append_frame_layout_hint(&mut self, key: NodeKey, hint: FrameLayoutHint) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        node.frame_layout_hints.push(hint);
        true
    }

    pub(crate) fn remove_frame_layout_hint_at(&mut self, key: NodeKey, hint_index: usize) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if hint_index >= node.frame_layout_hints.len() {
            return false;
        }
        node.frame_layout_hints.remove(hint_index);
        true
    }

    pub(crate) fn move_frame_layout_hint(
        &mut self,
        key: NodeKey,
        from_index: usize,
        to_index: usize,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if from_index >= node.frame_layout_hints.len()
            || to_index >= node.frame_layout_hints.len()
            || from_index == to_index
        {
            return false;
        }
        let hint = node.frame_layout_hints.remove(from_index);
        node.frame_layout_hints.insert(to_index, hint);
        true
    }

    pub(crate) fn set_frame_split_offer_suppressed(
        &mut self,
        key: NodeKey,
        suppressed: bool,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.frame_split_offer_suppressed == suppressed {
            return false;
        }
        node.frame_split_offer_suppressed = suppressed;
        true
    }

    pub fn frame_layout_hints(&self, key: NodeKey) -> Option<&[FrameLayoutHint]> {
        self.get_node(key)
            .map(|node| node.frame_layout_hints.as_slice())
    }

    pub fn frame_split_offer_suppressed(&self, key: NodeKey) -> Option<bool> {
        self.get_node(key)
            .map(|node| node.frame_split_offer_suppressed)
    }

    pub(crate) fn insert_node_tag(&mut self, key: NodeKey, tag: String) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        let inserted = node.tags.insert(tag.clone());
        if inserted && !node.tag_presentation.ordered_tags.contains(&tag) {
            node.tag_presentation.ordered_tags.push(tag);
        }
        inserted
    }

    pub(crate) fn remove_node_tag(&mut self, key: NodeKey, tag: &str) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        let removed = node.tags.remove(tag);
        if removed {
            node.tag_presentation
                .ordered_tags
                .retain(|entry| entry != tag);
            node.tag_presentation.icon_overrides.remove(tag);
        }
        removed
    }

    /// Set a node's inline authored content body (a knot note's djot source).
    /// Returns whether the body actually changed. The sanctioned write path for
    /// `Node::body` — the note editor and web-clip previously reached it through
    /// `get_node_mut` (write-path migration, 2026-07-01).
    pub(crate) fn set_node_body(&mut self, key: NodeKey, body: Option<String>) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.body == body {
            return false;
        }
        node.body = body;
        true
    }

    /// Append an open literal property, deduplicating the full literal record
    /// `(predicate, value, datatype, lang)`. Returns whether the property was
    /// newly added. The sanctioned write path for `Node::properties` — the
    /// linked-data ingest previously pushed through `get_node_mut` (write-path
    /// migration, 2026-07-01).
    pub(crate) fn append_node_property(&mut self, key: NodeKey, property: NodeProperty) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if let Some(existing) = node
            .properties
            .iter_mut()
            .find(|existing| existing.content_eq(&property))
        {
            if existing.provenance_iri != property.provenance_iri
                || existing.asserted_at_ms != property.asserted_at_ms
            {
                existing.provenance_iri = property.provenance_iri;
                existing.asserted_at_ms = property.asserted_at_ms;
                return true;
            }
            return false;
        }
        node.properties.push(property);
        true
    }

    pub fn node_tags(&self, key: NodeKey) -> Option<&HashSet<String>> {
        self.get_node(key).map(|node| &node.tags)
    }

    pub fn node_tag_presentation(&self, key: NodeKey) -> Option<&NodeTagPresentationState> {
        self.get_node(key).map(|node| &node.tag_presentation)
    }

    pub fn node_import_provenance(&self, key: NodeKey) -> Option<&[NodeImportProvenance]> {
        self.get_node(key)
            .map(|node| node.import_provenance.as_slice())
    }

    // --- Classification accessors (Stage A) ---

    pub fn node_classifications(&self, key: NodeKey) -> Option<&[NodeClassification]> {
        self.get_node(key)
            .map(|node| node.classifications.as_slice())
    }

    /// Add a classification record to a node.
    ///
    /// Deduplicates by `(scheme, value)`. Returns `true` if the record was inserted.
    pub(crate) fn add_node_classification(
        &mut self,
        key: NodeKey,
        classification: NodeClassification,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        let already_exists = node
            .classifications
            .iter()
            .any(|c| c.scheme == classification.scheme && c.value == classification.value);
        if already_exists {
            return false;
        }
        node.classifications.push(classification);
        true
    }

    /// Append a [`NodeDerivation`](crate::types::NodeDerivation) to `key`'s
    /// provenance record — e.g. a harvested link node that was `ExtractedFrom`
    /// its source page (capture plan C3). Idempotent: a duplicate (same sub-kind
    /// + source) is not re-added. Returns whether it was added. Same-graph
    /// derivations are recorded after the fact here; cross-graph ones ride node
    /// creation via `copy_node_from`.
    pub(crate) fn record_derivation(
        &mut self,
        key: NodeKey,
        derivation: crate::types::NodeDerivation,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.derivations.contains(&derivation) {
            return false;
        }
        node.derivations.push(derivation);
        true
    }

    /// Remove all classification records matching `(scheme, value)`.
    ///
    /// Returns `true` if at least one record was removed.
    pub(crate) fn remove_node_classification(
        &mut self,
        key: NodeKey,
        scheme: &ClassificationScheme,
        value: &str,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        let before = node.classifications.len();
        node.classifications
            .retain(|c| !(c.scheme == *scheme && c.value == value));
        node.classifications.len() < before
    }

    /// Update the `status` of a classification record identified by `(scheme, value)`.
    ///
    /// Returns `true` if a matching record was found and updated.
    pub(crate) fn set_node_classification_status(
        &mut self,
        key: NodeKey,
        scheme: &ClassificationScheme,
        value: &str,
        status: ClassificationStatus,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        let mut found = false;
        for c in node.classifications.iter_mut() {
            if c.scheme == *scheme && c.value == value {
                c.status = status.clone();
                found = true;
            }
        }
        found
    }

    /// Promote a classification record to primary for its scheme; demotes all others.
    ///
    /// Returns `true` if a matching record was found.
    pub(crate) fn set_node_primary_classification(
        &mut self,
        key: NodeKey,
        scheme: &ClassificationScheme,
        value: &str,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        let mut found = false;
        for c in node.classifications.iter_mut() {
            if c.scheme == *scheme {
                c.primary = c.value == value;
                if c.value == value {
                    found = true;
                }
            }
        }
        found
    }

    pub(crate) fn set_node_tag_icon_override(
        &mut self,
        key: NodeKey,
        tag: &str,
        icon: Option<crate::types::BadgeIcon>,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if !node.tags.contains(tag) || tag.starts_with('#') || tag.starts_with("udc:") {
            return false;
        }
        match icon {
            Some(icon) => {
                if node.tag_presentation.icon_overrides.get(tag) == Some(&icon) {
                    return false;
                }
                node.tag_presentation
                    .icon_overrides
                    .insert(tag.to_string(), icon);
                true
            }
            None => node.tag_presentation.icon_overrides.remove(tag).is_some(),
        }
    }

    // `set_node_position` / `set_node_projected_position` / `node_projected_position`
    // left with `Node.position` (S2): a node's position is no longer graph truth.
    // The live position is seiche's; the durable position is the cartography
    // sidecar's; the host reads and writes those, not the graph.

    // `projected_centroid` (the mean of node positions) left with S2's position
    // dissolution: a centroid over positions is a view concern computed from
    // seiche's live layout host-side, not a kernel method over the transient
    // `Node.position`. It had no production caller.

    pub(crate) fn touch_node_last_visited_now(&mut self, key: NodeKey) -> bool {
        self.set_node_last_visited_at_ms(key, Graph::epoch_ms())
    }

    pub(crate) fn set_node_last_visited_at_ms(&mut self, key: NodeKey, timestamp_ms: u64) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        node.last_visited = UNIX_EPOCH + Duration::from_millis(timestamp_ms);
        true
    }

    /// Internal setter for `Node::history`. Tightened to `pub(crate)` so the
    /// only reachable write surface from outside `kernel` is
    /// `GraphDelta::UpdateNodeHistory`, dispatched via the
    /// `app::history::GraphBrowserApp::apply_node_history_change` helper.
    pub(crate) fn set_node_history_state(
        &mut self,
        key: NodeKey,
        history_entries: Vec<String>,
        history_index: usize,
    ) -> bool {
        let Some(id) = self.inner.node(key).map(|n| n.id) else {
            return false;
        };
        let clamped_index = if history_entries.is_empty() {
            0
        } else {
            history_index.min(history_entries.len() - 1)
        };
        let current = self.nav.projection(id);
        if current.entries == history_entries && current.current_index == clamped_index {
            return false;
        }
        // Reset this node's history to the given linear path (the shared visit
        // space owns it now; a linear reset replaces any prior tree for the node).
        self.nav.remove(id);
        self.nav.seed_linear(id, history_entries, clamped_index);
        true
    }

}
