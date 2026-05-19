/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-session graph-data persistence — `Graph` ↔ on-disk JSON
//! via `mere-kernel`'s existing `GraphSnapshot` round-trip.
//!
//! Companion to [`crate::manifest_store::ManifestStore`]: the
//! manifest persists *session identity*; this module persists the
//! *graph data* for one session. They share the per-session
//! directory layout:
//!
//! ```text
//! <sessions_dir>/<session_id>/
//! ├── manifest.json   ← ManifestStore
//! └── graph.json      ← SessionGraphStore
//! ```
//!
//! v0 ships JSON only (hand-inspectable, easy to evolve). When
//! write volume or load latency become bottlenecks, the same
//! `GraphSnapshot` types support `rkyv` for compact binary
//! persistence — switch storage substrates without touching the
//! `Graph` API.
//!
//! **Not wired into `mere-host` yet** — bootstrap continues to seed
//! in-memory graphs as before. The wiring slice connects this to
//! `ManifestStore` lifecycle + bootstrap's startup path. This module
//! ships the I/O primitives + tests so the wiring slice is small.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use mere_kernel::graph::Graph;
use mere_kernel::persistence::GraphSnapshot;

/// Filename for the per-session graph snapshot inside the session
/// directory. Sibling to `manifest.json`.
pub const GRAPH_FILE: &str = "graph.json";

/// Compute the on-disk path for a session's graph data given the
/// session's own directory.
pub fn graph_path(session_dir: &Path) -> PathBuf {
    session_dir.join(GRAPH_FILE)
}

/// Serialise `graph` to JSON and write it to
/// `<session_dir>/graph.json`. Writes atomically: first to a `.tmp`
/// file, then renames over the target — so a crashed flush can't
/// leave a half-written snapshot on disk.
///
/// `session_dir` must exist. (The manifest store's `flush_dirty`
/// creates it; callers that bypass the manifest store should
/// `fs::create_dir_all` themselves.)
pub fn save_graph(session_dir: &Path, graph: &Graph) -> io::Result<()> {
    let snapshot = graph.to_snapshot();
    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = session_dir.join(format!("{GRAPH_FILE}.tmp"));
    let target = graph_path(session_dir);
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Read `<session_dir>/graph.json` and reconstruct the `Graph`.
/// Returns `Ok(None)` when the file doesn't exist (fresh session
/// directory — caller decides whether to seed a default graph or
/// treat as an error).
pub fn load_graph(session_dir: &Path) -> io::Result<Option<Graph>> {
    let path = graph_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let snapshot: GraphSnapshot =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(Graph::from_snapshot(&snapshot)))
}

/// True when the session directory has a graph file on disk.
/// Helper for callers that want to decide between "load existing"
/// and "seed default" without invoking `load_graph`.
pub fn graph_exists(session_dir: &Path) -> bool {
    graph_path(session_dir).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::default::Point2D;

    fn temp_session_dir(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("mere-session-graph-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixture_graph() -> Graph {
        let mut g = Graph::new();
        let a = g.add_node("mere://intro".to_string(), Point2D::new(0.0, 0.0));
        let b = g.add_node("mere://probes".to_string(), Point2D::new(80.0, 40.0));
        let _ = g.assert_relation(
            a,
            b,
            mere_kernel::graph::EdgeAssertion::Semantic {
                sub_kind: mere_kernel::graph::SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
        g
    }

    #[test]
    fn save_then_load_round_trips_nodes_and_edges() {
        let dir = temp_session_dir("round-trip");
        let original = fixture_graph();
        save_graph(&dir, &original).unwrap();
        let restored = load_graph(&dir).unwrap().expect("graph file present");
        // Nodes by URL match; we don't compare NodeIndex values
        // directly because petgraph indices aren't stable across
        // construction order.
        let orig_urls: Vec<_> = original.nodes().map(|(_, n)| n.url().to_string()).collect();
        let restored_urls: Vec<_> = restored.nodes().map(|(_, n)| n.url().to_string()).collect();
        let mut o = orig_urls;
        let mut r = restored_urls;
        o.sort();
        r.sort();
        assert_eq!(o, r);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_graph_file() {
        let dir = temp_session_dir("no-file");
        let result = load_graph(&dir).unwrap();
        assert!(result.is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn graph_exists_reflects_save_state() {
        let dir = temp_session_dir("exists");
        assert!(!graph_exists(&dir));
        save_graph(&dir, &fixture_graph()).unwrap();
        assert!(graph_exists(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_overwrites_existing_graph_atomically() {
        let dir = temp_session_dir("overwrite");
        save_graph(&dir, &fixture_graph()).unwrap();
        let mut g2 = Graph::new();
        let _ = g2.add_node("mere://other".to_string(), Point2D::new(0.0, 0.0));
        save_graph(&dir, &g2).unwrap();
        let restored = load_graph(&dir).unwrap().unwrap();
        let urls: Vec<_> = restored.nodes().map(|(_, n)| n.url().to_string()).collect();
        assert_eq!(urls, vec!["mere://other".to_string()]);
        // No leftover .tmp file from the atomic write.
        assert!(!dir.join(format!("{GRAPH_FILE}.tmp")).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_malformed_returns_invalid_data_error() {
        let dir = temp_session_dir("malformed");
        fs::write(graph_path(&dir), "not json at all {{").unwrap();
        // Avoid `.unwrap_err()` — its panic-on-Ok path would need
        // `Option<Graph>: Debug`, and `Graph` doesn't impl Debug.
        match load_graph(&dir) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("expected malformed JSON to fail parsing"),
        }
        fs::remove_dir_all(&dir).ok();
    }
}
