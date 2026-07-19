// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Session graph store — a session's graph persisted as hand-inspectable JSON
//! (`graph.json`).
//!
//! The graph is session truth (per the multiplexer framing), persisted through
//! its serde [`GraphSnapshot`] — the URL-stable form, since petgraph NodeIndex
//! keys are not stable across sessions. This is the "serde `graph.json` as the
//! live store" cut; a content-addressed eidetic layer (blobs / manifests /
//! engrams) sits behind it later for media + history.
//!
//! Native-only: this is filesystem persistence. wasm hosts persist through a
//! different backend (IndexedDB / OPFS), so the module is excluded from
//! wasm32 builds to keep the crate wasm-clean.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use incipit::SessionId;
use kernel::graph::Graph;
use kernel::persistence::GraphSnapshot;

use crate::engine_profile_store::SESSIONS_DIR;

/// The graph sidecar file name under a session directory.
pub const GRAPH_FILE: &str = "graph.json";

/// The conventional per-session graph path
/// `<data_root>/sessions/<session_id>/graph.json`, matching the `ManifestStore`
/// and engine-profile layout. Used once manifests thread the session id; a
/// single-session host may persist a default graph at a flat path instead.
pub fn session_graph_path(data_root: &Path, session_id: SessionId) -> PathBuf {
    data_root
        .join(SESSIONS_DIR)
        .join(session_id.as_uuid().to_string())
        .join(GRAPH_FILE)
}

/// Persist `graph` to `path` as pretty JSON, creating parent directories. The
/// graph is written through its [`GraphSnapshot`] (the URL-stable persistence
/// form), so it round-trips across sessions where NodeIndex keys would not.
pub fn save(path: &Path, graph: &Graph) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let snapshot = graph.to_snapshot();
    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

/// Load a graph from `path`, or `Ok(None)` when the file does not exist (a fresh
/// session). A malformed file surfaces as an error so the host can decide whether
/// to fall back to a fresh graph rather than silently discard the session.
pub fn load(path: &Path) -> io::Result<Option<Graph>> {
    let json = match fs::read_to_string(path) {
        Ok(json) => json,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let snapshot: GraphSnapshot =
        serde_json::from_str(&json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(Graph::from_snapshot(&snapshot)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::default::Point2D;
    use kernel::graph::fixtures::GraphFixtures;
    use uuid::Uuid;

    #[test]
    fn round_trips_a_graph_through_json() {
        let mut graph = Graph::new();
        graph.add_node("https://a.example".to_string(), Point2D::new(1.0, 2.0));
        graph.add_node("https://b.example".to_string(), Point2D::new(3.0, 4.0));

        let dir = std::env::temp_dir().join("mere_session_graph_store_roundtrip");
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("graph.json");

        save(&path, &graph).expect("save");
        let loaded = load(&path).expect("load ok").expect("a graph is present");
        assert_eq!(
            loaded.nodes().count(),
            2,
            "both nodes survive the round trip"
        );
        assert!(
            loaded.get_node_by_url("https://a.example").is_some(),
            "the URL index rebuilds from the snapshot",
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn absent_file_loads_to_none() {
        let path = std::env::temp_dir().join("mere_session_graph_store_absent_zzz.json");
        let _ = fs::remove_file(&path);
        assert!(
            load(&path).expect("load ok").is_none(),
            "no file means a fresh session"
        );
    }

    #[test]
    fn session_graph_path_follows_the_sessions_convention() {
        let id = SessionId(Uuid::from_u128(0x4242));
        let path = session_graph_path(Path::new("/data"), id);
        let expected = Path::new("/data")
            .join("sessions")
            .join(id.as_uuid().to_string())
            .join("graph.json");
        assert_eq!(path, expected);
    }
}
