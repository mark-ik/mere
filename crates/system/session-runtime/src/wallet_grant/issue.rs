// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Issuing a remote-auth device grant: directly, from a pairing response, or
//! from a minted ticket.

use std::path::Path;
use std::io;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider, PersonaId};

use crate::wallet_store::*;

use super::*;

/// Issue one remote-auth device grant from the shared wallet root, persist it,
/// and update the identity wallet + device roster references coherently.
pub fn issue_remote_auth_device_grant(
    data_root: &Path,
    spec: &RemoteAuthGrantSpec,
) -> io::Result<SignedDeviceGrant> {
    validate_remote_auth_spec(data_root, spec)?;

    let seed = load_identity_seed(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "wallet root missing identity/master.seed; bootstrap the wallet first",
        )
    })?;
    let provider = InMemoryProvider::from_seed(seed);
    let mut payload = DeviceGrantPayload::new_remote_auth(
        spec.device_id,
        DevicePublicKey::from(provider.master_public_key()),
        spec.delegatee_pubkey,
        spec.issued_at_ms,
    );
    payload.expires_at_ms = spec.expires_at_ms;
    payload.personas = spec.personas.clone();
    payload.scopes = spec.scopes.clone();
    payload.attenuations = spec.attenuations.clone();
    payload.wrapped_private_epochs = spec.wrapped_private_epochs.clone();

    let grant = issue_device_grant(provider.master_keypair(), payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let grant_ref = save_signed_device_grant(data_root, &grant)?;

    let mut roster = load_device_roster(data_root)?.unwrap_or_else(DeviceRoster::new);
    if roster.revoked.contains(&spec.device_id) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("device {} is revoked", spec.device_id.as_uuid()),
        ));
    }
    upsert_remote_auth_device_record(&mut roster, spec, grant_ref);
    save_device_roster(data_root, &roster)?;

    let mut identity_wallet = load_identity_wallet(data_root)?.unwrap_or_default();
    for &persona in &spec.personas {
        if !identity_wallet
            .personas
            .iter()
            .any(|known| known.persona_id == persona)
        {
            identity_wallet.personas.push(PersonaWalletRef {
                persona_id: persona,
            });
        }
    }
    upsert_grant_index(&mut identity_wallet, spec.device_id, grant_ref);
    identity_wallet.device_roster_ref = Some(device_roster_ref(&roster)?);
    save_identity_wallet(data_root, &identity_wallet)?;
    upsert_persona_capability_slots(data_root, &spec.personas, spec.device_id, grant_ref)?;

    Ok(grant)
}

/// Issue one remote-auth device grant directly from a shared pairing secret and
/// plaintext private-epoch material. This is the seam pairing calls once its
/// PAKE/SAS exchange succeeds.
pub fn issue_remote_auth_device_grant_from_pairing(
    data_root: &Path,
    spec: &PairedRemoteAuthGrantSpec,
) -> io::Result<(SignedDeviceGrant, RemoteAuthPairingMaterial)> {
    validate_paired_remote_auth_spec(data_root, spec)?;

    let seed = load_identity_seed(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "wallet root missing identity/master.seed; bootstrap the wallet first",
        )
    })?;
    let provider = InMemoryProvider::from_seed(seed);
    let delegator_pubkey = DevicePublicKey::from(provider.master_public_key());
    let pairing = derive_remote_auth_pairing_material(
        &spec.pairing_secret,
        delegator_pubkey,
        spec.delegatee_pubkey,
        spec.device_id,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let wrapped_private_epochs = spec
        .private_epochs
        .iter()
        .map(|epoch| {
            wrap_private_epoch_material(
                epoch.persona_id,
                epoch.epoch_id,
                &epoch.epoch_secret,
                pairing.wrapping_key,
            )
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
        .collect::<io::Result<Vec<_>>>()?;

    let grant = issue_remote_auth_device_grant(
        data_root,
        &RemoteAuthGrantSpec {
            device_id: spec.device_id,
            delegatee_pubkey: spec.delegatee_pubkey,
            label: spec.label.clone(),
            exposure: spec.exposure,
            issued_at_ms: spec.issued_at_ms,
            expires_at_ms: spec.expires_at_ms,
            personas: spec.personas.clone(),
            scopes: spec.scopes.clone(),
            attenuations: spec.attenuations.clone(),
            wrapped_private_epochs,
        },
    )?;
    upsert_remote_auth_wrapping_key(data_root, spec.device_id, None, pairing.wrapping_key)?;
    Ok((grant, pairing))
}

/// Issue one remote-auth device grant from a previously minted pairing ticket
/// plus the new device's response. This is the typed seam between the QR/code
/// exchange and the grant-issuance step.
pub fn issue_remote_auth_device_grant_from_ticket(
    data_root: &Path,
    ticket: &RemoteAuthPairingTicket,
    response: &RemoteAuthPairingResponse,
    private_epochs: Vec<PrivateEpochPlaintext>,
) -> io::Result<(SignedDeviceGrant, RemoteAuthPairingMaterial)> {
    let now_ms = unix_time_ms()?;
    if is_expired(ticket.expires_at_ms, now_ms) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("remote-auth pairing ticket {} is expired", ticket.ticket_id),
        ));
    }
    let (grant, pairing) = issue_remote_auth_device_grant_from_pairing(
        data_root,
        &PairedRemoteAuthGrantSpec {
            device_id: response.device_id,
            delegatee_pubkey: response.delegatee_pubkey,
            label: response.label.clone(),
            exposure: response.exposure,
            issued_at_ms: ticket.issued_at_ms,
            expires_at_ms: ticket.expires_at_ms,
            personas: ticket.personas.clone(),
            scopes: ticket.scopes.clone(),
            attenuations: ticket.attenuations.clone(),
            pairing_secret: ticket.pairing_secret.to_vec(),
            private_epochs,
        },
    )?;
    upsert_remote_auth_wrapping_key(
        data_root,
        response.device_id,
        Some(ticket.ticket_id),
        pairing.wrapping_key,
    )?;
    Ok((grant, pairing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::*;
    use super::super::test_support::*;

    #[test]
    fn issue_remote_auth_device_grant_updates_wallet_and_roster_state() {
        let root = temp_data_root("remote-auth-issue");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let wrapping_key = [11; 32];
        let wrapped = wrap_private_epoch_material(
            fixture_persona(),
            fixture_epoch(),
            b"private-epoch-seed",
            wrapping_key,
        )
        .unwrap();
        let mut spec = sample_remote_auth_spec();
        spec.wrapped_private_epochs = vec![wrapped];
        let grant = issue_remote_auth_device_grant(&root, &spec).unwrap();
        assert!(verify_device_grant(&grant).unwrap());
        assert_eq!(
            unwrap_private_epoch_material(&grant.payload.wrapped_private_epochs[0], wrapping_key)
                .unwrap(),
            b"private-epoch-seed".to_vec()
        );

        let restored = load_signed_device_grant(&root, spec.device_id)
            .unwrap()
            .expect("grant should persist");
        let grant_ref = device_grant_ref(&restored).unwrap();

        let roster = crate::wallet_store::load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        let device = roster
            .devices
            .iter()
            .find(|record| record.device_id == spec.device_id)
            .expect("remote-auth device enrolled");
        assert_eq!(device.mode, DeviceMode::RemoteAuth);
        assert_eq!(device.exposure, DeviceExposure::ExposedEgress);
        assert_eq!(device.grant_ref, Some(grant_ref));

        let wallet = crate::wallet_store::load_identity_wallet(&root)
            .unwrap()
            .expect("identity wallet should exist");
        assert_eq!(
            wallet.device_roster_ref,
            Some(crate::wallet_store::device_roster_ref(&roster).unwrap())
        );
        assert!(
            wallet.grant_index.iter().any(
                |known| known.device_id == spec.device_id && known.grant_ref == Some(grant_ref)
            )
        );
        let persona_wallet = crate::wallet_store::load_persona_wallet(&root, fixture_persona())
            .unwrap()
            .expect("persona wallet should exist");
        assert!(persona_wallet.capability_slots.iter().any(|slot| {
            slot.slot_id == format!("device-grant:{}", spec.device_id.as_uuid())
                && slot.grant_ref == Some(grant_ref)
        }));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn issue_remote_auth_device_grant_updates_capability_slots_for_every_granted_persona() {
        let root = temp_data_root("remote-auth-multi-persona-slots");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        crate::wallet_store::ensure_wallet_state(&root, second_persona(), "Studio PC").unwrap();

        let spec = RemoteAuthGrantSpec {
            device_id: fixture_device(),
            delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
            label: "Pocket relay".into(),
            exposure: DeviceExposure::ExposedEgress,
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(1_800_000_001),
            personas: vec![fixture_persona(), second_persona()],
            scopes: vec!["identity.act".into()],
            attenuations: vec!["no-subdelegation".into()],
            wrapped_private_epochs: Vec::new(),
        };
        let grant = issue_remote_auth_device_grant(&root, &spec).unwrap();
        let grant_ref = device_grant_ref(&grant).unwrap();

        for persona in [fixture_persona(), second_persona()] {
            let wallet = crate::wallet_store::load_persona_wallet(&root, persona)
                .unwrap()
                .expect("persona wallet should exist");
            assert!(wallet.capability_slots.iter().any(|slot| {
                slot.slot_id == format!("device-grant:{}", spec.device_id.as_uuid())
                    && slot.grant_ref == Some(grant_ref)
            }));
        }

        let _ = std::fs::remove_dir_all(&root);
    }


    #[test]
    fn issue_remote_auth_device_grant_from_pairing_wraps_private_epochs() {
        let root = temp_data_root("remote-auth-pairing");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let spec = sample_paired_remote_auth_spec();
        let (grant, pairing) = issue_remote_auth_device_grant_from_pairing(&root, &spec).unwrap();
        assert!(verify_device_grant(&grant).unwrap());
        assert_eq!(pairing.short_auth_string.len(), 6);
        assert_eq!(
            unwrap_private_epoch_material(
                &grant.payload.wrapped_private_epochs[0],
                pairing.wrapping_key
            )
            .unwrap(),
            b"private-epoch-seed".to_vec()
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn issue_remote_auth_device_grant_from_pairing_rejects_private_read_without_epochs() {
        let root = temp_data_root("remote-auth-pairing-missing-epochs");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let mut spec = sample_paired_remote_auth_spec();
        spec.private_epochs.clear();
        let err = issue_remote_auth_device_grant_from_pairing(&root, &spec).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn issue_remote_auth_device_grant_from_pairing_rejects_epoch_outside_persona_set() {
        let root = temp_data_root("remote-auth-pairing-persona-mismatch");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let mut spec = sample_paired_remote_auth_spec();
        spec.private_epochs.push(PrivateEpochPlaintext {
            persona_id: second_persona(),
            epoch_id: second_epoch(),
            epoch_secret: b"bad".to_vec(),
        });
        let err = issue_remote_auth_device_grant_from_pairing(&root, &spec).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn issue_remote_auth_device_grant_from_ticket_wraps_private_epochs() {
        let root = temp_data_root("remote-auth-ticket");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let ticket = mint_remote_auth_pairing_ticket(&sample_pairing_ticket_request());
        let response = sample_pairing_response();
        let epochs = vec![PrivateEpochPlaintext {
            persona_id: fixture_persona(),
            epoch_id: fixture_epoch(),
            epoch_secret: b"private-epoch-seed".to_vec(),
        }];
        let (grant, pairing) =
            issue_remote_auth_device_grant_from_ticket(&root, &ticket, &response, epochs).unwrap();
        assert!(verify_device_grant(&grant).unwrap());
        assert_eq!(
            parse_remote_auth_pairing_code(&format_remote_auth_pairing_code(ticket.pairing_secret))
                .unwrap(),
            ticket.pairing_secret
        );
        assert_eq!(
            unwrap_private_epoch_material(
                &grant.payload.wrapped_private_epochs[0],
                pairing.wrapping_key
            )
            .unwrap(),
            b"private-epoch-seed".to_vec()
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn issue_remote_auth_device_grant_from_ticket_rejects_expired_ticket() {
        let root = temp_data_root("remote-auth-ticket-expired");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let mut ticket = mint_remote_auth_pairing_ticket(&sample_pairing_ticket_request());
        ticket.expires_at_ms = Some(1);
        let response = sample_pairing_response();
        let err = issue_remote_auth_device_grant_from_ticket(&root, &ticket, &response, Vec::new())
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        let _ = std::fs::remove_dir_all(&root);
    }

}
