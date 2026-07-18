/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-node facet-store sidecar (`facets.json`) — the durable home for atomic,
//! typed per-node metadata.
//!
//! Facets are the **runtime tier** of the one-node facet system (design doc
//! `2026-07-18_one_node_facets_layer_map.md`): optional, typed, schema-validated
//! records keyed by node id, so a node carries content-specific metadata (and a
//! modder defines new metadata) without changing the kernel `Node`. The store
//! type and its validation seam live in [`chartulary::facet`]; this module is the
//! mere-side **wiring**: it persists a [`FacetStore<Uuid>`](chartulary::FacetStore)
//! (mere's node id is a `Uuid`) beside the session graph, on the same sidecar
//! pattern as [`browser_node_state`](crate::browser_node_state) and
//! [`denizen_bindings`](crate::denizen_bindings).
//!
//! ```text
//! <sessions_dir>/<session_id>/
//! ├── graph.json               ← session_graph_store
//! ├── browser_nodes.json       ← browser_node_state   (web.* facets, later)
//! ├── denizen_bindings.json    ← denizen_bindings     (denizen.* facets, later)
//! └── facets.json              ← this module
//! ```
//!
//! **The convergence target.** The bespoke per-node sidecars (browser state,
//! denizen bindings, and cartography's per-node arrangement data) are facets
//! avant la lettre: typed metadata keyed by node id. This store is the one
//! mechanism they fold into over time (namespaces `web.*`, `denizen.*`,
//! `arrangement.*`), replacing N hand-rolled documents. This wiring is the
//! enabling rung; the migrations follow behind it, opportunistically.
//!
//! Validation (a [`FacetValidator`](chartulary::FacetValidator)) is a write-time
//! host concern (mere backs it with eidetic schema engrams); persistence here is
//! schema-agnostic. One JSON document, atomic write, `Ok(None)` when absent —
//! deleting it drops facets, never graph truth.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use uuid::Uuid;

pub use chartulary::{AcceptAll, FacetError, FacetId, FacetValidator, NodeFacets};

/// The mere node-facet store: a [`chartulary::FacetStore`] keyed by the node's
/// stable `Uuid`.
pub type NodeFacetStore = chartulary::FacetStore<Uuid>;

/// Filename of the sidecar document, sibling to `graph.json`.
pub const NODE_FACETS_FILE: &str = "facets.json";

/// Path of the sidecar document under the per-session root. Pure — callers can
/// use it for existence checks.
pub fn node_facets_path(session_dir: &Path) -> PathBuf {
    session_dir.join(NODE_FACETS_FILE)
}

/// Serialize the store to JSON and write atomically (tmp + rename).
pub fn save_node_facets(session_dir: &Path, facets: &NodeFacetStore) -> io::Result<()> {
    let target = node_facets_path(session_dir);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(facets)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Read the sidecar document. Returns `Ok(None)` when it doesn't exist (a
/// session with no facets yet).
pub fn load_node_facets(session_dir: &Path) -> io::Result<Option<NodeFacetStore>> {
    let path = node_facets_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let facets: NodeFacetStore =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(facets))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_session_dir(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("mere-facet-store-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> NodeFacetStore {
        let mut store = NodeFacetStore::new();
        let node = Uuid::from_u128(0xa);
        store
            .set(node, FacetId::new("web.viewer"), json!({ "mode": "reader" }), &AcceptAll)
            .unwrap();
        store
            .set(node, FacetId::new("arrangement.pin"), json!(true), &AcceptAll)
            .unwrap();
        store
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_session_dir("round-trip");
        let original = sample();
        save_node_facets(&dir, &original).unwrap();
        let restored = load_node_facets(&dir).unwrap().expect("sidecar present");
        assert_eq!(restored, original);
        let node = Uuid::from_u128(0xa);
        assert_eq!(
            restored.get(&node, &FacetId::new("web.viewer")),
            Some(&json!({ "mode": "reader" }))
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = temp_session_dir("no-file");
        assert!(load_node_facets(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_foreign_facet_survives_the_round_trip() {
        // A facet namespace this build has no schema for — a mod's, or a future
        // convergence namespace. The store persists it untouched.
        let dir = temp_session_dir("foreign");
        let mut store = NodeFacetStore::new();
        let node = Uuid::from_u128(0xb);
        store
            .set(node, FacetId::new("some-mod.exotic"), json!({ "k": [1, 2] }), &AcceptAll)
            .unwrap();
        save_node_facets(&dir, &store).unwrap();
        let restored = load_node_facets(&dir).unwrap().unwrap();
        assert_eq!(
            restored.get(&node, &FacetId::new("some-mod.exotic")),
            Some(&json!({ "k": [1, 2] }))
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_json_returns_invalid_data_error() {
        let dir = temp_session_dir("malformed");
        fs::write(node_facets_path(&dir), "{ not json").unwrap();
        match load_node_facets(&dir) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("expected malformed JSON to fail parsing"),
        }
        fs::remove_dir_all(&dir).ok();
    }
}
