/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Session-wide settings sidecar — `<session_dir>/settings.json`.
//!
//! Small, user-tunable preferences that are neither graph truth nor per-pane
//! view intent: the active-tab cap today, more (theme, edge-family visibility)
//! as their controls land. One flat JSON document beside `graph.json`, mirroring
//! the [view-intent sidecar](crate::view_intent_store)'s I/O shape — typed record
//! plus `save` / `load` / `exists`, atomic write (tmp + rename), `Ok(None)` when
//! absent so the host falls back to defaults.
//!
//! Each field is `#[serde(default)]`-friendly, so adding a preference later reads
//! older files without a migration — an unknown-to-old / missing-in-new field
//! takes its default rather than failing the parse.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Filename for the session-wide settings sidecar (sibling to `graph.json`).
pub const SETTINGS_FILENAME: &str = "settings.json";

/// The default active-tab cap — the most warm content actors the pool keeps
/// before LRU eviction. Mirrors the chrome's `Settings::default`.
fn default_tab_cap() -> usize {
    12
}

/// Persistable user settings. v0 carried the active-tab cap; `theme_id` joined
/// with the runtime theme switcher. Future preferences join as their controls
/// land, each with a serde default so old files keep parsing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSettings {
    /// The most warm tabs the actor pool keeps before LRU eviction.
    #[serde(default = "default_tab_cap")]
    pub tab_cap: usize,
    /// The active theme id (e.g. `theme:dark`); `None` falls back to the
    /// registry's default theme.
    #[serde(default)]
    pub theme_id: Option<String>,
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self { tab_cap: default_tab_cap(), theme_id: None }
    }
}

/// Build the path `<session_dir>/settings.json`. Pure — callers can use it for an
/// existence check before loading.
pub fn settings_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SETTINGS_FILENAME)
}

/// Serialise `settings` to pretty JSON and write it atomically (tmp + rename) to
/// the sidecar path, creating the session directory if needed.
pub fn save_settings(session_dir: &Path, settings: &PersistedSettings) -> io::Result<()> {
    let target = settings_path(session_dir);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Read `<session_dir>/settings.json` and parse it as [`PersistedSettings`].
/// Returns `Ok(None)` when the file doesn't exist (fresh session — the host falls
/// back to `PersistedSettings::default()`).
pub fn load_settings(session_dir: &Path) -> io::Result<Option<PersistedSettings>> {
    let path = settings_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let settings: PersistedSettings =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(settings))
}

/// True when the settings sidecar exists for this session.
pub fn settings_exist(session_dir: &Path) -> bool {
    settings_path(session_dir).exists()
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
        let dir = std::env::temp_dir().join(format!("mere-settings-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_session_dir("round-trip");
        let original = PersistedSettings { tab_cap: 7, theme_id: None };
        save_settings(&dir, &original).unwrap();
        let restored = load_settings(&dir).unwrap().expect("settings file should be present");
        assert_eq!(restored, original);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = temp_session_dir("no-file");
        assert!(load_settings(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn exists_reflects_save_state() {
        let dir = temp_session_dir("exists");
        assert!(!settings_exist(&dir));
        save_settings(&dir, &PersistedSettings::default()).unwrap();
        assert!(settings_exist(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_overwrites_atomically_with_no_tmp_left() {
        let dir = temp_session_dir("overwrite");
        save_settings(&dir, &PersistedSettings { tab_cap: 3, theme_id: None }).unwrap();
        save_settings(&dir, &PersistedSettings { tab_cap: 24, theme_id: None }).unwrap();
        let restored = load_settings(&dir).unwrap().unwrap();
        assert_eq!(restored.tab_cap, 24);
        let tmp = settings_path(&dir).with_extension("json.tmp");
        assert!(!tmp.exists(), "no leftover tmp from the atomic write");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_field_takes_its_default() {
        // An empty document parses to the default cap (forward-compat: a field
        // added later still reads an old file).
        let dir = temp_session_dir("default-field");
        fs::write(settings_path(&dir), "{}").unwrap();
        let restored = load_settings(&dir).unwrap().unwrap();
        assert_eq!(restored, PersistedSettings::default());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_json_returns_invalid_data_error() {
        let dir = temp_session_dir("malformed");
        fs::write(settings_path(&dir), "{ not json").unwrap();
        match load_settings(&dir) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("expected malformed JSON to fail parsing"),
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn settings_path_composition() {
        let dir = PathBuf::from("/tmp/mere-test");
        assert_eq!(settings_path(&dir), dir.join("settings.json"));
    }
}
