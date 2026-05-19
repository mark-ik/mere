/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Node-property setters and per-node query accessors.
//!
//! All of Graph's property mutators for individual nodes: title,
//! thumbnail, favicon, mime hint, viewer override, pinned, compat,
//! frame-layout hints, tags, classifications, tag-icon override,
//! position (committed + projected), session scroll, form draft,
//! lifecycle, plus the per-node query accessors that sit alongside
//! them (`node_tags`, `node_classifications`, `frame_layout_hints`,
//! `node_projected_position`, `node_committed_position`,
//! `projected_centroid`).
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel
//! decomposition pass. Lifecycle ops (`new`, `add_node`,
//! `add_node_with_id`, `remove_node`, `update_node_url`,
//! `recompute_cached_hosts`) stay in `mod.rs` since they own Graph's
//! construction shape; everything property-setter-shaped lives here.

use std::collections::HashSet;
use std::time::SystemTime;

use euclid::default::Point2D;

use super::Graph;
use super::identity::NodeKey;
use super::node::{Node, NodeLifecycle};
use crate::types::{
    ClassificationProvenance, ClassificationScheme, ClassificationStatus, FrameLayoutHint,
    NodeClassification, NodeImportProvenance, NodeTagPresentationState,
};

impl Graph {
    pub fn set_node_title(&mut self, key: NodeKey, title: String) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.title == title {
            return false;
        }
        node.title = title;
        true
    }

    pub fn set_node_thumbnail(
        &mut self,
        key: NodeKey,
        png_bytes: Vec<u8>,
        width: u32,
        height: u32,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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

    pub fn set_node_favicon(
        &mut self,
        key: NodeKey,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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

    pub fn set_node_mime_hint(&mut self, key: NodeKey, mime_hint: Option<String>) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.mime_hint == mime_hint {
            return false;
        }
        node.mime_hint = mime_hint;
        true
    }

    pub fn set_node_viewer_override(
        &mut self,
        key: NodeKey,
        viewer_override: Option<String>,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.viewer_override == viewer_override {
            return false;
        }
        node.viewer_override = viewer_override;
        true
    }

    pub fn set_node_pinned(&mut self, key: NodeKey, is_pinned: bool) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.is_pinned == is_pinned {
            return false;
        }
        node.is_pinned = is_pinned;
        true
    }

    pub fn set_node_compat_mode(&mut self, key: NodeKey, compat_mode: bool) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.compat_mode == compat_mode {
            return false;
        }
        node.compat_mode = compat_mode;
        true
    }

    pub fn node_compat_mode(&self, key: NodeKey) -> Option<bool> {
        self.get_node(key).map(|node| node.compat_mode)
    }

    pub fn append_frame_layout_hint(&mut self, key: NodeKey, hint: FrameLayoutHint) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        node.frame_layout_hints.push(hint);
        true
    }

    pub fn remove_frame_layout_hint_at(&mut self, key: NodeKey, hint_index: usize) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if hint_index >= node.frame_layout_hints.len() {
            return false;
        }
        node.frame_layout_hints.remove(hint_index);
        true
    }

    pub fn move_frame_layout_hint(
        &mut self,
        key: NodeKey,
        from_index: usize,
        to_index: usize,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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

    pub fn set_frame_split_offer_suppressed(&mut self, key: NodeKey, suppressed: bool) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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

    pub fn insert_node_tag(&mut self, key: NodeKey, tag: String) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        let inserted = node.tags.insert(tag.clone());
        if inserted && !node.tag_presentation.ordered_tags.contains(&tag) {
            node.tag_presentation.ordered_tags.push(tag);
        }
        inserted
    }

    pub fn remove_node_tag(&mut self, key: NodeKey, tag: &str) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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
    pub fn add_node_classification(
        &mut self,
        key: NodeKey,
        classification: NodeClassification,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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

    /// Remove all classification records matching `(scheme, value)`.
    ///
    /// Returns `true` if at least one record was removed.
    pub fn remove_node_classification(
        &mut self,
        key: NodeKey,
        scheme: &ClassificationScheme,
        value: &str,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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
    pub fn set_node_classification_status(
        &mut self,
        key: NodeKey,
        scheme: &ClassificationScheme,
        value: &str,
        status: ClassificationStatus,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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
    pub fn set_node_primary_classification(
        &mut self,
        key: NodeKey,
        scheme: &ClassificationScheme,
        value: &str,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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

    pub fn set_node_tag_icon_override(
        &mut self,
        key: NodeKey,
        tag: &str,
        icon: Option<crate::types::BadgeIcon>,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
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

    pub fn set_node_position(&mut self, key: NodeKey, position: Point2D<f32>) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.position == position && node.committed_position == position {
            return false;
        }
        node.position = position;
        node.committed_position = position;
        true
    }

    pub fn set_node_projected_position(&mut self, key: NodeKey, position: Point2D<f32>) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.position == position {
            return false;
        }
        node.position = position;
        true
    }

    pub fn node_projected_position(&self, key: NodeKey) -> Option<Point2D<f32>> {
        self.get_node(key).map(Node::projected_position)
    }

    pub fn node_committed_position(&self, key: NodeKey) -> Option<Point2D<f32>> {
        self.get_node(key).map(Node::committed_position)
    }

    pub fn projected_centroid(&self) -> Option<Point2D<f32>> {
        let mut sum_x = 0.0f32;
        let mut sum_y = 0.0f32;
        let mut count = 0.0f32;
        for (_, node) in self.nodes() {
            sum_x += node.position.x;
            sum_y += node.position.y;
            count += 1.0;
        }
        if count == 0.0 {
            None
        } else {
            Some(Point2D::new(sum_x / count, sum_y / count))
        }
    }

    pub fn set_node_form_draft(&mut self, key: NodeKey, form_draft: Option<String>) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.session_form_draft == form_draft {
            return false;
        }
        node.session_form_draft = form_draft;
        true
    }

    pub fn touch_node_last_visited_now(&mut self, key: NodeKey) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        node.last_visited = std::time::SystemTime::now();
        true
    }

    /// Internal setter for `Node::history`. Tightened to `pub(crate)` so the
    /// only reachable write surface from outside `mere-kernel` is
    /// `GraphDelta::UpdateNodeHistory`, dispatched via the
    /// `app::history::GraphBrowserApp::apply_node_history_change` helper.
    pub(crate) fn set_node_history_state(
        &mut self,
        key: NodeKey,
        history_entries: Vec<String>,
        history_index: usize,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        let clamped_index = if history_entries.is_empty() {
            0
        } else {
            history_index.min(history_entries.len() - 1)
        };
        let current = node.history_projection();
        if current.entries == history_entries && current.current_index == clamped_index {
            return false;
        }
        node.replace_history_state(history_entries, clamped_index);
        true
    }

    pub fn set_node_session_scroll(
        &mut self,
        key: NodeKey,
        session_scroll: Option<(f32, f32)>,
    ) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.session_scroll == session_scroll {
            return false;
        }
        node.session_scroll = session_scroll;
        true
    }

    pub fn set_node_lifecycle(&mut self, key: NodeKey, lifecycle: NodeLifecycle) -> bool {
        let Some(node) = self.inner.node_weight_mut(key) else {
            return false;
        };
        if node.lifecycle == lifecycle {
            return false;
        }
        node.lifecycle = lifecycle;
        true
    }
}
