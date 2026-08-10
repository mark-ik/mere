// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-flight checks: every grant spec and enrollment bundle is validated
//! against wallet state before anything is written.

use std::path::Path;
use std::io;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider, PersonaId};

use crate::wallet_store::*;

use super::*;

pub(crate) fn validate_remote_auth_spec(data_root: &Path, spec: &RemoteAuthGrantSpec) -> io::Result<()> {
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
                    wrapped.persona_id.as_uuid()
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
    match verify_device_grant(&bundle.grant)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?
    {
        true => {}
        false => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "remote-auth enrollment bundle grant failed signature verification",
            ));
        }
    }

    let local = load_local_device_identity(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "local delegated-device identity missing; generate a pairing response first",
        )
    })?;
    if bundle.grant.payload.device_id != local.device_id {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "grant targets device {}, but local delegated identity is {}",
                bundle.grant.payload.device_id.as_uuid(),
                local.device_id.as_uuid()
            ),
        ));
    }
    if bundle.grant.payload.delegatee_pubkey != local.public_key() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "grant delegatee pubkey does not match the local delegated-device identity",
        ));
    }
    if is_expired(bundle.grant.payload.expires_at_ms, unix_time_ms()?) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "remote-auth enrollment grant for device {} is expired",
                bundle.grant.payload.device_id.as_uuid()
            ),
        ));
    }

    let grant_personas: BTreeSet<_> = bundle
        .grant
        .payload
        .personas
        .iter()
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
    use super::*;
    use super::super::*;
    use super::super::test_support::*;

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
        spec.wrapped_private_epochs.push(WrappedEpochMaterial {
            persona_id: second_persona(),
            epoch_id: fixture_epoch(),
            wrap_format: "xchacha20poly1305-v1".into(),
            wrapped_key: vec![0xca, 0xfe],
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
        assert!(grant.payload.wrapped_private_epochs.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

}
