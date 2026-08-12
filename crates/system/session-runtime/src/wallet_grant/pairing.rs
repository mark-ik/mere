// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Remote-auth pairing: minting and reading tickets, the human-typed code,
//! and the transcript-bound key material both sides derive independently.

use std::io;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use rand_core::{OsRng, RngCore};

use super::*;

/// Mint a new QR / typed-code pairing ticket for a remote-auth enrollment.
pub fn mint_remote_auth_pairing_ticket(
    request: &RemoteAuthPairingTicketRequest,
) -> RemoteAuthPairingTicket {
    let mut pairing_secret = [0u8; REMOTE_AUTH_PAIRING_SECRET_LEN];
    OsRng.fill_bytes(&mut pairing_secret);
    RemoteAuthPairingTicket {
        schema_version: REMOTE_AUTH_PAIRING_TICKET_SCHEMA_VERSION,
        ticket_id: Uuid::new_v4(),
        issued_at_ms: request.issued_at_ms,
        expires_at_ms: request.expires_at_ms,
        personas: request.personas.clone(),
        scopes: request.scopes.clone(),
        attenuations: request.attenuations.clone(),
        pairing_secret,
    }
}

/// Canonical CBOR bytes for a remote-auth pairing ticket (the QR payload path).
pub fn encode_remote_auth_pairing_ticket(
    ticket: &RemoteAuthPairingTicket,
) -> Result<Vec<u8>, PairingTicketError> {
    encode_cbor(ticket).map_err(|_| PairingTicketError::Encode)
}

/// Decode a remote-auth pairing ticket from canonical CBOR bytes.
pub fn decode_remote_auth_pairing_ticket(
    bytes: &[u8],
) -> Result<RemoteAuthPairingTicket, PairingTicketError> {
    decode_cbor(bytes).map_err(|_| PairingTicketError::Decode)
}

/// Render the ticket's shared secret as a grouped uppercase hex code for
/// manual entry when QR is unavailable.
pub fn format_remote_auth_pairing_code(secret: [u8; REMOTE_AUTH_PAIRING_SECRET_LEN]) -> String {
    let mut hex = String::with_capacity(REMOTE_AUTH_PAIRING_SECRET_LEN * 2 + 7);
    for (idx, byte) in secret.iter().enumerate() {
        if idx > 0 && idx % 2 == 0 {
            hex.push('-');
        }
        use std::fmt::Write;
        let _ = write!(&mut hex, "{:02X}", byte);
    }
    hex
}

/// Parse the grouped uppercase/lowercase hex pairing code back into the shared
/// secret bytes.
pub fn parse_remote_auth_pairing_code(
    code: &str,
) -> Result<[u8; REMOTE_AUTH_PAIRING_SECRET_LEN], PairingCodeError> {
    let compact: String = code.chars().filter(|c| *c != '-').collect();
    if compact.len() != REMOTE_AUTH_PAIRING_SECRET_LEN * 2 {
        return Err(PairingCodeError::InvalidLength);
    }
    let mut secret = [0u8; REMOTE_AUTH_PAIRING_SECRET_LEN];
    for (idx, chunk) in compact.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(chunk).map_err(|_| PairingCodeError::InvalidHex)?;
        secret[idx] = u8::from_str_radix(pair, 16).map_err(|_| PairingCodeError::InvalidHex)?;
    }
    Ok(secret)
}

pub(crate) fn remote_auth_pairing_transcript(
    pairing_secret: &[u8],
    delegator_pubkey: DevicePublicKey,
    delegatee_pubkey: DevicePublicKey,
    device_id: DeviceId,
) -> Result<Vec<u8>, PairingMaterialError> {
    if pairing_secret.is_empty() {
        return Err(PairingMaterialError::EmptyPairingSecret);
    }
    let mut transcript = Vec::with_capacity(pairing_secret.len() + 32 + 32 + 16 + 48);
    transcript.extend_from_slice(pairing_secret);
    transcript.extend_from_slice(b"delegator");
    transcript.extend_from_slice(&delegator_pubkey.0);
    transcript.extend_from_slice(b"delegatee");
    transcript.extend_from_slice(&delegatee_pubkey.0);
    transcript.extend_from_slice(b"device");
    transcript.extend_from_slice(device_id.as_uuid().as_bytes());
    Ok(transcript)
}

pub(crate) fn derive_pairing_key_from_transcript(context: &str, transcript: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(context);
    hasher.update(transcript);
    *hasher.finalize().as_bytes()
}

/// Derive the remote-auth wrapping key plus a decimal short auth string from
/// the shared pairing secret and transcript identities.
pub fn derive_remote_auth_pairing_material(
    pairing_secret: &[u8],
    delegator_pubkey: DevicePublicKey,
    delegatee_pubkey: DevicePublicKey,
    device_id: DeviceId,
) -> Result<RemoteAuthPairingMaterial, PairingMaterialError> {
    let transcript = remote_auth_pairing_transcript(
        pairing_secret,
        delegator_pubkey,
        delegatee_pubkey,
        device_id,
    )?;
    let wrapping_key =
        derive_pairing_key_from_transcript(REMOTE_AUTH_PAIRING_WRAP_CONTEXT_V1, &transcript);
    let sas_bytes =
        derive_pairing_key_from_transcript(REMOTE_AUTH_PAIRING_SAS_CONTEXT_V1, &transcript);
    let sas =
        u32::from_be_bytes([sas_bytes[0], sas_bytes[1], sas_bytes[2], sas_bytes[3]]) % 1_000_000;
    Ok(RemoteAuthPairingMaterial {
        wrapping_key,
        short_auth_string: format!("{sas:06}"),
    })
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;
    use super::*;

    #[test]
    fn pairing_material_is_deterministic() {
        let delegatee_pubkey = DevicePublicKey::from(delegatee().public_key());
        let delegator_pubkey = DevicePublicKey::from(delegator().public_key());
        let first = derive_remote_auth_pairing_material(
            b"shared-secret",
            delegator_pubkey,
            delegatee_pubkey,
            fixture_device(),
        )
        .unwrap();
        let second = derive_remote_auth_pairing_material(
            b"shared-secret",
            delegator_pubkey,
            delegatee_pubkey,
            fixture_device(),
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.short_auth_string.len(), 6);
    }

    #[test]
    fn pairing_material_changes_with_device_identity() {
        let delegatee_pubkey = DevicePublicKey::from(delegatee().public_key());
        let delegator_pubkey = DevicePublicKey::from(delegator().public_key());
        let first = derive_remote_auth_pairing_material(
            b"shared-secret",
            delegator_pubkey,
            delegatee_pubkey,
            fixture_device(),
        )
        .unwrap();
        let second = derive_remote_auth_pairing_material(
            b"shared-secret",
            delegator_pubkey,
            delegatee_pubkey,
            DeviceId::from_uuid(Uuid::from_u128(0xaaa6)),
        )
        .unwrap();
        assert_ne!(first.wrapping_key, second.wrapping_key);
        assert_ne!(first.short_auth_string, second.short_auth_string);
    }

    #[test]
    fn pairing_material_rejects_empty_secret() {
        let delegatee_pubkey = DevicePublicKey::from(delegatee().public_key());
        let delegator_pubkey = DevicePublicKey::from(delegator().public_key());
        let err = derive_remote_auth_pairing_material(
            b"",
            delegator_pubkey,
            delegatee_pubkey,
            fixture_device(),
        )
        .unwrap_err();
        assert_eq!(err, PairingMaterialError::EmptyPairingSecret);
    }

    #[test]
    fn pairing_ticket_round_trips_through_cbor() {
        let ticket = mint_remote_auth_pairing_ticket(&sample_pairing_ticket_request());
        let bytes = encode_remote_auth_pairing_ticket(&ticket).unwrap();
        let restored = decode_remote_auth_pairing_ticket(&bytes).unwrap();
        assert_eq!(restored, ticket);
    }

    #[test]
    fn pairing_code_round_trips() {
        let ticket = mint_remote_auth_pairing_ticket(&sample_pairing_ticket_request());
        let code = format_remote_auth_pairing_code(ticket.pairing_secret);
        let restored = parse_remote_auth_pairing_code(&code).unwrap();
        assert_eq!(restored, ticket.pairing_secret);
    }

    #[test]
    fn pairing_code_rejects_wrong_length() {
        let err = parse_remote_auth_pairing_code("ABCD-1234").unwrap_err();
        assert_eq!(err, PairingCodeError::InvalidLength);
    }

    #[test]
    fn pairing_code_rejects_non_hex_digits() {
        let err =
            parse_remote_auth_pairing_code("ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ").unwrap_err();
        assert_eq!(err, PairingCodeError::InvalidHex);
    }
}
