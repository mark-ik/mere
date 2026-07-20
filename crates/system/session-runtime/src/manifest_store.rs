// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! In-memory + on-disk store for [`GraphSessionManifest`]s.
//!
//! The runtime layer of session persistence per the
//! [GraphSessionManifest implementation plan](../../../../design_docs/mere_docs/implementation_strategy/2026-05-11_graph_session_manifest_plan.md).
//! Owns the durable session identity records; host adapters (gpui or
//! otherwise) layer their runtime graph entities on top.
//!
//! Storage layout:
//!
//! ```text
//! <sessions_dir>/
//! ├── <session_id_1>/
//! │   └── manifest.json
//! ├── <session_id_2>/
//! │   └── manifest.json
//! └── .trash/
//!     └── <session_id_dead>/manifest.json
//! ```
//!
//! Graph-data persistence (the `graph/` directory under each
//! session) is the **next slice** — it depends on the eidetic graph
//! persistence boundary being clean for `kernel::graph::Graph`.
//! Until then, manifests persist independently; graph entities
//! reset on app restart.
//!
//! ## Lifecycle (host-side responsibilities)
//!
//! - App start: call [`ManifestStore::load_from_disk`] against the
//!   configured sessions dir.
//! - Session creation: call [`ManifestStore::insert`] with a fresh
//!   manifest; the store marks it dirty.
//! - Session mutation: call [`ManifestStore::mark_dirty`] when the
//!   manifest's contents have changed.
//! - Periodically (debounced) and on app exit: call
//!   [`ManifestStore::flush_dirty`] to write out the dirty
//!   manifests.
//! - Session kill: call [`ManifestStore::move_to_trash`]; the
//!   manifest is removed from the live map + the directory moves
//!   under `<sessions_dir>/.trash/`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use incipit::SessionId;

use crate::manifest::GraphSessionManifest;

/// Filename of the per-session manifest inside its directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Subdirectory under the sessions root where killed sessions are
/// moved instead of being deleted outright.
pub const TRASH_DIR: &str = ".trash";

/// In-memory + on-disk store for session manifests.
///
/// Wraps a `HashMap<SessionId, GraphSessionManifest>` plus a dirty
/// set. Mutations mark sessions dirty; [`Self::flush_dirty`] writes
/// them to disk in one pass. Reads (`get`, `iter`, `len`) are
/// pure-memory.
#[derive(Default)]
pub struct ManifestStore {
    manifests: HashMap<SessionId, GraphSessionManifest>,
    dirty: HashSet<SessionId>,
    /// Root directory the store reads from / writes to. Set by
    /// [`Self::with_root`] or [`Self::load_from_disk`].
    root: Option<PathBuf>,
}

/// Outcome of a load-from-disk pass over a sessions directory.
/// Surfaces enough detail for diagnostics receipts per the
/// multiplexer framing brief §8.
#[derive(Debug, Default)]
pub struct LoadReport {
    /// Sessions whose manifest parsed cleanly.
    pub loaded: Vec<SessionId>,
    /// Sessions whose directory existed but whose `manifest.json`
    /// failed to parse. Directory is left in place for recovery.
    pub failed: Vec<LoadFailure>,
}

/// One malformed manifest. The store leaves the on-disk directory
/// untouched so the user can inspect or restore.
#[derive(Debug)]
pub struct LoadFailure {
    pub dir_name: String,
    pub reason: String,
}

impl ManifestStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach the store to a sessions-root directory without
    /// reading anything from it. Useful for tests + the common case
    /// where the host wants the path bound up front so subsequent
    /// `flush_dirty` calls don't need a path argument.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            manifests: HashMap::new(),
            dirty: HashSet::new(),
            root: Some(root.into()),
        }
    }

    /// The sessions root, if one is bound.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Bind / re-bind the sessions root.
    pub fn set_root(&mut self, root: impl Into<PathBuf>) {
        self.root = Some(root.into());
    }

    /// Look up a manifest by its `SessionId`.
    pub fn get(&self, id: SessionId) -> Option<&GraphSessionManifest> {
        self.manifests.get(&id)
    }

    /// Mutable lookup. After mutation the caller should call
    /// [`Self::mark_dirty`] (or use [`Self::update`] which handles
    /// both).
    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut GraphSessionManifest> {
        self.manifests.get_mut(&id)
    }

    /// Mutate a manifest and mark it dirty in one call. Returns
    /// `true` if the session existed and was updated.
    pub fn update(&mut self, id: SessionId, f: impl FnOnce(&mut GraphSessionManifest)) -> bool {
        let Some(m) = self.manifests.get_mut(&id) else {
            return false;
        };
        f(m);
        m.touch();
        self.dirty.insert(id);
        true
    }

    /// Iterate over every `(SessionId, &Manifest)`.
    pub fn iter(&self) -> impl Iterator<Item = (SessionId, &GraphSessionManifest)> {
        self.manifests.iter().map(|(k, v)| (*k, v))
    }

    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    /// Insert a fresh manifest. Marks dirty. Returns the
    /// previously-stored manifest under that id if any (usually
    /// `None`).
    pub fn insert(&mut self, manifest: GraphSessionManifest) -> Option<GraphSessionManifest> {
        let id = manifest.session_id;
        self.dirty.insert(id);
        self.manifests.insert(id, manifest)
    }

    /// Remove a session from the in-memory map. Does **not** touch
    /// disk — use [`Self::move_to_trash`] for the disk side.
    pub fn remove(&mut self, id: SessionId) -> Option<GraphSessionManifest> {
        self.dirty.remove(&id);
        self.manifests.remove(&id)
    }

    /// Mark a session as needing a flush. Used by callers that
    /// mutate the manifest through `get_mut` directly.
    pub fn mark_dirty(&mut self, id: SessionId) {
        if self.manifests.contains_key(&id) {
            self.dirty.insert(id);
        }
    }

    pub fn is_dirty(&self, id: SessionId) -> bool {
        self.dirty.contains(&id)
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    /// Walk a sessions directory, deserialise every well-formed
    /// `manifest.json` underneath it, and populate the in-memory
    /// map. The `<root>/.trash/` directory is skipped.
    ///
    /// Returns a [`LoadReport`] enumerating successes and failures
    /// so the caller can emit diagnostics. The function itself
    /// only fails for I/O errors on the root directory traversal;
    /// per-session parse errors are accumulated into the report.
    pub fn load_from_disk(&mut self, root: impl Into<PathBuf>) -> io::Result<LoadReport> {
        let root = root.into();
        self.root = Some(root.clone());
        let mut report = LoadReport::default();
        if !root.exists() {
            // Fresh install: nothing to load. Caller will seed a
            // default session if it wants one.
            return Ok(report);
        }
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if dir_name == TRASH_DIR {
                continue;
            }
            let manifest_path = path.join(MANIFEST_FILE);
            if !manifest_path.exists() {
                report.failed.push(LoadFailure {
                    dir_name: dir_name.to_string(),
                    reason: "manifest.json missing".to_string(),
                });
                continue;
            }
            match fs::read_to_string(&manifest_path)
                .map_err(|e| e.to_string())
                .and_then(|text| {
                    serde_json::from_str::<GraphSessionManifest>(&text).map_err(|e| e.to_string())
                }) {
                Ok(manifest) => {
                    let id = manifest.session_id;
                    self.manifests.insert(id, manifest);
                    report.loaded.push(id);
                }
                Err(reason) => {
                    report.failed.push(LoadFailure {
                        dir_name: dir_name.to_string(),
                        reason,
                    });
                }
            }
        }
        Ok(report)
    }

    /// Write every dirty manifest to its session directory. Clears
    /// the dirty set. Requires a root to be bound (via
    /// [`Self::with_root`], [`Self::set_root`], or
    /// [`Self::load_from_disk`]).
    ///
    /// Returns the count of manifests flushed. Per-manifest write
    /// failures are surfaced as `io::Error` — callers should
    /// decide whether to keep going or bubble up.
    pub fn flush_dirty(&mut self) -> io::Result<usize> {
        let root = self.root.clone().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "ManifestStore::flush_dirty called with no bound root",
            )
        })?;
        let to_flush: Vec<SessionId> = self.dirty.iter().copied().collect();
        let mut written = 0;
        for id in to_flush {
            let Some(manifest) = self.manifests.get(&id) else {
                self.dirty.remove(&id);
                continue;
            };
            let session_dir = root.join(id.as_uuid().to_string());
            fs::create_dir_all(&session_dir)?;
            let manifest_path = session_dir.join(MANIFEST_FILE);
            let json = serde_json::to_string_pretty(manifest)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            // Atomic-ish write: write to a temp file, then rename.
            // On the same FS the rename is atomic; this prevents a
            // half-written manifest if the process dies mid-flush.
            let tmp_path = session_dir.join(format!("{MANIFEST_FILE}.tmp"));
            fs::write(&tmp_path, json)?;
            fs::rename(&tmp_path, &manifest_path)?;
            self.dirty.remove(&id);
            written += 1;
        }
        Ok(written)
    }

    /// Move a session's on-disk directory to `<root>/.trash/`. The
    /// in-memory manifest is removed too. Recoverable manually
    /// from disk until the user clears the trash.
    pub fn move_to_trash(&mut self, id: SessionId) -> io::Result<bool> {
        let root = self.root.clone().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "ManifestStore::move_to_trash called with no bound root",
            )
        })?;
        let removed_in_memory = self.remove(id).is_some();
        let source = root.join(id.as_uuid().to_string());
        if !source.exists() {
            // No on-disk artefacts; in-memory removal stands alone.
            return Ok(removed_in_memory);
        }
        let trash_root = root.join(TRASH_DIR);
        fs::create_dir_all(&trash_root)?;
        let target = trash_root.join(id.as_uuid().to_string());
        // If a trash entry under the same id exists, append a
        // disambiguating timestamp to avoid clobbering.
        let target = if target.exists() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            trash_root.join(format!("{}-{stamp}", id.as_uuid()))
        } else {
            target
        };
        fs::rename(&source, &target)?;
        Ok(true)
    }

    /// The trashed sessions' manifests, newest trashed-directory first (by
    /// directory modification time — good enough for a "removed" list). Each
    /// trash entry is a whole session directory ([`Self::move_to_trash`]
    /// moves it intact), so its `manifest.json` still describes it; an entry
    /// whose manifest is missing or malformed is skipped, never fatal. Reads
    /// the disk on every call — callers that render per frame should cache
    /// and refresh on trash/restore.
    pub fn list_trash(&self) -> Vec<GraphSessionManifest> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        let trash_root = root.join(TRASH_DIR);
        let Ok(entries) = fs::read_dir(&trash_root) else {
            return Vec::new();
        };
        let mut found: Vec<(std::time::SystemTime, GraphSessionManifest)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Ok(text) = fs::read_to_string(path.join(MANIFEST_FILE)) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<GraphSessionManifest>(&text) else {
                continue;
            };
            let when = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            found.push((when, manifest));
        }
        found.sort_by(|(a, _), (b, _)| b.cmp(a));
        found.into_iter().map(|(_, m)| m).collect()
    }

    /// Restore a trashed session: move its directory back under the root and
    /// re-insert its manifest into the in-memory map — the recovery half of
    /// [`Self::move_to_trash`], same-identity by construction (the directory
    /// moved intact, `SessionId` and all). Picks the newest trash entry when
    /// disambiguated duplicates exist. `Ok(false)` when no trash entry holds
    /// this id; an error when a LIVE session directory already occupies the
    /// slot (restore must never clobber the living).
    pub fn restore_from_trash(&mut self, id: SessionId) -> io::Result<bool> {
        let root = self.root.clone().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "ManifestStore::restore_from_trash called with no bound root",
            )
        })?;
        let trash_root = root.join(TRASH_DIR);
        let prefix = id.as_uuid().to_string();
        // Exact entry, else the newest `-stamp` disambiguated one.
        let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        if let Ok(entries) = fs::read_dir(&trash_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name == prefix || name.starts_with(&format!("{prefix}-")) {
                    let when = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    candidates.push((when, path));
                }
            }
        }
        candidates.sort_by(|(a, _), (b, _)| b.cmp(a));
        let Some((_, source)) = candidates.into_iter().next() else {
            return Ok(false);
        };
        let target = root.join(&prefix);
        if target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a live session directory already occupies this id",
            ));
        }
        fs::rename(&source, &target)?;
        let text = fs::read_to_string(target.join(MANIFEST_FILE))?;
        let manifest: GraphSessionManifest =
            serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.manifests.insert(manifest.session_id, manifest);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{EngineProfileBinding, EngramId, GraphSessionManifest};
    use incipit::{GraphId, SessionId};
    use uuid::Uuid;

    fn fixture_session(idx: u128) -> (SessionId, GraphId) {
        (
            SessionId::from_uuid(Uuid::from_u128(
                0x1000_0000_0000_0000_0000_0000_0000_0000 + idx,
            )),
            GraphId::from_uuid(Uuid::from_u128(
                0x2000_0000_0000_0000_0000_0000_0000_0000 + idx,
            )),
        )
    }

    fn temp_root(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("host-runtime-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn insert_marks_dirty_and_iter_returns_inserted() {
        let mut store = ManifestStore::new();
        let (sid, gid) = fixture_session(1);
        store.insert(GraphSessionManifest::new(sid, gid));
        assert_eq!(store.len(), 1);
        assert!(store.is_dirty(sid));
        let collected: Vec<_> = store.iter().map(|(s, _)| s).collect();
        assert_eq!(collected, vec![sid]);
    }

    #[test]
    fn update_runs_closure_and_marks_dirty() {
        let mut store = ManifestStore::new();
        let (sid, gid) = fixture_session(2);
        store.insert(GraphSessionManifest::new(sid, gid));
        // Insert marks dirty; flush would need a root. We just check
        // the update path independently of flushing.
        store.dirty.clear();
        assert!(!store.is_dirty(sid));
        let ok = store.update(sid, |m| {
            m.display_name = Some("research".to_string());
            m.engine_profile = EngineProfileBinding::SessionScoped;
        });
        assert!(ok);
        assert!(store.is_dirty(sid));
        let m = store.get(sid).unwrap();
        assert_eq!(m.display_name.as_deref(), Some("research"));
        assert_eq!(m.engine_profile, EngineProfileBinding::SessionScoped);
    }

    #[test]
    fn update_returns_false_for_missing_session() {
        let mut store = ManifestStore::new();
        let (sid, _) = fixture_session(3);
        assert!(!store.update(sid, |_| {}));
    }

    #[test]
    fn flush_writes_dirty_manifests_to_disk() {
        let root = temp_root("flush");
        let mut store = ManifestStore::with_root(&root);
        let (sid, gid) = fixture_session(4);
        store.insert(GraphSessionManifest::new(sid, gid));
        let written = store.flush_dirty().unwrap();
        assert_eq!(written, 1);
        assert_eq!(store.dirty_count(), 0);
        let manifest_path = root.join(sid.as_uuid().to_string()).join(MANIFEST_FILE);
        assert!(manifest_path.exists());
        let json = fs::read_to_string(&manifest_path).unwrap();
        assert!(json.contains(&sid.as_uuid().to_string()));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn load_from_disk_recovers_what_flush_wrote() {
        let root = temp_root("load");
        let (sid_a, gid_a) = fixture_session(5);
        let (sid_b, gid_b) = fixture_session(6);
        {
            let mut store = ManifestStore::with_root(&root);
            store.insert(GraphSessionManifest::new(sid_a, gid_a));
            store.insert(GraphSessionManifest::new(sid_b, gid_b));
            store.flush_dirty().unwrap();
        }
        let mut store = ManifestStore::new();
        let report = store.load_from_disk(&root).unwrap();
        assert_eq!(report.loaded.len(), 2);
        assert!(report.failed.is_empty());
        assert!(store.get(sid_a).is_some());
        assert!(store.get(sid_b).is_some());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn load_from_disk_records_malformed_manifests_without_panicking() {
        let root = temp_root("malformed");
        // Create a session dir with garbage in its manifest.
        let bad_dir = root.join("bad-session");
        fs::create_dir_all(&bad_dir).unwrap();
        fs::write(bad_dir.join(MANIFEST_FILE), "not json {{").unwrap();
        // And one well-formed for contrast.
        let (sid, gid) = fixture_session(7);
        let good_dir = root.join(sid.as_uuid().to_string());
        fs::create_dir_all(&good_dir).unwrap();
        let manifest = GraphSessionManifest::new(sid, gid);
        fs::write(
            good_dir.join(MANIFEST_FILE),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let mut store = ManifestStore::new();
        let report = store.load_from_disk(&root).unwrap();
        assert_eq!(report.loaded.len(), 1);
        assert_eq!(report.loaded[0], sid);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].dir_name, "bad-session");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn move_to_trash_relocates_directory_and_drops_memory_entry() {
        let root = temp_root("trash");
        let mut store = ManifestStore::with_root(&root);
        let (sid, gid) = fixture_session(8);
        store.insert(GraphSessionManifest::new(sid, gid));
        store.flush_dirty().unwrap();

        let trashed = store.move_to_trash(sid).unwrap();
        assert!(trashed);
        assert!(store.get(sid).is_none());
        let live_path = root.join(sid.as_uuid().to_string());
        assert!(!live_path.exists());
        let trash_path = root.join(TRASH_DIR).join(sid.as_uuid().to_string());
        assert!(trash_path.exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn trash_lists_and_restores_with_identity_intact() {
        let root = temp_root("trash-restore");
        let mut store = ManifestStore::with_root(&root);
        let (sid, gid) = fixture_session(10);
        let mut m = GraphSessionManifest::new(sid, gid);
        m.display_name = Some("expedition".to_string());
        store.insert(m);
        store.flush_dirty().unwrap();
        // A sibling session file inside the dir must survive the round trip
        // (the whole directory moves, sidecars and all).
        fs::write(root.join(sid.as_uuid().to_string()).join("facets.json"), "{}").unwrap();

        store.move_to_trash(sid).unwrap();
        let listed = store.list_trash();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].session_id, sid);
        assert_eq!(listed[0].display_name.as_deref(), Some("expedition"));

        assert!(store.restore_from_trash(sid).unwrap());
        assert!(store.get(sid).is_some(), "the manifest re-listed in memory");
        assert_eq!(store.get(sid).unwrap().root_graph_id, gid, "identity intact");
        assert!(
            root.join(sid.as_uuid().to_string()).join("facets.json").exists(),
            "the session's sidecars came back with it"
        );
        assert!(store.list_trash().is_empty(), "the trash entry is consumed");
        assert!(
            !store.restore_from_trash(sid).unwrap(),
            "a second restore finds nothing"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn restore_never_clobbers_a_live_session() {
        let root = temp_root("trash-clobber");
        let mut store = ManifestStore::with_root(&root);
        let (sid, gid) = fixture_session(11);
        store.insert(GraphSessionManifest::new(sid, gid));
        store.flush_dirty().unwrap();
        store.move_to_trash(sid).unwrap();
        // The same id comes alive again (a re-mint under a hand-copied id).
        store.insert(GraphSessionManifest::new(sid, gid));
        store.flush_dirty().unwrap();
        assert!(
            store.restore_from_trash(sid).is_err(),
            "restore must refuse to overwrite a live session directory"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn consolidation_receipts_round_trip_through_disk() {
        let root = temp_root("consolidate");
        let (sid, gid) = fixture_session(9);
        {
            let mut store = ManifestStore::with_root(&root);
            store.insert(GraphSessionManifest::new(sid, gid));
            store.update(sid, |m| {
                m.record_consolidation(EngramId("engram-receipt-xyz".to_string()));
            });
            store.flush_dirty().unwrap();
        }
        let mut store = ManifestStore::new();
        store.load_from_disk(&root).unwrap();
        let restored = store.get(sid).unwrap();
        assert_eq!(restored.consolidated_engrams.len(), 1);
        assert_eq!(
            restored.consolidated_engrams[0],
            EngramId("engram-receipt-xyz".to_string())
        );
        assert!(restored.last_consolidated_at.is_some());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn flush_without_root_returns_error() {
        let mut store = ManifestStore::new();
        let (sid, gid) = fixture_session(10);
        store.insert(GraphSessionManifest::new(sid, gid));
        let err = store.flush_dirty().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
