// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The transitional per-persona private-epoch bridge: plaintext epoch
//! material staged for pairing-time wrapping, sealed at rest once a secret
//! store is available.

use std::io;
use std::path::Path;

use identity::{InMemoryProvider, PersonaId};

use super::io::{load_json_optional, save_json_atomic};
use super::manifests::load_persona_wallet;
use super::paths::persona_epoch_bridge_path;
use super::secrets::{io_backend_error, looks_like_sealed_record, wallet_persona_secret_store};
use super::{KeyEpochId, PersonaEpochBridge, PrivateEpochRecord};

/// Load one persona epoch bridge, or `None` when absent.
pub fn load_persona_epoch_bridge(
    data_root: &Path,
    persona: PersonaId,
) -> io::Result<Option<PersonaEpochBridge>> {
    let path = persona_epoch_bridge_path(data_root, persona);
    if !path.is_file() {
        return Ok(None);
    }
    let relative = path
        .strip_prefix(data_root)
        .expect("wallet path under data root");
    let secret_store = wallet_persona_secret_store(data_root)?;
    if let Some(store) = &secret_store {
        match store.load_record::<PersonaEpochBridge>(relative) {
            Ok(bridge) => return Ok(bridge),
            Err(_) => {}
        }
    }
    if secret_store.is_none() && looks_like_sealed_record(&path) {
        return Ok(None);
    }
    let legacy = load_json_optional(&path)?;
    if let (Some(bridge), Some(_)) = (&legacy, &secret_store) {
        save_persona_epoch_bridge(data_root, bridge)?;
    }
    Ok(legacy)
}

/// Save one persona epoch bridge atomically.
pub fn save_persona_epoch_bridge(data_root: &Path, bridge: &PersonaEpochBridge) -> io::Result<()> {
    let path = persona_epoch_bridge_path(data_root, bridge.persona_id);
    if let Some(store) = wallet_persona_secret_store(data_root)? {
        return store
            .save_record(
                path.strip_prefix(data_root)
                    .expect("wallet path under data root"),
                bridge,
            )
            .map_err(io_backend_error);
    }
    save_json_atomic(&path, bridge)
}

/// Ensure the temporary plaintext epoch bridge contains `epoch_id`.
pub fn ensure_persona_epoch_bridge(
    data_root: &Path,
    persona: PersonaId,
    epoch_id: KeyEpochId,
) -> io::Result<PersonaEpochBridge> {
    let mut bridge = load_persona_epoch_bridge(data_root, persona)?
        .unwrap_or_else(|| PersonaEpochBridge::new(persona));
    if !bridge.epochs.iter().any(|epoch| epoch.epoch_id == epoch_id) {
        bridge.epochs.push(PrivateEpochRecord {
            epoch_id,
            epoch_secret: InMemoryProvider::random()
                .master_keypair()
                .to_seed()
                .to_vec(),
        });
        save_persona_epoch_bridge(data_root, &bridge)?;
    }
    Ok(bridge)
}

/// Stage a known plaintext private epoch in the temporary host bridge.
pub fn stage_persona_private_epoch(
    data_root: &Path,
    persona: PersonaId,
    epoch_id: KeyEpochId,
    epoch_secret: &[u8],
) -> io::Result<PersonaEpochBridge> {
    let mut bridge = load_persona_epoch_bridge(data_root, persona)?
        .unwrap_or_else(|| PersonaEpochBridge::new(persona));
    let mut changed = false;
    match bridge
        .epochs
        .iter_mut()
        .find(|epoch| epoch.epoch_id == epoch_id)
    {
        Some(existing) => {
            if existing.epoch_secret != epoch_secret {
                existing.epoch_secret = epoch_secret.to_vec();
                changed = true;
            }
        }
        None => {
            bridge.epochs.push(PrivateEpochRecord {
                epoch_id,
                epoch_secret: epoch_secret.to_vec(),
            });
            changed = true;
        }
    }
    if changed {
        save_persona_epoch_bridge(data_root, &bridge)?;
    }
    Ok(bridge)
}

/// Load the current plaintext private epoch for `persona`, if the temporary
/// host bridge has one matching the wallet's `private_epoch_head`.
pub fn load_current_private_epoch(
    data_root: &Path,
    persona: PersonaId,
) -> io::Result<Option<PrivateEpochRecord>> {
    let Some(wallet) = load_persona_wallet(data_root, persona)? else {
        return Ok(None);
    };
    let Some(bridge) = load_persona_epoch_bridge(data_root, persona)? else {
        return Ok(None);
    };
    Ok(bridge
        .epochs
        .into_iter()
        .find(|epoch| epoch.epoch_id == wallet.private_epoch_head))
}

#[cfg(test)]
mod tests {
    use super::super::secrets::save_identity_seed;
    use super::super::test_support::*;
    use super::super::*;
    use super::*;
    use std::fs;

    #[test]
    fn persona_epoch_bridge_round_trips() {
        let root = temp_data_root("epoch-bridge");
        let bridge = PersonaEpochBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id: fixture_persona(),
            epochs: vec![PrivateEpochRecord {
                epoch_id: fixture_epoch(),
                epoch_secret: vec![0x44; 32],
            }],
        };
        save_persona_epoch_bridge(&root, &bridge).unwrap();
        let restored = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn copy_seeded_persona_epoch_bridge_round_trips_as_a_sealed_record() {
        let root = temp_data_root("epoch-bridge-sealed-copy");
        save_identity_seed(&root, [0x44; 32]).unwrap();
        let bridge = PersonaEpochBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id: fixture_persona(),
            epochs: vec![PrivateEpochRecord {
                epoch_id: fixture_epoch(),
                epoch_secret: b"known-private-epoch".to_vec(),
            }],
        };

        save_persona_epoch_bridge(&root, &bridge).unwrap();
        let restored = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);

        let text = fs::read_to_string(persona_epoch_bridge_path(&root, fixture_persona())).unwrap();
        assert!(!text.contains("\"epoch_secret\":["));
        assert!(!text.contains("known-private-epoch"));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn delegated_persona_epoch_bridge_round_trips_as_a_sealed_record() {
        let root = temp_data_root("epoch-bridge-sealed-delegated");
        ensure_local_device_identity(&root, "Pocket Meerkat").unwrap();
        let bridge = PersonaEpochBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id: fixture_persona(),
            epochs: vec![PrivateEpochRecord {
                epoch_id: fixture_epoch(),
                epoch_secret: b"known-private-epoch".to_vec(),
            }],
        };

        save_persona_epoch_bridge(&root, &bridge).unwrap();
        let restored = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);

        let text = fs::read_to_string(persona_epoch_bridge_path(&root, fixture_persona())).unwrap();
        assert!(!text.contains("\"epoch_secret\":["));
        assert!(!text.contains("known-private-epoch"));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn loading_a_legacy_plaintext_persona_epoch_bridge_migrates_it_to_a_sealed_record() {
        let root = temp_data_root("epoch-bridge-migrate");
        save_identity_seed(&root, [0x51; 32]).unwrap();
        let bridge = PersonaEpochBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id: fixture_persona(),
            epochs: vec![PrivateEpochRecord {
                epoch_id: fixture_epoch(),
                epoch_secret: b"known-private-epoch".to_vec(),
            }],
        };
        save_json_atomic(
            &persona_epoch_bridge_path(&root, fixture_persona()),
            &bridge,
        )
        .unwrap();

        let restored = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);

        let text = fs::read_to_string(persona_epoch_bridge_path(&root, fixture_persona())).unwrap();
        assert!(!text.contains("\"epoch_secret\":["));
        assert!(!text.contains("known-private-epoch"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_persona_epoch_bridge_is_idempotent_for_the_same_epoch() {
        let root = temp_data_root("ensure-epoch-bridge");
        let first = ensure_persona_epoch_bridge(&root, fixture_persona(), fixture_epoch()).unwrap();
        let second =
            ensure_persona_epoch_bridge(&root, fixture_persona(), fixture_epoch()).unwrap();
        assert_eq!(first, second);
        assert_eq!(second.epochs.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stage_persona_private_epoch_replaces_existing_secret() {
        let root = temp_data_root("stage-epoch-bridge");
        ensure_persona_epoch_bridge(&root, fixture_persona(), fixture_epoch()).unwrap();

        stage_persona_private_epoch(
            &root,
            fixture_persona(),
            fixture_epoch(),
            b"known-private-epoch",
        )
        .unwrap();

        let bridge = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .expect("bridge should exist");
        assert_eq!(bridge.epochs.len(), 1);
        assert_eq!(bridge.epochs[0].epoch_id, fixture_epoch());
        assert_eq!(
            bridge.epochs[0].epoch_secret,
            b"known-private-epoch".to_vec()
        );
        let _ = fs::remove_dir_all(&root);
    }
}
