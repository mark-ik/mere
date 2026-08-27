// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pre-flight checks: every grant spec and enrollment bundle is validated
//! against wallet state before anything is written.

use std::io;
use std::path::Path;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider, PersonaId};

use crate::wallet_store::*;

use super::*;

pub(crate) fn validate_remote_auth_spec(
    data_root: &Path,
    spec: &RemoteAuthGrantSpec,
) -> io::Result<()> {
    for &persona in &spec.personas {
        if load_persona_wallet(data_root, persona)?.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("persona wallet missing for {}", persona.as_uuid()),
            ));
        }
    }
    for wrapped in &spec.wrapped_private_epochs {
        if !spec.personas.contains(&wrapped.persona_id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "wrapped epoch persona {} is not authorized by this grant",
                    "<blinded>"
                ),
            ));
        }
    }
    if spec.scopes.iter().any(|scope| scope == "private.read")
        && spec.wrapped_private_epochs.is_empty()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "remote-auth grant with private.read must carry wrapped private epoch material",
        ));
    }
    Ok(())
}

pub(crate) fn validate_paired_remote_auth_spec(
    data_root: &Path,
    spec: &PairedRemoteAuthGrantSpec,
) -> io::Result<()> {
    for &persona in &spec.personas {
        if load_persona_wallet(data_root, persona)?.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("persona wallet missing for {}", persona.as_uuid()),
            ));
        }
    }
    for epoch in &spec.private_epochs {
        if !spec.personas.contains(&epoch.persona_id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "plaintext private epoch persona {} is not authorized by this grant",
                    epoch.persona_id.as_uuid()
                ),
            ));
        }
    }
    if spec.scopes.iter().any(|scope| scope == "private.read") && spec.private_epochs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "remote-auth pairing grant with private.read must carry plaintext private epochs",
        ));
    }
    Ok(())
}

pub(crate) fn validate_remote_auth_enrollment_bundle(
    data_root: &Path,
    bundle: &RemoteAuthEnrollmentBundle,
) -> io::Result<()> {
    if bundle.grant.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "remote-auth enrollment bundle carries no grant certificates",
        ));
    }
    if !bundle
        .grant
        .certificates()
        .all(|certificate| certificate.verify())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "remote-auth enrollment bundle grant failed signature verification",
        ));
    }

    let local = load_local_device_identity(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "local delegated-device identity missing; generate a pairing response first",
        )
    })?;
    // Every certificate in the set must address this device and name this
    // holder. The old envelope carried one device id and one delegatee for the
    // whole grant; a set has to be checked member by member, or one stray
    // certificate would ride in on the others' validity.
    for certificate in bundle.grant.certificates() {
        match certificate_device_id(certificate) {
            Some(device) if device == local.device_id => {}
            Some(device) => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "grant targets device {}, but local delegated identity is {}",
                        device.as_uuid(),
                        local.device_id.as_uuid()
                    ),
                ));
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "grant certificate does not address a device",
                ));
            }
        }
        if certificate.certificate.subject != local.public_key().0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "grant subject does not match the local delegated-device identity",
            ));
        }
    }
    let earliest_expiry = bundle
        .grant
        .certificates()
        .filter_map(|certificate| certificate.certificate.expires_at_ms)
        .min();
    if is_expired(earliest_expiry, unix_time_ms()?) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "remote-auth enrollment grant for device {} is expired",
                local.device_id.as_uuid()
            ),
        ));
    }

    let grant_personas: BTreeSet<_> = bundle
        .grant
        .personas
        .keys()
        .map(|persona| *persona.as_uuid())
        .collect();
    let bundled_personas: BTreeSet<_> = bundle
        .persona_wallets
        .iter()
        .map(|wallet| *wallet.persona_id.as_uuid())
        .collect();
    if grant_personas != bundled_personas {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "enrollment bundle persona wallets do not match the grant persona set",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;
    use super::*;

    #[test]
    fn issue_remote_auth_device_grant_rejects_unknown_persona_wallet() {
        let root = temp_data_root("remote-auth-missing-persona");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let mut spec = sample_remote_auth_spec();
        spec.personas.push(second_persona());
        let err = issue_remote_auth_device_grant(&root, &spec).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn issue_remote_auth_device_grant_rejects_wrapped_epoch_outside_persona_set() {
        let root = temp_data_root("remote-auth-mismatch");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let mut spec = sample_remote_auth_spec();
        spec.wrapped_private_epochs.push(EpochCarriage {
            persona_id: second_persona(),
            material: WrappedEpochMaterial {
                index: blinded_epoch_index(second_persona(), fixture_epoch(), FIXTURE_WRAPPING_KEY),
                wrap_format: "xchacha20poly1305-v1".into(),
                wrapped_key: vec![0xca, 0xfe],
            },
        });
        let err = issue_remote_auth_device_grant(&root, &spec).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn issue_remote_auth_device_grant_rejects_private_read_without_wrapped_epoch() {
        let root = temp_data_root("remote-auth-missing-wrap");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let mut spec = sample_remote_auth_spec();
        spec.wrapped_private_epochs.clear();
        let err = issue_remote_auth_device_grant(&root, &spec).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn private_read_free_grant_can_skip_wrapped_epoch_material() {
        let root = temp_data_root("remote-auth-no-private");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let mut spec = sample_remote_auth_spec();
        spec.scopes = vec!["identity.act".into(), "transport.egress".into()];
        spec.wrapped_private_epochs.clear();
        let grant = issue_remote_auth_device_grant(&root, &spec).unwrap();
        assert!(stored_epochs_for(&root, &grant, fixture_persona()).is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }
}
