// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! First-launch bootstrap: seed a copy-style root, or preserve whatever a
//! delegated device already installed.

use std::io;
use std::path::Path;

use identity::{IdentityProvider, InMemoryProvider, PersonaId};

use super::devices::load_local_device_identity;
use super::devices::local_device_identity_locked_at_startup;
use super::epochs::ensure_persona_epoch_bridge;
use super::manifests::{
    device_roster_ref, load_device_roster, load_identity_wallet, load_persona_wallet,
    save_device_roster, save_identity_wallet, save_persona_wallet,
};
use super::secrets::{identity_seed_locked_at_startup, load_identity_seed, save_identity_seed};
use super::{
    DeviceExposure, DeviceId, DeviceMode, DevicePublicKey, DeviceRecord, DeviceRoster, KeyEpochId,
    PersonaWalletManifest, PersonaWalletRef, WALLET_SCHEMA_VERSION, derive_persona_chain_root,
};

/// Which wallet bootstrap posture the current data root resolved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletBootstrapMode {
    /// A copy-style wallet root with a shared identity seed is live.
    CopySeeded,
    /// A sealed local secret exists, but the current startup-unlock mode left it
    /// locked at launch, so copy/delegated state must not be clobbered.
    Locked,
    /// A delegated-device identity exists locally, but enrollment has not yet
    /// installed persona wallet state.
    DelegatedPending,
    /// Delegated-device wallet state is already installed and must not be
    /// overwritten by copy-style seed bootstrap.
    DelegatedEnrolled,
}

/// Bootstrap wallet state without clobbering a delegated-device install.
///
/// Fresh roots still seed the copy-style identity bridge. Once a delegated
/// device has either minted its local identity or installed a remote-auth
/// enrollment bundle, startup must preserve that state instead of creating
/// `identity/master.seed` and a copy-mode roster entry.
pub fn bootstrap_wallet_state(
    data_root: &Path,
    persona: PersonaId,
    device_label: &str,
) -> io::Result<WalletBootstrapMode> {
    if load_identity_seed(data_root)?.is_some() {
        ensure_wallet_state(data_root, persona, device_label)?;
        return Ok(WalletBootstrapMode::CopySeeded);
    }
    if identity_seed_locked_at_startup(data_root) {
        return Ok(WalletBootstrapMode::Locked);
    }
    if load_identity_wallet(data_root)?.is_some() {
        return Ok(WalletBootstrapMode::DelegatedEnrolled);
    }
    if load_local_device_identity(data_root)?.is_some() {
        return Ok(WalletBootstrapMode::DelegatedPending);
    }
    if local_device_identity_locked_at_startup(data_root) {
        return Ok(WalletBootstrapMode::Locked);
    }
    ensure_wallet_state(data_root, persona, device_label)?;
    Ok(WalletBootstrapMode::CopySeeded)
}

/// Ensure the shared identity root and one persona wallet exist, seeding the
/// minimal carry-layer files on first launch.
///
/// The bridge seed is currently a raw 32-byte file under `identity/`; the
/// encrypted persona vault remains the follow-up that supersedes it.
pub fn ensure_wallet_state(
    data_root: &Path,
    persona: PersonaId,
    device_label: &str,
) -> io::Result<[u8; 32]> {
    let seed = match load_identity_seed(data_root)? {
        Some(seed) => seed,
        None => {
            let seed = InMemoryProvider::random().master_keypair().to_seed();
            save_identity_seed(data_root, seed)?;
            seed
        }
    };

    let mut identity_wallet = load_identity_wallet(data_root)?.unwrap_or_default();
    if !identity_wallet
        .personas
        .iter()
        .any(|known| known.persona_id == persona)
    {
        identity_wallet.personas.push(PersonaWalletRef {
            persona_id: persona,
        });
    }

    let roster = match load_device_roster(data_root)? {
        Some(roster) => roster,
        None => {
            let provider = InMemoryProvider::from_seed(seed);
            let roster = DeviceRoster {
                schema_version: WALLET_SCHEMA_VERSION,
                devices: vec![DeviceRecord {
                    device_id: DeviceId::new(),
                    device_pubkey: DevicePublicKey::from(provider.master_public_key()),
                    label: device_label.to_string(),
                    mode: DeviceMode::Copy,
                    exposure: DeviceExposure::HiddenClient,
                    grant_ref: None,
                }],
                revoked: Vec::new(),
            };
            save_device_roster(data_root, &roster)?;
            roster
        }
    };

    let persona_wallet = match load_persona_wallet(data_root, persona)? {
        Some(wallet) => wallet,
        None => {
            let chain_root = derive_persona_chain_root(seed, persona)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let wallet = PersonaWalletManifest::new(persona, chain_root, KeyEpochId::new());
            save_persona_wallet(data_root, &wallet)?;
            wallet
        }
    };
    ensure_persona_epoch_bridge(data_root, persona, persona_wallet.private_epoch_head)?;

    identity_wallet.device_roster_ref = Some(device_roster_ref(&roster)?);
    save_identity_wallet(data_root, &identity_wallet)?;
    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;
    use super::*;
    use crate::device_settings_store::{DeviceSettings, save_device_settings};
    use identity::StartupUnlockMode;
    use std::fs;

    #[test]
    fn ensure_wallet_state_seeds_identity_and_persona_files() {
        let root = temp_data_root("bootstrap");
        let persona = fixture_persona();
        let seed = ensure_wallet_state(&root, persona, "Studio PC").unwrap();

        assert_eq!(load_identity_seed(&root).unwrap(), Some(seed));
        let identity_wallet = load_identity_wallet(&root)
            .unwrap()
            .expect("identity wallet should exist");
        assert_eq!(
            identity_wallet.personas,
            vec![PersonaWalletRef {
                persona_id: persona
            }]
        );
        let roster = load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        assert_eq!(roster.devices.len(), 1, "one local device is seeded");
        assert_eq!(roster.devices[0].label, "Studio PC");
        assert_eq!(roster.devices[0].mode, DeviceMode::Copy);
        let persona_wallet = load_persona_wallet(&root, persona)
            .unwrap()
            .expect("persona wallet should exist");
        assert_eq!(persona_wallet.persona_id, persona);
        assert_eq!(
            persona_wallet.chain_root,
            derive_persona_chain_root(seed, persona).unwrap()
        );
        let epoch = load_current_private_epoch(&root, persona)
            .unwrap()
            .expect("current private epoch should exist");
        assert_eq!(epoch.epoch_id, persona_wallet.private_epoch_head);
        assert_eq!(epoch.epoch_secret.len(), 32);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_wallet_state_is_idempotent_for_existing_seeded_files() {
        let root = temp_data_root("bootstrap-idempotent");
        let persona = fixture_persona();
        let first = ensure_wallet_state(&root, persona, "Studio PC").unwrap();
        let second = ensure_wallet_state(&root, persona, "Other Label").unwrap();
        assert_eq!(first, second, "bootstrap reuses the same master seed");
        let roster = load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        assert_eq!(
            roster.devices.len(),
            1,
            "bootstrap does not duplicate the device"
        );
        assert_eq!(
            roster.devices[0].label, "Studio PC",
            "bootstrap leaves the seeded device record intact"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bootstrap_wallet_state_seeds_a_fresh_copy_root() {
        let root = temp_data_root("bootstrap-copy");
        let mode = bootstrap_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::CopySeeded);
        assert!(load_identity_seed(&root).unwrap().is_some());
        assert!(load_identity_wallet(&root).unwrap().is_some());
        assert!(load_local_device_identity(&root).unwrap().is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bootstrap_wallet_state_preserves_a_pending_delegated_identity() {
        let root = temp_data_root("bootstrap-delegated-pending");
        let local = ensure_local_device_identity(&root, "Pocket Meerkat").unwrap();

        let mode = bootstrap_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::DelegatedPending);
        assert!(load_identity_seed(&root).unwrap().is_none());
        assert_eq!(load_local_device_identity(&root).unwrap(), Some(local));
        assert!(load_identity_wallet(&root).unwrap().is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bootstrap_wallet_state_preserves_an_enrolled_delegated_wallet() {
        let root = temp_data_root("bootstrap-delegated-enrolled");
        let persona = fixture_persona();
        save_identity_wallet(
            &root,
            &IdentityWalletManifest {
                schema_version: WALLET_SCHEMA_VERSION,
                device_roster_ref: None,
                recovery_policy: RecoveryPolicy::default(),
                personas: vec![PersonaWalletRef {
                    persona_id: persona,
                }],
                grant_index: Vec::new(),
            },
        )
        .unwrap();

        let mode = bootstrap_wallet_state(&root, persona, "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::DelegatedEnrolled);
        assert!(load_identity_seed(&root).unwrap().is_none());
        assert_eq!(
            load_identity_wallet(&root)
                .unwrap()
                .expect("identity wallet should persist")
                .personas,
            vec![PersonaWalletRef {
                persona_id: persona
            }]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn bootstrap_wallet_state_reports_locked_when_copy_seed_is_sealed_but_startup_stays_locked() {
        let root = temp_data_root("bootstrap-locked-copy");
        let persona = fixture_persona();
        ensure_wallet_state(&root, persona, "Studio PC").unwrap();
        save_device_settings(
            &root,
            &DeviceSettings {
                startup_unlock_mode: StartupUnlockMode::Locked,
            },
        )
        .unwrap();

        let mode = bootstrap_wallet_state(&root, persona, "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::Locked);
        assert!(identity_seed_locked_at_startup(&root));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn bootstrap_wallet_state_reports_locked_when_delegated_identity_is_sealed_but_startup_stays_locked()
     {
        let root = temp_data_root("bootstrap-locked-delegated");
        let persona = fixture_persona();
        ensure_local_device_identity(&root, "Pocket Meerkat").unwrap();
        save_device_settings(
            &root,
            &DeviceSettings {
                startup_unlock_mode: StartupUnlockMode::Prompt,
            },
        )
        .unwrap();

        let mode = bootstrap_wallet_state(&root, persona, "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::Locked);

        let _ = fs::remove_dir_all(&root);
    }
}
