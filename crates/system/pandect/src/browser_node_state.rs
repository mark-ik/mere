// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Per-node browser-state sidecar — durable storage for browser-runtime
//! state that doesn't belong in graph truth.
//!
//! The kernel `Node` carries what the graph knows about a node
//! (addresses, title, tags, relations, authored body). What the
//! *browser* knows about a node — restore fidelity (scroll offset,
//! form draft) and viewing preference (viewer override, compat mode) —
//! lives here, keyed by the node's stable UUID, per the
//! [mere/turnstone boundary pass plan](../../../../design_docs/mere_docs/implementation_strategy/2026-07-09_mere_turnstone_boundary_pass_plan.md)
//! slice C. A browser engine id must never become a graph-library fact.
//!
//! Same shape as the `view_intent_store` sidecar: one JSON document
//! beside the session graph, atomic write, `Ok(None)` when absent.
//!
//! ```text
//! <sessions_dir>/
//! └── <session_id>/
//!     ├── manifest.json            ← ManifestStore
//!     ├── graph.json               ← session_graph_store
//!     ├── browser_nodes.json       ← this module
//!     └── views/...                ← view_intent_store
//! ```
//!
//! Before slice C, scroll and form draft persisted inside the graph
//! snapshot (`PersistedNodeSessionState`) and viewer override / compat
//! mode did not survive restarts at all (their graph deltas fed only
//! the env-gated diagnostics log). [`BrowserNodeStates::absorb_legacy`]
//! is the one-time migration entry: the host drains a pre-split
//! snapshot's session state into this sidecar at load.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Filename of the sidecar document, sibling to `graph.json`.
pub const BROWSER_NODES_FILE: &str = "browser_nodes.json";

/// Browser-runtime state for one node. Everything here is the
/// browser's business: the graph stays correct if this document is
/// deleted (a fresh visit re-derives or defaults each field).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BrowserNodeState {
    /// Last known scroll offset for higher-fidelity cold restore.
    #[serde(default)]
    pub scroll: Option<(f32, f32)>,
    /// Best-effort form draft payload (feature-guarded by caller policy).
    #[serde(default)]
    pub form_draft: Option<String>,
    /// User-set viewer override that takes precedence over MIME/address
    /// based selection. `None` means automatic viewer selection.
    #[serde(default)]
    pub viewer_override: Option<String>,
    /// Per-node compatibility mode: route web-managed content through the
    /// platform WebView regardless of the app-level default web backend.
    #[serde(default)]
    pub compat_mode: bool,
    /// Whether the node's live content was ON at save — the browser's
    /// handling of the node, so a restart respawns its session (turnstone's
    /// rung-6 content-state restore). Additive with a default, so existing
    /// sidecars load unchanged.
    #[serde(default)]
    pub content_on: bool,
    /// Requested page-zoom scale for the node (`1.0` = 100%) — a document
    /// scale, distinct from UI zoom and from the canvas camera zoom. The
    /// owning engine applies its own quantization and bounds; the effective
    /// applied value is engine business and is never stored here. `None`
    /// means the node has never had zoom set (default scale).
    #[serde(default)]
    pub page_scale: Option<f32>,
}

impl BrowserNodeState {
    /// True when nothing here is worth persisting; the save path prunes
    /// empty entries so the document only names nodes with real state.
    pub fn is_empty(&self) -> bool {
        self.scroll.is_none()
            && self.form_draft.is_none()
            && self.viewer_override.is_none()
            && !self.compat_mode
            && !self.content_on
            && self.page_scale.is_none()
    }
}

/// The live sidecar map the host holds, keyed by stable node UUID.
/// `BTreeMap` for deterministic JSON output (stable diffs).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BrowserNodeStates {
    #[serde(default)]
    pub nodes: BTreeMap<Uuid, BrowserNodeState>,
}

impl BrowserNodeStates {
    pub fn new() -> Self {
        Self::default()
    }

    /// Shared read access; absent nodes read as no browser state.
    pub fn get(&self, node_id: Uuid) -> Option<&BrowserNodeState> {
        self.nodes.get(&node_id)
    }

    /// Mutable access, creating the entry on first write.
    pub fn entry(&mut self, node_id: Uuid) -> &mut BrowserNodeState {
        self.nodes.entry(node_id).or_default()
    }

    /// Drop a node's browser state (node removed from the graph, or
    /// forgotten under memory policy).
    pub fn remove(&mut self, node_id: Uuid) -> Option<BrowserNodeState> {
        self.nodes.remove(&node_id)
    }

    /// True when no node carries persistable state.
    pub fn is_empty(&self) -> bool {
        self.nodes.values().all(BrowserNodeState::is_empty)
    }

    /// One-time migration entry: absorb a pre-split snapshot's per-node
    /// session state (scroll / form draft, the fields that used to ride
    /// `PersistedNodeSessionState` inside `graph.json`). Existing sidecar
    /// values win — the sidecar is the authority once it exists; the
    /// legacy snapshot only seeds nodes it hasn't seen.
    pub fn absorb_legacy(
        &mut self,
        node_id: Uuid,
        scroll: Option<(f32, f32)>,
        form_draft: Option<String>,
    ) {
        if scroll.is_none() && form_draft.is_none() {
            return;
        }
        let entry = self.nodes.entry(node_id).or_default();
        if entry.scroll.is_none() {
            entry.scroll = scroll;
        }
        if entry.form_draft.is_none() {
            entry.form_draft = form_draft;
        }
    }
}

/// Path of the sidecar document under the per-session root (sibling
/// to `manifest.json` and `graph.json`). Pure — callers can use it
/// for existence checks.
pub fn browser_node_state_path(session_dir: &Path) -> PathBuf {
    session_dir.join(BROWSER_NODES_FILE)
}

// `save_browser_node_states` left with the web.* facet convergence
// (2026-07-20): the host persists this state as `web.*` facets in facets.json
// (`web_facets::write_web_states`) and never writes browser_nodes.json again.
// The load below stays as the LEGACY fallback — a pre-convergence profile's
// sidecar is absorbed into facets on first load, then goes stale on disk.

/// Read the legacy sidecar document. Returns `Ok(None)` when it doesn't exist
/// (fresh session, pre-split session awaiting migration, or a post-convergence
/// profile whose state lives in facets.json).
pub fn load_browser_node_states(session_dir: &Path) -> io::Result<Option<BrowserNodeStates>> {
    let path = browser_node_state_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let states: BrowserNodeStates =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(states))
}

/// Load the sidecar for a session, seeding it from a pre-split `graph.json`'s
/// legacy per-node session state (scroll / form draft) on first sight.
///
/// The one-time migration entry the host calls wherever it loads a session
/// graph. Idempotent: an existing sidecar is authoritative (its values win via
/// [`BrowserNodeStates::absorb_legacy`]); the legacy snapshot only seeds nodes
/// the sidecar hasn't recorded. Best-effort on the legacy side — a missing or
/// malformed `graph.json` yields whatever the sidecar alone holds (graph load
/// errors are the graph loader's to report, not this migration's).
///
/// Returns `(states, migrated)`; when `migrated` is true the caller should
/// persist promptly (as `web.*` facets, `web_facets::write_web_states` +
/// `save_node_facets`) so the migration is visibly one-time (the next graph
/// re-save drops the legacy fields; the facet store is the record from here).
#[cfg(not(target_arch = "wasm32"))]
pub fn load_or_migrate_browser_node_states(session_dir: &Path) -> (BrowserNodeStates, bool) {
    let mut states = load_browser_node_states(session_dir)
        .ok()
        .flatten()
        .unwrap_or_default();

    let graph_json = session_dir.join(crate::session_graph_store::GRAPH_FILE);
    let legacy: Option<kernel::persistence::GraphSnapshot> = fs::read_to_string(&graph_json)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let mut migrated = false;
    if let Some(snapshot) = legacy {
        for pnode in &snapshot.nodes {
            let Some(session) = &pnode.session_state else {
                continue;
            };
            let scroll = session.scroll_x.zip(session.scroll_y);
            if scroll.is_none() && session.form_draft.is_none() {
                continue;
            }
            let Ok(node_id) = Uuid::parse_str(&pnode.node_id) else {
                continue;
            };
            states.absorb_legacy(node_id, scroll, session.form_draft.clone());
            migrated = true;
        }
    }
    (states, migrated)
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
            std::env::temp_dir().join(format!("mere-browser-nodes-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_states() -> BrowserNodeStates {
        let mut states = BrowserNodeStates::new();
        let a = states.entry(Uuid::from_u128(0xa));
        a.scroll = Some((20.0, 640.0));
        a.form_draft = Some("draft body".to_string());
        let b = states.entry(Uuid::from_u128(0xb));
        b.viewer_override = Some("viewer:note".to_string());
        b.compat_mode = true;
        b.page_scale = Some(1.5);
        states
    }

    /// Write a legacy `browser_nodes.json` fixture the way the retired save
    /// half did (pretty JSON of the map), standing in for a pre-convergence
    /// profile on disk.
    fn write_legacy_fixture(dir: &Path, states: &BrowserNodeStates) {
        let json = serde_json::to_string_pretty(states).unwrap();
        fs::write(browser_node_state_path(dir), json).unwrap();
    }

    #[test]
    fn a_legacy_sidecar_still_loads() {
        let dir = temp_session_dir("legacy-load");
        let original = sample_states();
        write_legacy_fixture(&dir, &original);
        let restored = load_browser_node_states(&dir)
            .unwrap()
            .expect("sidecar should be present");
        assert_eq!(restored, original);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = temp_session_dir("no-file");
        assert!(load_browser_node_states(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn absent_node_reads_as_no_state() {
        let states = sample_states();
        assert!(states.get(Uuid::from_u128(0xdead)).is_none());
    }

    // `empty_entries_are_pruned_on_save` / `removed_node_state_is_gone_after_
    // round_trip` left with the save half: pruning-on-save lives in
    // `web_facets` now (default fields write no facet; the whole-set rewrite
    // clears stale nodes), tested there.

    #[test]
    fn absorb_legacy_seeds_unseen_nodes_only() {
        let mut states = BrowserNodeStates::new();
        let id = Uuid::from_u128(0xa);
        // The sidecar already knows a newer scroll for this node.
        states.entry(id).scroll = Some((1.0, 2.0));
        states.absorb_legacy(id, Some((99.0, 99.0)), Some("old draft".into()));
        let entry = states.get(id).unwrap();
        assert_eq!(entry.scroll, Some((1.0, 2.0)), "sidecar value wins");
        assert_eq!(
            entry.form_draft.as_deref(),
            Some("old draft"),
            "unset field seeds from legacy"
        );
        // A node the sidecar has never seen seeds wholesale.
        let fresh = Uuid::from_u128(0xb);
        states.absorb_legacy(fresh, Some((3.0, 4.0)), None);
        assert_eq!(states.get(fresh).unwrap().scroll, Some((3.0, 4.0)));
        // Nothing-to-absorb creates no entry.
        let untouched = Uuid::from_u128(0xc);
        states.absorb_legacy(untouched, None, None);
        assert!(states.get(untouched).is_none());
    }

    /// A pre-split `graph.json` (the legacy inline session_state form) seeds
    /// the sidecar once; after the sidecar exists its values win on re-runs.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn migrates_legacy_graph_json_session_state_once() {
        let dir = temp_session_dir("migrate");
        let node_id = Uuid::from_u128(0xfeed);
        let legacy_graph = format!(
            r#"{{"nodes":[{{"node_id":"{node_id}","title":"legacy","is_pinned":false,
                "thumbnail_png":null,"thumbnail_width":0,"thumbnail_height":0,
                "favicon_rgba":null,"favicon_width":0,"favicon_height":0,
                "session_state":{{"scroll_x":20.0,"scroll_y":640.0,"form_draft":"draft body"}},
                "mime_hint":null}}],
                "edges":[],"import_records":[],"timestamp_secs":0}}"#
        );
        fs::write(dir.join("graph.json"), legacy_graph).unwrap();

        let (mut states, migrated) = load_or_migrate_browser_node_states(&dir);
        assert!(migrated, "legacy session state must be absorbed");
        let entry = states.get(node_id).expect("migrated entry");
        assert_eq!(entry.scroll, Some((20.0, 640.0)));
        assert_eq!(entry.form_draft.as_deref(), Some("draft body"));

        // A newer sidecar value wins over the still-present legacy snapshot on
        // a second load (the sidecar is authoritative once it exists). Written
        // as a legacy fixture — the live save path is web.* facets now.
        states.entry(node_id).scroll = Some((0.0, 5.0));
        write_legacy_fixture(&dir, &states);
        let (states, _) = load_or_migrate_browser_node_states(&dir);
        assert_eq!(states.get(node_id).unwrap().scroll, Some((0.0, 5.0)));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_json_returns_invalid_data_error() {
        let dir = temp_session_dir("malformed");
        fs::write(browser_node_state_path(&dir), "{ not json").unwrap();
        match load_browser_node_states(&dir) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("expected malformed JSON to fail parsing"),
        }
        fs::remove_dir_all(&dir).ok();
    }
}
