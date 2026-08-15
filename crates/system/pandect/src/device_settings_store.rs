// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Device-local settings.
//!
//! Device policy must not follow a graph session or silently become persona
//! truth. The data root is the install-local persistence boundary supplied by
//! the host.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use identity::StartupUnlockMode;
use serde::{Deserialize, Serialize};

pub const DEVICE_SETTINGS_DIR: &str = "device";
pub const DEVICE_SETTINGS_FILENAME: &str = "settings.json";

/// Device-local policy for startup wallet handling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceSettings {
    /// scope=device; movement=local-only; mutability=startup-only;
    /// security=secret-reference (the secret stays in Personae or the vault).
    pub startup_unlock_mode: StartupUnlockMode,
}

pub fn device_settings_path(data_root: &Path) -> PathBuf {
    data_root
        .join(DEVICE_SETTINGS_DIR)
        .join(DEVICE_SETTINGS_FILENAME)
}

pub fn save_device_settings(data_root: &Path, settings: &DeviceSettings) -> io::Result<()> {
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
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
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
        let root = std::env::temp_dir().join(format!(
            "mere-device-settings-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let original = DeviceSettings {
            startup_unlock_mode: StartupUnlockMode::Locked,
        };
        save_device_settings(&root, &original).unwrap();
        assert_eq!(load_device_settings(&root).unwrap(), Some(original));
        assert!(device_settings_exist(&root));
        let _ = fs::remove_dir_all(root);
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    }
}
