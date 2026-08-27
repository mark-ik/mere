// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Per-persona UI settings — persona-scoped configuration the shell's UI reads and
//! writes, distinct from the session/dataspace [`settings_store`](crate::settings_store)
//! sidecar and the application/device stores.
//!
//! Path: `<data_root>/personas/<persona_id>/settings/ui.json`, under the same
//! `personas/<id>/` root the engine-profile boundary uses (so a persona's engine UDFs,
//! vault, and UI config sit together). v0 has a single
//! [`default_persona`](crate::manifest::PersonaId); v1 swaps in the active persona id and
//! the rest of this module is unchanged.
//!
//! First field: the configurable context menu's command list (command registry P4). Fields
//! are `Option` / defaulted so a missing or partial file degrades to "host default" rather
//! than empty UI — the host supplies the default menu when `menu_actions` is `None`.

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::engine_profile_store::PERSONAS_DIR;
use crate::manifest::PersonaId;
use crate::memory_levels::EvictionPolicy;

/// Subdirectory under a persona holding persona settings.
pub const PERSONA_SETTINGS_DIR: &str = "settings";

/// The persona UI settings file under [`PERSONA_SETTINGS_DIR`].
pub const PERSONA_UI_FILENAME: &str = "ui.json";

/// Per-persona UI configuration. Each field is optional / defaulted so an absent or older
/// file still parses and falls back to the host's defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaSettings {
    /// scope=persona; movement=persona-synced opt-in; mutability=live;
    /// security=ordinary. The context menu's command ids, in order — the user's curated menu
    /// (command registry P4). `None` = the host's default menu; `Some(list)` = this persona's
    /// curation. Each id is a registry id (a `Command` verb or a context-action id).
    #[serde(default)]
    pub menu_actions: Option<Vec<String>>,
    /// scope=persona; movement=persona-synced opt-in; mutability=live;
    /// security=ordinary. How many times each registry command has been invoked — the frequency
    /// signal behind the context menu's auto-suggestions (command registry S3). Keyed by registry
    /// id (a `Command` verb or a context-action id), a `BTreeMap` for deterministic on-disk order.
    /// Empty until the first command runs.
    #[serde(default)]
    pub command_usage: std::collections::BTreeMap<String, u32>,
    /// scope=persona; movement=persona-synced opt-in; mutability=live;
    /// security=ordinary. The short-term memory eviction policy (the Alembic Recent header's
    /// editable control, B4). Absent / older file falls back to the host default (`KeepDays(30)`).
    #[serde(default)]
    pub eviction_policy: EvictionPolicy,
    /// scope=persona; movement=local-only; mutability=live; security=ordinary. Runtime
    /// bookkeeping riding in the persona settings file: incremented once at boot and saved back
    /// immediately. A crash between increment and save just under-counts by one launch, never
    /// duplicates a count. The host stamps each navigated node's
    /// [`PersistedNode::last_session_visited`](kernel::persistence::PersistedNode::last_session_visited)
    /// with this value, so `0` is reserved for "never stamped" and the first real launch is `1`.
    /// (Alembic B5 — by-sessions eviction.)
    #[serde(default)]
    pub session_count: u64,
}

/// The `<data_root>/personas/<id>/settings/` directory for `persona`. Pure.
pub fn persona_settings_dir(data_root: &Path, persona: PersonaId) -> PathBuf {
    data_root
        .join(PERSONAS_DIR)
        .join(persona.as_uuid().to_string())
        .join(PERSONA_SETTINGS_DIR)
}

/// The `ui.json` path for `persona`. Pure — callers can use it for an existence check.
pub fn persona_settings_path(data_root: &Path, persona: PersonaId) -> PathBuf {
    persona_settings_dir(data_root, persona).join(PERSONA_UI_FILENAME)
}

/// Load this persona's UI settings, or `None` when the file does not exist yet (a fresh
/// persona). A malformed file is an error, not a silent reset — the caller decides whether to
/// fall back to defaults.
pub fn load_persona_settings(
    data_root: &Path,
    persona: PersonaId,
) -> io::Result<Option<PersonaSettings>> {
    let path = persona_settings_path(data_root, persona);
    match fs::read_to_string(&path) {
        Ok(text) => {
            let settings = serde_json::from_str(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Ok(Some(settings))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Serialise `settings` to pretty JSON and write it atomically (tmp + rename) to the
/// persona's `ui.json`, creating the persona settings directory if needed.
pub fn save_persona_settings(
    data_root: &Path,
    persona: PersonaId,
    settings: &PersonaSettings,
) -> io::Result<()> {
    let dir = persona_settings_dir(data_root, persona);
    fs::create_dir_all(&dir)?;
    let target = dir.join(PERSONA_UI_FILENAME);
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use uuid::Uuid;

    fn temp_data_root(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "mere-persona-settings-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn fixture_persona() -> PersonaId {
        PersonaId(Uuid::from_u128(0x9999))
    }

    #[test]
    fn path_composes_under_personas_settings_root() {
        let path = persona_settings_path(Path::new("/data"), fixture_persona());
        let expected = Path::new("/data")
            .join("personas")
            .join(fixture_persona().as_uuid().to_string())
            .join("settings")
            .join("ui.json");
        assert_eq!(path, expected);
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let root = temp_data_root("none");
        assert!(
            load_persona_settings(&root, fixture_persona())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn save_then_load_round_trips_menu_actions() {
        let root = temp_data_root("round-trip");
        let original = PersonaSettings {
            menu_actions: Some(vec![
                "add_node".into(),
                "open_splits".into(),
                "settings".into(),
            ]),
            command_usage: [("settings".to_string(), 4u32), ("back".to_string(), 9u32)]
                .into_iter()
                .collect(),
            eviction_policy: EvictionPolicy::KeepDays(90),
            session_count: 7,
        };
        save_persona_settings(&root, fixture_persona(), &original).unwrap();
        let restored = load_persona_settings(&root, fixture_persona())
            .unwrap()
            .expect("settings file should be present");
        assert_eq!(restored, original);
        assert_eq!(
            restored.eviction_policy,
            EvictionPolicy::KeepDays(90),
            "the policy persists"
        );
        assert_eq!(restored.session_count, 7, "the launch counter persists");
        let _ = fs::remove_dir_all(&root);
    }
}
