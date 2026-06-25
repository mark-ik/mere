/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Cross-graph node copy — the kernel half of the tear-out "fork" / cross-graph
//! composability gesture (tear-out brief §7.5; tearout-composability plan C4).
//!
//! A node→node derivation *within* one graph rides a petgraph `Provenance`
//! edge. A cross-graph copy's source node lives in a *different* graph, so the
//! relation cannot be a petgraph edge; the derivation is recorded on the minted
//! node as a [`NodeDerivation`] instead (see `node.rs` / `types.rs`). That
//! record is the lineage pointer back to the source and projects to a
//! `<copy> {provenance predicate} <source>` statement under the RDF projection.
//!
//! WASM-clean: must compile to `wasm32-unknown-unknown` (mirrors `graph/mod.rs`).

use euclid::default::{Point2D, Vector2D};
use uuid::Uuid;

use super::{Graph, Node, NodeKey, NodeLifecycle, ProvenanceSubKind};
use crate::types::NodeDerivation;

impl Graph {
    /// Copy `source`'s content into this graph, minting a fresh node and
    /// recording cross-graph derivation provenance back to the source.
    ///
    /// `source` is a donor [`Node`] from another [`Graph`]; `source_graph` is the
    /// donor graph's id (the constellation `GraphId` rendered as a string), or
    /// `None` when unknown / same-graph. The copy is a *new entity*: it gets a
    /// fresh identity, and its **content** (title, tags, classifications, open
    /// properties, address, visuals, viewer preferences) is cloned, while its
    /// **identity / runtime / session / arrangement** state is reset (fresh id,
    /// `Cold` lifecycle, unpinned, no session scroll/draft, no frame hints). A
    /// [`NodeDerivation`] records `(CopiedFrom, source.id, source_graph)` so the
    /// lineage survives and projects to a `wasDerivedFrom` statement.
    ///
    /// The donor's `import_provenance` is intentionally *not* carried over: that
    /// describes how the donor entered *its* graph from an external source; the
    /// copy's provenance is the derivation record, not the donor's import.
    ///
    /// Not available on `wasm32` (mints a random id, like [`add_node`]); use
    /// [`copy_node_from_with_id`](Self::copy_node_from_with_id) with a
    /// host-provided UUID there.
    ///
    /// [`add_node`]: Self::add_node
    #[cfg(not(target_arch = "wasm32"))]
    pub fn copy_node_from(
        &mut self,
        source: &Node,
        source_graph: Option<String>,
        position: Point2D<f32>,
    ) -> NodeKey {
        self.copy_node_from_with_id(Uuid::new_v4(), source, source_graph, position)
    }

    /// [`copy_node_from`](Self::copy_node_from) with a caller-supplied id (the
    /// wasm-clean entry point, mirroring
    /// [`add_node_with_id`](Self::add_node_with_id)).
    pub fn copy_node_from_with_id(
        &mut self,
        id: Uuid,
        source: &Node,
        source_graph: Option<String>,
        position: Point2D<f32>,
    ) -> NodeKey {
        let derivation = NodeDerivation {
            sub_kind: ProvenanceSubKind::CopiedFrom,
            source_node: source.id.to_string(),
            source_graph,
        };
        let url = source.primary_address().as_url_str().to_string();

        let key = self.inner.add_node(Node {
            id,
            // --- content: cloned from the source ---
            cached_host: source.cached_host.clone(),
            title: source.title.clone(),
            tags: source.tags.clone(),
            tag_presentation: source.tag_presentation.clone(),
            classifications: source.classifications.clone(),
            properties: source.properties.clone(),
            addresses: source.addresses.clone(),
            mime_hint: source.mime_hint.clone(),
            viewer_override: source.viewer_override.clone(),
            compat_mode: source.compat_mode,
            thumbnail_png: source.thumbnail_png.clone(),
            thumbnail_width: source.thumbnail_width,
            thumbnail_height: source.thumbnail_height,
            favicon_rgba: source.favicon_rgba.clone(),
            favicon_width: source.favicon_width,
            favicon_height: source.favicon_height,
            // --- placement: the drop point in this graph ---
            position,
            velocity: Vector2D::zero(),
            // --- provenance: the cross-graph derivation record ---
            derivations: vec![derivation],
            // describes the donor's own external import, not this copy:
            import_provenance: Vec::new(),
            // --- identity / runtime / session / arrangement: reset ---
            is_pinned: false,
            last_visited: std::time::SystemTime::now(),
            session_scroll: None,
            session_form_draft: None,
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
            lifecycle: NodeLifecycle::Cold,
        });

        self.url_to_nodes.entry(url).or_default().push(key);
        self.id_to_node.insert(id, key);
        self.bump_revision();
        key
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn donor_with_content() -> (Graph, NodeKey) {
        let mut a = Graph::new();
        let key = a.add_node("https://example.com/article".to_string(), Point2D::new(1.0, 2.0));
        let node = a.inner.node_weight_mut(key).unwrap();
        node.title = "An Article".to_string();
        node.tags = HashSet::from(["read-later".to_string(), "research".to_string()]);
        node.properties = vec![crate::types::NodeProperty {
            predicate: "https://schema.org/datePublished".to_string(),
            value: "2026-01-01".to_string(),
        }];
        node.is_pinned = true;
        (a, key)
    }

    #[test]
    fn copy_mints_a_fresh_node_cloning_content_with_derivation() {
        let (a, src_key) = donor_with_content();
        let source = a.get_node(src_key).unwrap().clone();
        let source_id = source.id;

        let mut b = Graph::new();
        let rev_before = b.revision();
        let copy_key = b.copy_node_from(&source, Some("graph-A".to_string()), Point2D::new(9.0, 9.0));
        assert!(b.revision() > rev_before, "copying a node in is structural and advances the revision");
        let copy = b.get_node(copy_key).unwrap();

        // Fresh identity, content cloned.
        assert_ne!(copy.id, source_id, "copy is a new entity");
        assert_eq!(copy.title, "An Article");
        assert_eq!(copy.tags, source.tags);
        assert_eq!(copy.properties, source.properties);
        assert_eq!(copy.url(), source.url(), "same content, same address");

        // Placement reset, not inherited.
        assert_eq!(copy.projected_position(), Point2D::new(9.0, 9.0));
        assert!(!copy.is_pinned, "pin is per-placement, not copied");

        // Derivation records the lineage back to the source.
        assert_eq!(copy.derivations.len(), 1);
        let d = &copy.derivations[0];
        assert_eq!(d.sub_kind, ProvenanceSubKind::CopiedFrom);
        assert_eq!(d.source_node, source_id.to_string());
        assert_eq!(d.source_graph.as_deref(), Some("graph-A"));

        // The copy is discoverable by its address in the destination index.
        assert_eq!(b.get_nodes_by_url(source.url()).len(), 1);
    }

    #[test]
    fn copy_does_not_carry_the_donor_import_provenance() {
        let (mut a, src_key) = donor_with_content();
        a.inner.node_weight_mut(src_key).unwrap().import_provenance =
            vec![crate::types::NodeImportProvenance {
                source_id: "firefox".to_string(),
                source_label: "Firefox bookmarks".to_string(),
            }];
        let source = a.get_node(src_key).unwrap().clone();

        let mut b = Graph::new();
        let copy_key = b.copy_node_from(&source, None, Point2D::zero());
        let copy = b.get_node(copy_key).unwrap();

        assert!(
            copy.import_provenance.is_empty(),
            "donor import provenance is the donor's, not the copy's"
        );
        assert_eq!(copy.derivations[0].source_graph, None, "same-graph / unknown source graph");
    }
}
