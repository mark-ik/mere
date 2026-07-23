// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

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

use euclid::default::Point2D;
use uuid::Uuid;

use super::{Graph, Node, NodeKey, ProvenanceSubKind};
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
    /// unpinned, no frame hints; browser-runtime state lives in the host's
    /// `BrowserNodeState` sidecar and never travels with a copy). A
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
        _position: Point2D<f32>,
    ) -> NodeKey {
        let derivation = NodeDerivation {
            sub_kind: ProvenanceSubKind::CopiedFrom,
            source_node: source.id.to_string(),
            source_graph,
        };
        let url = source.primary_address().as_url_str().to_string();

        let key = self.inner.insert(Node {
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
            body: source.body.clone(),
            thumbnail_png: source.thumbnail_png.clone(),
            thumbnail_width: source.thumbnail_width,
            thumbnail_height: source.thumbnail_height,
            favicon_rgba: source.favicon_rgba.clone(),
            favicon_width: source.favicon_width,
            favicon_height: source.favicon_height,
            // --- provenance: the cross-graph derivation record ---
            derivations: vec![derivation],
            // describes the donor's own external import, not this copy:
            import_provenance: Vec::new(),
            // --- identity / runtime / session / arrangement: reset ---
            is_pinned: false,
            last_visited: std::time::SystemTime::now(),
            last_session_visited: 0,
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
            // Deliberately NOT carried: a copy is un-resided. Carrying the
            // LogId would leave two nodes bearing ONE world file; the fork
            // instead re-homes worlds when the slot-convention move lands.
            nested: None,
        });

        self.url_to_nodes.entry(url).or_default().push(key);
        self.bump_revision();
        key
    }

    /// [`copy_node_from`](Self::copy_node_from) taking plain `x` / `y` screen-graph
    /// coords, so a host can place the copy without depending on `euclid`. (Tear-out
    /// gestures G5.)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn copy_node_from_xy(
        &mut self,
        source: &Node,
        source_graph: Option<String>,
        x: f32,
        y: f32,
    ) -> NodeKey {
        self.copy_node_from(source, source_graph, Point2D::new(x, y))
    }

    /// Copy the connected component containing `seed` (a node in `source`) into this
    /// graph: the kernel half of a tear-out **fork** (the reachable-subgraph snapshot,
    /// tear-out brief §4.3). Each node is minted fresh (with a `CopiedFrom`
    /// [`NodeDerivation`] back to its source); the component's internal edges are
    /// re-pointed onto the new nodes by cloning each edge's payload verbatim. Edges
    /// leaving the component are dropped (the snapshot is closed under the component).
    /// Returns the [`ComponentCopy`]: the new keys plus the `source id → new id`
    /// remap the host's facet-carry maps donor facets through (G4-R R0 — positions
    /// and every other per-node character ride `arrangement.*` / `web.*` / … facets,
    /// not the graph, so the fork's layout carry needs this mapping).
    /// (Tear-out gestures G4.)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn copy_component_from(
        &mut self,
        source: &Graph,
        seed: Uuid,
        source_graph: Option<String>,
    ) -> ComponentCopy {
        use petgraph::visit::{EdgeRef, IntoEdgeReferences};
        let Some(seed_key) = source.get_node_key_by_id(seed) else {
            return ComponentCopy::default();
        };
        // The weakly-connected component containing the seed (every node is in exactly
        // one; fall back to the lone seed if the partition somehow omits it).
        let component = source
            .weakly_connected_components()
            .into_iter()
            .find(|c| c.contains(&seed_key))
            .unwrap_or_else(|| vec![seed_key]);
        // Copy each node, mapping its old key to the fresh one for edge re-pointing
        // (and its old id to the fresh id for the host's facet-carry).
        let mut key_remap: std::collections::HashMap<NodeKey, NodeKey> =
            std::collections::HashMap::with_capacity(component.len());
        let mut copy = ComponentCopy {
            new_keys: Vec::with_capacity(component.len()),
            id_remap: Vec::with_capacity(component.len()),
        };
        for old_key in component {
            if let Some(node) = source.inner.node(old_key) {
                let source_id = node.id;
                // Position is no longer a node field; the copy is placed by the
                // destination's layout (seiche / arrangement facets), not carried over.
                let new_key = self.copy_node_from(node, source_graph.clone(), Point2D::zero());
                key_remap.insert(old_key, new_key);
                copy.new_keys.push(new_key);
                if let Some(new_node) = self.inner.node(new_key) {
                    copy.id_remap.push((source_id, new_node.id));
                }
            }
        }
        // Re-point the component's internal edges (both endpoints copied): clone each
        // edge's payload verbatim onto the new node pair.
        for edge in source.inner.inner().edge_references() {
            if let (Some(&from), Some(&to)) =
                (key_remap.get(&edge.source()), key_remap.get(&edge.target()))
            {
                self.inner.connect(from, to, edge.weight().clone());
            }
        }
        self.bump_revision();
        copy
    }
}

/// The result of [`Graph::copy_component_from`]: the minted keys plus the
/// `(source id, new id)` pairs, in copy order. The id remap is the seam the
/// host's facet-carry maps donor facets through (a forked node keeps its whole
/// per-node character — `arrangement.*`, `web.*`, foreign namespaces — by
/// copying facets from `source` to `new`).
#[derive(Clone, Debug, Default)]
pub struct ComponentCopy {
    /// The fresh node keys in this graph, in copy order.
    pub new_keys: Vec<NodeKey>,
    /// `(source node id, minted node id)` per copied node, in copy order.
    pub id_remap: Vec<(Uuid, Uuid)>,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn donor_with_content() -> (Graph, NodeKey) {
        let mut a = Graph::new();
        let key = a.add_node(
            "https://example.com/article".to_string(),
            Point2D::new(1.0, 2.0),
        );
        let node = a.inner.node_mut(key).unwrap();
        node.title = "An Article".to_string();
        node.tags = HashSet::from(["read-later".to_string(), "research".to_string()]);
        node.properties = vec![crate::types::NodeProperty::new(
            "https://schema.org/datePublished".to_string(),
            "2026-01-01".to_string(),
        )];
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
        let copy_key =
            b.copy_node_from(&source, Some("graph-A".to_string()), Point2D::new(9.0, 9.0));
        assert!(
            b.revision() > rev_before,
            "copying a node in is structural and advances the revision"
        );
        let copy = b.get_node(copy_key).unwrap();

        // Fresh identity, content cloned.
        assert_ne!(copy.id, source_id, "copy is a new entity");
        assert_eq!(copy.title, "An Article");
        assert_eq!(copy.tags, source.tags);
        assert_eq!(copy.properties, source.properties);
        assert_eq!(copy.url(), source.url(), "same content, same address");

        // Position is not a node field (not inherited, not carried); the copy is
        // placed by the destination's layout.
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
        a.inner.node_mut(src_key).unwrap().import_provenance =
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
        assert_eq!(
            copy.derivations[0].source_graph, None,
            "same-graph / unknown source graph"
        );
    }

    #[test]
    fn copy_component_clones_the_connected_subgraph_with_edges_and_provenance() {
        use crate::graph::{EdgeAssertion, SemanticSubKind};
        let mut a = Graph::new();
        let k1 = a.add_node("https://a.com/1".to_string(), Point2D::new(0.0, 0.0));
        let k2 = a.add_node("https://a.com/2".to_string(), Point2D::new(1.0, 0.0));
        let _k3 = a.add_node("https://a.com/3".to_string(), Point2D::new(9.0, 9.0)); // disconnected
        a.assert_relation(
            k1,
            k2,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
        let seed = a.get_node(k1).unwrap().id;

        let mut b = Graph::new();
        let copy = b.copy_component_from(&a, seed, Some("graph-A".to_string()));

        // The connected component is {k1, k2}; the disconnected k3 is not pulled in.
        assert_eq!(
            copy.new_keys.len(),
            2,
            "copied the 2-node component, not the lone disconnected node"
        );
        assert_eq!(b.nodes().count(), 2);
        // The component's internal edge is re-pointed onto the copies.
        assert_eq!(b.relations().count(), 1, "the internal edge is re-pointed");
        // Each copy records `CopiedFrom` provenance back to the source graph.
        for (_, n) in b.nodes() {
            assert_eq!(n.derivations.len(), 1);
            assert_eq!(n.derivations[0].sub_kind, ProvenanceSubKind::CopiedFrom);
            assert_eq!(n.derivations[0].source_graph.as_deref(), Some("graph-A"));
        }
        // The id remap pairs each source id with its minted copy's id — the seam
        // the host's facet-carry maps donor facets through (G4-R R0).
        assert_eq!(copy.id_remap.len(), 2);
        for (source_id, new_id) in &copy.id_remap {
            assert_ne!(source_id, new_id, "a copy is a new entity");
            let (_, source_node) = a.get_node_by_id(*source_id).map(|(k, n)| (k, n)).unwrap();
            let (_, new_node) = b.get_node_by_id(*new_id).map(|(k, n)| (k, n)).unwrap();
            assert_eq!(source_node.url(), new_node.url(), "pairs align by content");
            assert_eq!(new_node.derivations[0].source_node, source_id.to_string());
        }
    }
}
