//! Atomic facets: typed, optional, per-node metadata.
//!
//! One node — [`Container`](crate::Container) — carries the shared minimum
//! (identity, addresses, body, title, tags). *Everything else* a node might
//! carry is a **facet**: a typed record keyed by node id and [`FacetId`], stored
//! beside the graph, so metadata is content-specific in principle and a modder
//! can define new metadata without touching the node type. This is the runtime
//! tier of the facet system (the compile-time tier is the capability-trait
//! table in [`caps`](crate::caps)).
//!
//! chartulary stays **schema-agnostic**: a facet's payload is an opaque
//! [`serde_json::Value`], and validation is injected through the
//! [`FacetValidator`] seam. A host supplies the real validator (mere backs it
//! with eidetic's `SchemaDefinition` engrams); chartulary ships only
//! [`AcceptAll`]. An unknown facet id is never an error — it is data, preserved
//! untouched across load and re-save, so a facet a newer build or another app
//! wrote survives a round-trip here.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The kind of a facet: a stable string id like `web.viewer`,
/// `arrangement.pin`, `note.reading-state`. Namespaced by convention (a `.`
/// prefix per owning content class); chartulary does not interpret it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FacetId(pub String);

impl FacetId {
    /// Name a facet kind.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a facet was refused by the validator. Nothing was stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacetError {
    /// The facet that failed.
    pub facet: FacetId,
    /// A human-readable reason from the validator.
    pub reason: String,
}

impl fmt::Display for FacetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "facet `{}` invalid: {}", self.facet.0, self.reason)
    }
}

impl std::error::Error for FacetError {}

/// A facet value with a deterministic shelf life.
///
/// Expiry is indexed by graph revision rather than wall time: the value is
/// live while `revision < expires_at_revision` and stale at the named revision
/// itself. Reading never removes or rewrites the stored envelope, so replaying
/// the same journal prefix asks the same question of the same bytes.
///
/// This is deliberately generic and does not designate a signal namespace.
/// Durable state and transient signals can both opt into the envelope without
/// chartulary interpreting the facet id or payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpiringFacet<T> {
    /// The facet payload.
    pub value: T,
    /// The first graph revision at which the payload is stale.
    pub expires_at_revision: u64,
}

impl<T> ExpiringFacet<T> {
    /// Wrap `value` with a revision-indexed expiry boundary.
    pub fn new(value: T, expires_at_revision: u64) -> Self {
        Self {
            value,
            expires_at_revision,
        }
    }

    /// Whether the payload is live at `revision`.
    pub fn is_live_at(&self, revision: u64) -> bool {
        revision < self.expires_at_revision
    }

    /// Borrow the payload when it is live at `revision`.
    pub fn value_at(&self, revision: u64) -> Option<&T> {
        self.is_live_at(revision).then_some(&self.value)
    }

    /// Take the payload when it is live at `revision`.
    pub fn into_value_at(self, revision: u64) -> Option<T> {
        self.is_live_at(revision).then_some(self.value)
    }
}

/// The validation seam. chartulary stays schema-agnostic; a host injects a
/// validator that checks a facet's value against the facet's schema. The
/// gate/host validates on write; the store never validates on load (stored data
/// is already-accepted, and unknown facets must survive).
pub trait FacetValidator {
    /// Accept or reject a facet value for `facet_id`.
    fn validate(&self, facet_id: &FacetId, value: &Value) -> Result<(), FacetError>;
}

/// The permissive default: accept every facet. chartulary's only shipped
/// validator; real schema-checking is a host concern (eidetic mere-side).
#[derive(Clone, Copy, Debug, Default)]
pub struct AcceptAll;

impl FacetValidator for AcceptAll {
    fn validate(&self, _facet_id: &FacetId, _value: &Value) -> Result<(), FacetError> {
        Ok(())
    }
}

/// The facets one node carries: at most one value per [`FacetId`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeFacets {
    #[serde(flatten)]
    facets: BTreeMap<FacetId, Value>,
}

impl NodeFacets {
    /// An empty facet set.
    pub fn new() -> Self {
        Self::default()
    }

    /// The value of one facet, if present.
    pub fn get(&self, facet_id: &FacetId) -> Option<&Value> {
        self.facets.get(facet_id)
    }

    /// Whether this node carries `facet_id`.
    pub fn has(&self, facet_id: &FacetId) -> bool {
        self.facets.contains_key(facet_id)
    }

    /// Iterate this node's facets.
    pub fn iter(&self) -> impl Iterator<Item = (&FacetId, &Value)> {
        self.facets.iter()
    }

    /// Number of facets carried.
    pub fn len(&self) -> usize {
        self.facets.len()
    }

    /// Whether this node carries no facets.
    pub fn is_empty(&self) -> bool {
        self.facets.is_empty()
    }
}

/// The runtime-facet store for a graph: every node's facets, keyed by node id.
/// Persisted beside the graph as one document; the node id type is the graph's
/// `Identified::Id` (a `String` for [`Container`](crate::Container)). Use a
/// string-serializable id with a JSON codec (the id is a map key).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FacetStore<Id: Ord> {
    nodes: BTreeMap<Id, NodeFacets>,
}

// Manual, so `Id` need not be `Default` (a `BTreeMap` is empty by default).
impl<Id: Ord> Default for FacetStore<Id> {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }
}

impl<Id: Ord + Clone> FacetStore<Id> {
    /// An empty store.
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }

    /// Set (or replace) a facet on a node, validating it first. On rejection
    /// nothing changes and the [`FacetError`] is returned.
    pub fn set(
        &mut self,
        node: Id,
        facet_id: FacetId,
        value: Value,
        validator: &impl FacetValidator,
    ) -> Result<(), FacetError> {
        validator.validate(&facet_id, &value)?;
        self.nodes
            .entry(node)
            .or_default()
            .facets
            .insert(facet_id, value);
        Ok(())
    }

    /// The value of one facet on one node, if present.
    pub fn get(&self, node: &Id, facet_id: &FacetId) -> Option<&Value> {
        self.nodes.get(node)?.facets.get(facet_id)
    }

    /// All facets a node carries, or `None` if the node has none.
    pub fn facets_of(&self, node: &Id) -> Option<&NodeFacets> {
        self.nodes.get(node)
    }

    /// Remove one facet from a node, returning its value if it was present. An
    /// emptied node is dropped so the store lists only nodes with facets.
    pub fn remove(&mut self, node: &Id, facet_id: &FacetId) -> Option<Value> {
        let removed = self.nodes.get_mut(node)?.facets.remove(facet_id);
        if self.nodes.get(node).is_some_and(NodeFacets::is_empty) {
            self.nodes.remove(node);
        }
        removed
    }

    /// Drop every facet of a node (node removed from the graph).
    pub fn remove_node(&mut self, node: &Id) -> Option<NodeFacets> {
        self.nodes.remove(node)
    }

    /// Whether no node carries any facet.
    pub fn is_empty(&self) -> bool {
        self.nodes.values().all(NodeFacets::is_empty)
    }

    /// Iterate (node id, its facets).
    pub fn iter(&self) -> impl Iterator<Item = (&Id, &NodeFacets)> {
        self.nodes.iter()
    }

    /// Overlay every facet from `other`, replacing values with the same
    /// `(node, facet)` key and preserving unrelated local or foreign facets.
    ///
    /// This is the load-time convergence seam: a graph may first import legacy
    /// inline columns into facets, then overlay the already-canonical sidecar.
    /// The sidecar wins where both contain the same facet.
    pub fn overlay(&mut self, other: Self) {
        for (node, facets) in other.nodes {
            self.nodes
                .entry(node)
                .or_default()
                .facets
                .extend(facets.facets);
        }
    }
}

// Persistence through muniment, beside the graph (one slot).
impl<Id> FacetStore<Id>
where
    Id: Ord + Clone + Serialize + serde::de::DeserializeOwned,
{
    /// Write the store to a muniment slot.
    pub async fn save<B, C>(
        &self,
        slots: &muniment::SlotStore<B, C>,
        key: &str,
    ) -> Result<(), muniment::StoreError>
    where
        B: muniment::Backend,
        C: muniment::Codec,
    {
        slots.save(key, self).await
    }

    /// Load the store from a muniment slot, or an empty store if absent.
    pub async fn load<B, C>(
        slots: &muniment::SlotStore<B, C>,
        key: &str,
    ) -> Result<Self, muniment::StoreError>
    where
        B: muniment::Backend,
        C: muniment::Codec,
    {
        Ok(slots.load(key).await?.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muniment::{JsonSlots, MemoryBackend};
    use serde_json::json;

    /// A validator that rejects one named facet, to exercise the seam.
    struct RejectFacet(&'static str);
    impl FacetValidator for RejectFacet {
        fn validate(&self, facet_id: &FacetId, _value: &Value) -> Result<(), FacetError> {
            if facet_id.as_str() == self.0 {
                Err(FacetError {
                    facet: facet_id.clone(),
                    reason: "rejected by test validator".into(),
                })
            } else {
                Ok(())
            }
        }
    }

    fn viewer() -> FacetId {
        FacetId::new("web.viewer")
    }

    #[test]
    fn expiring_facets_are_live_before_the_revision_boundary_only() {
        let envelope = ExpiringFacet::new(json!({ "level": 7 }), 12);
        assert_eq!(envelope.value_at(11), Some(&json!({ "level": 7 })));
        assert_eq!(envelope.value_at(12), None);
        assert_eq!(envelope.value_at(13), None);

        let wire = serde_json::to_value(&envelope).unwrap();
        let restored: ExpiringFacet<Value> = serde_json::from_value(wire).unwrap();
        assert_eq!(restored, envelope);
    }

    #[test]
    fn a_facet_sets_and_reads_back() {
        let mut store: FacetStore<String> = FacetStore::new();
        store
            .set(
                "n1".into(),
                viewer(),
                json!({ "mode": "reader" }),
                &AcceptAll,
            )
            .unwrap();
        assert_eq!(
            store.get(&"n1".to_string(), &viewer()),
            Some(&json!({ "mode": "reader" }))
        );
        assert!(
            store.facets_of(&"n2".to_string()).is_none(),
            "other nodes have none"
        );
    }

    #[test]
    fn the_validator_seam_refuses_and_stores_nothing() {
        let mut store: FacetStore<String> = FacetStore::new();
        let err = store
            .set(
                "n1".into(),
                viewer(),
                json!(true),
                &RejectFacet("web.viewer"),
            )
            .unwrap_err();
        assert_eq!(err.facet, viewer());
        assert!(
            store.get(&"n1".to_string(), &viewer()).is_none(),
            "rejected: nothing stored"
        );
        // A different facet passes the same validator.
        store
            .set(
                "n1".into(),
                FacetId::new("note.pin"),
                json!(1),
                &RejectFacet("web.viewer"),
            )
            .unwrap();
        assert!(
            store
                .get(&"n1".to_string(), &FacetId::new("note.pin"))
                .is_some()
        );
    }

    #[test]
    fn removing_a_facet_prunes_an_emptied_node() {
        let mut store: FacetStore<String> = FacetStore::new();
        store
            .set("n1".into(), viewer(), json!("x"), &AcceptAll)
            .unwrap();
        assert_eq!(store.remove(&"n1".to_string(), &viewer()), Some(json!("x")));
        assert!(
            store.facets_of(&"n1".to_string()).is_none(),
            "emptied node dropped"
        );
    }

    #[test]
    fn overlay_replaces_matching_values_and_preserves_foreign_facets() {
        let node = "n1".to_string();
        let mut base: FacetStore<String> = FacetStore::new();
        base.set(
            node.clone(),
            FacetId::new("visit.history"),
            json!({"last": 1}),
            &AcceptAll,
        )
        .unwrap();
        base.set(
            node.clone(),
            FacetId::new("foreign.keep"),
            json!(true),
            &AcceptAll,
        )
        .unwrap();

        let mut sidecar: FacetStore<String> = FacetStore::new();
        sidecar
            .set(
                node.clone(),
                FacetId::new("visit.history"),
                json!({"last": 2}),
                &AcceptAll,
            )
            .unwrap();
        sidecar
            .set(
                node.clone(),
                FacetId::new("web.viewer"),
                json!("reader"),
                &AcceptAll,
            )
            .unwrap();

        base.overlay(sidecar);

        assert_eq!(
            base.get(&node, &FacetId::new("visit.history")),
            Some(&json!({"last": 2}))
        );
        assert_eq!(
            base.get(&node, &FacetId::new("foreign.keep")),
            Some(&json!(true))
        );
        assert_eq!(
            base.get(&node, &FacetId::new("web.viewer")),
            Some(&json!("reader"))
        );
    }

    #[test]
    fn an_unknown_facet_id_survives_a_round_trip() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());
            let mut store: FacetStore<String> = FacetStore::new();
            // A facet id this build has no schema for — arbitrary future/foreign metadata.
            let foreign = FacetId::new("some-other-app.exotic");
            store
                .set(
                    "n1".into(),
                    foreign.clone(),
                    json!({ "k": [1, 2, 3] }),
                    &AcceptAll,
                )
                .unwrap();
            store
                .set("n1".into(), viewer(), json!("reader"), &AcceptAll)
                .unwrap();
            store.save(&slots, "facets").await.unwrap();

            let restored = FacetStore::<String>::load(&slots, "facets").await.unwrap();
            assert_eq!(restored, store, "the whole store round-trips");
            assert_eq!(
                restored.get(&"n1".to_string(), &foreign),
                Some(&json!({ "k": [1, 2, 3] })),
                "the unknown facet is preserved untouched"
            );
        });
    }

    #[test]
    fn absent_slot_loads_an_empty_store() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());
            let store = FacetStore::<String>::load(&slots, "never-saved")
                .await
                .unwrap();
            assert!(store.is_empty());
        });
    }
}
