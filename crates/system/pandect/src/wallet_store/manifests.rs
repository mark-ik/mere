// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The plain-JSON manifests: the identity wallet, the device roster, and one
//! wallet per persona. Nothing here is sealed; secrets live in `secrets` and
//! `devices`.

use std::io;
use std::path::Path;

use identity::PersonaId;

use super::io::{json_pretty_bytes, load_json_optional, save_json_atomic};
use super::paths::{device_roster_path, identity_wallet_path, persona_wallet_path};
use super::{CarryRef, DeviceRoster, IdentityWalletManifest, PersonaWalletManifest};

/// Load the identity wallet manifest, or `None` when absent.
pub fn load_identity_wallet(data_root: &Path) -> io::Result<Option<IdentityWalletManifest>> {
    load_json_optional(&identity_wallet_path(data_root))
}

/// Save the identity wallet manifest atomically.
pub fn save_identity_wallet(data_root: &Path, wallet: &IdentityWalletManifest) -> io::Result<()> {
    save_json_atomic(&identity_wallet_path(data_root), wallet)
}

/// Load the device roster, or `None` when absent.
pub fn load_device_roster(data_root: &Path) -> io::Result<Option<DeviceRoster>> {
    load_json_optional(&device_roster_path(data_root))
}

/// Save the device roster atomically.
pub fn save_device_roster(data_root: &Path, roster: &DeviceRoster) -> io::Result<()> {
    save_json_atomic(&device_roster_path(data_root), roster)
}

/// Stable content ref of a device roster's on-disk JSON bytes.
pub fn device_roster_ref(roster: &DeviceRoster) -> io::Result<CarryRef> {
    let bytes = json_pretty_bytes(roster)?;
    Ok(CarryRef::of(bytes.as_slice()))
}

/// Load one persona wallet manifest, or `None` when absent.
pub fn load_persona_wallet(
    data_root: &Path,
    persona: PersonaId,
) -> io::Result<Option<PersonaWalletManifest>> {
    load_json_optional(&persona_wallet_path(data_root, persona))
}

/// Save one persona wallet manifest atomically.
pub fn save_persona_wallet(data_root: &Path, wallet: &PersonaWalletManifest) -> io::Result<()> {
    save_json_atomic(&persona_wallet_path(data_root, wallet.persona_id), wallet)
}

#[cfg(test)]
mod tests {
    use super::super::devices::load_device_grant;
    use super::super::test_support::*;
    use super::super::*;
    use super::super::{
        CapabilitySlotRef, DeviceExposure, DeviceGrantRef, DeviceId, DeviceMode, DevicePublicKey,
        DeviceRecord, PersonaWalletRef,
    };
    use super::*;
    use identity::{IdentityProvider, InMemoryProvider};
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn missing_wallet_files_return_none() {
        let root = temp_data_root("missing");
        assert!(load_identity_wallet(&root).unwrap().is_none());
        assert!(load_device_roster(&root).unwrap().is_none());
        assert!(load_local_device_identity(&root).unwrap().is_none());
        assert!(
            load_persona_wallet(&root, fixture_persona())
                .unwrap()
                .is_none()
        );
        assert!(
            load_device_grant(&root, fixture_device())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn identity_wallet_round_trips() {
        let root = temp_data_root("identity");
        let wallet = IdentityWalletManifest {
            device_roster_ref: Some(CarryRef::of(b"roster")),
            personas: vec![PersonaWalletRef {
                persona_id: fixture_persona(),
            }],
            grant_index: vec![DeviceGrantRef {
                device_id: fixture_device(),
                grant_ref: Some(CarryRef::of(b"grant")),
            }],
            ..IdentityWalletManifest::default()
        };
        save_identity_wallet(&root, &wallet).unwrap();
        let restored = load_identity_wallet(&root).unwrap().unwrap();
        assert_eq!(restored, wallet);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn device_roster_round_trips() {
        let root = temp_data_root("roster");
        let provider = InMemoryProvider::from_seed([4u8; 32]);
        let roster = DeviceRoster {
            devices: vec![DeviceRecord {
                device_id: fixture_device(),
                device_pubkey: DevicePublicKey::from(provider.master_public_key()),
                label: "home-server".to_string(),
                mode: DeviceMode::RemoteAuth,
                exposure: DeviceExposure::ExposedEgress,
                grant_ref: Some(CarryRef::of(b"grant")),
            }],
            revoked: vec![DeviceId::from_uuid(Uuid::from_u128(0x4444))],
            ..DeviceRoster::new()
        };
        save_device_roster(&root, &roster).unwrap();
        let restored = load_device_roster(&root).unwrap().unwrap();
        assert_eq!(restored, roster);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn persona_wallet_round_trips() {
        let root = temp_data_root("persona");
        let mut wallet =
            PersonaWalletManifest::new(fixture_persona(), fixture_chain_root(), fixture_epoch());
        wallet.epoch_history_ref = Some(CarryRef::of(b"epochs"));
        wallet.private_roots.primary_root = Some(CarryRef::of(b"private-root"));
        wallet
            .private_roots
            .typed_roots
            .insert("eidetic".to_string(), CarryRef::of(b"typed-private"));
        wallet.public_roots.primary_root = Some(CarryRef::of(b"public-root"));
        wallet.capability_slots.push(CapabilitySlotRef {
            slot_id: "cluster-read".to_string(),
            grant_ref: Some(CarryRef::of(b"cap")),
        });
        save_persona_wallet(&root, &wallet).unwrap();
        let restored = load_persona_wallet(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, wallet);
        let _ = fs::remove_dir_all(&root);
    }
}
