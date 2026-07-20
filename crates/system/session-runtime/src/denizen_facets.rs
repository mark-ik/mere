// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `denizen.*` facet namespace — which nodes are denizens, as facets.
//!
//! A **denizen** (a servitor, an agent, a scenario runner, a peer, a pack) can
//! reside in the graph as a node. What makes a node a denizen is not a graph
//! fact — it is host knowledge *about* the node: the denizen's keyholder
//! identity and the id of its nested graph (its inner world, an ordinary
//! chartulary `GraphLog` of grant projections, storage markers, registered
//! commands, journal cursors). Per the one-node ruling, denizen-ness is a
//! **facet bundle, not a node class**: containment is structure, denizen-ness
//! is agency, orthogonal facets on the one node.
//!
//! This module supersedes the transitional `denizen_bindings.json` sidecar
//! (removed 2026-07-20 before any host ever wrote one — the same born-as-facets
//! path the cartography sidecar took): the binding persists as a
//! [`DENIZEN_BINDING`] facet in [`facets.json`](crate::facet_store), keyed by
//! the node's stable UUID like every other facet. Deleting a node's facet
//! un-resides its denizen without touching the graph or the nested graph
//! (which persists under its own slot and can be re-bound).
//!
//! The gate (the `servitor` crate) consumes a binding to run petitions against
//! the denizen's nested graph; this module only stores the pointer.

use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::facet_store::{AcceptAll, FacetId, NodeFacetStore};

/// Facet id of a node's denizen binding. Payload:
/// `{"subject": hex, "nested_log": str, "kind": kebab-case}` — one coherent
/// record (a subject without a nested graph is not a resident), the
/// `arrangement.material` precedent, not three fragment facets.
pub const DENIZEN_BINDING: &str = "denizen.binding";

/// What kind of denizen a node hosts. Descriptive metadata, never a second
/// identity axis (identity is the [`subject`](DenizenBinding::subject) key); the
/// gate treats every kind the same. Serialized kebab-case; unknown-forward via
/// the `#[serde(other)]` catch so a newer kind loads on an older build.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DenizenKind {
    /// A resident helper (the default).
    #[default]
    Servitor,
    /// A model-backed agent working the graph.
    Agent,
    /// A remote collaborator acting through the gate.
    Peer,
    /// A saved scenario / macro runner.
    Scenario,
    /// An installed pack's own resident denizen.
    Pack,
    /// A kind this build does not recognize (forward compatibility).
    #[serde(other)]
    Unknown,
}

/// The binding for one denizen node: its keyholder identity and the id of its
/// nested graph. A binding exists only for a node that is a denizen.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenizenBinding {
    /// The denizen's keyholder identity: lowercase hex of the 32-byte public
    /// key (matches `servitor::Subject::to_hex`). Hand-inspectable in the JSON.
    pub subject: String,
    /// The chartulary `LogId` (string form) of the denizen's nested graph — its
    /// inner world, persisted under its own slot.
    pub nested_log: String,
    /// What kind of denizen this is.
    #[serde(default)]
    pub kind: DenizenKind,
}

impl DenizenBinding {
    /// A binding of `subject` to the nested graph at `nested_log`.
    pub fn new(
        subject: impl Into<String>,
        nested_log: impl Into<String>,
        kind: DenizenKind,
    ) -> Self {
        Self {
            subject: subject.into(),
            nested_log: nested_log.into(),
            kind,
        }
    }

    /// True when this binding names no nested graph — nothing worth persisting.
    /// [`write_denizen_binding`] treats these as a remove, so the store only
    /// carries real denizens.
    pub fn is_empty(&self) -> bool {
        self.nested_log.is_empty()
    }
}

/// Bind (or rebind) `node` as a denizen: write its [`DENIZEN_BINDING`] facet.
/// An [empty](DenizenBinding::is_empty) binding removes instead (nothing worth
/// persisting). Facets in other namespaces are untouched.
pub fn write_denizen_binding(store: &mut NodeFacetStore, node: Uuid, binding: &DenizenBinding) {
    if binding.is_empty() {
        remove_denizen_binding(store, node);
        return;
    }
    let payload = json!({
        "subject": binding.subject,
        "nested_log": binding.nested_log,
        "kind": serde_json::to_value(binding.kind).unwrap_or(serde_json::Value::Null),
    });
    let _ = store.set(node, FacetId::new(DENIZEN_BINDING), payload, &AcceptAll);
}

/// Un-reside `node`'s denizen (node removed, or the helper uninstalled),
/// returning whether a binding was present. The nested graph is not touched
/// here; archiving it is the gate's concern.
pub fn remove_denizen_binding(store: &mut NodeFacetStore, node: Uuid) -> bool {
    store
        .remove(&node, &FacetId::new(DENIZEN_BINDING))
        .is_some()
}

/// The binding for one node, or `None` if the node is not a denizen (no facet,
/// or a malformed payload — a bad facet reads as not-a-denizen rather than
/// failing the caller).
pub fn read_denizen_binding(store: &NodeFacetStore, node: Uuid) -> Option<DenizenBinding> {
    let value = store.get(&node, &FacetId::new(DENIZEN_BINDING))?;
    parse_binding(value)
}

/// Every denizen node and its binding, in node-id order (the store's iteration
/// order). Malformed payloads are skipped.
pub fn read_denizen_bindings(store: &NodeFacetStore) -> Vec<(Uuid, DenizenBinding)> {
    let facet = FacetId::new(DENIZEN_BINDING);
    store
        .iter()
        .filter_map(|(id, facets)| parse_binding(facets.get(&facet)?).map(|b| (*id, b)))
        .collect()
}

/// Whether `node` carries a (well-formed) denizen binding.
pub fn is_denizen(store: &NodeFacetStore, node: Uuid) -> bool {
    read_denizen_binding(store, node).is_some()
}

fn parse_binding(value: &serde_json::Value) -> Option<DenizenBinding> {
    let binding: DenizenBinding = serde_json::from_value(value.clone()).ok()?;
    (!binding.is_empty()).then_some(binding)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_store() -> NodeFacetStore {
        let mut store = NodeFacetStore::new();
        write_denizen_binding(
            &mut store,
            Uuid::from_u128(0xa),
            &DenizenBinding::new("aa".repeat(32), "servitor.trail-keeper", DenizenKind::Servitor),
        );
        write_denizen_binding(
            &mut store,
            Uuid::from_u128(0xb),
            &DenizenBinding::new("bb".repeat(32), "agent.gardener", DenizenKind::Agent),
        );
        store
    }

    #[test]
    fn write_then_read_round_trips() {
        let store = sample_store();
        let bindings = read_denizen_bindings(&store);
        assert_eq!(bindings.len(), 2);
        let a = read_denizen_binding(&store, Uuid::from_u128(0xa)).unwrap();
        assert_eq!(a.nested_log, "servitor.trail-keeper");
        assert_eq!(a.kind, DenizenKind::Servitor);
        assert!(is_denizen(&store, Uuid::from_u128(0xa)));
    }

    #[test]
    fn non_denizen_node_reads_as_none() {
        let store = sample_store();
        assert!(read_denizen_binding(&store, Uuid::from_u128(0xdead)).is_none());
        assert!(!is_denizen(&store, Uuid::from_u128(0xdead)));
    }

    #[test]
    fn an_empty_binding_writes_as_a_remove() {
        let mut store = sample_store();
        // Rebinding node a with no nested graph un-resides it.
        write_denizen_binding(&mut store, Uuid::from_u128(0xa), &DenizenBinding::default());
        assert!(!is_denizen(&mut store, Uuid::from_u128(0xa)));
        assert_eq!(read_denizen_bindings(&store).len(), 1, "b remains");
    }

    #[test]
    fn rebind_replaces() {
        let mut store = NodeFacetStore::new();
        let id = Uuid::from_u128(0xa);
        write_denizen_binding(
            &mut store,
            id,
            &DenizenBinding::new("aa", "first", DenizenKind::Servitor),
        );
        write_denizen_binding(
            &mut store,
            id,
            &DenizenBinding::new("aa", "second", DenizenKind::Agent),
        );
        let binding = read_denizen_binding(&store, id).unwrap();
        assert_eq!(binding.nested_log, "second");
        assert_eq!(binding.kind, DenizenKind::Agent);
    }

    #[test]
    fn remove_un_resides() {
        let mut store = sample_store();
        assert!(remove_denizen_binding(&mut store, Uuid::from_u128(0xa)));
        assert!(!remove_denizen_binding(&mut store, Uuid::from_u128(0xa)));
        assert!(read_denizen_binding(&store, Uuid::from_u128(0xa)).is_none());
    }

    #[test]
    fn unknown_kind_loads_forward() {
        // A kind a newer build might write; this build accepts it as Unknown.
        let mut store = NodeFacetStore::new();
        let id = Uuid::from_u128(0xa);
        store
            .set(
                id,
                FacetId::new(DENIZEN_BINDING),
                serde_json::json!({"subject": "aa", "nested_log": "x", "kind": "oracle"}),
                &AcceptAll,
            )
            .unwrap();
        assert_eq!(
            read_denizen_binding(&store, id).unwrap().kind,
            DenizenKind::Unknown
        );
    }

    #[test]
    fn a_malformed_payload_reads_as_not_a_denizen() {
        let mut store = NodeFacetStore::new();
        let id = Uuid::from_u128(0xa);
        store
            .set(id, FacetId::new(DENIZEN_BINDING), serde_json::json!([1, 2]), &AcceptAll)
            .unwrap();
        assert!(read_denizen_binding(&store, id).is_none());
        assert!(!is_denizen(&store, id));
    }
}
