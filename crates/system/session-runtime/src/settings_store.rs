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

use kernel::permissions::Permission;
use serde::{Deserialize, Serialize};

/// Filename for the session-wide settings sidecar (sibling to `graph.json`).
pub const SETTINGS_FILENAME: &str = "settings.json";

/// The default active-tab cap — the most warm content actors the pool keeps
/// before LRU eviction. Mirrors the chrome's `Settings::default`.
fn default_tab_cap() -> usize {
    12
}

/// Which window edge the shellbar strip is docked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShellbarEdge {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

/// Per-capability **Session-scope** permission opinions for DocumentScripts
/// (§11.4, the document-script substrate). `None` on a capability = no opinion at
/// this scope, so the App default (`Allow`) stands; `Some(Deny)` makes a
/// `>attach-script` fail at instantiation for that capability — a session-wide "no
/// script may touch the page" switch. The host resolves these through
/// [`kernel::permissions::resolve_permission`] (the narrowing rule) at attach time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptPermissionPrefs {
    #[serde(default)]
    pub log: Option<Permission>,
    #[serde(default)]
    pub document: Option<Permission>,
    /// Network egress (`net.fetch`). Denied by default; set `Allow` to let scripts
    /// fetch this session. `None` = no opinion (the default-deny baseline stands).
    #[serde(default)]
    pub net: Option<Permission>,
}

/// Persistable user settings. v0 carried the active-tab cap; `theme_id` joined
/// with the runtime theme switcher. Future preferences join as their controls
/// land, each with a serde default so old files keep parsing.
// Not `Eq`: `physics_damping` is an `f32`. `PartialEq` is all the `==` checks need.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedSettings {
    /// The most warm tabs the actor pool keeps before LRU eviction.
    #[serde(default = "default_tab_cap")]
    pub tab_cap: usize,
    /// The active theme id (e.g. `theme:dark`); `None` falls back to the
    /// registry's default theme.
    #[serde(default)]
    pub theme_id: Option<String>,
    /// Which window edge the shellbar is docked to. Defaults to Left.
    #[serde(default)]
    pub shellbar_edge: ShellbarEdge,
    /// Whether the shellbar is hidden on this (primary) window. Defaults to shown.
    /// Distinct from a leaf window's slim chrome (which always omits the shellbar);
    /// this is the user's explicit hide toggle, restored across restart and revealed
    /// again from the command palette / `>shellbar`. (Hide-shellbar.)
    #[serde(default)]
    pub shellbar_hidden: bool,
    /// Linear damping for orrery node bodies — the "inertia" physics setting: lower
    /// keeps more drift after a settle, higher rests sooner. Defaults to the layout
    /// engine's tuned value.
    #[serde(default = "default_physics_damping")]
    pub physics_damping: f32,
    /// Globally deactivated engine ids (e.g. `scrying.web`). A present-but-disabled
    /// engine is never routed to and spawns no actors; the picker shows it as off.
    /// Empty = every engine the build carries is active. (engine-picker Phase 1.)
    #[serde(default)]
    pub disabled_engines: Vec<String>,
    /// The user's document typography (a serialized `DocumentStyleSheet`), stored
    /// as embedded JSON so this low-level crate need not depend on the document
    /// engine — the host owns the (de)serialization. `None` = the built-in look.
    /// (Document typography surface.)
    #[serde(default)]
    pub document_typography: Option<serde_json::Value>,
    /// Session-scope DocumentScript capability permissions (§11.4). Default = no
    /// opinion (the App-scope `Allow` default stands); set `document: Deny` to forbid
    /// any attached script from mutating the page this session.
    #[serde(default)]
    pub script_permissions: ScriptPermissionPrefs,
    /// The crawl scope a `>crawl` roams under, as a stable key (`same_host` /
    /// `same_domain` / `any_host`). `None` = the same-host default. (Crawl controls.)
    #[serde(default)]
    pub crawl_scope: Option<String>,
    /// The crawl depth (link-hops from the seed) a `>crawl` reaches. `None` = the
    /// default 2-hop depth. (Crawl controls.)
    #[serde(default)]
    pub crawl_depth: Option<u32>,
    /// "Crawl whole site" mode: seed from the site's `sitemap.xml` rather than only the
    /// seed page's links. `None` = off (focused neighborhood). (Crawl controls.)
    #[serde(default)]
    pub crawl_sitemap: Option<bool>,
    /// The hard page cap a `>crawl` stops at (the runaway backstop). `None` = the default
    /// 50-page cap. (Crawl controls.)
    #[serde(default)]
    pub crawl_max_pages: Option<usize>,
}

/// The layout engine's tuned default linear damping (mirrors gyre's
/// `DEFAULT_LINEAR_DAMPING`); the seed when no setting is persisted.
fn default_physics_damping() -> f32 {
    2.5
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            tab_cap: default_tab_cap(),
            theme_id: None,
            shellbar_edge: ShellbarEdge::default(),
            shellbar_hidden: false,
            physics_damping: default_physics_damping(),
            disabled_engines: Vec::new(),
            document_typography: None,
            script_permissions: ScriptPermissionPrefs::default(),
            crawl_scope: None,
            crawl_depth: None,
            crawl_sitemap: None,
            crawl_max_pages: None,
        }
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
        let original = PersistedSettings { tab_cap: 7, theme_id: None, shellbar_edge: ShellbarEdge::Left, shellbar_hidden: false, physics_damping: 2.5, disabled_engines: vec!["scrying.web".into()], document_typography: None, script_permissions: ScriptPermissionPrefs { log: None, document: Some(Permission::Deny), net: Some(Permission::Allow) }, crawl_scope: None, crawl_depth: None, crawl_sitemap: None, crawl_max_pages: None };
        save_settings(&dir, &original).unwrap();
        let restored = load_settings(&dir).unwrap().expect("settings file should be present");
        assert_eq!(restored, original);
        // The script-permission opinion round-trips (the §11.4 session-scope switch).
        assert_eq!(restored.script_permissions.document, Some(Permission::Deny));
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
        save_settings(&dir, &PersistedSettings { tab_cap: 3, theme_id: None, shellbar_edge: ShellbarEdge::Left, shellbar_hidden: false, physics_damping: 2.5, disabled_engines: Vec::new(), document_typography: None, script_permissions: ScriptPermissionPrefs::default(), crawl_scope: None, crawl_depth: None, crawl_sitemap: None, crawl_max_pages: None }).unwrap();
        save_settings(&dir, &PersistedSettings { tab_cap: 24, theme_id: None, shellbar_edge: ShellbarEdge::Right, shellbar_hidden: false, physics_damping: 2.5, disabled_engines: Vec::new(), document_typography: None, script_permissions: ScriptPermissionPrefs::default(), crawl_scope: None, crawl_depth: None, crawl_sitemap: None, crawl_max_pages: None }).unwrap();
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
