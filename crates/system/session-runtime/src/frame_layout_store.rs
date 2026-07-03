/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Frame-layout sidecar — the content region's pane split tree
//! ([`frame::FrameLayout`]) persisted beside `graph.json`, so a window's pane
//! arrangement (which panes are open, their split ratios) survives a restart.
//!
//! ```text
//! <session_dir>/
//! ├── graph.json            ← session_graph_store
//! ├── settings.json         ← settings_store
//! └── frame.json            ← this module
//! ```
//!
//! v0 stores the single content frame as one file. Per-window / per-frame files
//! arrive with multi-window (the `FrameId` would key a `frames/` subdirectory the
//! way `views/` keys view-intent). Native-only (the `fs` path), so the crate
//! stays wasm-clean.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use frame::FrameLayout;

/// Filename for the content frame's layout sidecar (beside `graph.json`).
pub const FRAME_FILE: &str = "frame.json";

/// The frame sidecar path under `session_dir`.
pub fn frame_layout_path(session_dir: &Path) -> PathBuf {
    session_dir.join(FRAME_FILE)
}

/// Serialize `layout` to JSON and write it atomically (tmp + rename).
pub fn save_frame_layout(session_dir: &Path, layout: &FrameLayout) -> io::Result<()> {
    let target = frame_layout_path(session_dir);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(layout)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Read + parse the frame sidecar. `Ok(None)` when it doesn't exist (fresh
/// session — the host falls back to its default single-pane layout).
pub fn load_frame_layout(session_dir: &Path) -> io::Result<Option<FrameLayout>> {
    let path = frame_layout_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let layout: FrameLayout =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(layout))
}

#[cfg(test)]
mod tests {
    use frame::{FrameId, GraphId, InsertSide, PaneContent, PaneId, PaneNode};

    use super::*;

    fn temp_session_dir(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("mere-frame-store-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_layout() -> FrameLayout {
        let mut layout = FrameLayout {
            id: FrameId::new("content"),
            label: "content".into(),
            root: PaneNode::Leaf {
                pane_id: PaneId(0),
                content: PaneContent::Workbench,
                graph_id: GraphId::from_uuid(uuid::Uuid::from_u128(1)),
            },
        };
        layout.summon_leaf(
            &[],
            InsertSide::Right,
            PaneNode::Leaf {
                pane_id: PaneId(1),
                content: PaneContent::Roster,
                graph_id: GraphId::from_uuid(uuid::Uuid::from_u128(1)),
            },
        );
        layout.set_split_ratio(&[], 0.66);
        layout
    }

    #[test]
    fn save_then_load_round_trips_layout() {
        let dir = temp_session_dir("round-trip");
        let original = sample_layout();
        save_frame_layout(&dir, &original).unwrap();
        let restored = load_frame_layout(&dir)
            .unwrap()
            .expect("frame file present");
        assert_eq!(restored, original);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = temp_session_dir("no-file");
        assert!(load_frame_layout(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_json_is_invalid_data() {
        let dir = temp_session_dir("malformed");
        fs::write(frame_layout_path(&dir), "{ not json").unwrap();
        match load_frame_layout(&dir) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("expected malformed JSON to fail"),
        }
        fs::remove_dir_all(&dir).ok();
    }
}
