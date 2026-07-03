/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Native on-disk graph persistence (the `store` feature) — [`Graph`] ↔
//! `graph.json` via the kernel's own [`GraphSnapshot`] round-trip.
//!
//! Lives with its type owner (the kernel), gated by the `store` feature so the
//! kernel stays portable-by-default; the native host enables it. The host
//! supplies the directory; v1 ships JSON (hand-inspectable). The same
//! `GraphSnapshot` types support `rkyv` if compact binary persistence is needed
//! later, without touching the [`Graph`] API.
//!
//! Relocated from `session-runtime::session_graph_store` so the clean host can
//! persist the graph without depending on the substrate session crate (mirrors
//! the `forme::store` relocation).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::graph::Graph;
use crate::persistence::GraphSnapshot;

/// Filename for a session's graph snapshot inside its directory.
pub const GRAPH_FILE: &str = "graph.json";

/// `<session_dir>/graph.json`.
pub fn graph_path(session_dir: &Path) -> PathBuf {
    session_dir.join(GRAPH_FILE)
}

/// Serialise `graph` to `<session_dir>/graph.json`, written atomically
/// (tmp + rename) so a crashed flush can't leave a half-written snapshot.
/// `session_dir` must exist.
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

/// Read `<session_dir>/graph.json` and reconstruct the [`Graph`]. `Ok(None)`
/// when the file doesn't exist (fresh session — the caller seeds a default).
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
            std::env::temp_dir().join(format!("mere-kernel-graph-store-{label}-{pid}-{nanos}"));
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
            crate::graph::EdgeAssertion::Semantic {
                sub_kind: crate::graph::SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
        g
    }

    #[test]
    fn save_then_load_round_trips_nodes() {
        let dir = temp_session_dir("round-trip");
        let original = fixture_graph();
        save_graph(&dir, &original).unwrap();
        let restored = load_graph(&dir).unwrap().expect("graph file present");
        let mut o: Vec<_> = original.nodes().map(|(_, n)| n.url().to_string()).collect();
        let mut r: Vec<_> = restored.nodes().map(|(_, n)| n.url().to_string()).collect();
        o.sort();
        r.sort();
        assert_eq!(o, r);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn node_ids_survive_the_round_trip() {
        let dir = temp_session_dir("ids");
        let original = fixture_graph();
        let mut orig_ids: Vec<_> = original.nodes().map(|(_, n)| n.id).collect();
        save_graph(&dir, &original).unwrap();
        let restored = load_graph(&dir).unwrap().unwrap();
        let mut restored_ids: Vec<_> = restored.nodes().map(|(_, n)| n.id).collect();
        orig_ids.sort();
        restored_ids.sort();
        assert_eq!(
            orig_ids, restored_ids,
            "node ids must be stable across restart"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_graph_file() {
        let dir = temp_session_dir("no-file");
        assert!(load_graph(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_overwrites_atomically_no_tmp_left() {
        let dir = temp_session_dir("overwrite");
        save_graph(&dir, &fixture_graph()).unwrap();
        assert!(!dir.join(format!("{GRAPH_FILE}.tmp")).exists());
        fs::remove_dir_all(&dir).ok();
    }
}
