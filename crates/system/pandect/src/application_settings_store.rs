// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Application-scoped settings.
//!
//! These values describe the installed application rather than one graph
//! session. They therefore live under the data root, not beside a session's
//! `graph.json`. Hosts choose the data root; this crate owns the typed shape
//! and the small atomic file operation.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

fn default_tab_cap() -> usize {
    12
}

fn default_ui_zoom() -> f32 {
    1.1
}

fn default_snapshot_idle_refresh() -> bool {
    true
}

/// Which window edge the application shellbar is docked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShellbarEdge {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

/// Typed application preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplicationSettings {
    /// scope=application; movement=local-only; mutability=live; security=ordinary.
    #[serde(default = "default_tab_cap")]
    pub tab_cap: usize,
    /// scope=application; movement=persona-synced opt-in; mutability=live;
    /// security=ordinary.
    pub theme_id: Option<String>,
    /// scope=application; movement=persona-synced opt-in; mutability=live;
    /// security=ordinary.
    pub theme_mode: Option<String>,
    /// scope=application; movement=local-only; mutability=live; security=ordinary.
    pub shellbar_edge: ShellbarEdge,
    /// scope=application; movement=local-only; mutability=live; security=ordinary.
    pub shellbar_hidden: bool,
    /// scope=application; movement=local-only; mutability=restart-required;
    /// security=ordinary.
    pub disabled_engines: Vec<String>,
    /// scope=application; movement=persona-synced opt-in; mutability=live;
    /// security=ordinary.
    pub document_typography: Option<serde_json::Value>,
    /// scope=application; movement=local-only; mutability=live; security=ordinary.
    pub ui_zoom: f32,
    /// scope=application; movement=local-only; mutability=live; security=ordinary.
    #[serde(default = "default_snapshot_idle_refresh")]
    pub snapshot_idle_refresh: bool,
    /// scope=application; movement=local-only; mutability=live; security=ordinary.
    pub snapshot_byte_cap_mb: Option<u32>,
    /// How many rounds one graph-behavior cascade may run before it is stopped
    /// and the behaviors still waking each other are named.
    ///
    /// scope=application; movement=local-only; mutability=live; security=ordinary.
    ///
    /// There is deliberately no "unlimited": an unbounded cascade is the
    /// condition the budget exists to report, so a way to switch the reporting
    /// off would be a way to hang the application quietly. The consumer floors
    /// this at 1 (`servitor::CascadeBudget::new`), so a drifted zero leaves
    /// behaviors working rather than silently disabling them.
    #[serde(default = "default_cascade_budget")]
    pub cascade_budget: u32,
}

fn default_cascade_budget() -> u32 {
    4
}

impl Default for ApplicationSettings {
    fn default() -> Self {
        Self {
            tab_cap: default_tab_cap(),
            theme_id: None,
            theme_mode: None,
            shellbar_edge: ShellbarEdge::default(),
            shellbar_hidden: false,
            disabled_engines: Vec::new(),
            document_typography: None,
            ui_zoom: default_ui_zoom(),
            snapshot_idle_refresh: default_snapshot_idle_refresh(),
            snapshot_byte_cap_mb: None,
            cascade_budget: default_cascade_budget(),
        }
    }
}

pub const APPLICATION_SETTINGS_DIR: &str = "application";
pub const APPLICATION_SETTINGS_FILENAME: &str = "settings.json";

/// Build `<data_root>/application/settings.json`.
pub fn application_settings_path(data_root: &Path) -> PathBuf {
    data_root
        .join(APPLICATION_SETTINGS_DIR)
        .join(APPLICATION_SETTINGS_FILENAME)
}

/// Write application settings atomically.
pub fn save_application_settings(
    data_root: &Path,
    settings: &ApplicationSettings,
) -> io::Result<()> {
    let target = application_settings_path(data_root);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let temporary = target.with_extension("json.tmp");
    fs::write(&temporary, json)?;
    fs::rename(&temporary, &target)?;
    Ok(())
}

/// Load application settings, returning `None` when the application has not
/// written a settings file yet.
pub fn load_application_settings(data_root: &Path) -> io::Result<Option<ApplicationSettings>> {
    let path = application_settings_path(data_root);
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn application_settings_exist(data_root: &Path) -> bool {
    application_settings_path(data_root).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_application_scoped() {
        assert_eq!(
            application_settings_path(Path::new("/data")),
            Path::new("/data/application/settings.json")
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let root = std::env::temp_dir().join(format!(
            "mere-application-settings-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let original = ApplicationSettings {
            tab_cap: 24,
            theme_id: Some("theme:dark".into()),
            theme_mode: Some("dark".into()),
            shellbar_edge: ShellbarEdge::Right,
            shellbar_hidden: true,
            disabled_engines: vec!["scrying.web".into()],
            document_typography: Some(serde_json::json!({"scale": 1.15})),
            ui_zoom: 1.2,
            snapshot_idle_refresh: false,
            snapshot_byte_cap_mb: Some(32),
            cascade_budget: 2,
        };
        save_application_settings(&root, &original).unwrap();
        assert_eq!(load_application_settings(&root).unwrap(), Some(original));
        assert!(application_settings_exist(&root));
        let _ = fs::remove_dir_all(root);
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    }
}
