// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Device-local settings.
//!
//! Device policy must not follow a graph session or silently become persona
//! truth. The data root is the install-local persistence boundary supplied by
//! the host.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use identity::StartupUnlockMode;
use serde::{Deserialize, Serialize};

pub const DEVICE_SETTINGS_DIR: &str = "device";
pub const DEVICE_SETTINGS_FILENAME: &str = "settings.json";

/// Device-local policy for startup wallet handling.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceSettings {
    /// scope=device; movement=local-only; mutability=startup-only;
    /// security=secret-reference (the secret stays in Personae or the vault).
    pub startup_unlock_mode: StartupUnlockMode,

    /// Compute-lending posture for mesh work on this device. Absent means this
    /// device has never stated one, and a composition that needs it must refuse
    /// rather than assume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_lending: Option<MeshLendingSettings>,
}

/// A local-wall-clock window during which this device does not lend to mesh
/// work. `start_hour == end_hour` means the whole day; a window may wrap
/// midnight (interpretation is the consumer's job — this is plain data).
/// Each bound is `0..24`.
///
/// scope=device; movement=local-only; mutability=owner-set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuietHoursSettings {
    pub start_hour: u8,
    pub end_hour: u8,
}

/// The owner's fallback claims for conditions this device cannot sense on its
/// own (no battery driver, no thermal sensor the OS exposes, and so on).
///
/// Every field is optional, and the meaning of absence is load-bearing:
/// `None` means "not stated," not "favorable." A composition that needs a
/// signal this device can neither sense nor recall a stated value for must
/// treat the signal as absent and disable whichever lending rule depends on
/// it — never assume a value on the owner's behalf. A stated claim is not a
/// live reading either; it is the owner's word, held until they change it.
///
/// scope=device; movement=local-only; mutability=owner-set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatedConditionSettings {
    pub idle_ms: Option<u64>,
    pub battery_pct: Option<u8>,
    pub on_mains: Option<bool>,
    pub thermal_c: Option<u16>,
    /// Same closed set as [`MeshLendingSettings::min_network`].
    pub network: Option<String>,
    pub bandwidth_in_use_kbps: Option<u32>,
}

/// Compute-lending posture for mesh work on this device: what the owner will
/// lend, and their stated fallbacks for what the host cannot sense.
///
/// This is plain, serializable data shaped after (but not coupled to) the
/// mesh crate's `DevicePolicy` and `DeviceConditions` — pandect must not
/// depend on `mesh`, so the consumer (Djinn) converts these fields into mesh
/// types at composition time.
///
/// No [`Default`] impl on purpose: every field here is an owner statement,
/// not a plausible starting point. A default would let a caller manufacture
/// a lending posture nobody actually agreed to; the doctrine this module
/// states is that device policy must not silently become anything but what
/// the device's owner said.
///
/// scope=device; movement=local-only; mutability=owner-set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshLendingSettings {
    /// Work only after this much continuous idleness. `0` lends immediately.
    pub min_idle_ms: u64,
    /// Refuse below this battery percentage unless on mains. `0` disables the
    /// check.
    pub min_battery_pct: u8,
    /// Refuse at or above this package temperature. `0` disables the check.
    pub max_thermal_c: u16,
    /// Slowest link this device will work over: one of `"offline"`,
    /// `"metered"`, `"wifi"`, `"wired"`. `"offline"` disables the check (any
    /// link, including none, is accepted).
    pub min_network: String,
    /// Refuse when this much of the link is already spoken for, in kbps. `0`
    /// disables the check.
    pub max_bandwidth_in_use_kbps: u32,
    pub quiet_hours: Option<QuietHoursSettings>,
    /// Must be at least 1.
    pub max_concurrent_jobs: u32,
    /// Mesh resource ids this device will run work for. A written `[]` is
    /// the owner stating "no restriction" — mesh reads an empty allowlist as
    /// "every resource this device has registered", and that is exactly what
    /// an owner who wrote `[]` meant. This field carries no
    /// `#[serde(default)]`: a file that omits it entirely has not made that
    /// statement, and the existing required-field behaviour refuses it
    /// rather than guessing which the owner meant. Pandect stays mesh-free,
    /// so `validate` checks only that each entry could plausibly be an
    /// identifier (non-empty, no whitespace, no duplicates) — the consumer
    /// parses each one with the real grammar.
    pub allowed_resources: Vec<String>,
    /// Which interruption promises this device is willing to take on, from
    /// the closed set `"restart"`, `"resumable"`, `"non-interruptible"`. Same
    /// absence discipline as `allowed_resources`: a written `[]` is the
    /// owner's own "every class", and omitting the field is refused rather
    /// than defaulted.
    pub accepted_checkpoints: Vec<String>,
    /// How long a non-interruptible job may keep running after the owner
    /// asks for the device back.
    pub reclaim_grace_ms: u64,
    /// Whether this host actually supervises running work (heartbeats
    /// progress, can stop a run on reclaim). `false` until a supervisor
    /// exists on this device.
    pub supervises_leases: bool,
    /// The owner's fallback claims for signals the platform cannot sense.
    pub stated: StatedConditionSettings,
}

fn validate_network_class(value: &str, field: &str) -> Result<(), String> {
    match value {
        "offline" | "metered" | "wifi" | "wired" => Ok(()),
        other => Err(format!(
            "{field} must be one of \"offline\", \"metered\", \"wifi\", \"wired\" (got {other:?})"
        )),
    }
}

fn validate_checkpoint_class(value: &str, field: &str) -> Result<(), String> {
    match value {
        "restart" | "resumable" | "non-interruptible" => Ok(()),
        other => Err(format!(
            "{field} must be one of \"restart\", \"resumable\", \"non-interruptible\" (got {other:?})"
        )),
    }
}

impl MeshLendingSettings {
    /// Rejects a block that could not be an honest owner statement: a
    /// percentage over 100, an hour outside `0..24`, a network string
    /// outside the closed set, a concurrency cap of `0`, a stated
    /// temperature of `0` (temperature is never stated as "disabled" the way
    /// the live check is — `stated.thermal_c` absent already means that), a
    /// resource id that is empty, contains whitespace, or repeats, or a
    /// checkpoint-class string outside its closed set or repeated.
    pub fn validate(&self) -> Result<(), String> {
        if self.min_battery_pct > 100 {
            return Err(format!(
                "min_battery_pct must be 0..=100 (got {})",
                self.min_battery_pct
            ));
        }
        validate_network_class(&self.min_network, "min_network")?;
        if let Some(hours) = &self.quiet_hours {
            if hours.start_hour >= 24 {
                return Err(format!(
                    "quiet_hours.start_hour must be 0..24 (got {})",
                    hours.start_hour
                ));
            }
            if hours.end_hour >= 24 {
                return Err(format!(
                    "quiet_hours.end_hour must be 0..24 (got {})",
                    hours.end_hour
                ));
            }
        }
        if self.max_concurrent_jobs == 0 {
            return Err("max_concurrent_jobs must be at least 1".to_string());
        }
        for resource in &self.allowed_resources {
            if resource.is_empty() {
                return Err("allowed_resources entries must not be empty".to_string());
            }
            if resource.chars().any(char::is_whitespace) {
                return Err(format!(
                    "allowed_resources entry {resource:?} must not contain whitespace"
                ));
            }
        }
        {
            let mut seen = HashSet::with_capacity(self.allowed_resources.len());
            for resource in &self.allowed_resources {
                if !seen.insert(resource) {
                    return Err(format!(
                        "allowed_resources contains a duplicate: {resource:?}"
                    ));
                }
            }
        }
        for checkpoint in &self.accepted_checkpoints {
            validate_checkpoint_class(checkpoint, "accepted_checkpoints")?;
        }
        {
            let mut seen = HashSet::with_capacity(self.accepted_checkpoints.len());
            for checkpoint in &self.accepted_checkpoints {
                if !seen.insert(checkpoint) {
                    return Err(format!(
                        "accepted_checkpoints contains a duplicate: {checkpoint:?}"
                    ));
                }
            }
        }
        if let Some(battery_pct) = self.stated.battery_pct
            && battery_pct > 100
        {
            return Err(format!(
                "stated.battery_pct must be 0..=100 (got {battery_pct})"
            ));
        }
        if let Some(network) = &self.stated.network {
            validate_network_class(network, "stated.network")?;
        }
        if let Some(0) = self.stated.thermal_c {
            return Err("stated.thermal_c must be > 0 when stated".to_string());
        }
        Ok(())
    }
}

pub fn device_settings_path(data_root: &Path) -> PathBuf {
    data_root
        .join(DEVICE_SETTINGS_DIR)
        .join(DEVICE_SETTINGS_FILENAME)
}

pub fn save_device_settings(data_root: &Path, settings: &DeviceSettings) -> io::Result<()> {
    if let Some(mesh_lending) = &settings.mesh_lending {
        mesh_lending
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    }
    let target = device_settings_path(data_root);
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

pub fn load_device_settings(data_root: &Path) -> io::Result<Option<DeviceSettings>> {
    let path = device_settings_path(data_root);
    let settings: DeviceSettings = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if let Some(mesh_lending) = &settings.mesh_lending {
        mesh_lending
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    }
    Ok(Some(settings))
}

pub fn device_settings_exist(data_root: &Path) -> bool {
    device_settings_path(data_root).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_device_scoped() {
        assert_eq!(
            device_settings_path(Path::new("/data")),
            Path::new("/data/device/settings.json")
        );
    }

    #[test]
    fn save_then_load_round_trips_unlock_policy() {
        let root = temp_root("unlock-policy");
        let original = DeviceSettings {
            startup_unlock_mode: StartupUnlockMode::Locked,
            mesh_lending: None,
        };
        save_device_settings(&root, &original).unwrap();
        assert_eq!(load_device_settings(&root).unwrap(), Some(original));
        assert!(device_settings_exist(&root));
        let _ = fs::remove_dir_all(root);
    }

    fn fixture_mesh_lending() -> MeshLendingSettings {
        MeshLendingSettings {
            min_idle_ms: 120_000,
            min_battery_pct: 40,
            max_thermal_c: 85,
            min_network: "wifi".to_string(),
            max_bandwidth_in_use_kbps: 0,
            quiet_hours: Some(QuietHoursSettings {
                start_hour: 22,
                end_hour: 8,
            }),
            max_concurrent_jobs: 2,
            allowed_resources: vec!["mesh.blake3/v1".to_string(), "mesh.echo/v1".to_string()],
            accepted_checkpoints: vec!["restart".to_string(), "resumable".to_string()],
            reclaim_grace_ms: 5_000,
            supervises_leases: true,
            stated: StatedConditionSettings {
                idle_ms: Some(30_000),
                battery_pct: Some(75),
                on_mains: Some(true),
                thermal_c: Some(50),
                network: Some("wired".to_string()),
                bandwidth_in_use_kbps: Some(0),
            },
        }
    }

    #[test]
    fn save_then_load_round_trips_mesh_lending_with_stated_fallbacks() {
        let root = temp_root("mesh-lending");
        let original = DeviceSettings {
            startup_unlock_mode: StartupUnlockMode::Prompt,
            mesh_lending: Some(fixture_mesh_lending()),
        };
        save_device_settings(&root, &original).unwrap();
        assert_eq!(load_device_settings(&root).unwrap(), Some(original));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_old_format_file_without_the_mesh_block_still_loads() {
        let root = temp_root("legacy-format");
        let path = device_settings_path(&root);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"startup_unlock_mode":"locked"}"#).unwrap();

        let loaded = load_device_settings(&root).unwrap().unwrap();
        assert_eq!(loaded.startup_unlock_mode, StartupUnlockMode::Locked);
        assert_eq!(loaded.mesh_lending, None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_invalid_network_string_is_refused_on_load() {
        let root = temp_root("invalid-network");
        let mut lending = fixture_mesh_lending();
        lending.min_network = "ethernet".to_string();
        write_raw_mesh_lending(&root, &lending);

        let error = load_device_settings(&root).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_out_of_range_quiet_hour_is_refused_on_load() {
        let root = temp_root("invalid-quiet-hour");
        let mut lending = fixture_mesh_lending();
        lending.quiet_hours = Some(QuietHoursSettings {
            start_hour: 24,
            end_hour: 8,
        });
        write_raw_mesh_lending(&root, &lending);

        let error = load_device_settings(&root).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_unknown_checkpoint_string_is_refused_on_load() {
        let root = temp_root("unknown-checkpoint");
        let mut lending = fixture_mesh_lending();
        lending.accepted_checkpoints = vec!["paused".to_string()];
        write_raw_mesh_lending(&root, &lending);

        let error = load_device_settings(&root).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_duplicate_resource_id_is_refused_on_load() {
        let root = temp_root("duplicate-resource");
        let mut lending = fixture_mesh_lending();
        lending.allowed_resources =
            vec!["mesh.blake3/v1".to_string(), "mesh.blake3/v1".to_string()];
        write_raw_mesh_lending(&root, &lending);

        let error = load_device_settings(&root).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_resource_id_with_whitespace_is_refused_on_load() {
        let root = temp_root("whitespace-resource");
        let mut lending = fixture_mesh_lending();
        lending.allowed_resources = vec!["mesh.blake3 /v1".to_string()];
        write_raw_mesh_lending(&root, &lending);

        let error = load_device_settings(&root).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_invalid_block_is_also_refused_on_save() {
        let root = temp_root("invalid-on-save");
        let mut lending = fixture_mesh_lending();
        lending.max_concurrent_jobs = 0;
        let settings = DeviceSettings {
            startup_unlock_mode: StartupUnlockMode::Locked,
            mesh_lending: Some(lending),
        };
        let error = save_device_settings(&root, &settings).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!device_settings_exist(&root));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn absent_stated_signals_deserialize_as_none() {
        let root = temp_root("absent-stated");
        let mut lending = fixture_mesh_lending();
        lending.stated = StatedConditionSettings::default();
        let original = DeviceSettings {
            startup_unlock_mode: StartupUnlockMode::Locked,
            mesh_lending: Some(lending),
        };
        save_device_settings(&root, &original).unwrap();

        let loaded = load_device_settings(&root).unwrap().unwrap();
        let stated = loaded.mesh_lending.unwrap().stated;
        assert_eq!(stated.idle_ms, None);
        assert_eq!(stated.battery_pct, None);
        assert_eq!(stated.on_mains, None);
        assert_eq!(stated.thermal_c, None);
        assert_eq!(stated.network, None);
        assert_eq!(stated.bandwidth_in_use_kbps, None);
        let _ = fs::remove_dir_all(root);
    }

    /// Writes `settings.json` directly, bypassing `save_device_settings`'s own
    /// validation, so a load-time rejection is what's under test.
    fn write_raw_mesh_lending(data_root: &Path, mesh_lending: &MeshLendingSettings) {
        let settings = DeviceSettings {
            startup_unlock_mode: StartupUnlockMode::default(),
            mesh_lending: Some(mesh_lending.clone()),
        };
        let path = device_settings_path(data_root);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let json = serde_json::to_string_pretty(&settings).unwrap();
        fs::write(&path, json).unwrap();
    }

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mere-device-settings-{label}-{}-{}",
            std::process::id(),
            unique_suffix()
        ))
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    }
}
