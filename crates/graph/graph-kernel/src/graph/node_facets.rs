// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed projections over the graph's atomic per-node facet store.
//!
//! The store remains JSON-shaped and unknown-forward at its boundary. These
//! helpers give kernel consumers concrete types without putting optional
//! metadata columns back on [`Node`](super::Node).

use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use super::{Graph, NodeKey};

pub const ARRANGEMENT_PIN: &str = "arrangement.pin";
pub const ARRANGEMENT_FRAME_LAYOUT: &str = "arrangement.frame-layout";
pub const ARRANGEMENT_SPLIT_OFFER_SUPPRESSED: &str = "arrangement.split-offer-suppressed";
pub const PRESENTATION_TAGS: &str = "presentation.tags";
pub const VISIT_HISTORY: &str = "visit.history";
pub const PROVENANCE_IMPORT: &str = "provenance.import";
pub const PROVENANCE_DERIVATIONS: &str = "provenance.derivations";
pub const SEMANTIC_CLASSIFICATIONS: &str = "semantic.classifications";
pub const SEMANTIC_PROPERTIES: &str = "semantic.properties";

pub type NodeFacetStore = chartulary::FacetStore<Uuid>;

/// The durable visit clocks formerly stored as two columns on `Node`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct VisitHistoryFacet {
    #[serde(default)]
    pub last_visited_ms: Option<u64>,
    #[serde(default)]
    pub last_session_visited: u64,
}

impl Graph {
    /// The graph's single live atomic-facet authority.
    pub fn facets(&self) -> &NodeFacetStore {
        &self.facets
    }

    /// Mutable facet access for the host and extension gate.
    ///
    /// Kernel-owned facet families should still use their typed graph methods;
    /// this surface exists for host-defined and unknown-forward namespaces.
    pub fn facets_mut(&mut self) -> &mut NodeFacetStore {
        &mut self.facets
    }

    /// Overlay a canonical sidecar after snapshot load.
    ///
    /// Legacy snapshot columns are imported while reconstructing the graph.
    /// Existing `facets.json` values then win for matching keys, while both
    /// stores preserve unrelated namespaces.
    pub fn overlay_facets(&mut self, facets: NodeFacetStore) {
        self.facets.overlay(facets);
    }

    pub(crate) fn node_facet<T: DeserializeOwned>(
        &self,
        key: NodeKey,
        facet: &'static str,
    ) -> Option<T> {
        let node = self.inner.node(key)?;
        let value = self
            .facets
            .get(&node.id, &chartulary::FacetId::new(facet))?;
        serde_json::from_value(value.clone()).ok()
    }

    pub(crate) fn node_facet_or_default<T: DeserializeOwned + Default>(
        &self,
        key: NodeKey,
        facet: &'static str,
    ) -> T {
        self.node_facet(key, facet).unwrap_or_default()
    }

    pub(crate) fn set_node_facet<T: Serialize>(
        &mut self,
        key: NodeKey,
        facet: &'static str,
        value: &T,
    ) -> bool {
        let Some(node_id) = self.inner.node(key).map(|node| node.id) else {
            return false;
        };
        let facet_id = chartulary::FacetId::new(facet);
        let value = serde_json::to_value(value).expect("kernel facet must serialize");
        if self.facets.get(&node_id, &facet_id) == Some(&value) {
            return false;
        }
        self.facets
            .set(node_id, facet_id, value, &chartulary::AcceptAll)
            .expect("AcceptAll cannot reject a kernel facet");
        true
    }

    pub(crate) fn remove_facets_for_node(&mut self, node_id: Uuid) {
        self.facets.remove_node(&node_id);
    }
}
