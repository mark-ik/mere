// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-node denizen-binding sidecar — which graph nodes are denizens, and
//! where each one's inner world lives.
//!
//! A **denizen** (a servitor, an agent, a scenario runner, a peer, a pack) can
//! reside in the graph as a node. What makes a node a denizen is not a graph
//! fact — the kernel `Node` is a web-page anchor and stays that — it is host
//! knowledge *about* a node: the denizen's keyholder identity and the id of its
//! nested graph (its inner world, an ordinary chartulary `GraphLog` of grant
//! projections, storage markers, registered commands, journal cursors).
//!
//! So a binding rides beside the graph, keyed by the node's stable UUID, the
//! same slice-C doctrine that moved browser-runtime state into
//! [`browser_node_state`](crate::browser_node_state): the graph library holds
//! graph facts; what the host knows about a node rides in a sidecar. The gate
//! (the `servitor` crate) consumes a binding to run petitions against the
//! denizen's nested graph; this module only stores the pointer.
//!
//! ```text
//! <sessions_dir>/<session_id>/
//! ├── manifest.json            ← ManifestStore
//! ├── graph.json               ← session_graph_store
//! ├── browser_nodes.json       ← browser_node_state
//! └── denizen_bindings.json    ← this module
//! ```
//!
//! Same shape as the browser-node sidecar: one JSON document beside the session
//! graph, atomic write, `Ok(None)` when absent. Deleting it does not corrupt the
//! graph; it un-resides every denizen (their nested graphs persist under their
//! own slots and can be re-bound).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Filename of the sidecar document, sibling to `graph.json`.
pub const DENIZEN_BINDINGS_FILE: &str = "denizen_bindings.json";

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
/// nested graph. `None`-valued fields are meaningless — a binding exists only
/// for a node that is a denizen.
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
    /// The save path prunes these so the document only lists real denizens.
    pub fn is_empty(&self) -> bool {
        self.nested_log.is_empty()
    }
}

/// The live sidecar map the host holds, keyed by stable node UUID. `BTreeMap`
/// for deterministic JSON output (stable diffs).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenizenBindings {
    #[serde(default)]
    pub nodes: BTreeMap<Uuid, DenizenBinding>,
}

impl DenizenBindings {
    /// An empty binding set.
    pub fn new() -> Self {
        Self::default()
    }

    /// The binding for a node, or `None` if the node is not a denizen.
    pub fn get(&self, node_id: Uuid) -> Option<&DenizenBinding> {
        self.nodes.get(&node_id)
    }

    /// Whether a node is a denizen.
    pub fn is_denizen(&self, node_id: Uuid) -> bool {
        self.nodes.contains_key(&node_id)
    }

    /// Bind (or rebind) a node to a denizen, returning any prior binding.
    pub fn bind(&mut self, node_id: Uuid, binding: DenizenBinding) -> Option<DenizenBinding> {
        self.nodes.insert(node_id, binding)
    }

    /// Un-reside a node's denizen (node removed, or the helper uninstalled). The
    /// nested graph is not touched here; archiving it is the gate's concern.
    pub fn remove(&mut self, node_id: Uuid) -> Option<DenizenBinding> {
        self.nodes.remove(&node_id)
    }

    /// True when no node is a denizen.
    pub fn is_empty(&self) -> bool {
        self.nodes.values().all(DenizenBinding::is_empty)
    }

    /// Iterate the denizen nodes and their bindings.
    pub fn iter(&self) -> impl Iterator<Item = (Uuid, &DenizenBinding)> {
        self.nodes.iter().map(|(id, binding)| (*id, binding))
    }
}

/// Path of the sidecar document under the per-session root (sibling to
/// `manifest.json` and `graph.json`). Pure — callers can use it for existence
/// checks.
pub fn denizen_bindings_path(session_dir: &Path) -> PathBuf {
    session_dir.join(DENIZEN_BINDINGS_FILE)
}

/// Serialize `bindings` to JSON and write atomically (tmp + rename). Empty
/// entries are pruned so the document only names real denizens.
pub fn save_denizen_bindings(session_dir: &Path, bindings: &DenizenBindings) -> io::Result<()> {
    let target = denizen_bindings_path(session_dir);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let pruned = DenizenBindings {
        nodes: bindings
            .nodes
            .iter()
            .filter(|(_, b)| !b.is_empty())
            .map(|(k, v)| (*k, v.clone()))
            .collect(),
    };
    let json = serde_json::to_string_pretty(&pruned)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Read the sidecar document. Returns `Ok(None)` when it doesn't exist (a
/// session with no denizens yet).
pub fn load_denizen_bindings(session_dir: &Path) -> io::Result<Option<DenizenBindings>> {
    let path = denizen_bindings_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let bindings: DenizenBindings =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(bindings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_session_dir(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("mere-denizen-bindings-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> DenizenBindings {
        let mut bindings = DenizenBindings::new();
        bindings.bind(
            Uuid::from_u128(0xa),
            DenizenBinding::new("aa".repeat(32), "servitor.trail-keeper", DenizenKind::Servitor),
        );
        bindings.bind(
            Uuid::from_u128(0xb),
            DenizenBinding::new("bb".repeat(32), "agent.gardener", DenizenKind::Agent),
        );
        bindings
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_session_dir("round-trip");
        let original = sample();
        save_denizen_bindings(&dir, &original).unwrap();
        let restored = load_denizen_bindings(&dir).unwrap().expect("sidecar present");
        assert_eq!(restored, original);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = temp_session_dir("no-file");
        assert!(load_denizen_bindings(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_denizen_node_reads_as_none() {
        let bindings = sample();
        assert!(bindings.get(Uuid::from_u128(0xdead)).is_none());
        assert!(!bindings.is_denizen(Uuid::from_u128(0xdead)));
        assert!(bindings.is_denizen(Uuid::from_u128(0xa)));
    }

    #[test]
    fn empty_bindings_are_pruned_on_save() {
        let dir = temp_session_dir("prune");
        let mut bindings = sample();
        // A node bound with no nested graph — meaningless, must not persist.
        bindings.bind(Uuid::from_u128(0xc), DenizenBinding::default());
        assert_eq!(bindings.nodes.len(), 3);
        save_denizen_bindings(&dir, &bindings).unwrap();
        let restored = load_denizen_bindings(&dir).unwrap().unwrap();
        assert_eq!(restored.nodes.len(), 2, "empty binding must not persist");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn removed_binding_is_gone_after_round_trip() {
        let dir = temp_session_dir("remove");
        let mut bindings = sample();
        bindings.remove(Uuid::from_u128(0xa));
        save_denizen_bindings(&dir, &bindings).unwrap();
        let restored = load_denizen_bindings(&dir).unwrap().unwrap();
        assert!(restored.get(Uuid::from_u128(0xa)).is_none());
        assert!(restored.get(Uuid::from_u128(0xb)).is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rebind_replaces_and_reports_the_prior() {
        let mut bindings = DenizenBindings::new();
        let id = Uuid::from_u128(0xa);
        assert!(bindings.bind(id, DenizenBinding::new("aa", "first", DenizenKind::Servitor)).is_none());
        let prior = bindings
            .bind(id, DenizenBinding::new("aa", "second", DenizenKind::Agent))
            .expect("prior binding returned");
        assert_eq!(prior.nested_log, "first");
        assert_eq!(bindings.get(id).unwrap().nested_log, "second");
    }

    #[test]
    fn unknown_kind_loads_forward() {
        // A kind a newer build might write; this build accepts it as Unknown.
        let json = r#"{"nodes":{"00000000-0000-0000-0000-00000000000a":
            {"subject":"aa","nested_log":"x","kind":"oracle"}}}"#;
        let bindings: DenizenBindings = serde_json::from_str(json).unwrap();
        assert_eq!(bindings.get(Uuid::from_u128(0xa)).unwrap().kind, DenizenKind::Unknown);
    }

    #[test]
    fn malformed_json_returns_invalid_data_error() {
        let dir = temp_session_dir("malformed");
        fs::write(denizen_bindings_path(&dir), "{ not json").unwrap();
        match load_denizen_bindings(&dir) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("expected malformed JSON to fail parsing"),
        }
        fs::remove_dir_all(&dir).ok();
    }
}
