// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Typed projections over the graph's atomic per-node facet store.
//!
//! The store remains JSON-shaped and unknown-forward at its boundary. These
//! helpers give kernel consumers concrete types without putting optional
//! metadata columns back on [`Node`](super::Node).

use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use super::capture::{CapturedDelta, record_captured_delta};
use super::{Graph, NodeKey};
use crate::types::{NodeClassification, NodeProperty, NodeTagPresentationState};

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
        // Borrowing deserializer: `&Value` is itself a `Deserializer`, so the
        // stored subtree is read in place. `from_value` would deep-clone the
        // whole tree first, and these accessors sit on the per-frame paint path.
        T::deserialize(value).ok()
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

// ---------------------------------------------------------------------------
// Batched facet-family writers (the linked-data ingest's fast path).
//
// The single-item mutators in `node_props.rs` each deserialize a whole facet
// array, edit one element and reserialize it. Applying a contribution's P
// properties one delta at a time is therefore Theta(P^2) JSON work; these read
// the facet once, dedup in memory and write once. They sit here at the facet
// layer rather than beside their single-item twins because `node_props.rs` is
// already at the workspace's 600-line ceiling.
//
// Each accepted item is recorded as exactly the `Replay*ById` captured delta
// the equivalent per-item `GraphDelta` would have recorded, so a host with a
// capture hook installed sees an identical journal — that is what makes it safe
// for an out-of-kernel caller to reach these directly. For the same reason they
// must NOT be routed through `apply_graph_delta`, which would capture twice.
// ---------------------------------------------------------------------------

impl Graph {
    /// Insert several tags, writing the presentation facet once. Returns
    /// whether any tag was newly added.
    pub fn insert_node_tags(&mut self, key: NodeKey, tags: Vec<String>) -> bool {
        let Some(node_id) = self.inner.node(key).map(|node| node.id) else {
            return false;
        };
        let accepted: Vec<String> = {
            let node = self.inner.node_mut(key).expect("node resolved just above");
            tags.into_iter()
                .filter(|tag| node.tags.insert(tag.clone()))
                .collect()
        };
        if accepted.is_empty() {
            return false;
        }
        let mut presentation =
            self.node_facet_or_default::<NodeTagPresentationState>(key, PRESENTATION_TAGS);
        let mut ordering_changed = false;
        for tag in &accepted {
            if !presentation.ordered_tags.contains(tag) {
                presentation.ordered_tags.push(tag.clone());
                ordering_changed = true;
            }
        }
        if ordering_changed {
            self.set_node_facet(key, PRESENTATION_TAGS, &presentation);
        }
        for tag in accepted {
            record_captured_delta(&CapturedDelta::ReplayInsertNodeTagById {
                node_id: node_id.to_string(),
                tag,
            });
        }
        true
    }

    /// Append several open literal properties, writing the facet once. Each is
    /// deduplicated on the full literal record the same way
    /// `append_node_property` does, including the provenance refresh of an
    /// already-present record. Returns whether the facet changed.
    pub fn append_node_properties(&mut self, key: NodeKey, incoming: Vec<NodeProperty>) -> bool {
        let Some(node_id) = self.inner.node(key).map(|node| node.id) else {
            return false;
        };
        let mut properties =
            self.node_facet_or_default::<Vec<NodeProperty>>(key, SEMANTIC_PROPERTIES);
        let mut accepted: Vec<NodeProperty> = Vec::new();
        for property in incoming {
            match properties
                .iter_mut()
                .find(|existing| existing.content_eq(&property))
            {
                Some(existing) => {
                    if existing.provenance_iri == property.provenance_iri
                        && existing.asserted_at_ms == property.asserted_at_ms
                    {
                        continue;
                    }
                    existing.provenance_iri = property.provenance_iri.clone();
                    existing.asserted_at_ms = property.asserted_at_ms;
                }
                None => properties.push(property.clone()),
            }
            accepted.push(property);
        }
        if accepted.is_empty() {
            return false;
        }
        let changed = self.set_node_facet(key, SEMANTIC_PROPERTIES, &properties);
        if changed {
            for property in accepted {
                record_captured_delta(&CapturedDelta::ReplayAppendNodePropertyById {
                    node_id: node_id.to_string(),
                    property,
                });
            }
        }
        changed
    }

    /// Add several classification records, writing the facet once. Deduplicates
    /// on `(scheme, value)` against both the stored records and the earlier
    /// members of `incoming`. Returns whether the facet changed.
    pub fn add_node_classifications(
        &mut self,
        key: NodeKey,
        incoming: Vec<NodeClassification>,
    ) -> bool {
        let Some(node_id) = self.inner.node(key).map(|node| node.id) else {
            return false;
        };
        let mut classifications =
            self.node_facet_or_default::<Vec<NodeClassification>>(key, SEMANTIC_CLASSIFICATIONS);
        // Accepted records are appended, so the tail past this mark is exactly
        // the set to capture.
        let first_appended = classifications.len();
        for classification in incoming {
            let already_exists = classifications
                .iter()
                .any(|c| c.scheme == classification.scheme && c.value == classification.value);
            if already_exists {
                continue;
            }
            classifications.push(classification);
        }
        if classifications.len() == first_appended {
            return false;
        }
        let changed = self.set_node_facet(key, SEMANTIC_CLASSIFICATIONS, &classifications);
        if changed {
            for classification in &classifications[first_appended..] {
                record_captured_delta(&CapturedDelta::ReplayAddNodeClassificationById {
                    node_id: node_id.to_string(),
                    classification: classification.clone(),
                });
            }
        }
        changed
    }
}
