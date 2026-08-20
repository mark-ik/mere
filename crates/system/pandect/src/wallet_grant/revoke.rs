// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Revoking a delegated device: clearing its slots, rotating the epochs it
//! could read, and cutting its access to each persona.

use std::io;
use std::path::Path;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider, PersonaId};

use crate::wallet_store::*;

use identity::carry::DeviceGrantSet;

use super::*;

/// Revoke one delegated remote-auth device, clear its active persona wallet
/// slots, and rotate future-write private epochs when the grant carried
/// `private.read`.
pub fn revoke_remote_auth_device(
    data_root: &Path,
    device_id: DeviceId,
) -> io::Result<RemoteAuthRevocationOutcome> {
    let grant = load_device_grant_set(data_root, device_id)?;
    if grant.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            legacy_grant_hint(data_root, device_id),
        ));
    }
    if !grant.certificates().all(|certificate| certificate.verify()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "device grant certificate failed signature verification",
        ));
    }

    let mut roster = load_device_roster(data_root)?.unwrap_or_else(DeviceRoster::new);
    let device = roster
        .devices
        .iter()
        .find(|record| record.device_id == device_id)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("device roster missing {}", device_id.as_uuid()),
            )
        })?;
    if device.mode == DeviceMode::Copy {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "device {} is a copy-mode device; revoke by rotating the master root",
                device_id.as_uuid()
            ),
        ));
    }

    // Mint the portable statements first, then let the roster follow them.
    // The list is a projection now: the signed statements are the record, and
    // they are what a peer or a stolen radio's neighbours can actually verify.
    let seed = load_identity_seed(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "wallet root missing identity/master.seed; bootstrap the wallet first",
        )
    })?;
    let statements = revoke_device_certificates(data_root, seed, device_id, unix_time_ms()?)?;

    let already_revoked = roster.revoked.contains(&device_id);
    if !already_revoked {
        roster.revoked.push(device_id);
        save_device_roster(data_root, &roster)?;
    }
    let mut identity_wallet = load_identity_wallet(data_root)?.unwrap_or_default();
    identity_wallet.device_roster_ref = Some(device_roster_ref(&roster)?);
    save_identity_wallet(data_root, &identity_wallet)?;

    let rotated_personas = revoke_persona_grant_access(data_root, device_id, &grant)?;
    remove_remote_auth_wrapping_key(data_root, device_id)?;
    let refreshed_devices = refresh_remote_auth_private_read_grants(data_root, &rotated_personas)?;
    Ok(RemoteAuthRevocationOutcome {
        device_id,
        already_revoked,
        rotated_personas,
        refreshed_devices,
        statements,
    })
}

pub(crate) fn revoke_persona_grant_access(
    data_root: &Path,
    device_id: DeviceId,
    grant: &DeviceGrantSet,
) -> io::Result<Vec<PersonaId>> {
    let slot_id = remote_auth_capability_slot_id(device_id);
    // Whether a persona's private lane was exposed is now a per-certificate
    // question, not one flag for the whole grant: revoking a device that could
    // read one persona's private lane and only act for another must rotate the
    // first and leave the second alone.
    let mut rotated_personas = Vec::new();
    for (&persona, certificate) in &grant.personas {
        let private_read = requires_epoch_material(certificate);
        let mut wallet = load_persona_wallet(data_root, persona)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("persona wallet missing for {}", persona.as_uuid()),
            )
        })?;
        let mut changed = false;
        if let Some(existing) = wallet
            .capability_slots
            .iter_mut()
            .find(|slot| slot.slot_id == slot_id)
        {
            if existing.grant_ref.take().is_some() {
                changed = true;
            }
        }

        // The epochs the device actually holds now live in the record keyed by
        // this persona's certificate, so ask that record rather than the grant.
        let certificate_id = certificate.certificate.id();
        let held = load_wrapped_epoch_record(data_root, certificate_id)?;
        // The question is whether this device was given private-lane material
        // for this persona at all. It used to be asked by reading the epoch id
        // out of the record and comparing it to the head; the entries are
        // blinded now, and matching them would need the device's wrapping key,
        // which the direct issue path never retains.
        //
        // Presence is the right question anyway. Over-rotating on a revocation
        // costs a re-wrap; under-rotating leaves a withdrawn device holding a
        // live epoch. Idempotence comes from removing the record below rather
        // than from narrowing the test.
        let should_rotate = private_read && held.is_some_and(|record| !record.epochs.is_empty());
        let next_epoch = if should_rotate {
            let next_epoch = KeyEpochId::new();
            wallet.private_epoch_head = next_epoch;
            rotated_personas.push(persona);
            changed = true;
            Some(next_epoch)
        } else {
            None
        };

        // A withdrawn device keeps no carriage. This is also what makes a second
        // revoke a no-op rather than a second rotation.
        remove_wrapped_epoch_record(data_root, certificate_id)?;

        if changed {
            save_persona_wallet(data_root, &wallet)?;
        }
        if let Some(next_epoch) = next_epoch {
            ensure_persona_epoch_bridge(data_root, persona, next_epoch)?;
        }
    }
    Ok(rotated_personas)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;
    use super::*;

    #[test]
    fn revoke_remote_auth_device_clears_slots_and_rotates_future_write_epochs() {
        let root = temp_data_root("remote-auth-revoke");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        crate::wallet_store::ensure_wallet_state(&root, second_persona(), "Studio PC").unwrap();

        let first_epoch = crate::wallet_store::load_current_private_epoch(&root, fixture_persona())
            .unwrap()
            .expect("first persona epoch bridge should exist");
        let second_epoch = crate::wallet_store::load_current_private_epoch(&root, second_persona())
            .unwrap()
            .expect("second persona epoch bridge should exist");
        let spec = RemoteAuthGrantSpec {
            device_id: fixture_device(),
            delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
            label: "Pocket relay".into(),
            exposure: DeviceExposure::ExposedEgress,
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona(), second_persona()],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
            wrapped_private_epochs: vec![
                EpochCarriage {
                    persona_id: fixture_persona(),
                    material: wrap_private_epoch_material(
                        fixture_persona(),
                        first_epoch.epoch_id,
                        &first_epoch.epoch_secret,
                        [21; 32],
                    )
                    .unwrap(),
                },
                EpochCarriage {
                    persona_id: second_persona(),
                    material: wrap_private_epoch_material(
                        second_persona(),
                        second_epoch.epoch_id,
                        &second_epoch.epoch_secret,
                        [22; 32],
                    )
                    .unwrap(),
                },
            ],
        };
        let grant = issue_remote_auth_device_grant(&root, &spec).unwrap();
        let grant_ref = device_grant_set_ref(&grant);

        let outcome = revoke_remote_auth_device(&root, spec.device_id).unwrap();
        assert!(!outcome.already_revoked);
        assert_eq!(outcome.rotated_personas.len(), 2);
        assert!(outcome.rotated_personas.contains(&fixture_persona()));
        assert!(outcome.rotated_personas.contains(&second_persona()));
        assert!(outcome.refreshed_devices.is_empty());

        let roster = crate::wallet_store::load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        assert!(roster.revoked.contains(&spec.device_id));

        let identity_wallet = crate::wallet_store::load_identity_wallet(&root)
            .unwrap()
            .expect("identity wallet should exist");
        assert_eq!(
            identity_wallet.device_roster_ref,
            Some(crate::wallet_store::device_roster_ref(&roster).unwrap())
        );
        assert!(
            identity_wallet.grant_index.iter().any(
                |known| known.device_id == spec.device_id && known.grant_ref == Some(grant_ref)
            )
        );

        for (persona, old_epoch) in [
            (fixture_persona(), first_epoch.epoch_id),
            (second_persona(), second_epoch.epoch_id),
        ] {
            let wallet = crate::wallet_store::load_persona_wallet(&root, persona)
                .unwrap()
                .expect("persona wallet should exist");
            assert_ne!(wallet.private_epoch_head, old_epoch);
            assert!(wallet.capability_slots.iter().any(|slot| {
                slot.slot_id == format!("device-grant:{}", spec.device_id.as_uuid())
                    && slot.grant_ref.is_none()
            }));

            let rotated = crate::wallet_store::load_current_private_epoch(&root, persona)
                .unwrap()
                .expect("rotated epoch should be staged");
            assert_eq!(rotated.epoch_id, wallet.private_epoch_head);
        }

        let err = build_remote_auth_enrollment_bundle(&root, spec.device_id).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The seam M4 exists for: revoking produces statements a peer could
    /// verify, not just a local list entry.
    #[test]
    fn revoking_returns_portable_statements_and_folds_them() {
        let root = temp_data_root("remote-auth-revoke-statements");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        let spec = sample_remote_auth_spec();
        let set = issue_remote_auth_device_grant(&root, &spec).unwrap();

        let outcome = revoke_remote_auth_device(&root, spec.device_id).unwrap();

        assert_eq!(outcome.statements.len(), set.certificates().count());
        assert!(
            outcome
                .statements
                .iter()
                .all(|statement| statement.verify())
        );
        assert!(
            crate::wallet_grant::device_is_fully_revoked(&root, spec.device_id).unwrap(),
            "the wallet's own ledger should already carry them"
        );
    }

    #[test]
    fn revoke_remote_auth_device_is_idempotent_once_rotation_has_landed() {
        let root = temp_data_root("remote-auth-revoke-idempotent");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let current_epoch =
            crate::wallet_store::load_current_private_epoch(&root, fixture_persona())
                .unwrap()
                .expect("epoch bridge should exist");
        let spec = RemoteAuthGrantSpec {
            device_id: fixture_device(),
            delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
            label: "Pocket relay".into(),
            exposure: DeviceExposure::ExposedEgress,
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
            wrapped_private_epochs: vec![EpochCarriage {
                persona_id: fixture_persona(),
                material: wrap_private_epoch_material(
                    fixture_persona(),
                    current_epoch.epoch_id,
                    &current_epoch.epoch_secret,
                    [23; 32],
                )
                .unwrap(),
            }],
        };
        issue_remote_auth_device_grant(&root, &spec).unwrap();

        let first = revoke_remote_auth_device(&root, spec.device_id).unwrap();
        let rotated_head = crate::wallet_store::load_persona_wallet(&root, fixture_persona())
            .unwrap()
            .expect("persona wallet should exist")
            .private_epoch_head;
        let second = revoke_remote_auth_device(&root, spec.device_id).unwrap();
        let second_head = crate::wallet_store::load_persona_wallet(&root, fixture_persona())
            .unwrap()
            .expect("persona wallet should exist")
            .private_epoch_head;

        assert!(!first.already_revoked);
        assert_eq!(first.rotated_personas, vec![fixture_persona()]);
        assert!(first.refreshed_devices.is_empty());
        assert!(second.already_revoked);
        assert!(second.rotated_personas.is_empty());
        assert!(second.refreshed_devices.is_empty());
        assert_eq!(second_head, rotated_head);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn revoke_remote_auth_device_rejects_copy_mode_devices() {
        let root = temp_data_root("remote-auth-revoke-copy");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let copy_device = DeviceId::new();
        let grant = identity::carry::issue_device_grant_set(
            [21; 32],
            copy_device,
            DevicePublicKey::from(delegatee().public_key()),
            &["identity.act", "private.read"],
            &[fixture_persona()],
            100_000,
            1_700_000_001,
        )
        .unwrap();
        save_device_grant_set(&root, copy_device, &grant).unwrap();

        let mut roster = crate::wallet_store::load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        roster.devices.push(DeviceRecord {
            device_id: copy_device,
            device_pubkey: DevicePublicKey::from(delegatee().public_key()),
            label: "Laptop clone".into(),
            mode: DeviceMode::Copy,
            exposure: DeviceExposure::ExposedEgress,
            carriage: CarriagePolicy::default(),
            grant_ref: None,
        });
        crate::wallet_store::save_device_roster(&root, &roster).unwrap();

        let err = revoke_remote_auth_device(&root, copy_device).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        let _ = std::fs::remove_dir_all(&root);
    }
}
