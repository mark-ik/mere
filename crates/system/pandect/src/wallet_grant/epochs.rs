// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Wrapping and unwrapping the private-lane epoch material a delegated
//! device needs to read already-issued private content.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};

use identity::PersonaId;

use serde::{Deserialize, Serialize};

use super::*;

/// Derivation context turning a device's wrapping key into its blinding key.
///
/// Distinct from the key-wrapping use of the same secret, so the index cannot
/// be confused with, or substituted for, wrapped material.
const BLIND_INDEX_CONTEXT: &str = "mere.wallet.private-epoch.blind-index.v1";

/// A lookup handle for one (persona, epoch), computable only with the device's
/// wrapping key.
///
/// Two devices holding material for the same persona produce different
/// indices, because each has its own pairing-derived key. That is what stops a
/// replicated record from linking personas across devices.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlindedEpochIndex(pub [u8; 32]);

impl std::fmt::Debug for BlindedEpochIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlindedEpochIndex({})", hex::encode(&self.0[..8]))
    }
}

/// Blind one (persona, epoch) under a device's wrapping key.
///
/// Reuses [`wrapped_epoch_aad`] as the canonical encoding of the pair, so the
/// index and the AEAD's associated data commit to exactly the same bytes.
pub fn blinded_epoch_index(
    persona_id: PersonaId,
    epoch_id: KeyEpochId,
    wrapping_key: [u8; 32],
) -> BlindedEpochIndex {
    let blinding_key = blake3::derive_key(BLIND_INDEX_CONTEXT, &wrapping_key);
    BlindedEpochIndex(
        *blake3::keyed_hash(&blinding_key, &wrapped_epoch_aad(persona_id, epoch_id)).as_bytes(),
    )
}

/// Derivation context turning a device's wrapping key into its slot-blinding
/// key. Its own context, so a slot id can never collide with, or be mistaken
/// for, an epoch index derived from the same secret.
const BLIND_SLOT_CONTEXT: &str = "mere.wallet.carriage.blind-slot.v1";

/// The carriage lane's addressing handle for one device-persona certificate,
/// computable only with the device's wrapping key.
///
/// A [`WrappedEpochRecord`] is keyed by its certificate's `DelegationId`, and
/// publishing that id on an announced topic would be a stable identifier for
/// one (device, persona) pair, reintroducing at the lane what
/// [`blinded_epoch_index`] closed at the record (lane grammar, 2026-08-18).
/// So the slot takes the same construction under its own context. Holder and
/// issuer can compute it; a replica cannot, and does not need to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlindedSlotId(pub [u8; 32]);

impl std::fmt::Debug for BlindedSlotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlindedSlotId({})", hex::encode(&self.0[..8]))
    }
}

/// The carriage slot addressing one certificate's leased record.
pub fn blinded_slot_id(
    certificate: identity::delegation::DelegationId,
    wrapping_key: [u8; 32],
) -> BlindedSlotId {
    let blinding_key = blake3::derive_key(BLIND_SLOT_CONTEXT, &wrapping_key);
    BlindedSlotId(*blake3::keyed_hash(&blinding_key, &certificate.0).as_bytes())
}

pub(crate) fn wrapped_epoch_aad(persona_id: PersonaId, epoch_id: KeyEpochId) -> Vec<u8> {
    let mut aad = Vec::with_capacity(16 + 16 + 30);
    aad.extend_from_slice(b"mere.wallet.private-epoch.v1");
    aad.extend_from_slice(persona_id.as_uuid().as_bytes());
    aad.extend_from_slice(epoch_id.0.as_bytes());
    aad
}

/// Wrap one private-epoch secret under a device/pairing-derived wrapping key.
///
/// v1 keeps the wrapping key abstract: pairing can derive it via PAKE later,
/// while this storage seam already lands the concrete sealed payload shape.
pub fn wrap_private_epoch_material(
    persona_id: PersonaId,
    epoch_id: KeyEpochId,
    epoch_secret: &[u8],
    wrapping_key: [u8; 32],
) -> Result<WrappedEpochMaterial, WrappedEpochError> {
    let mut nonce_bytes = [0u8; 24];
    OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = XChaCha20Poly1305::new(
        &Key::try_from(&wrapping_key[..]).expect("fixed-length key material"),
    );
    let ciphertext = cipher
        .encrypt(
            &XNonce::try_from(&nonce_bytes[..]).expect("fixed-length key material"),
            Payload {
                msg: epoch_secret,
                aad: &wrapped_epoch_aad(persona_id, epoch_id),
            },
        )
        .map_err(|_| WrappedEpochError::Encrypt)?;
    let mut wrapped_key = nonce_bytes.to_vec();
    wrapped_key.extend_from_slice(&ciphertext);
    Ok(WrappedEpochMaterial {
        index: blinded_epoch_index(persona_id, epoch_id, wrapping_key),
        wrap_format: WRAPPED_PRIVATE_EPOCH_FORMAT_V1.into(),
        wrapped_key,
    })
}

/// Recover one wrapped private-epoch secret with the same wrapping key used to
/// produce it.
///
/// The caller states which persona and epoch it expects. That used to be read
/// off the record; blinding moves it to the caller, which is the better shape
/// anyway: the pair is now checked twice, once against the blinded index for a
/// legible error and once by the AEAD's associated data for the guarantee.
pub fn unwrap_private_epoch_material(
    material: &WrappedEpochMaterial,
    persona_id: PersonaId,
    epoch_id: KeyEpochId,
    wrapping_key: [u8; 32],
) -> Result<Vec<u8>, WrappedEpochError> {
    if material.index != blinded_epoch_index(persona_id, epoch_id, wrapping_key) {
        return Err(WrappedEpochError::WrongEntry);
    }
    if material.wrap_format != WRAPPED_PRIVATE_EPOCH_FORMAT_V1 {
        return Err(WrappedEpochError::UnsupportedWrapFormat(
            material.wrap_format.clone(),
        ));
    }
    if material.wrapped_key.len() < 24 {
        return Err(WrappedEpochError::InvalidWrappedKeyLength);
    }
    let (nonce_bytes, ciphertext) = material.wrapped_key.split_at(24);
    let cipher = XChaCha20Poly1305::new(
        &Key::try_from(&wrapping_key[..]).expect("fixed-length key material"),
    );
    cipher
        .decrypt(
            &XNonce::try_from(&nonce_bytes[..]).expect("fixed-length key material"),
            Payload {
                msg: ciphertext,
                aad: &wrapped_epoch_aad(persona_id, epoch_id),
            },
        )
        .map_err(|_| WrappedEpochError::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;
    use super::*;

    #[test]
    fn blinded_slots_do_not_link_devices_and_stay_stable_per_device() {
        let certificate = identity::delegation::DelegationId([0x42; 32]);
        // Two devices holding the same certificate's material produce
        // different slots, because each has its own pairing-derived key.
        // That is the unlinkability the lane inherits from the record.
        assert_ne!(
            blinded_slot_id(certificate, [1; 32]),
            blinded_slot_id(certificate, [2; 32])
        );
        // One device recomputes the same slot every time, which is what lets
        // the holder and the issuer address it without publishing anything.
        assert_eq!(
            blinded_slot_id(certificate, [1; 32]),
            blinded_slot_id(certificate, [1; 32])
        );
        // The slot and the epoch index derive from the same secret under
        // different contexts, so one can never be presented as the other.
        let index = blinded_epoch_index(fixture_persona(), fixture_epoch(), [1; 32]);
        assert_ne!(blinded_slot_id(certificate, [1; 32]).0, index.0);
    }

    #[test]
    fn wrap_private_epoch_material_round_trips() {
        let wrapping_key = [9; 32];
        let epoch_secret = b"persona-private-epoch-secret-v1".to_vec();
        let wrapped = wrap_private_epoch_material(
            fixture_persona(),
            fixture_epoch(),
            &epoch_secret,
            wrapping_key,
        )
        .unwrap();
        assert_eq!(wrapped.wrap_format, WRAPPED_PRIVATE_EPOCH_FORMAT_V1);
        let restored = unwrap_private_epoch_material(
            &wrapped,
            fixture_persona(),
            fixture_epoch(),
            wrapping_key,
        )
        .unwrap();
        assert_eq!(restored, epoch_secret);
    }

    #[test]
    fn wrapped_private_epoch_rejects_wrong_key() {
        let wrapped =
            wrap_private_epoch_material(fixture_persona(), fixture_epoch(), b"secret", [9; 32])
                .unwrap();
        // A wrong key fails at the index before it reaches the AEAD, because
        // the index is blinded under that same key.
        let err =
            unwrap_private_epoch_material(&wrapped, fixture_persona(), fixture_epoch(), [7; 32])
                .unwrap_err();
        assert_eq!(err, WrappedEpochError::WrongEntry);
    }

    #[test]
    fn wrapped_private_epoch_rejects_unknown_format() {
        let material = WrappedEpochMaterial {
            index: blinded_epoch_index(fixture_persona(), fixture_epoch(), [9; 32]),
            wrap_format: "unknown".into(),
            wrapped_key: vec![1, 2, 3],
        };
        let err =
            unwrap_private_epoch_material(&material, fixture_persona(), fixture_epoch(), [9; 32])
                .unwrap_err();
        assert_eq!(
            err,
            WrappedEpochError::UnsupportedWrapFormat("unknown".into())
        );
    }

    #[test]
    fn wrapped_epoch_aad_binds_persona_and_epoch_identity() {
        let wrapping_key = [5; 32];
        let wrapped = wrap_private_epoch_material(
            fixture_persona(),
            fixture_epoch(),
            b"secret",
            wrapping_key,
        )
        .unwrap();
        // Ask for the same material under a different epoch. The index refuses
        // it first; the AEAD would have refused it too, which is the point of
        // keeping both checks.
        let err = unwrap_private_epoch_material(
            &wrapped,
            fixture_persona(),
            second_epoch(),
            wrapping_key,
        )
        .unwrap_err();
        assert_eq!(err, WrappedEpochError::WrongEntry);

        // And with the index forced to match, the AAD still binds: swapping the
        // epoch cannot survive decryption.
        let forged = WrappedEpochMaterial {
            index: blinded_epoch_index(fixture_persona(), second_epoch(), wrapping_key),
            wrap_format: wrapped.wrap_format.clone(),
            wrapped_key: wrapped.wrapped_key.clone(),
        };
        let err =
            unwrap_private_epoch_material(&forged, fixture_persona(), second_epoch(), wrapping_key)
                .unwrap_err();
        assert_eq!(err, WrappedEpochError::Decrypt);
    }

    /// The property the blinding exists for: the same persona and epoch, held
    /// by two devices, produce unlinkable entries.
    #[test]
    fn two_devices_blind_the_same_persona_differently() {
        let first = blinded_epoch_index(fixture_persona(), fixture_epoch(), [1; 32]);
        let second = blinded_epoch_index(fixture_persona(), fixture_epoch(), [2; 32]);
        assert_ne!(first, second);
    }

    #[test]
    fn a_blinded_index_is_stable_for_one_device() {
        assert_eq!(
            blinded_epoch_index(fixture_persona(), fixture_epoch(), [1; 32]),
            blinded_epoch_index(fixture_persona(), fixture_epoch(), [1; 32])
        );
    }

    #[test]
    fn the_index_separates_personas_and_epochs() {
        let key = [4; 32];
        let base = blinded_epoch_index(fixture_persona(), fixture_epoch(), key);
        assert_ne!(
            base,
            blinded_epoch_index(second_persona(), fixture_epoch(), key)
        );
        assert_ne!(
            base,
            blinded_epoch_index(fixture_persona(), second_epoch(), key)
        );
    }
}
