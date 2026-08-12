// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wrapping and unwrapping the private-lane epoch material a delegated
//! device needs to read already-issued private content.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};

use identity::PersonaId;

use super::*;

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
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&wrapping_key));
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce_bytes),
            Payload {
                msg: epoch_secret,
                aad: &wrapped_epoch_aad(persona_id, epoch_id),
            },
        )
        .map_err(|_| WrappedEpochError::Encrypt)?;
    let mut wrapped_key = nonce_bytes.to_vec();
    wrapped_key.extend_from_slice(&ciphertext);
    Ok(WrappedEpochMaterial {
        persona_id,
        epoch_id,
        wrap_format: WRAPPED_PRIVATE_EPOCH_FORMAT_V1.into(),
        wrapped_key,
    })
}

/// Recover one wrapped private-epoch secret with the same wrapping key used to
/// produce it.
pub fn unwrap_private_epoch_material(
    material: &WrappedEpochMaterial,
    wrapping_key: [u8; 32],
) -> Result<Vec<u8>, WrappedEpochError> {
    if material.wrap_format != WRAPPED_PRIVATE_EPOCH_FORMAT_V1 {
        return Err(WrappedEpochError::UnsupportedWrapFormat(
            material.wrap_format.clone(),
        ));
    }
    if material.wrapped_key.len() < 24 {
        return Err(WrappedEpochError::InvalidWrappedKeyLength);
    }
    let (nonce_bytes, ciphertext) = material.wrapped_key.split_at(24);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&wrapping_key));
    cipher
        .decrypt(
            XNonce::from_slice(nonce_bytes),
            Payload {
                msg: ciphertext,
                aad: &wrapped_epoch_aad(material.persona_id, material.epoch_id),
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
        let restored = unwrap_private_epoch_material(&wrapped, wrapping_key).unwrap();
        assert_eq!(restored, epoch_secret);
    }

    #[test]
    fn wrapped_private_epoch_rejects_wrong_key() {
        let wrapped =
            wrap_private_epoch_material(fixture_persona(), fixture_epoch(), b"secret", [9; 32])
                .unwrap();
        let err = unwrap_private_epoch_material(&wrapped, [7; 32]).unwrap_err();
        assert_eq!(err, WrappedEpochError::Decrypt);
    }

    #[test]
    fn wrapped_private_epoch_rejects_unknown_format() {
        let material = WrappedEpochMaterial {
            persona_id: fixture_persona(),
            epoch_id: fixture_epoch(),
            wrap_format: "unknown".into(),
            wrapped_key: vec![1, 2, 3],
        };
        let err = unwrap_private_epoch_material(&material, [9; 32]).unwrap_err();
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
        let wrong_epoch = WrappedEpochMaterial {
            persona_id: fixture_persona(),
            epoch_id: second_epoch(),
            wrap_format: wrapped.wrap_format.clone(),
            wrapped_key: wrapped.wrapped_key.clone(),
        };
        let err = unwrap_private_epoch_material(&wrong_epoch, wrapping_key).unwrap_err();
        assert_eq!(err, WrappedEpochError::Decrypt);
    }
}
