// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The device fabric's stored pieces: opaque signed grants, this host's
//! delegated-device identity, and the retained remote-auth wrapping keys.

use std::path::Path;
use std::{fs, io};

use identity::InMemoryProvider;

use super::io::{load_json_optional, save_bytes_atomic, save_json_atomic};
use super::paths::{device_grant_path, local_device_identity_path, remote_auth_wrapping_keys_path};
use super::secrets::{
    io_backend_error, looks_like_sealed_record, wallet_local_secret_store, wallet_secret_store,
};
use super::{DeviceId, LocalDeviceIdentity, RemoteAuthWrappingKeyBridge};

/// Load the opaque grant payload for one device, or `None` when absent.
pub fn load_device_grant(data_root: &Path, device_id: DeviceId) -> io::Result<Option<Vec<u8>>> {
    let path = device_grant_path(data_root, device_id);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Save the opaque grant payload for one device atomically.
pub fn save_device_grant(data_root: &Path, device_id: DeviceId, bytes: &[u8]) -> io::Result<()> {
    let path = device_grant_path(data_root, device_id);
    save_bytes_atomic(&path, bytes)
}

/// Load the local delegated-device identity, or `None` when absent.
pub fn load_local_device_identity(data_root: &Path) -> io::Result<Option<LocalDeviceIdentity>> {
    let path = local_device_identity_path(data_root);
    if !path.is_file() {
        return Ok(None);
    }
    let relative = path
        .strip_prefix(data_root)
        .expect("wallet path under data root");
    let local_store = wallet_local_secret_store(data_root)?;
    if let Some(store) = &local_store {
        match store.load_record::<LocalDeviceIdentity>(relative) {
            Ok(identity) => return Ok(identity),
            Err(_) => {}
        }
    }
    if local_store.is_none() && looks_like_sealed_record(&path) {
        return Ok(None);
    }
    let legacy = load_json_optional(&path)?;
    if let (Some(identity), Some(_)) = (&legacy, &local_store) {
        save_local_device_identity(data_root, identity)?;
    }
    Ok(legacy)
}

pub(super) fn local_device_identity_locked_at_startup(data_root: &Path) -> bool {
    let path = local_device_identity_path(data_root);
    path.is_file()
        && looks_like_sealed_record(&path)
        && load_local_device_identity(data_root)
            .ok()
            .flatten()
            .is_none()
}

/// Save the local delegated-device identity atomically.
pub fn save_local_device_identity(
    data_root: &Path,
    identity: &LocalDeviceIdentity,
) -> io::Result<()> {
    let path = local_device_identity_path(data_root);
    if let Some(store) = wallet_local_secret_store(data_root)? {
        return store
            .save_record(
                path.strip_prefix(data_root)
                    .expect("wallet path under data root"),
                identity,
            )
            .map_err(io_backend_error);
    }
    save_json_atomic(&path, identity)
}

/// Load the retained remote-auth wrapping-key bridge, or `None` when absent.
pub fn load_remote_auth_wrapping_key_bridge(
    data_root: &Path,
) -> io::Result<Option<RemoteAuthWrappingKeyBridge>> {
    let path = remote_auth_wrapping_keys_path(data_root);
    if !path.is_file() {
        return Ok(None);
    }
    if let Some(store) = wallet_secret_store(data_root)? {
        match store.load_record(
            path.strip_prefix(data_root)
                .expect("wallet path under data root"),
        ) {
            Ok(bridge) => return Ok(bridge),
            Err(err) => {
                if let Some(bridge) = load_json_optional(&path)? {
                    save_remote_auth_wrapping_key_bridge(data_root, &bridge)?;
                    return Ok(Some(bridge));
                }
                return Err(io_backend_error(err));
            }
        }
    }
    if looks_like_sealed_record(&path) {
        return Ok(None);
    }
    load_json_optional(&path)
}

/// Save the retained remote-auth wrapping-key bridge atomically.
pub fn save_remote_auth_wrapping_key_bridge(
    data_root: &Path,
    bridge: &RemoteAuthWrappingKeyBridge,
) -> io::Result<()> {
    let store = wallet_secret_store(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "identity seed missing; cannot seal remote-auth wrapping keys",
        )
    })?;
    let path = remote_auth_wrapping_keys_path(data_root);
    store
        .save_record(
            path.strip_prefix(data_root)
                .expect("wallet path under data root"),
            bridge,
        )
        .map_err(io_backend_error)
}

/// Ensure a stable delegated-device identity exists for this data root.
pub fn ensure_local_device_identity(
    data_root: &Path,
    device_label: &str,
) -> io::Result<LocalDeviceIdentity> {
    if let Some(identity) = load_local_device_identity(data_root)? {
        return Ok(identity);
    }
    let seed = InMemoryProvider::random().master_keypair().to_seed();
    let identity = LocalDeviceIdentity::new(DeviceId::new(), seed, device_label.to_string());
    save_local_device_identity(data_root, &identity)?;
    Ok(identity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::*;
    use super::super::test_support::*;
    use super::super::secrets::save_identity_seed;
    use super::super::RemoteAuthWrappingKeyRecord;
    use uuid::Uuid;

    #[test]
    fn opaque_device_grant_round_trips() {
        let root = temp_data_root("grant");
        let bytes = vec![0xa1, 0x62, 0x6f, 0x6b];
        save_device_grant(&root, fixture_device(), &bytes).unwrap();
        let restored = load_device_grant(&root, fixture_device()).unwrap().unwrap();
        assert_eq!(restored, bytes);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_device_identity_round_trips() {
        let root = temp_data_root("local-device");
        let identity = LocalDeviceIdentity::new(fixture_device(), [0x33; 32], "Tablet".into());
        save_local_device_identity(&root, &identity).unwrap();
        let restored = load_local_device_identity(&root).unwrap().unwrap();
        assert_eq!(restored, identity);
        assert_eq!(
            restored.public_key(),
            DevicePublicKey::from(restored.keypair().public_key())
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn loading_a_legacy_local_device_identity_migrates_it_to_a_sealed_record() {
        let root = temp_data_root("local-device-migrate");
        let identity = LocalDeviceIdentity::new(fixture_device(), [0x33; 32], "Tablet".into());
        save_json_atomic(&local_device_identity_path(&root), &identity).unwrap();

        let restored = load_local_device_identity(&root).unwrap().unwrap();
        assert_eq!(restored, identity);
        assert!(identity_auto_unlock_root_path(&root).is_file());

        let text = fs::read_to_string(local_device_identity_path(&root)).unwrap();
        assert!(!text.contains("\"label\": \"Tablet\""));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn remote_auth_wrapping_key_bridge_round_trips_as_a_sealed_record() {
        let root = temp_data_root("wrapping-keys");
        save_identity_seed(&root, [0x66; 32]).unwrap();
        let bridge = RemoteAuthWrappingKeyBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            keys: vec![RemoteAuthWrappingKeyRecord {
                device_id: fixture_device(),
                ticket_id: Some(Uuid::from_u128(0xfeed)),
                wrapping_key: [0x5a; 32],
            }],
        };

        save_remote_auth_wrapping_key_bridge(&root, &bridge).unwrap();
        let restored = load_remote_auth_wrapping_key_bridge(&root)
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);

        let text = fs::read_to_string(remote_auth_wrapping_keys_path(&root)).unwrap();
        assert!(!text.contains("\"wrapping_key\""));
        assert!(!text.contains("feed"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_local_device_identity_is_idempotent() {
        let root = temp_data_root("ensure-local-device");
        let first = ensure_local_device_identity(&root, "Phone").unwrap();
        let second = ensure_local_device_identity(&root, "Other Label").unwrap();
        assert_eq!(first, second);
        assert_eq!(second.label, "Phone");
        let _ = fs::remove_dir_all(&root);
    }
}
