/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-pane view-intent sidecar — durable storage for view-local
//! state that doesn't belong in graph truth.
//!
//! v0 carries only the per-orrery `hidden_relations` set so a user's
//! hide gestures survive restarts. Per the
//! [`ViewIntent` sidecar plan](../../../../design_docs/mere_docs/implementation_strategy/2026-05-14_view_intent_sidecar_plan.md)
//! and the [framing brief §5.3](../../../../design_docs/mere_docs/research/2026-05-11_browser_multiplexer_framing.md#53-view-intent-persistence-viewintent-sidecar):
//!
//! ```text
//! <sessions_dir>/
//! └── <session_id>/
//!     ├── manifest.json            ← ManifestStore
//!     ├── graph.json               ← session_graph_store
//!     └── views/
//!         └── <frame_id>/
//!             └── <pane_id>.json   ← this module
//! ```
//!
//! v0a (this module) is the I/O primitive: type definitions plus
//! `save_view_intent` / `load_view_intent` / `view_intent_exists`.
//! v0b host wiring (load on pane bootstrap, save on `HideRelation`
//! toggle) lands in `host` as a follow-up; see the plan's §4 for
//! the open questions around session-id resolution at pane scope.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Subdirectory under the session root holding per-frame /
/// per-pane view sidecars.
pub const VIEW_INTENT_DIR: &str = "views";

/// One entry in [`ViewIntent::hidden_relations`]. Keyed by stable
/// node UUIDs and the [`kernel::graph::RelationKind::tag`]
/// ordinal so a Hyperlink line and a Cites line on the same node
/// pair persist independently.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HiddenRelationRecord {
    pub from_id: Uuid,
    pub to_id: Uuid,
    /// Round-tripped via `RelationKind::from_tag` on load.
    pub relation_tag: u32,
}

impl HiddenRelationRecord {
    pub fn new(from_id: Uuid, to_id: Uuid, relation_tag: u32) -> Self {
        Self {
            from_id,
            to_id,
            relation_tag,
        }
    }
}

/// 2D affine snapshot for serialisation. Stores the six kurbo
/// `Affine` coefficients verbatim — round-trips identity, pan,
/// zoom, and rotation.
///
/// Wire-shape kept compact (a single `[f64; 6]`) so the JSON
/// representation reads clearly and diffs cleanly:
///
/// ```text
/// "camera": { "coefficients": [1.0, 0.0, 0.0, 1.0, 100.0, 50.0] }
/// ```
///
/// (That value is a pure pan of (100, 50) with identity scale +
/// no rotation.)
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraSnapshot {
    /// kurbo `Affine::as_coeffs()` order: `[a, b, c, d, e, f]` for
    /// the affine `[[a, c, e], [b, d, f]]`.
    pub coefficients: [f64; 6],
}

impl CameraSnapshot {
    /// Identity transform — no pan, no zoom, no rotation.
    pub const IDENTITY: Self = Self {
        coefficients: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    /// True when this snapshot equals identity. The save path can
    /// skip persisting an identity camera (no user gesture worth
    /// recording).
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }
}

impl Default for CameraSnapshot {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Persistable per-pane view state. v0 carried `hidden_relations`
/// only; `camera` joined per the spatial-chrome IR brief §6's
/// pan/zoom-as-substrate-camera framing; `focus` (the focused node's
/// URL) joined when the host began restoring it on reload. Future fields
/// (`form_factor`, `filter`, `strategy`, `overlays`) land as their
/// producers wire up. Per the plan §2, bundling unwired
/// fields now would be the half-finished shape the user's memory
/// warns against.
///
/// `BTreeSet` over `HashSet` for deterministic JSON output —
/// dev-tier inspection wants stable diffs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewIntent {
    #[serde(default)]
    pub hidden_relations: BTreeSet<HiddenRelationRecord>,
    /// Substrate camera snapshot (pan + zoom). `None` when the pane
    /// has no saved camera (fresh pane → host falls back to
    /// `CameraSnapshot::IDENTITY`). Save path uses `None` for
    /// identity cameras too, since persisting an unmoved camera is
    /// wasted I/O.
    #[serde(default)]
    pub camera: Option<CameraSnapshot>,
    /// URL of the node this pane is focused on, so a reload re-opens its media.
    /// Keyed by URL (URL identity) rather than petgraph `NodeKey`, which isn't
    /// stable across save/load — the same reason `GraphSnapshot` is URL-keyed.
    /// `None` when nothing is focused.
    #[serde(default)]
    pub focus: Option<String>,
    /// The orrery pane's active layout strategy (a cartography adapter `projection_id`,
    /// e.g. `"phyllotaxis.default"`), or `None` for force-directed (gyre, the default).
    /// The host re-applies it on load; the positions themselves are recomputed, never
    /// persisted (analytic layouts re-derive from the node set). (Layout picker.)
    #[serde(default)]
    pub strategy: Option<String>,
    /// Whether the orrery pane is in live workbench-mirror mode: scoped each frame to
    /// the workbench's open tiles. Restored on load (the scope itself re-derives from
    /// the open tiles, never persisted). `false` by default. (Workbench mirror.)
    #[serde(default)]
    pub mirror_tiles: bool,
}

impl ViewIntent {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when nothing in this `ViewIntent` is worth persisting.
    /// The save path can skip the file for the empty case; the load
    /// path returns `Ok(None)` and the host falls back to default.
    pub fn is_empty(&self) -> bool {
        self.hidden_relations.is_empty()
            && self.camera.is_none()
            && self.focus.is_none()
            && self.strategy.is_none()
    }
}

/// Filename for one pane's view-intent sidecar. The pane-id portion
/// is rendered as the decimal `PaneId(u64)`; the host computes the
/// containing frame directory.
pub fn pane_intent_filename(pane_id: u64) -> String {
    format!("{pane_id}.json")
}

/// Build the path `<session_dir>/views/<frame_id>/<pane_id>.json`.
/// `session_dir` is the per-session root (sibling to `manifest.json`
/// and `graph.json`). `frame_id_str` should be the `FrameId`'s
/// underlying UUID string (lowercase, hyphenated). Constructing the
/// path is pure — callers can use it for existence checks before
/// loading.
pub fn view_intent_path(session_dir: &Path, frame_id_str: &str, pane_id: u64) -> PathBuf {
    session_dir
        .join(VIEW_INTENT_DIR)
        .join(frame_id_str)
        .join(pane_intent_filename(pane_id))
}

/// Serialise `intent` to JSON and write it atomically (tmp + rename)
/// to the sidecar path. Creates intermediate directories as needed
/// — sessions don't usually start with a `views/` folder.
///
/// Returns `Ok(())` even when `intent.is_empty()`; callers that want
/// to skip empty writes should check via [`ViewIntent::is_empty`]
/// before calling. Saving the empty document is harmless but creates
/// a file the loader will faithfully restore as the default value.
pub fn save_view_intent(
    session_dir: &Path,
    frame_id_str: &str,
    pane_id: u64,
    intent: &ViewIntent,
) -> io::Result<()> {
    let target = view_intent_path(session_dir, frame_id_str, pane_id);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(intent)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Read `<session_dir>/views/<frame_id>/<pane_id>.json` and parse it
/// as `ViewIntent`. Returns `Ok(None)` when the file doesn't exist
/// (fresh pane — the host falls back to `ViewIntent::default()`).
pub fn load_view_intent(
    session_dir: &Path,
    frame_id_str: &str,
    pane_id: u64,
) -> io::Result<Option<ViewIntent>> {
    let path = view_intent_path(session_dir, frame_id_str, pane_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let intent: ViewIntent =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(intent))
}

/// True when the sidecar exists for this pane. Helper for callers
/// that want to skip the read when nothing's there to load.
pub fn view_intent_exists(session_dir: &Path, frame_id_str: &str, pane_id: u64) -> bool {
    view_intent_path(session_dir, frame_id_str, pane_id).exists()
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
        let dir = std::env::temp_dir().join(format!("mere-view-intent-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixture_frame() -> String {
        "11111111-2222-3333-4444-555555555555".to_string()
    }

    fn sample_intent() -> ViewIntent {
        let mut intent = ViewIntent::new();
        intent.hidden_relations.insert(HiddenRelationRecord::new(
            Uuid::from_u128(0xa1),
            Uuid::from_u128(0xa2),
            // Cites tag: Semantic family (0 << 24) + sub ordinal 3.
            3,
        ));
        intent.hidden_relations.insert(HiddenRelationRecord::new(
            Uuid::from_u128(0xb1),
            Uuid::from_u128(0xb2),
            // Hyperlink tag: family 0, sub 0.
            0,
        ));
        intent
    }

    #[test]
    fn save_then_load_round_trips_intent() {
        let dir = temp_session_dir("round-trip");
        let frame = fixture_frame();
        let original = sample_intent();
        save_view_intent(&dir, &frame, 7, &original).unwrap();
        let restored = load_view_intent(&dir, &frame, 7)
            .unwrap()
            .expect("intent file should be present");
        assert_eq!(restored, original);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn focus_round_trips_and_counts_as_nonempty() {
        let dir = temp_session_dir("focus");
        let frame = fixture_frame();
        let mut intent = ViewIntent::new();
        intent.focus = Some("https://example.com/page".to_string());
        assert!(!intent.is_empty(), "a focus alone is worth persisting");
        save_view_intent(&dir, &frame, 1, &intent).unwrap();
        let restored = load_view_intent(&dir, &frame, 1).unwrap().unwrap();
        assert_eq!(restored.focus.as_deref(), Some("https://example.com/page"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn strategy_round_trips_and_counts_as_nonempty() {
        let dir = temp_session_dir("strategy");
        let frame = fixture_frame();
        let mut intent = ViewIntent::new();
        intent.strategy = Some("phyllotaxis.default".to_string());
        assert!(
            !intent.is_empty(),
            "a layout strategy alone is worth persisting"
        );
        save_view_intent(&dir, &frame, 1, &intent).unwrap();
        let restored = load_view_intent(&dir, &frame, 1).unwrap().unwrap();
        assert_eq!(restored.strategy.as_deref(), Some("phyllotaxis.default"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = temp_session_dir("no-file");
        let frame = fixture_frame();
        let result = load_view_intent(&dir, &frame, 42).unwrap();
        assert!(result.is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn exists_reflects_save_state() {
        let dir = temp_session_dir("exists");
        let frame = fixture_frame();
        assert!(!view_intent_exists(&dir, &frame, 1));
        save_view_intent(&dir, &frame, 1, &sample_intent()).unwrap();
        assert!(view_intent_exists(&dir, &frame, 1));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_overwrites_existing_intent_atomically() {
        let dir = temp_session_dir("overwrite");
        let frame = fixture_frame();
        save_view_intent(&dir, &frame, 1, &sample_intent()).unwrap();
        let mut updated = ViewIntent::new();
        updated.hidden_relations.insert(HiddenRelationRecord::new(
            Uuid::from_u128(0xc1),
            Uuid::from_u128(0xc2),
            0,
        ));
        save_view_intent(&dir, &frame, 1, &updated).unwrap();
        let restored = load_view_intent(&dir, &frame, 1).unwrap().unwrap();
        assert_eq!(restored.hidden_relations.len(), 1);
        assert!(
            restored
                .hidden_relations
                .iter()
                .any(|r| r.from_id == Uuid::from_u128(0xc1))
        );
        // No leftover .tmp file from the atomic write.
        let tmp = view_intent_path(&dir, &frame, 1).with_extension("json.tmp");
        assert!(!tmp.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_intent_round_trips_identically() {
        let dir = temp_session_dir("empty");
        let frame = fixture_frame();
        let original = ViewIntent::new();
        save_view_intent(&dir, &frame, 9, &original).unwrap();
        let restored = load_view_intent(&dir, &frame, 9).unwrap().unwrap();
        assert!(restored.is_empty());
        assert_eq!(restored, ViewIntent::default());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_json_returns_invalid_data_error() {
        let dir = temp_session_dir("malformed");
        let frame = fixture_frame();
        let path = view_intent_path(&dir, &frame, 5);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json at all").unwrap();
        match load_view_intent(&dir, &frame, 5) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("expected malformed JSON to fail parsing"),
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_creates_views_and_frame_subdirectories() {
        let dir = temp_session_dir("dir-create");
        let frame = fixture_frame();
        assert!(!dir.join(VIEW_INTENT_DIR).exists());
        save_view_intent(&dir, &frame, 1, &sample_intent()).unwrap();
        assert!(dir.join(VIEW_INTENT_DIR).join(&frame).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn intent_path_composition_matches_spec() {
        let dir = PathBuf::from("/tmp/mere-test");
        let path = view_intent_path(&dir, "frame-abc", 42);
        let expected = dir.join("views").join("frame-abc").join("42.json");
        assert_eq!(path, expected);
    }

    #[test]
    fn relation_tag_round_trips_through_disk() {
        // Cites tag in the file → RelationKind::Cites in memory.
        // Pins the contract: relation_tag is the
        // `RelationKind::tag()` ordinal and survives the round-trip
        // intact. If the tag ordinal scheme ever shifts, this test
        // breaks and forces an explicit decision on migration.
        let dir = temp_session_dir("tag-round-trip");
        let frame = fixture_frame();
        let mut original = ViewIntent::new();
        let from = Uuid::from_u128(1);
        let to = Uuid::from_u128(2);
        let cites_tag =
            kernel::graph::RelationKind::Semantic(kernel::graph::SemanticSubKind::Cites).tag();
        original
            .hidden_relations
            .insert(HiddenRelationRecord::new(from, to, cites_tag));
        save_view_intent(&dir, &frame, 1, &original).unwrap();
        let restored = load_view_intent(&dir, &frame, 1).unwrap().unwrap();
        let record = restored.hidden_relations.iter().next().unwrap();
        assert_eq!(
            kernel::graph::RelationKind::from_tag(record.relation_tag),
            Some(kernel::graph::RelationKind::Semantic(
                kernel::graph::SemanticSubKind::Cites,
            )),
        );
        fs::remove_dir_all(&dir).ok();
    }
}
