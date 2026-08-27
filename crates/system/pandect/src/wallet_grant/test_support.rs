// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Shared fixtures for the grant modulesّ test suites.

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider, PersonaId};
use uuid::Uuid;

use super::*;

pub(super) fn temp_data_root(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mere-wallet-grant-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The wrapping key the fixtures blind against.
pub(super) const FIXTURE_WRAPPING_KEY: [u8; 32] = [0x2b; 32];

pub(super) fn fixture_device() -> DeviceId {
    DeviceId::from_uuid(Uuid::from_u128(0xaaa1))
}

pub(super) fn fixture_persona() -> PersonaId {
    PersonaId::from_uuid(Uuid::from_u128(0xaaa2))
}

pub(super) fn fixture_epoch() -> KeyEpochId {
    KeyEpochId(Uuid::from_u128(0xaaa3))
}

pub(super) fn second_epoch() -> KeyEpochId {
    KeyEpochId(Uuid::from_u128(0xaaa5))
}

pub(super) fn second_persona() -> PersonaId {
    PersonaId::from_uuid(Uuid::from_u128(0xaaa4))
}

pub(super) fn delegator() -> Ed25519Keypair {
    InMemoryProvider::from_seed([3; 32])
        .derive_keypair(b"wallet-grant-delegator")
        .unwrap()
}

pub(super) fn delegatee() -> Ed25519Keypair {
    InMemoryProvider::from_seed([4; 32])
        .derive_keypair(b"wallet-grant-delegatee")
        .unwrap()
}

pub(super) fn sample_remote_auth_spec() -> RemoteAuthGrantSpec {
    RemoteAuthGrantSpec {
        device_id: fixture_device(),
        delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
        label: "Pocket relay".into(),
        exposure: DeviceExposure::ExposedEgress,
        issued_at_ms: 1_700_000_001,
        expires_at_ms: Some(1_800_000_001),
        personas: vec![fixture_persona()],
        scopes: vec!["identity.act".into(), "private.read".into()],
        attenuations: vec!["no-subdelegation".into()],
        wrapped_private_epochs: vec![EpochCarriage {
            persona_id: fixture_persona(),
            material: WrappedEpochMaterial {
                index: blinded_epoch_index(
                    fixture_persona(),
                    fixture_epoch(),
                    FIXTURE_WRAPPING_KEY,
                ),
                wrap_format: "xchacha20poly1305-v1".into(),
                wrapped_key: vec![0xde, 0xad, 0xbe, 0xef],
            },
        }],
    }
}

pub(super) fn sample_paired_remote_auth_spec() -> PairedRemoteAuthGrantSpec {
    PairedRemoteAuthGrantSpec {
        device_id: fixture_device(),
        delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
        label: "Pocket relay".into(),
        exposure: DeviceExposure::ExposedEgress,
        issued_at_ms: 1_700_000_001,
        expires_at_ms: Some(1_800_000_001),
        personas: vec![fixture_persona()],
        scopes: vec!["identity.act".into(), "private.read".into()],
        attenuations: vec!["no-subdelegation".into()],
        pairing_secret: b"qr-code-derived-shared-secret".to_vec(),
        private_epochs: vec![PrivateEpochPlaintext {
            persona_id: fixture_persona(),
            epoch_id: fixture_epoch(),
            epoch_secret: b"private-epoch-seed".to_vec(),
        }],
    }
}

pub(super) fn sample_pairing_ticket_request() -> RemoteAuthPairingTicketRequest {
    RemoteAuthPairingTicketRequest {
        issued_at_ms: 1_700_000_001,
        // Far-future (2100-01-01), like the grant fixtures below: the
        // ticket-issue path checks expiry against the real clock, and the
        // old seconds-scale 1_800_000_001 in this ms field reads as 1970,
        // so every ticket-consuming test failed as "expired". The
        // deliberately-expired test overrides this to Some(1) itself.
        expires_at_ms: Some(4_102_444_800_000),
        personas: vec![fixture_persona()],
        scopes: vec!["identity.act".into(), "private.read".into()],
        attenuations: vec!["no-subdelegation".into()],
    }
}

pub(super) fn sample_pairing_response() -> RemoteAuthPairingResponse {
    RemoteAuthPairingResponse {
        device_id: fixture_device(),
        delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
        label: "Pocket relay".into(),
        exposure: DeviceExposure::ExposedEgress,
    }
}

#[cfg(test)]
pub(super) fn stored_epochs_for(
    data_root: &std::path::Path,
    set: &identity::carry::DeviceGrantSet,
    persona: identity::PersonaId,
) -> Vec<WrappedEpochMaterial> {
    let Some(certificate) = set.personas.get(&persona) else {
        return Vec::new();
    };
    load_wrapped_epoch_record(data_root, certificate.certificate.id())
        .expect("reading the epoch record")
        .map(|record| record.epochs)
        .unwrap_or_default()
}

#[cfg(test)]
pub(super) fn all_certificates_verify(set: &identity::carry::DeviceGrantSet) -> bool {
    !set.is_empty() && set.certificates().all(|certificate| certificate.verify())
}
