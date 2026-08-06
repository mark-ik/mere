// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Session-wide settings sidecar — `<session_dir>/settings.json`.
//!
//! Session/dataspace policy that is neither graph truth nor per-pane view intent.
//! Application preferences and device policy have separate owners and stores;
//! they must not travel with this sidecar.
//!
//! Each field is `#[serde(default)]`-friendly, so adding a preference later reads
//! older files without a migration — an unknown-to-old / missing-in-new field
//! takes its default rather than failing the parse.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use kernel::permissions::Permission;
use serde::{Deserialize, Serialize};

use crate::application_settings_store::{ApplicationSettings, ShellbarEdge};
use crate::device_settings_store::DeviceSettings;

/// Filename for the session-wide settings sidecar (sibling to `graph.json`).
pub const SETTINGS_FILENAME: &str = "settings.json";

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

/// Persistable session/dataspace policy.
// Not `Eq`: this record contains no floats, but `PartialEq` matches the older
// settings API and keeps callers from acquiring a stronger contract here.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedSettings {
    /// scope=session/dataspace; movement=local-only; mutability=live;
    /// security=ordinary/private policy. Session-scope DocumentScript capability
    /// permissions (§11.4). Default = no
    /// opinion (the App-scope `Allow` default stands); set `document: Deny` to forbid
    /// any attached script from mutating the page this session.
    pub script_permissions: ScriptPermissionPrefs,
    /// scope=session/dataspace; movement=local-only; mutability=live;
    /// security=ordinary. The crawl scope a `>crawl` roams under, as a stable key (`same_host` /
    /// `same_domain` / `any_host`). `None` = the same-host default. (Crawl controls.)
    pub crawl_scope: Option<String>,
    /// scope=session/dataspace; movement=local-only; mutability=live;
    /// security=ordinary. The crawl depth (link-hops from the seed) a `>crawl` reaches. `None` = the
    /// default 2-hop depth. (Crawl controls.)
    pub crawl_depth: Option<u32>,
    /// scope=session/dataspace; movement=local-only; mutability=live;
    /// security=ordinary. "Crawl whole site" mode: seed from the site's `sitemap.xml` rather than only the
    /// seed page's links. `None` = off (focused neighborhood). (Crawl controls.)
    pub crawl_sitemap: Option<bool>,
    /// scope=session/dataspace; movement=local-only; mutability=live;
    /// security=ordinary. The hard page cap a `>crawl` stops at (the runaway backstop). `None` = the default
    /// 50-page cap. (Crawl controls.)
    pub crawl_max_pages: Option<usize>,
    /// scope=session/dataspace; movement=local-only; mutability=live;
    /// security=private policy. The browse-capture consent level (capture/provenance/consent plan C4), as a
    /// stable key (`off` / `corridor` / `full`). `None` = the `full` default (the
    /// pre-consent behaviour): governs whether the live recorder writes traces and
    /// at what granularity.
    pub capture_consent: Option<String>,
    /// scope=session/dataspace; movement=local-only; mutability=live;
    /// security=ordinary. The browse-trace retention cap (capture/provenance/consent plan C4): the
    /// number of most-recent traces kept; older ones age out on a per-launch quota
    /// pass. `None` = the host default (generous; traces are tiny).
    pub retention_keep_n: Option<usize>,
}

/// Values found in a legacy session `settings.json` that now belong to an
/// application or device store. `None` means the old file did not mention that
/// owner at all; a host can then preserve the new store's existing value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegacySettingsMigration {
    pub application: Option<ApplicationSettings>,
    pub device: Option<DeviceSettings>,
}

/// The combined result of reading a session sidecar during the transition.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsLoad {
    pub settings: PersistedSettings,
    pub legacy: LegacySettingsMigration,
}

#[derive(Default, Deserialize)]
struct LegacySettingsFields {
    tab_cap: Option<usize>,
    theme_id: Option<String>,
    theme_mode: Option<String>,
    shellbar_edge: Option<ShellbarEdge>,
    shellbar_hidden: Option<bool>,
    disabled_engines: Option<Vec<String>>,
    document_typography: Option<serde_json::Value>,
    ui_zoom: Option<f32>,
    startup_unlock_mode: Option<identity::StartupUnlockMode>,
    snapshot_idle_refresh: Option<bool>,
    snapshot_byte_cap_mb: Option<u32>,
}

impl LegacySettingsFields {
    fn migration(self) -> LegacySettingsMigration {
        let application_present = self.tab_cap.is_some()
            || self.theme_id.is_some()
            || self.theme_mode.is_some()
            || self.shellbar_edge.is_some()
            || self.shellbar_hidden.is_some()
            || self.disabled_engines.is_some()
            || self.document_typography.is_some()
            || self.ui_zoom.is_some()
            || self.snapshot_idle_refresh.is_some()
            || self.snapshot_byte_cap_mb.is_some();
        let application = application_present.then(|| {
            let defaults = ApplicationSettings::default();
            ApplicationSettings {
                tab_cap: self.tab_cap.unwrap_or(defaults.tab_cap),
                theme_id: self.theme_id,
                theme_mode: self.theme_mode,
                shellbar_edge: self.shellbar_edge.unwrap_or(defaults.shellbar_edge),
                shellbar_hidden: self.shellbar_hidden.unwrap_or(defaults.shellbar_hidden),
                disabled_engines: self.disabled_engines.unwrap_or(defaults.disabled_engines),
                document_typography: self.document_typography,
                ui_zoom: self.ui_zoom.unwrap_or(defaults.ui_zoom),
                snapshot_idle_refresh: self
                    .snapshot_idle_refresh
                    .unwrap_or(defaults.snapshot_idle_refresh),
                snapshot_byte_cap_mb: self.snapshot_byte_cap_mb,
            }
        });
        let device = self
            .startup_unlock_mode
            .map(|startup_unlock_mode| DeviceSettings {
                startup_unlock_mode,
            });
        LegacySettingsMigration {
            application,
            device,
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

/// Read a session sidecar and expose both its current session policy and any
/// fields that were written before the C1 ownership split.
pub fn load_settings_with_legacy(session_dir: &Path) -> io::Result<Option<SettingsLoad>> {
    let path = settings_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let settings: PersistedSettings = serde_json::from_str(&text)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let legacy: LegacySettingsFields = serde_json::from_str(&text)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(Some(SettingsLoad {
        settings,
        legacy: legacy.migration(),
    }))
}

/// Read `<session_dir>/settings.json` and parse its session policy.
/// Returns `Ok(None)` when the file doesn't exist. Legacy application/device
/// fields are intentionally ignored here; callers that own the data root can
/// use [`load_settings_with_legacy`] to migrate them explicitly.
pub fn load_settings(session_dir: &Path) -> io::Result<Option<PersistedSettings>> {
    Ok(load_settings_with_legacy(session_dir)?.map(|loaded| loaded.settings))
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
        let original = PersistedSettings {
            script_permissions: ScriptPermissionPrefs {
                log: None,
                document: Some(Permission::Deny),
                net: Some(Permission::Allow),
            },
            crawl_scope: None,
            crawl_depth: None,
            crawl_sitemap: None,
            crawl_max_pages: None,
            capture_consent: None,
            retention_keep_n: None,
        };
        save_settings(&dir, &original).unwrap();
        let restored = load_settings(&dir)
            .unwrap()
            .expect("settings file should be present");
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
        save_settings(
            &dir,
            &PersistedSettings {
                script_permissions: ScriptPermissionPrefs::default(),
                crawl_scope: None,
                crawl_depth: None,
                crawl_sitemap: None,
                crawl_max_pages: None,
                capture_consent: None,
                retention_keep_n: None,
            },
        )
        .unwrap();
        save_settings(
            &dir,
            &PersistedSettings {
                script_permissions: ScriptPermissionPrefs::default(),
                crawl_scope: None,
                crawl_depth: None,
                crawl_sitemap: None,
                crawl_max_pages: None,
                capture_consent: None,
                retention_keep_n: None,
            },
        )
        .unwrap();
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
    fn legacy_application_and_device_fields_are_exposed_for_migration() {
        let dir = temp_session_dir("legacy-migration");
        fs::write(
            settings_path(&dir),
            r#"{
                "tab_cap": 7,
                "theme_id": "theme:dark",
                "startup_unlock_mode": "locked",
                "crawl_depth": 4
            }"#,
        )
        .unwrap();

        let loaded = load_settings_with_legacy(&dir).unwrap().unwrap();
        assert_eq!(loaded.settings.crawl_depth, Some(4));
        assert_eq!(loaded.legacy.application.unwrap().tab_cap, 7);
        assert_eq!(
            loaded.legacy.device.unwrap().startup_unlock_mode,
            identity::StartupUnlockMode::Locked
        );
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
