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
use std::time::{SystemTime, UNIX_EPOCH};

use super::Graph;
use super::identity::NodeKey;
use super::node_facets::{
    ARRANGEMENT_FRAME_LAYOUT, ARRANGEMENT_PIN, ARRANGEMENT_SPLIT_OFFER_SUPPRESSED,
    PRESENTATION_TAGS, PROVENANCE_DERIVATIONS, PROVENANCE_IMPORT, SEMANTIC_CLASSIFICATIONS,
    SEMANTIC_PROPERTIES, VISIT_HISTORY, VisitHistoryFacet,
};
use crate::types::{
    ClassificationScheme, ClassificationStatus, FrameLayoutHint, ImageRef, ImageRole,
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

    /// Attach a stored image reference under `role`, reporting whether the
    /// node changed.
    ///
    /// The caller stores the blob first (`pandect::image_store::
    /// save_image`) and passes the handle; the kernel neither hashes nor
    /// holds pixels. Because references are content-addressed, re-depositing
    /// an unchanged image compares equal here and reports `false`, so an
    /// identical re-capture is a no-op instead of a graph-dirtying rewrite.
    pub(crate) fn set_node_image(
        &mut self,
        key: NodeKey,
        role: ImageRole,
        image: ImageRef,
    ) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.images.get(&role) == Some(&image) {
            return false;
        }
        node.images.insert(role, image);
        true
    }

    /// Directly bear (or clear) a nested graph on a node WITHOUT running the
    /// delta spine — for copy/load paths building a graph that has no journal
    /// yet (a fork re-bearing the worlds it carried as real file copies; the
    /// component copy itself deliberately drops `nested` so two LIVE nodes
    /// never share one world). Journaled edits go through
    /// `GraphDelta::SetNodeNested` instead.
    pub fn bear_nested(&mut self, key: NodeKey, nested: Option<codicil::LogId>) -> bool {
        self.set_node_nested(key, nested)
    }

    /// Set or clear the node's borne graph (`Node.nested`). Structural
    /// containment per the one-node ruling; journals as
    /// `ReplaySetNodeNestedById` so installs replay attributed.
    pub(crate) fn set_node_nested(&mut self, key: NodeKey, nested: Option<codicil::LogId>) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.nested == nested {
            return false;
        }
        node.nested = nested;
        true
    }

    pub(crate) fn set_node_mime_hint(&mut self, key: NodeKey, mime_hint: Option<String>) -> bool {
        let Some(node) = self.inner.node_mut(key) else {
            return false;
        };
        if node.media_type == mime_hint {
            return false;
        }
        node.media_type = mime_hint;
        true
    }

    pub(crate) fn set_node_pinned(&mut self, key: NodeKey, is_pinned: bool) -> bool {
        if self.inner.node(key).is_none() {
            return false;
        }
        if self.node_is_pinned(key) == Some(is_pinned) {
            return false;
        }
        self.set_node_facet(key, ARRANGEMENT_PIN, &is_pinned)
    }

    pub(crate) fn append_frame_layout_hint(&mut self, key: NodeKey, hint: FrameLayoutHint) -> bool {
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut hints =
            self.node_facet_or_default::<Vec<FrameLayoutHint>>(key, ARRANGEMENT_FRAME_LAYOUT);
        hints.push(hint);
        self.set_node_facet(key, ARRANGEMENT_FRAME_LAYOUT, &hints)
    }

    pub(crate) fn remove_frame_layout_hint_at(&mut self, key: NodeKey, hint_index: usize) -> bool {
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut hints =
            self.node_facet_or_default::<Vec<FrameLayoutHint>>(key, ARRANGEMENT_FRAME_LAYOUT);
        if hint_index >= hints.len() {
            return false;
        }
        hints.remove(hint_index);
        self.set_node_facet(key, ARRANGEMENT_FRAME_LAYOUT, &hints)
    }

    pub(crate) fn move_frame_layout_hint(
        &mut self,
        key: NodeKey,
        from_index: usize,
        to_index: usize,
    ) -> bool {
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut hints =
            self.node_facet_or_default::<Vec<FrameLayoutHint>>(key, ARRANGEMENT_FRAME_LAYOUT);
        if from_index >= hints.len() || to_index >= hints.len() || from_index == to_index {
            return false;
        }
        let hint = hints.remove(from_index);
        hints.insert(to_index, hint);
        self.set_node_facet(key, ARRANGEMENT_FRAME_LAYOUT, &hints)
    }

    pub(crate) fn set_frame_split_offer_suppressed(
        &mut self,
        key: NodeKey,
        suppressed: bool,
    ) -> bool {
        if self.inner.node(key).is_none() {
            return false;
        }
        if self.frame_split_offer_suppressed(key) == Some(suppressed) {
            return false;
        }
        self.set_node_facet(key, ARRANGEMENT_SPLIT_OFFER_SUPPRESSED, &suppressed)
    }

    pub fn frame_layout_hints(&self, key: NodeKey) -> Option<Vec<FrameLayoutHint>> {
        self.get_node(key)?;
        Some(self.node_facet_or_default(key, ARRANGEMENT_FRAME_LAYOUT))
    }

    pub fn frame_split_offer_suppressed(&self, key: NodeKey) -> Option<bool> {
        self.get_node(key)?;
        Some(self.node_facet_or_default(key, ARRANGEMENT_SPLIT_OFFER_SUPPRESSED))
    }

    pub fn node_is_pinned(&self, key: NodeKey) -> Option<bool> {
        self.get_node(key)?;
        Some(self.node_facet_or_default(key, ARRANGEMENT_PIN))
    }

    pub(crate) fn insert_node_tag(&mut self, key: NodeKey, tag: String) -> bool {
        let inserted = {
            let Some(node) = self.inner.node_mut(key) else {
                return false;
            };
            node.tags.insert(tag.clone())
        };
        if inserted {
            let mut presentation =
                self.node_facet_or_default::<NodeTagPresentationState>(key, PRESENTATION_TAGS);
            if !presentation.ordered_tags.contains(&tag) {
                presentation.ordered_tags.push(tag);
                self.set_node_facet(key, PRESENTATION_TAGS, &presentation);
            }
        }
        inserted
    }

    pub(crate) fn remove_node_tag(&mut self, key: NodeKey, tag: &str) -> bool {
        let removed = {
            let Some(node) = self.inner.node_mut(key) else {
                return false;
            };
            node.tags.remove(tag)
        };
        if removed {
            let mut presentation =
                self.node_facet_or_default::<NodeTagPresentationState>(key, PRESENTATION_TAGS);
            presentation.ordered_tags.retain(|entry| entry != tag);
            presentation.icon_overrides.remove(tag);
            self.set_node_facet(key, PRESENTATION_TAGS, &presentation);
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
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut properties =
            self.node_facet_or_default::<Vec<NodeProperty>>(key, SEMANTIC_PROPERTIES);
        if let Some(existing) = properties
            .iter_mut()
            .find(|existing| existing.content_eq(&property))
        {
            if existing.provenance_iri != property.provenance_iri
                || existing.asserted_at_ms != property.asserted_at_ms
            {
                existing.provenance_iri = property.provenance_iri;
                existing.asserted_at_ms = property.asserted_at_ms;
                return self.set_node_facet(key, SEMANTIC_PROPERTIES, &properties);
            }
            return false;
        }
        properties.push(property);
        self.set_node_facet(key, SEMANTIC_PROPERTIES, &properties)
    }

    pub fn node_tags(&self, key: NodeKey) -> Option<&HashSet<String>> {
        self.get_node(key).map(|node| &node.tags)
    }

    pub fn node_tag_presentation(&self, key: NodeKey) -> Option<NodeTagPresentationState> {
        self.get_node(key)?;
        Some(self.node_facet_or_default(key, PRESENTATION_TAGS))
    }

    pub fn node_import_provenance(&self, key: NodeKey) -> Option<Vec<NodeImportProvenance>> {
        self.get_node(key)?;
        Some(self.node_facet_or_default(key, PROVENANCE_IMPORT))
    }

    // --- Classification accessors (Stage A) ---

    pub fn node_classifications(&self, key: NodeKey) -> Option<Vec<NodeClassification>> {
        self.get_node(key)?;
        Some(self.node_facet_or_default(key, SEMANTIC_CLASSIFICATIONS))
    }

    pub fn node_properties(&self, key: NodeKey) -> Option<Vec<NodeProperty>> {
        self.get_node(key)?;
        Some(self.node_facet_or_default(key, SEMANTIC_PROPERTIES))
    }

    pub fn node_derivations(&self, key: NodeKey) -> Option<Vec<crate::types::NodeDerivation>> {
        self.get_node(key)?;
        Some(self.node_facet_or_default(key, PROVENANCE_DERIVATIONS))
    }

    /// Add a classification record to a node.
    ///
    /// Deduplicates by `(scheme, value)`. Returns `true` if the record was inserted.
    pub(crate) fn add_node_classification(
        &mut self,
        key: NodeKey,
        classification: NodeClassification,
    ) -> bool {
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut classifications =
            self.node_facet_or_default::<Vec<NodeClassification>>(key, SEMANTIC_CLASSIFICATIONS);
        let already_exists = classifications
            .iter()
            .any(|c| c.scheme == classification.scheme && c.value == classification.value);
        if already_exists {
            return false;
        }
        classifications.push(classification);
        self.set_node_facet(key, SEMANTIC_CLASSIFICATIONS, &classifications)
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
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut derivations = self.node_facet_or_default::<Vec<crate::types::NodeDerivation>>(
            key,
            PROVENANCE_DERIVATIONS,
        );
        if derivations.contains(&derivation) {
            return false;
        }
        derivations.push(derivation);
        self.set_node_facet(key, PROVENANCE_DERIVATIONS, &derivations)
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
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut classifications =
            self.node_facet_or_default::<Vec<NodeClassification>>(key, SEMANTIC_CLASSIFICATIONS);
        let before = classifications.len();
        classifications.retain(|c| !(c.scheme == *scheme && c.value == value));
        if classifications.len() == before {
            return false;
        }
        self.set_node_facet(key, SEMANTIC_CLASSIFICATIONS, &classifications)
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
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut classifications =
            self.node_facet_or_default::<Vec<NodeClassification>>(key, SEMANTIC_CLASSIFICATIONS);
        let mut found = false;
        for c in &mut classifications {
            if c.scheme == *scheme && c.value == value {
                c.status = status.clone();
                found = true;
            }
        }
        found && self.set_node_facet(key, SEMANTIC_CLASSIFICATIONS, &classifications)
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
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut classifications =
            self.node_facet_or_default::<Vec<NodeClassification>>(key, SEMANTIC_CLASSIFICATIONS);
        let mut found = false;
        for c in &mut classifications {
            if c.scheme == *scheme {
                c.primary = c.value == value;
                if c.value == value {
                    found = true;
                }
            }
        }
        found && self.set_node_facet(key, SEMANTIC_CLASSIFICATIONS, &classifications)
    }

    pub(crate) fn set_node_tag_icon_override(
        &mut self,
        key: NodeKey,
        tag: &str,
        icon: Option<crate::types::BadgeIcon>,
    ) -> bool {
        let Some(node) = self.inner.node(key) else {
            return false;
        };
        if !node.tags.contains(tag) || tag.starts_with('#') || tag.starts_with("udc:") {
            return false;
        }
        let mut presentation =
            self.node_facet_or_default::<NodeTagPresentationState>(key, PRESENTATION_TAGS);
        match icon {
            Some(icon) => {
                if presentation.icon_overrides.get(tag) == Some(&icon) {
                    return false;
                }
                presentation.icon_overrides.insert(tag.to_string(), icon);
                self.set_node_facet(key, PRESENTATION_TAGS, &presentation)
            }
            None => {
                if presentation.icon_overrides.remove(tag).is_none() {
                    return false;
                }
                self.set_node_facet(key, PRESENTATION_TAGS, &presentation)
            }
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
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut history = self.node_facet_or_default::<VisitHistoryFacet>(key, VISIT_HISTORY);
        history.last_visited_ms = Some(timestamp_ms);
        self.set_node_facet(key, VISIT_HISTORY, &history)
    }

    pub(crate) fn set_node_last_session_visited(
        &mut self,
        key: NodeKey,
        last_session_visited: u64,
    ) -> bool {
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut history = self.node_facet_or_default::<VisitHistoryFacet>(key, VISIT_HISTORY);
        if history.last_session_visited == last_session_visited {
            return false;
        }
        history.last_session_visited = last_session_visited;
        self.set_node_facet(key, VISIT_HISTORY, &history)
    }

    pub fn node_last_visited(&self, key: NodeKey) -> Option<SystemTime> {
        self.get_node(key)?;
        self.node_facet_or_default::<VisitHistoryFacet>(key, VISIT_HISTORY)
            .last_visited_ms
            .map(|milliseconds| UNIX_EPOCH + std::time::Duration::from_millis(milliseconds))
    }

    pub fn node_last_session_visited(&self, key: NodeKey) -> Option<u64> {
        self.get_node(key)?;
        Some(
            self.node_facet_or_default::<VisitHistoryFacet>(key, VISIT_HISTORY)
                .last_session_visited,
        )
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
