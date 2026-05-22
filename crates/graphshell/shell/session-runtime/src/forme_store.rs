// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `FormeStore` — per-session persistence of curated [`FormeDocument`]s.
//!
//! A session carries *many* formes (curated workbenches, compare benches, …) —
//! unlike the single `graph.json` — so they live in a `formes/` directory, one
//! file per [`FormeId`]. The orrery's `Identity` arrangement is implicit and
//! is **not** stored here (only its projection geometry persists, elsewhere).
//!
//! Ownership boundary (composition spine §15): `forme` owns the
//! [`FormeDocument`] types + arrangement schema + pure mutation; this module
//! owns disk I/O + lifecycle (create / load / delete) + timestamp stamping,
//! mirroring [`crate::session_graph_store`]. The clock is injected (`now_ms`)
//! so the store stays testable and portable.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use forme::{FormeDocument, FormeId};

/// Subdirectory under a session dir holding one JSON file per forme.
pub const FORMES_DIR: &str = "formes";

/// `<session_dir>/formes/`.
pub fn formes_dir(session_dir: &Path) -> PathBuf {
    session_dir.join(FORMES_DIR)
}

/// `<session_dir>/formes/<forme_id>.json`.
pub fn forme_path(session_dir: &Path, id: FormeId) -> PathBuf {
    formes_dir(session_dir).join(format!("{}.json", id.as_uuid()))
}

/// Stamp timestamps (`created_at_ms` if unset, `updated_at_ms` always) and
/// write the forme atomically (tmp + rename), creating `formes/` if needed.
/// `now_ms` is Unix-millis supplied by the host's clock.
pub fn save_forme(session_dir: &Path, doc: &mut FormeDocument, now_ms: u64) -> io::Result<()> {
    if doc.created_at_ms == 0 {
        doc.created_at_ms = now_ms;
    }
    doc.updated_at_ms = now_ms;

    let dir = formes_dir(session_dir);
    fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(doc)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let target = forme_path(session_dir, doc.id);
    let tmp = dir.join(format!("{}.json.tmp", doc.id.as_uuid()));
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Load one forme by id. `Ok(None)` if it isn't on disk.
pub fn load_forme(session_dir: &Path, id: FormeId) -> io::Result<Option<FormeDocument>> {
    let path = forme_path(session_dir, id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let doc = serde_json::from_str(&text)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(doc))
}

/// Load every well-formed forme under `formes/`, sorted by creation time.
/// Missing dir → empty. Malformed files are skipped (not fatal), so one bad
/// forme can't block the rest of a session from loading.
pub fn load_all_formes(session_dir: &Path) -> io::Result<Vec<FormeDocument>> {
    let dir = formes_dir(session_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        // `<uuid>.json` only — skips `<uuid>.json.tmp` (extension "tmp").
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(doc) = serde_json::from_str::<FormeDocument>(&text) {
                out.push(doc);
            }
        }
    }
    out.sort_by_key(|d| d.created_at_ms);
    Ok(out)
}

/// Delete a forme from disk. Returns whether a file was removed.
pub fn delete_forme(session_dir: &Path, id: FormeId) -> io::Result<bool> {
    let path = forme_path(session_dir, id);
    if path.exists() {
        fs::remove_file(&path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_session() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mere-forme-store-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_stamps_timestamps_and_round_trips() {
        let dir = temp_session();
        let mut doc = FormeDocument::new(uuid::Uuid::from_u128(1), Some("research".into()));
        doc.arrangement.add_tile_intent(Some(uuid::Uuid::from_u128(9)));
        let id = doc.id;

        save_forme(&dir, &mut doc, 1_700_000_000_000).unwrap();
        assert_eq!(doc.created_at_ms, 1_700_000_000_000);
        assert_eq!(doc.updated_at_ms, 1_700_000_000_000);

        let loaded = load_forme(&dir, id).unwrap().expect("forme on disk");
        assert_eq!(loaded, doc);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn second_save_keeps_created_advances_updated() {
        let dir = temp_session();
        let mut doc = FormeDocument::new(uuid::Uuid::from_u128(2), None);
        save_forme(&dir, &mut doc, 1000).unwrap();
        save_forme(&dir, &mut doc, 2000).unwrap();
        assert_eq!(doc.created_at_ms, 1000);
        assert_eq!(doc.updated_at_ms, 2000);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_all_returns_creation_sorted_and_skips_missing_dir() {
        let dir = temp_session();
        // No formes/ dir yet.
        assert!(load_all_formes(&dir).unwrap().is_empty());

        let mut a = FormeDocument::new(uuid::Uuid::from_u128(1), Some("a".into()));
        let mut b = FormeDocument::new(uuid::Uuid::from_u128(1), Some("b".into()));
        save_forme(&dir, &mut b, 2000).unwrap();
        save_forme(&dir, &mut a, 1000).unwrap();

        let all = load_all_formes(&dir).unwrap();
        assert_eq!(all.len(), 2);
        // Sorted by created_at_ms: a (1000) before b (2000).
        assert_eq!(all[0].label.as_deref(), Some("a"));
        assert_eq!(all[1].label.as_deref(), Some("b"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_removes_only_the_target() {
        let dir = temp_session();
        let mut a = FormeDocument::new(uuid::Uuid::from_u128(1), None);
        let mut b = FormeDocument::new(uuid::Uuid::from_u128(1), None);
        save_forme(&dir, &mut a, 1).unwrap();
        save_forme(&dir, &mut b, 1).unwrap();

        assert!(delete_forme(&dir, a.id).unwrap());
        assert!(load_forme(&dir, a.id).unwrap().is_none());
        assert!(load_forme(&dir, b.id).unwrap().is_some());
        // Deleting again is a no-op false.
        assert!(!delete_forme(&dir, a.id).unwrap());
        fs::remove_dir_all(&dir).ok();
    }
}
