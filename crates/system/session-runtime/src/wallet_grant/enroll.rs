// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The enrollment bundle a delegator hands a newly accepted device, and the
//! install path that turns it into local wallet state.

use std::io;
use std::path::Path;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider, PersonaId};

use crate::wallet_store::*;

use super::*;

use p2panda_core::cbor::{decode_cbor, encode_cbor};

pub fn build_remote_auth_enrollment_bundle(
    data_root: &Path,
    device_id: DeviceId,
) -> io::Result<RemoteAuthEnrollmentBundle> {
    let roster = load_device_roster(data_root)?.unwrap_or_else(DeviceRoster::new);
    if roster.revoked.contains(&device_id) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("device {} is revoked", device_id.as_uuid()),
        ));
    }
    let grant = load_device_grant_set(data_root, device_id)?;
    if grant.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("device grant certificates missing for {}", device_id.as_uuid()),
        ));
    }
    if !grant.certificates().all(|certificate| certificate.verify()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "device grant certificate failed signature verification",
        ));
    }
    let granted_personas: Vec<PersonaId> = grant.personas.keys().copied().collect();
    let mut epochs = Vec::new();
    for certificate in grant.personas.values() {
        if let Some(record) = load_wrapped_epoch_record(data_root, certificate.certificate.id())? {
            epochs.push(record);
        }
    }
    let mut persona_wallets = Vec::with_capacity(granted_personas.len());
    for &persona in &granted_personas {
        let wallet = load_persona_wallet(data_root, persona)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("persona wallet missing for {}", persona.as_uuid()),
            )
        })?;
        persona_wallets.push(wallet);
    }
    Ok(RemoteAuthEnrollmentBundle {
        epochs,
        schema_version: REMOTE_AUTH_ENROLLMENT_BUNDLE_SCHEMA_VERSION,
        ticket_id: None,
        grant,
        persona_wallets,
    })
}

/// Canonical CBOR bytes for a remote-auth enrollment bundle.
pub fn encode_remote_auth_enrollment_bundle(
    bundle: &RemoteAuthEnrollmentBundle,
) -> Result<Vec<u8>, EnrollmentBundleError> {
    encode_cbor(bundle).map_err(|_| EnrollmentBundleError::Encode)
}

/// Decode a remote-auth enrollment bundle from canonical CBOR bytes.
pub fn decode_remote_auth_enrollment_bundle(
    bytes: &[u8],
) -> Result<RemoteAuthEnrollmentBundle, EnrollmentBundleError> {
    decode_cbor(bytes).map_err(|_| EnrollmentBundleError::Decode)
}

/// Install a remote-auth enrollment bundle on the delegatee side, validating
/// that the signed grant matches the local delegated-device identity bridge.
///
/// Identity-only grants can use this directly. `private.read` grants that carry
/// wrapped epochs need [`install_remote_auth_enrollment_bundle_with_wrapping_key`].
pub fn install_remote_auth_enrollment_bundle(
    data_root: &Path,
    bundle: &RemoteAuthEnrollmentBundle,
) -> io::Result<()> {
    install_remote_auth_enrollment_bundle_inner(data_root, bundle, None)
}

/// Install a remote-auth enrollment bundle on the delegatee side and restore
/// its wrapped private epochs with the pairing-derived wrapping key.
pub fn install_remote_auth_enrollment_bundle_with_wrapping_key(
    data_root: &Path,
    bundle: &RemoteAuthEnrollmentBundle,
    wrapping_key: [u8; 32],
) -> io::Result<()> {
    install_remote_auth_enrollment_bundle_inner(data_root, bundle, Some(wrapping_key))
}

pub(crate) fn install_remote_auth_enrollment_bundle_inner(
    data_root: &Path,
    bundle: &RemoteAuthEnrollmentBundle,
    wrapping_key: Option<[u8; 32]>,
) -> io::Result<()> {
    validate_remote_auth_enrollment_bundle(data_root, bundle)?;
    let local = load_local_device_identity(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "local delegated-device identity missing; generate a pairing response first",
        )
    })?;

    let grant_ref = save_device_grant_set(data_root, local.device_id, &bundle.grant)?;
    for record in &bundle.epochs {
        save_wrapped_epoch_record(data_root, record)?;
    }
    for wallet in &bundle.persona_wallets {
        save_persona_wallet(data_root, wallet)?;
    }
    if !bundle.epochs.is_empty() {
        let wrapping_key = wrapping_key.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "remote-auth enrollment bundle carries wrapped private epochs; install needs the pairing-derived wrapping key",
            )
        })?;
        restore_wrapped_private_epochs(data_root, &bundle.epochs, wrapping_key)?;
    }

    let mut roster = load_device_roster(data_root)?.unwrap_or_else(DeviceRoster::new);
    if roster.revoked.contains(&local.device_id) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("device {} is revoked", local.device_id.as_uuid()),
        ));
    }
    upsert_local_remote_auth_record(&mut roster, &local, grant_ref);
    save_device_roster(data_root, &roster)?;

    let mut identity_wallet = load_identity_wallet(data_root)?.unwrap_or_default();
    for wallet in &bundle.persona_wallets {
        if !identity_wallet
            .personas
            .iter()
            .any(|known| known.persona_id == wallet.persona_id)
        {
            identity_wallet.personas.push(PersonaWalletRef {
                persona_id: wallet.persona_id,
            });
        }
    }
    upsert_grant_index(&mut identity_wallet, local.device_id, grant_ref);
    identity_wallet.device_roster_ref = Some(device_roster_ref(&roster)?);
    save_identity_wallet(data_root, &identity_wallet)?;
    upsert_persona_capability_slots(
        data_root,
        &bundle.grant.personas.keys().copied().collect::<Vec<_>>(),
        local.device_id,
        grant_ref,
    )?;
    Ok(())
}

pub(crate) fn restore_wrapped_private_epochs(
    data_root: &Path,
    records: &[WrappedEpochRecord],
    wrapping_key: [u8; 32],
) -> io::Result<()> {
    for wrapped in records.iter().flat_map(|record| &record.epochs) {
        let epoch_secret = unwrap_private_epoch_material(wrapped, wrapping_key)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        stage_persona_private_epoch(
            data_root,
            wrapped.persona_id,
            wrapped.epoch_id,
            &epoch_secret,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;
    use super::*;

    #[test]
    fn remote_auth_enrollment_bundle_round_trips_through_cbor() {
        let chain_root =
            crate::wallet_store::derive_persona_chain_root([21; 32], fixture_persona()).unwrap();
        let bundle = RemoteAuthEnrollmentBundle {
            schema_version: REMOTE_AUTH_ENROLLMENT_BUNDLE_SCHEMA_VERSION,
            ticket_id: Some(Uuid::from_u128(0xfeed)),
            grant: identity::carry::issue_device_grant_set(
                [21; 32],
                fixture_device(),
                DevicePublicKey::from(delegatee().public_key()),
                &["identity.act", "private.read"],
                &[fixture_persona()],
                100_000,
                1_700_000_001,
            )
            .unwrap(),
            epochs: Vec::new(),
            persona_wallets: vec![PersonaWalletManifest::new(
                fixture_persona(),
                chain_root,
                fixture_epoch(),
            )],
        };

        let bytes = encode_remote_auth_enrollment_bundle(&bundle).unwrap();
        let restored = decode_remote_auth_enrollment_bundle(&bytes).unwrap();
        assert_eq!(restored, bundle);
    }

    #[test]
    fn build_remote_auth_enrollment_bundle_includes_granted_persona_wallets() {
        let root = temp_data_root("enrollment-build");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let local = crate::wallet_store::LocalDeviceIdentity::new(
            fixture_device(),
            [33; 32],
            "Pocket relay".into(),
        );
        let spec = RemoteAuthGrantSpec {
            device_id: local.device_id,
            delegatee_pubkey: local.public_key(),
            label: local.label.clone(),
            exposure: DeviceExposure::ExposedEgress,
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into()],
            attenuations: vec!["no-subdelegation".into()],
            wrapped_private_epochs: Vec::new(),
        };
        let grant = issue_remote_auth_device_grant(&root, &spec).unwrap();

        let bundle = build_remote_auth_enrollment_bundle(&root, local.device_id).unwrap();
        assert_eq!(
            bundle.schema_version,
            REMOTE_AUTH_ENROLLMENT_BUNDLE_SCHEMA_VERSION
        );
        assert_eq!(bundle.ticket_id, None);
        assert_eq!(bundle.grant, grant);
        assert_eq!(bundle.persona_wallets.len(), 1);
        assert_eq!(bundle.persona_wallets[0].persona_id, fixture_persona());
        assert_eq!(
            bundle.persona_wallets[0],
            crate::wallet_store::load_persona_wallet(&root, fixture_persona())
                .unwrap()
                .expect("persona wallet should exist")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_remote_auth_enrollment_bundle_restores_wallet_state_for_local_device() {
        let delegator_root = temp_data_root("enrollment-install-from");
        let delegatee_root = temp_data_root("enrollment-install-to");
        crate::wallet_store::ensure_wallet_state(&delegator_root, fixture_persona(), "Studio PC")
            .unwrap();

        let local = crate::wallet_store::LocalDeviceIdentity::new(
            fixture_device(),
            [44; 32],
            "Pocket relay".into(),
        );
        crate::wallet_store::save_local_device_identity(&delegatee_root, &local).unwrap();

        let spec = RemoteAuthGrantSpec {
            device_id: local.device_id,
            delegatee_pubkey: local.public_key(),
            label: local.label.clone(),
            exposure: DeviceExposure::ExposedEgress,
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into()],
            attenuations: vec!["no-subdelegation".into()],
            wrapped_private_epochs: Vec::new(),
        };
        let grant = issue_remote_auth_device_grant(&delegator_root, &spec).unwrap();
        let expected_persona_wallet =
            crate::wallet_store::load_persona_wallet(&delegator_root, fixture_persona())
                .unwrap()
                .expect("delegator persona wallet should exist");
        let bundle = build_remote_auth_enrollment_bundle(&delegator_root, local.device_id).unwrap();

        install_remote_auth_enrollment_bundle(&delegatee_root, &bundle).unwrap();

        let restored_grant = load_device_grant_set(&delegatee_root, local.device_id).unwrap();
        let grant_ref = device_grant_set_ref(&restored_grant);
        assert_eq!(restored_grant, grant);

        let restored_wallet =
            crate::wallet_store::load_persona_wallet(&delegatee_root, fixture_persona())
                .unwrap()
                .expect("delegatee persona wallet should persist");
        assert_eq!(restored_wallet, expected_persona_wallet);
        assert!(restored_wallet.capability_slots.iter().any(|slot| {
            slot.slot_id == format!("device-grant:{}", local.device_id.as_uuid())
                && slot.grant_ref == Some(grant_ref)
        }));

        let roster = crate::wallet_store::load_device_roster(&delegatee_root)
            .unwrap()
            .expect("delegatee roster should exist");
        let device = roster
            .devices
            .iter()
            .find(|record| record.device_id == local.device_id)
            .expect("delegatee roster should include the local device");
        assert_eq!(device.device_pubkey, local.public_key());
        assert_eq!(device.label, local.label);
        assert_eq!(device.mode, DeviceMode::RemoteAuth);
        assert_eq!(device.exposure, DeviceExposure::HiddenClient);
        assert_eq!(device.grant_ref, Some(grant_ref));

        let identity_wallet = crate::wallet_store::load_identity_wallet(&delegatee_root)
            .unwrap()
            .expect("delegatee identity wallet should exist");
        assert_eq!(
            identity_wallet.device_roster_ref,
            Some(crate::wallet_store::device_roster_ref(&roster).unwrap())
        );
        assert!(
            identity_wallet
                .personas
                .iter()
                .any(|known| known.persona_id == fixture_persona())
        );
        assert!(
            identity_wallet
                .grant_index
                .iter()
                .any(|known| known.device_id == local.device_id
                    && known.grant_ref == Some(grant_ref))
        );

        let _ = std::fs::remove_dir_all(&delegator_root);
        let _ = std::fs::remove_dir_all(&delegatee_root);
    }

    #[test]
    fn install_remote_auth_enrollment_bundle_with_wrapping_key_restores_private_epoch_bridge() {
        let delegator_root = temp_data_root("enrollment-install-private-read-from");
        let delegatee_root = temp_data_root("enrollment-install-private-read-to");
        crate::wallet_store::ensure_wallet_state(&delegator_root, fixture_persona(), "Studio PC")
            .unwrap();

        let local = crate::wallet_store::LocalDeviceIdentity::new(
            fixture_device(),
            [44; 32],
            "Pocket relay".into(),
        );
        crate::wallet_store::save_local_device_identity(&delegatee_root, &local).unwrap();

        let ticket = mint_remote_auth_pairing_ticket(&sample_pairing_ticket_request());
        let current_epoch =
            crate::wallet_store::load_current_private_epoch(&delegator_root, fixture_persona())
                .unwrap()
                .expect("delegator epoch bridge should exist");
        let (grant, pairing) = issue_remote_auth_device_grant_from_ticket(
            &delegator_root,
            &ticket,
            &RemoteAuthPairingResponse {
                device_id: local.device_id,
                delegatee_pubkey: local.public_key(),
                label: local.label.clone(),
                exposure: DeviceExposure::HiddenClient,
            },
            vec![PrivateEpochPlaintext {
                persona_id: fixture_persona(),
                epoch_id: current_epoch.epoch_id,
                epoch_secret: current_epoch.epoch_secret.clone(),
            }],
        )
        .unwrap();
        let mut bundle =
            build_remote_auth_enrollment_bundle(&delegator_root, local.device_id).unwrap();
        bundle.ticket_id = Some(ticket.ticket_id);

        install_remote_auth_enrollment_bundle_with_wrapping_key(
            &delegatee_root,
            &bundle,
            pairing.wrapping_key,
        )
        .unwrap();

        let restored_epoch =
            crate::wallet_store::load_current_private_epoch(&delegatee_root, fixture_persona())
                .unwrap()
                .expect("delegatee current epoch should be restored");
        assert_eq!(bundle.grant, grant);
        assert_eq!(restored_epoch.epoch_id, current_epoch.epoch_id);
        assert_eq!(restored_epoch.epoch_secret, current_epoch.epoch_secret);

        let _ = std::fs::remove_dir_all(&delegator_root);
        let _ = std::fs::remove_dir_all(&delegatee_root);
    }

    #[test]
    fn install_remote_auth_enrollment_bundle_rejects_expired_grant() {
        let delegator_root = temp_data_root("enrollment-install-expired-from");
        let delegatee_root = temp_data_root("enrollment-install-expired-to");
        crate::wallet_store::ensure_wallet_state(&delegator_root, fixture_persona(), "Studio PC")
            .unwrap();

        let local = crate::wallet_store::LocalDeviceIdentity::new(
            fixture_device(),
            [44; 32],
            "Pocket relay".into(),
        );
        crate::wallet_store::save_local_device_identity(&delegatee_root, &local).unwrap();

        let spec = RemoteAuthGrantSpec {
            device_id: local.device_id,
            delegatee_pubkey: local.public_key(),
            label: local.label.clone(),
            exposure: DeviceExposure::HiddenClient,
            issued_at_ms: 1,
            expires_at_ms: Some(2),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into()],
            attenuations: vec!["no-subdelegation".into()],
            wrapped_private_epochs: Vec::new(),
        };
        issue_remote_auth_device_grant(&delegator_root, &spec).unwrap();
        let bundle = build_remote_auth_enrollment_bundle(&delegator_root, local.device_id).unwrap();

        let err = install_remote_auth_enrollment_bundle(&delegatee_root, &bundle).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        let _ = std::fs::remove_dir_all(&delegator_root);
        let _ = std::fs::remove_dir_all(&delegatee_root);
    }
}
