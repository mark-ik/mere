// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The signed grant envelope: canonical CBOR, issue and verify, content ref,
//! and the on-disk read/write for one device's grant.

use std::io;
use std::path::Path;

use p2panda_core::cbor::{decode_cbor, encode_cbor};

use identity::Ed25519Keypair;

use super::*;

pub(crate) fn encode_payload(payload: &DeviceGrantPayload) -> Result<Vec<u8>, DeviceGrantError> {
    encode_cbor(payload).map_err(|_| DeviceGrantError::Encode)
}

/// Canonical CBOR bytes of the signed grant envelope.
pub fn encode_signed_device_grant(grant: &SignedDeviceGrant) -> Result<Vec<u8>, DeviceGrantError> {
    encode_cbor(grant).map_err(|_| DeviceGrantError::Encode)
}

/// Decode a signed grant envelope from canonical CBOR bytes.
pub fn decode_signed_device_grant(bytes: &[u8]) -> Result<SignedDeviceGrant, DeviceGrantError> {
    decode_cbor(bytes).map_err(|_| DeviceGrantError::Decode)
}

/// Sign a device-grant payload with the delegator's Ed25519 keypair.
pub fn issue_device_grant(
    delegator: &Ed25519Keypair,
    payload: DeviceGrantPayload,
) -> Result<SignedDeviceGrant, DeviceGrantError> {
    let expected = DevicePublicKey::from(delegator.public_key());
    if payload.delegator_pubkey != expected {
        return Err(DeviceGrantError::DelegatorMismatch);
    }
    let bytes = encode_payload(&payload)?;
    Ok(SignedDeviceGrant {
        payload,
        signature: delegator.sign(&bytes).into(),
    })
}

/// Whether a device grant's signature verifies against its delegator key and
/// canonical CBOR payload bytes.
pub fn verify_device_grant(grant: &SignedDeviceGrant) -> Result<bool, DeviceGrantError> {
    let delegator: Ed25519PublicKey = grant
        .payload
        .delegator_pubkey
        .to_public_key()
        .map_err(|_| DeviceGrantError::InvalidDelegatorPublicKey)?;
    let signature = grant.signature.to_signature()?;
    let payload = encode_payload(&grant.payload)?;
    Ok(delegator.verify(&payload, &signature))
}

/// Stable content ref of the signed grant envelope bytes.
pub fn device_grant_ref(grant: &SignedDeviceGrant) -> Result<CarryRef, DeviceGrantError> {
    let bytes = encode_signed_device_grant(grant)?;
    Ok(CarryRef::of(bytes.as_slice()))
}

/// Load one signed device grant from `identity/grants/<device_id>.cbor`.
pub fn load_signed_device_grant(
    data_root: &Path,
    device_id: DeviceId,
) -> io::Result<Option<SignedDeviceGrant>> {
    match load_device_grant(data_root, device_id)? {
        Some(bytes) => decode_signed_device_grant(&bytes)
            .map(Some)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        None => Ok(None),
    }
}

/// Save one signed device grant to `identity/grants/<device_id>.cbor`, returning
/// the stable content hash callers can store in `grant_ref`.
pub fn save_signed_device_grant(
    data_root: &Path,
    grant: &SignedDeviceGrant,
) -> io::Result<CarryRef> {
    let bytes = encode_signed_device_grant(grant)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    save_device_grant(data_root, grant.payload.device_id, &bytes)?;
    device_grant_ref(grant).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The on-disk path of a signed grant for `device_id`.
pub fn signed_device_grant_path(data_root: &Path, device_id: DeviceId) -> std::path::PathBuf {
    device_grant_path(data_root, device_id)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;
    use super::*;

    #[test]
    fn issued_device_grant_verifies() {
        let grant = issue_device_grant(&delegator(), sample_payload()).unwrap();
        assert!(verify_device_grant(&grant).unwrap());
    }

    #[test]
    fn tampering_the_payload_breaks_verification() {
        let mut grant = issue_device_grant(&delegator(), sample_payload()).unwrap();
        grant.payload.scopes.push("transport.egress".into());
        assert!(!verify_device_grant(&grant).unwrap());
    }

    #[test]
    fn issue_rejects_a_payload_signed_by_the_wrong_delegator() {
        let payload = sample_payload();
        let wrong = InMemoryProvider::from_seed([8; 32])
            .derive_keypair(b"wallet-grant-wrong")
            .unwrap();
        let err = issue_device_grant(&wrong, payload).unwrap_err();
        assert_eq!(err, DeviceGrantError::DelegatorMismatch);
    }

    #[test]
    fn signed_device_grant_round_trips_through_cbor() {
        let grant = issue_device_grant(&delegator(), sample_payload()).unwrap();
        let bytes = encode_signed_device_grant(&grant).unwrap();
        let restored = decode_signed_device_grant(&bytes).unwrap();
        assert_eq!(restored, grant);
        assert!(verify_device_grant(&restored).unwrap());
    }

    /// The wire format of a signed grant, pinned byte for byte.
    ///
    /// Grants are handed between machines and stored on disk, so the encoding
    /// is a compatibility surface, not an implementation detail. This fixture
    /// is fully deterministic (fixed seeds, uuids, and timestamps; Ed25519
    /// signing is deterministic per RFC 8032), so any change to the codec has
    /// to come here and argue for itself.
    ///
    /// Recorded 2026-08-10 while ruling the fold-in plan's W3 question. The
    /// current encoder is `p2panda_core::cbor`, which is a thin wrapper over
    /// `ciborium::ser::into_writer` with no framing of its own, so swapping to
    /// a direct ciborium dependency would keep these exact bytes.
    #[test]
    fn signed_device_grant_wire_format_is_pinned() {
        let grant = issue_device_grant(&delegator(), sample_payload()).unwrap();
        let bytes = encode_signed_device_grant(&grant).unwrap();
        assert_eq!(hex::encode(&bytes), PINNED_SIGNED_GRANT_HEX);
    }

    const PINNED_SIGNED_GRANT_HEX: &str = concat!(
        "a2677061796c6f6164aa6e736368656d615f76657273696f6e01696465766963655f69645000",
        "00000000000000000000000000aaa17064656c656761746f725f7075626b65799820183a1878",
        "18ac18c1000e18a8182b18f118a518a5081874186718b112188e18341824187418b6183218a8",
        "18d418b7181f18c7184d1845186a183a18657064656c6567617465655f7075626b6579982018",
        "f21828186618f71873185718e5185a1890186218fe18711846188b18e0188c18e3184c18c318",
        "f3189a184618d418a018fd183818731832187f18c6188c184b6c6973737565645f61745f6d73",
        "1a6553f1016d657870697265735f61745f6d731a6b49d20168706572736f6e61738150000000",
        "0000000000000000000000aaa26673636f706573826c6964656e746974792e6163746c707269",
        "766174652e726561646c617474656e756174696f6e7381706e6f2d73756264656c6567617469",
        "6f6e76777261707065645f707269766174655f65706f63687381a46a706572736f6e615f6964",
        "500000000000000000000000000000aaa26865706f63685f6964500000000000000000000000",
        "000000aaa36b777261705f666f726d617474786368616368613230706f6c79313330352d7631",
        "6b777261707065645f6b65798418de18ad18be18ef697369676e6174757265984018e118a318",
        "d018180618b218b718e318b218d9121618be18e1182918bf1851185e187d1865188718711863",
        "181818691857182518c1187e186518411830184d18d418f5186918ce18b818e418c31837184e",
        "18cc18ae18f3181f183418b7188418e40d1824184c189d1835188e1897188118b8183c18ef18",
        "3f186705",
    );

    #[test]
    fn save_and_load_signed_device_grant_round_trip() {
        let root = temp_data_root("round-trip");
        let grant = issue_device_grant(&delegator(), sample_payload()).unwrap();
        let expected_ref = save_signed_device_grant(&root, &grant).unwrap();
        let restored = load_signed_device_grant(&root, fixture_device())
            .unwrap()
            .expect("grant file should exist");
        assert_eq!(restored, grant);
        assert_eq!(device_grant_ref(&restored).unwrap(), expected_ref);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn signed_grant_path_uses_the_existing_cbor_location() {
        let path = signed_device_grant_path(Path::new("/data"), fixture_device());
        assert_eq!(
            path,
            Path::new("/data")
                .join("identity")
                .join("grants")
                .join(format!("{}.cbor", fixture_device().as_uuid()))
        );
    }
}
