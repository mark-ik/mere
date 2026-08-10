// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Signed device-grant vocabulary for the wallet carry layer.
//!
//! This is the next slice after the wallet manifest store: a typed remote-auth
//! grant that lives under `identity/grants/<device_id>.cbor`, with canonical
//! CBOR bytes, a signed delegation payload, and verification helpers. Pairing
//! UX, wrapped-key generation, and revocation flow still layer on top.
//!
//! Split 2026-08-10 (wallet carry fold-in plan, W3). The envelope codec stays
//! here rather than moving into `personae::carry`: personae already owns a
//! delegation model, and a second one beside it would be duplication, not a
//! fold-in. See the plan's W3 ruling.

use std::collections::BTreeSet;
use std::fmt;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use identity::{
    Ed25519Keypair, Ed25519PublicKey, Ed25519Signature, IdentityProvider, InMemoryProvider,
};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::manifest::PersonaId;
use crate::wallet_store::{
    CapabilitySlotRef, CarryRef, DeviceExposure, DeviceGrantRef, DeviceId, DeviceMode, DevicePublicKey,
    DeviceRecord, DeviceRoster, IdentityWalletManifest, KeyEpochId, LocalDeviceIdentity,
    PersonaWalletManifest, PersonaWalletRef, RemoteAuthWrappingKeyBridge,
    RemoteAuthWrappingKeyRecord, device_grant_path, device_roster_ref, ensure_persona_epoch_bridge,
    load_current_private_epoch, load_device_grant, load_device_roster, load_identity_seed,
    load_identity_wallet, load_local_device_identity, load_persona_wallet,
    load_remote_auth_wrapping_key_bridge, save_device_grant, save_device_roster,
    save_identity_wallet, save_persona_wallet, save_remote_auth_wrapping_key_bridge,
    stage_persona_private_epoch,
};


mod enroll;
mod envelope;
mod epochs;
mod errors;
mod issue;
mod pairing;
mod records;
mod refresh;
mod revoke;
mod types;
mod validate;
mod wrapping;
#[cfg(test)]
mod test_support;

/// Current schema version for typed device grants.
pub const DEVICE_GRANT_SCHEMA_VERSION: u32 = 1;
/// Current wrap format for private-epoch material in remote-auth grants.
pub const WRAPPED_PRIVATE_EPOCH_FORMAT_V1: &str = "xchacha20poly1305-v1";
/// Pairing transcript context for deriving the remote-auth wrapping key.
pub const REMOTE_AUTH_PAIRING_WRAP_CONTEXT_V1: &str = "mere.wallet.remote-auth.wrap.v1";
/// Pairing transcript context for deriving the short auth string.
pub const REMOTE_AUTH_PAIRING_SAS_CONTEXT_V1: &str = "mere.wallet.remote-auth.sas.v1";
/// Schema version for QR / code transported remote-auth pairing tickets.
pub const REMOTE_AUTH_PAIRING_TICKET_SCHEMA_VERSION: u32 = 1;
/// Random secret bytes carried by a remote-auth pairing ticket.
pub const REMOTE_AUTH_PAIRING_SECRET_LEN: usize = 16;
/// Schema version for a typed remote-auth enrollment bundle.
pub const REMOTE_AUTH_ENROLLMENT_BUNDLE_SCHEMA_VERSION: u32 = 1;


pub(crate) fn unix_time_ms() -> io::Result<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| io::Error::other(format!("system clock before unix epoch: {err}")))?;
    u64::try_from(now.as_millis()).map_err(|_| io::Error::other("unix time overflowed u64"))
}

pub(crate) fn is_expired(expires_at_ms: Option<u64>, now_ms: u64) -> bool {
    matches!(expires_at_ms, Some(expires_at_ms) if expires_at_ms <= now_ms)
}

/// The capability-slot id a remote-auth device occupies in a persona wallet.
///
/// The `device-grant:` prefix is a persisted key: it is matched when slots are
/// upserted and when a revocation clears them, so changing it would orphan
/// every slot already written.
pub(crate) fn remote_auth_capability_slot_id(device_id: DeviceId) -> String {
    format!("device-grant:{}", device_id.as_uuid())
}

pub(crate) use enroll::{install_remote_auth_enrollment_bundle_inner, restore_wrapped_private_epochs};
pub(crate) use envelope::encode_payload;
pub(crate) use epochs::wrapped_epoch_aad;
pub(crate) use pairing::{derive_pairing_key_from_transcript, remote_auth_pairing_transcript};
pub(crate) use records::{
    upsert_grant_index, upsert_local_remote_auth_record, upsert_remote_auth_device_record,
};
pub(crate) use refresh::{
    refresh_remote_auth_private_read_grant, refresh_remote_auth_private_read_grants,
    upsert_persona_capability_slots,
};
pub(crate) use revoke::revoke_persona_grant_access;
pub(crate) use validate::{
    validate_paired_remote_auth_spec, validate_remote_auth_enrollment_bundle,
    validate_remote_auth_spec,
};
pub(crate) use wrapping::{
    load_remote_auth_wrapping_key, remove_remote_auth_wrapping_key, upsert_remote_auth_wrapping_key,
};

pub use enroll::{
    build_remote_auth_enrollment_bundle, decode_remote_auth_enrollment_bundle,
    encode_remote_auth_enrollment_bundle, install_remote_auth_enrollment_bundle,
    install_remote_auth_enrollment_bundle_with_wrapping_key,
};
pub use envelope::{
    decode_signed_device_grant, device_grant_ref, encode_signed_device_grant, issue_device_grant,
    load_signed_device_grant, save_signed_device_grant, signed_device_grant_path,
    verify_device_grant,
};
pub use epochs::{unwrap_private_epoch_material, wrap_private_epoch_material};
pub use errors::{
    DeviceGrantError, EnrollmentBundleError, PairingCodeError, PairingMaterialError,
    PairingTicketError, WrappedEpochError,
};
pub use issue::{
    issue_remote_auth_device_grant, issue_remote_auth_device_grant_from_pairing,
    issue_remote_auth_device_grant_from_ticket,
};
pub use pairing::{
    decode_remote_auth_pairing_ticket, derive_remote_auth_pairing_material,
    encode_remote_auth_pairing_ticket, format_remote_auth_pairing_code,
    mint_remote_auth_pairing_ticket, parse_remote_auth_pairing_code,
};
pub use revoke::revoke_remote_auth_device;
pub use types::{
    DeviceGrantPayload, DeviceGrantSignature, PairedRemoteAuthGrantSpec, PrivateEpochPlaintext,
    RemoteAuthEnrollmentBundle, RemoteAuthGrantSpec, RemoteAuthPairingMaterial,
    RemoteAuthPairingResponse, RemoteAuthPairingTicket, RemoteAuthPairingTicketRequest,
    RemoteAuthRevocationOutcome, SignedDeviceGrant, WrappedEpochMaterial,
};
