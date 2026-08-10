// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unlock ladder and the sealed-record stores it opens, plus the identity
//! seed those stores are keyed from.
//!
//! Startup-unlock mode is device policy (`device_settings_store`), so it is
//! read here rather than modelled in `personae::carry`: the carry model
//! travels, this policy is about one machine.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::{fs, io};

use identity::{
    IdentityError, SealedRecordStorage, StartupUnlockMode, load_or_create_auto_unlock_root,
};

use crate::device_settings_store;

use super::devices::{load_local_device_identity, local_device_identity_locked_at_startup};
use super::io::save_bytes_atomic;
use super::paths::{identity_auto_unlock_root_path, identity_seed_path};

pub(super) fn io_backend_error(err: IdentityError) -> io::Error {
    io::Error::other(err.to_string())
}

fn runtime_manual_unlocks() -> &'static Mutex<HashSet<PathBuf>> {
    static MANUAL_UNLOCKS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    MANUAL_UNLOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn runtime_manual_unlock_active(data_root: &Path) -> bool {
    runtime_manual_unlocks()
        .lock()
        .map(|roots| roots.contains(data_root))
        .unwrap_or(false)
}

fn set_runtime_manual_unlock(data_root: &Path, active: bool) {
    let Ok(mut roots) = runtime_manual_unlocks().lock() else {
        return;
    };
    if active {
        roots.insert(data_root.to_path_buf());
    } else {
        roots.remove(data_root);
    }
}

fn wallet_startup_unlock_mode(data_root: &Path) -> StartupUnlockMode {
    device_settings_store::load_device_settings(data_root)
        .ok()
        .flatten()
        .map(|settings| settings.startup_unlock_mode)
        .unwrap_or_default()
}

pub(super) fn looks_like_sealed_record(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    object.contains_key("version")
        && object.contains_key("nonce")
        && object.contains_key("ciphertext")
}

pub(super) fn wallet_local_secret_store(data_root: &Path) -> io::Result<Option<SealedRecordStorage>> {
    if runtime_manual_unlock_active(data_root) {
        return load_or_create_auto_unlock_root(identity_auto_unlock_root_path(data_root))
            .map(|root| root.map(|key| SealedRecordStorage::open_with_key(data_root, key)))
            .map_err(io_backend_error);
    }
    match wallet_startup_unlock_mode(data_root) {
        StartupUnlockMode::AutoOs => {
            load_or_create_auto_unlock_root(identity_auto_unlock_root_path(data_root))
                .map(|root| root.map(|key| SealedRecordStorage::open_with_key(data_root, key)))
                .map_err(io_backend_error)
        }
        StartupUnlockMode::Prompt | StartupUnlockMode::Locked => Ok(None),
    }
}

fn wallet_secret_record_root_key(seed: [u8; 32]) -> [u8; 32] {
    blake3::derive_key(
        "mere.session_runtime.wallet_store.secret_records.v1",
        seed.as_slice(),
    )
}

pub(super) fn wallet_secret_store_from_seed(data_root: &Path, seed: [u8; 32]) -> SealedRecordStorage {
    SealedRecordStorage::open_with_key(data_root, wallet_secret_record_root_key(seed))
}

pub(super) fn wallet_secret_store(data_root: &Path) -> io::Result<Option<SealedRecordStorage>> {
    Ok(load_identity_seed(data_root)?.map(|seed| wallet_secret_store_from_seed(data_root, seed)))
}

pub(super) fn wallet_persona_secret_store(data_root: &Path) -> io::Result<Option<SealedRecordStorage>> {
    if let Some(store) = wallet_secret_store(data_root)? {
        return Ok(Some(store));
    }
    if load_local_device_identity(data_root)?.is_some() {
        return wallet_local_secret_store(data_root);
    }
    Ok(None)
}

/// Load the shared master seed, or `None` when the bridge file is absent.
pub fn load_identity_seed(data_root: &Path) -> io::Result<Option<[u8; 32]>> {
    let path = identity_seed_path(data_root);
    if !path.is_file() {
        return Ok(None);
    }
    let relative = path
        .strip_prefix(data_root)
        .expect("wallet path under data root");
    let local_store = wallet_local_secret_store(data_root)?;
    if let Some(store) = &local_store {
        match store.load_record::<[u8; 32]>(relative) {
            Ok(Some(seed)) => return Ok(Some(seed)),
            Ok(None) => return Ok(None),
            Err(_) => {}
        }
    }
    if local_store.is_none() && looks_like_sealed_record(&path) {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let seed = <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("identity seed at {path:?} is neither a sealed record nor 32 raw bytes"),
        )
    })?;
    if local_store.is_some() {
        save_identity_seed(data_root, seed)?;
    }
    Ok(Some(seed))
}

/// Whether the startup seed record exists but is currently inaccessible because
/// the install stayed locked at launch.
pub fn identity_seed_locked_at_startup(data_root: &Path) -> bool {
    let path = identity_seed_path(data_root);
    path.is_file()
        && looks_like_sealed_record(&path)
        && load_identity_seed(data_root).ok().flatten().is_none()
}

/// Whether any device-local sealed wallet secret is still unavailable this launch.
pub fn wallet_local_secrets_locked(data_root: &Path) -> bool {
    identity_seed_locked_at_startup(data_root) || local_device_identity_locked_at_startup(data_root)
}

/// Explicitly unlock device-local sealed wallet secrets with the local OS store for this
/// session only, without changing the persisted startup policy.
pub fn unlock_wallet_with_auto_os(data_root: &Path) -> io::Result<bool> {
    let Some(_) = load_or_create_auto_unlock_root(identity_auto_unlock_root_path(data_root))
        .map_err(io_backend_error)?
    else {
        return Ok(false);
    };
    set_runtime_manual_unlock(data_root, true);
    let unlocked = load_identity_seed(data_root)?.is_some()
        || load_local_device_identity(data_root)?.is_some();
    if !unlocked {
        set_runtime_manual_unlock(data_root, false);
    }
    Ok(unlocked)
}

/// Clear the session-scoped explicit unlock override so later secret loads respect the
/// persisted startup policy again.
pub fn relock_wallet_after_manual_unlock(data_root: &Path) {
    set_runtime_manual_unlock(data_root, false);
}

/// Save the shared master seed atomically.
pub fn save_identity_seed(data_root: &Path, seed: [u8; 32]) -> io::Result<()> {
    let path = identity_seed_path(data_root);
    if let Some(store) = wallet_local_secret_store(data_root)? {
        return store
            .save_record(
                path.strip_prefix(data_root)
                    .expect("wallet path under data root"),
                &seed,
            )
            .map_err(io_backend_error);
    }
    save_bytes_atomic(&path, &seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::*;
    use super::super::test_support::*;
    use crate::device_settings_store::{DeviceSettings, save_device_settings};

    #[test]
    fn identity_seed_round_trips() {
        let root = temp_data_root("seed");
        let seed = [0x55; 32];
        save_identity_seed(&root, seed).unwrap();
        assert_eq!(load_identity_seed(&root).unwrap(), Some(seed));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn loading_a_legacy_raw_seed_migrates_it_to_a_sealed_record() {
        let root = temp_data_root("seed-migrate");
        let seed = [0x7a; 32];
        save_bytes_atomic(&identity_seed_path(&root), &seed).unwrap();

        let restored = load_identity_seed(&root).unwrap().unwrap();
        assert_eq!(restored, seed);
        assert!(identity_auto_unlock_root_path(&root).is_file());

        let text = fs::read_to_string(identity_seed_path(&root)).unwrap();
        assert!(text.contains("ciphertext"));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn explicit_auto_os_unlock_opens_a_locked_seed_without_changing_startup_mode() {
        let root = temp_data_root("explicit-auto-os-unlock");
        let persona = fixture_persona();
        ensure_wallet_state(&root, persona, "Studio PC").unwrap();
        save_device_settings(
            &root,
            &DeviceSettings {
                startup_unlock_mode: StartupUnlockMode::Locked,
            },
        )
        .unwrap();

        assert!(wallet_local_secrets_locked(&root));
        assert_eq!(load_identity_seed(&root).unwrap(), None);

        assert!(unlock_wallet_with_auto_os(&root).unwrap());
        assert!(!wallet_local_secrets_locked(&root));
        assert!(load_identity_seed(&root).unwrap().is_some());
        assert_eq!(
            device_settings_store::load_device_settings(&root)
                .unwrap()
                .unwrap_or_default()
                .startup_unlock_mode,
            StartupUnlockMode::Locked
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn relock_after_manual_unlock_hides_the_seed_again_under_locked_startup() {
        let root = temp_data_root("manual-relock");
        let persona = fixture_persona();
        ensure_wallet_state(&root, persona, "Studio PC").unwrap();
        save_device_settings(
            &root,
            &DeviceSettings {
                startup_unlock_mode: StartupUnlockMode::Prompt,
            },
        )
        .unwrap();

        assert!(unlock_wallet_with_auto_os(&root).unwrap());
        assert!(load_identity_seed(&root).unwrap().is_some());

        relock_wallet_after_manual_unlock(&root);

        assert!(wallet_local_secrets_locked(&root));
        assert_eq!(load_identity_seed(&root).unwrap(), None);

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(not(windows))]
    #[test]
    fn explicit_auto_os_unlock_reports_unavailable_without_platform_support() {
        let root = temp_data_root("explicit-auto-os-unlock-unsupported");
        assert!(!unlock_wallet_with_auto_os(&root).unwrap());
        let _ = fs::remove_dir_all(&root);
    }
}
