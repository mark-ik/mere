// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Re-issuing the private-read grants of the devices that remain after a
//! revocation, so they can still read under the rotated epoch.

use std::io;
use std::path::Path;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider, PersonaId};

use crate::wallet_store::*;

use super::*;

pub(crate) fn refresh_remote_auth_private_read_grants(
    data_root: &Path,
    rotated_personas: &[PersonaId],
) -> io::Result<Vec<DeviceId>> {
    if rotated_personas.is_empty() {
        return Ok(Vec::new());
    }
    let seed = load_identity_seed(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "wallet root missing identity/master.seed; remote-auth grant refresh needs the delegator identity",
        )
    })?;
    let provider = InMemoryProvider::from_seed(seed);
    let roster = load_device_roster(data_root)?.unwrap_or_else(DeviceRoster::new);
    let refreshed = roster
        .devices
        .iter()
        .filter(|record| record.mode == DeviceMode::RemoteAuth)
        .filter(|record| !roster.revoked.contains(&record.device_id))
        .map(|record| {
            refresh_remote_auth_private_read_grant(
                data_root,
                &provider,
                record.device_id,
                rotated_personas,
            )
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(refreshed.into_iter().flatten().collect())
}

pub(crate) fn refresh_remote_auth_private_read_grant(
    data_root: &Path,
    provider: &InMemoryProvider,
    device_id: DeviceId,
    rotated_personas: &[PersonaId],
) -> io::Result<Option<DeviceId>> {
    let Some(wrapping_key) = load_remote_auth_wrapping_key(data_root, device_id)? else {
        return Ok(None);
    };
    let grant = load_device_grant_set(data_root, device_id)?;
    if grant.is_empty() {
        return Ok(None);
    }
    if !grant.certificates().all(|certificate| certificate.verify()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "device grant certificates for {} failed signature verification",
                device_id.as_uuid()
            ),
        ));
    }

    let mut changed = false;
    for &persona in rotated_personas {
        let Some(certificate) = grant.personas.get(&persona) else {
            continue;
        };
        if !requires_epoch_material(certificate) {
            continue;
        }
        let Some(epoch) = load_current_private_epoch(data_root, persona)? else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "pairing private epoch missing for persona {} during grant refresh",
                    persona.as_uuid()
                ),
            ));
        };
        let id = certificate.certificate.id();
        let mut record =
            load_wrapped_epoch_record(data_root, id)?.unwrap_or_else(|| WrappedEpochRecord::new(id));
        record.epochs.retain(|wrapped| wrapped.persona_id != persona);
        record.epochs.push(
            wrap_private_epoch_material(persona, epoch.epoch_id, &epoch.epoch_secret, wrapping_key)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?,
        );
        save_wrapped_epoch_record(data_root, &record)?;
        changed = true;
    }
    if !changed {
        return Ok(None);
    }

    // Nothing else is written. The capability statement did not change, so its
    // certificate, its signature, and every ref standing for it are untouched.
    // Under the old envelope this same rotation re-signed the grant and pushed
    // a new grant_ref through the roster, the wallet index, and every persona
    // capability slot.
    Ok(Some(device_id))
}

pub(crate) fn upsert_persona_capability_slots(
    data_root: &Path,
    personas: &[PersonaId],
    device_id: DeviceId,
    grant_ref: CarryRef,
) -> io::Result<()> {
    let slot_id = remote_auth_capability_slot_id(device_id);
    for &persona in personas {
        let mut wallet = load_persona_wallet(data_root, persona)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("persona wallet missing for {}", persona.as_uuid()),
            )
        })?;
        if let Some(existing) = wallet
            .capability_slots
            .iter_mut()
            .find(|slot| slot.slot_id == slot_id)
        {
            existing.grant_ref = Some(grant_ref);
        } else {
            wallet.capability_slots.push(CapabilitySlotRef {
                slot_id: slot_id.clone(),
                grant_ref: Some(grant_ref),
            });
        }
        save_persona_wallet(data_root, &wallet)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;
    use super::*;

    #[test]
    fn revoke_remote_auth_device_refreshes_remaining_private_read_grants() {
        let root = temp_data_root("remote-auth-revoke-refresh");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let first_device = DeviceId::from_uuid(Uuid::from_u128(0xbeef1));
        let second_device = DeviceId::from_uuid(Uuid::from_u128(0xbeef2));
        let initial_epoch =
            crate::wallet_store::load_current_private_epoch(&root, fixture_persona())
                .unwrap()
                .expect("initial epoch bridge should exist");

        let ticket_a = mint_remote_auth_pairing_ticket(&RemoteAuthPairingTicketRequest {
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
        });
        let response_a = RemoteAuthPairingResponse {
            device_id: first_device,
            delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
            label: "Pocket relay".into(),
            exposure: DeviceExposure::ExposedEgress,
        };
        issue_remote_auth_device_grant_from_ticket(
            &root,
            &ticket_a,
            &response_a,
            vec![PrivateEpochPlaintext {
                persona_id: fixture_persona(),
                epoch_id: initial_epoch.epoch_id,
                epoch_secret: initial_epoch.epoch_secret.clone(),
            }],
        )
        .unwrap();

        let second_delegatee = InMemoryProvider::from_seed([7; 32])
            .derive_keypair(b"wallet-grant-second-delegatee")
            .unwrap();
        let ticket_b = mint_remote_auth_pairing_ticket(&RemoteAuthPairingTicketRequest {
            issued_at_ms: 1_700_000_002,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
        });
        let response_b = RemoteAuthPairingResponse {
            device_id: second_device,
            delegatee_pubkey: DevicePublicKey::from(second_delegatee.public_key()),
            label: "Desk relay".into(),
            exposure: DeviceExposure::HiddenClient,
        };
        let second_grant_before = issue_remote_auth_device_grant_from_ticket(
            &root,
            &ticket_b,
            &response_b,
            vec![PrivateEpochPlaintext {
                persona_id: fixture_persona(),
                epoch_id: initial_epoch.epoch_id,
                epoch_secret: initial_epoch.epoch_secret.clone(),
            }],
        )
        .unwrap();
        let second_grant_ref_before = device_grant_set_ref(&second_grant_before.0);

        let outcome = revoke_remote_auth_device(&root, first_device).unwrap();
        assert_eq!(outcome.rotated_personas, vec![fixture_persona()]);
        assert_eq!(outcome.refreshed_devices, vec![second_device]);

        let refreshed_wallet = crate::wallet_store::load_persona_wallet(&root, fixture_persona())
            .unwrap()
            .expect("persona wallet should exist");
        let refreshed_epoch =
            crate::wallet_store::load_current_private_epoch(&root, fixture_persona())
                .unwrap()
                .expect("rotated epoch should exist");
        assert_eq!(
            refreshed_epoch.epoch_id,
            refreshed_wallet.private_epoch_head
        );
        assert_ne!(refreshed_epoch.epoch_id, initial_epoch.epoch_id);

        let refreshed_second_grant = load_device_grant_set(&root, second_device).unwrap();
        // The ref is UNCHANGED, and that is the point of the split. Rotating a
        // private epoch replaces key material the device holds; it does not
        // change what the device is permitted to do. Under the old envelope
        // this assertion was assert_ne!, because appending an epoch re-signed
        // the capability statement and pushed a new ref through the roster,
        // the wallet index, and every persona capability slot.
        let refreshed_second_grant_ref = device_grant_set_ref(&refreshed_second_grant);
        assert_eq!(refreshed_second_grant_ref, second_grant_ref_before);
        assert_eq!(
            stored_epochs_for(&root, &refreshed_second_grant, fixture_persona()).len(),
            1
        );
        assert_eq!(
            stored_epochs_for(&root, &refreshed_second_grant, fixture_persona())[0].epoch_id,
            refreshed_epoch.epoch_id
        );
        assert_eq!(
            unwrap_private_epoch_material(
                &stored_epochs_for(&root, &refreshed_second_grant, fixture_persona())[0],
                second_grant_before.1.wrapping_key,
            )
            .unwrap(),
            refreshed_epoch.epoch_secret
        );

        let roster = crate::wallet_store::load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        let second_record = roster
            .devices
            .iter()
            .find(|record| record.device_id == second_device)
            .expect("second remote-auth device should remain enrolled");
        assert_eq!(second_record.grant_ref, Some(refreshed_second_grant_ref));

        let identity_wallet = crate::wallet_store::load_identity_wallet(&root)
            .unwrap()
            .expect("identity wallet should exist");
        assert!(identity_wallet.grant_index.iter().any(|known| {
            known.device_id == second_device && known.grant_ref == Some(refreshed_second_grant_ref)
        }));
        let refreshed_wallet = crate::wallet_store::load_persona_wallet(&root, fixture_persona())
            .unwrap()
            .expect("persona wallet should exist");
        assert!(refreshed_wallet.capability_slots.iter().any(|slot| {
            slot.slot_id == format!("device-grant:{}", second_device.as_uuid())
                && slot.grant_ref == Some(refreshed_second_grant_ref)
        }));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn revoke_remote_auth_device_blocks_revoked_enrollment_and_remaining_device_can_restore_refreshed_epoch()
     {
        let delegator_root = temp_data_root("remote-auth-revoke-enrollment-from");
        let delegatee_root = temp_data_root("remote-auth-revoke-enrollment-to");
        crate::wallet_store::ensure_wallet_state(&delegator_root, fixture_persona(), "Studio PC")
            .unwrap();

        let first_device = DeviceId::from_uuid(Uuid::from_u128(0xcafe1));
        let second_device = DeviceId::from_uuid(Uuid::from_u128(0xcafe2));
        let initial_epoch =
            crate::wallet_store::load_current_private_epoch(&delegator_root, fixture_persona())
                .unwrap()
                .expect("initial epoch bridge should exist");

        let ticket_a = mint_remote_auth_pairing_ticket(&RemoteAuthPairingTicketRequest {
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
        });
        let local_a = crate::wallet_store::LocalDeviceIdentity::new(
            first_device,
            [81; 32],
            "Pocket relay".into(),
        );
        let response_a = RemoteAuthPairingResponse {
            device_id: local_a.device_id,
            delegatee_pubkey: local_a.public_key(),
            label: local_a.label.clone(),
            exposure: DeviceExposure::ExposedEgress,
        };
        issue_remote_auth_device_grant_from_ticket(
            &delegator_root,
            &ticket_a,
            &response_a,
            vec![PrivateEpochPlaintext {
                persona_id: fixture_persona(),
                epoch_id: initial_epoch.epoch_id,
                epoch_secret: initial_epoch.epoch_secret.clone(),
            }],
        )
        .unwrap();

        let ticket_b = mint_remote_auth_pairing_ticket(&RemoteAuthPairingTicketRequest {
            issued_at_ms: 1_700_000_002,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
        });
        let local_b = crate::wallet_store::LocalDeviceIdentity::new(
            second_device,
            [82; 32],
            "Desk relay".into(),
        );
        let response_b = RemoteAuthPairingResponse {
            device_id: local_b.device_id,
            delegatee_pubkey: local_b.public_key(),
            label: local_b.label.clone(),
            exposure: DeviceExposure::HiddenClient,
        };
        let second_pairing = issue_remote_auth_device_grant_from_ticket(
            &delegator_root,
            &ticket_b,
            &response_b,
            vec![PrivateEpochPlaintext {
                persona_id: fixture_persona(),
                epoch_id: initial_epoch.epoch_id,
                epoch_secret: initial_epoch.epoch_secret.clone(),
            }],
        )
        .unwrap();

        let outcome = revoke_remote_auth_device(&delegator_root, first_device).unwrap();
        assert_eq!(outcome.refreshed_devices, vec![second_device]);

        let revoked_err =
            build_remote_auth_enrollment_bundle(&delegator_root, first_device).unwrap_err();
        assert_eq!(revoked_err.kind(), io::ErrorKind::PermissionDenied);

        crate::wallet_store::save_local_device_identity(&delegatee_root, &local_b).unwrap();
        let bundle = build_remote_auth_enrollment_bundle(&delegator_root, second_device).unwrap();
        install_remote_auth_enrollment_bundle_with_wrapping_key(
            &delegatee_root,
            &bundle,
            second_pairing.1.wrapping_key,
        )
        .unwrap();

        let delegator_epoch =
            crate::wallet_store::load_current_private_epoch(&delegator_root, fixture_persona())
                .unwrap()
                .expect("delegator epoch should exist");
        let restored_epoch =
            crate::wallet_store::load_current_private_epoch(&delegatee_root, fixture_persona())
                .unwrap()
                .expect("delegatee epoch should restore");
        assert_eq!(restored_epoch.epoch_id, delegator_epoch.epoch_id);
        assert_eq!(restored_epoch.epoch_secret, delegator_epoch.epoch_secret);

        let restored_grant = load_device_grant_set(&delegatee_root, second_device).unwrap();
        assert_eq!(
            stored_epochs_for(&delegatee_root, &restored_grant, fixture_persona())[0].epoch_id,
            delegator_epoch.epoch_id
        );

        let _ = std::fs::remove_dir_all(&delegator_root);
        let _ = std::fs::remove_dir_all(&delegatee_root);
    }
}
