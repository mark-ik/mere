/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Signed device-grant vocabulary for the wallet carry layer.
//!
//! This is the next slice after the wallet manifest store: a typed remote-auth
//! grant that lives under `identity/grants/<device_id>.cbor`, with canonical
//! CBOR bytes, a signed delegation payload, and verification helpers. Pairing
//! UX, wrapped-key generation, and revocation flow still layer on top.

use std::collections::BTreeSet;
use std::fmt;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use eidetic::Hash;
use identity::{
    Ed25519Keypair, Ed25519PublicKey, Ed25519Signature, IdentityProvider, InMemoryProvider,
};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::manifest::PersonaId;
use crate::wallet_store::{
    CapabilitySlotRef, DeviceExposure, DeviceGrantRef, DeviceId, DeviceMode, DevicePublicKey,
    DeviceRecord, DeviceRoster, IdentityWalletManifest, KeyEpochId, LocalDeviceIdentity,
    PersonaWalletManifest, PersonaWalletRef, device_grant_path, device_roster_ref,
    ensure_persona_epoch_bridge, load_device_grant, load_device_roster, load_identity_seed,
    load_identity_wallet, load_local_device_identity, load_persona_wallet, save_device_grant,
    save_device_roster, save_identity_wallet, save_persona_wallet, stage_persona_private_epoch,
};

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

fn unix_time_ms() -> io::Result<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| io::Error::other(format!("system clock before unix epoch: {err}")))?;
    u64::try_from(now.as_millis()).map_err(|_| io::Error::other("unix time overflowed u64"))
}

fn is_expired(expires_at_ms: Option<u64>, now_ms: u64) -> bool {
    matches!(expires_at_ms, Some(expires_at_ms) if expires_at_ms <= now_ms)
}

/// Wire/validation error for a signed device grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceGrantError {
    Encode,
    Decode,
    DelegatorMismatch,
    InvalidDelegatorPublicKey,
    InvalidSignatureLength,
}

impl fmt::Display for DeviceGrantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode => write!(f, "device grant CBOR encoding failed"),
            Self::Decode => write!(f, "device grant CBOR decoding failed"),
            Self::DelegatorMismatch => write!(f, "delegator keypair does not match payload"),
            Self::InvalidDelegatorPublicKey => {
                write!(f, "device grant carries invalid delegator public key bytes")
            }
            Self::InvalidSignatureLength => {
                write!(f, "device grant signature is not 64 bytes")
            }
        }
    }
}

impl std::error::Error for DeviceGrantError {}

/// Crypto/format error for wrapped private-epoch material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WrappedEpochError {
    UnsupportedWrapFormat(String),
    InvalidWrappedKeyLength,
    Encrypt,
    Decrypt,
}

impl fmt::Display for WrappedEpochError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedWrapFormat(format) => {
                write!(f, "unsupported wrapped private epoch format: {format}")
            }
            Self::InvalidWrappedKeyLength => {
                write!(
                    f,
                    "wrapped private epoch bytes are shorter than an XChaCha20 nonce"
                )
            }
            Self::Encrypt => write!(f, "private epoch wrap encryption failed"),
            Self::Decrypt => write!(f, "private epoch wrap decryption failed"),
        }
    }
}

impl std::error::Error for WrappedEpochError {}

/// Pairing-transcript derivation error for remote-auth grants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingMaterialError {
    EmptyPairingSecret,
}

impl fmt::Display for PairingMaterialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPairingSecret => write!(f, "pairing secret must not be empty"),
        }
    }
}

impl std::error::Error for PairingMaterialError {}

/// Encode/decode failure for remote-auth pairing tickets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingTicketError {
    Encode,
    Decode,
}

impl fmt::Display for PairingTicketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode => write!(f, "remote-auth pairing ticket encoding failed"),
            Self::Decode => write!(f, "remote-auth pairing ticket decoding failed"),
        }
    }
}

impl std::error::Error for PairingTicketError {}

/// Human entry failure for a formatted pairing code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingCodeError {
    InvalidLength,
    InvalidHex,
}

impl fmt::Display for PairingCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => write!(f, "pairing code does not carry 16 secret bytes"),
            Self::InvalidHex => write!(f, "pairing code contains non-hex digits"),
        }
    }
}

impl std::error::Error for PairingCodeError {}

/// Encode/decode failure for a remote-auth enrollment bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnrollmentBundleError {
    Encode,
    Decode,
}

impl fmt::Display for EnrollmentBundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode => write!(f, "remote-auth enrollment bundle encoding failed"),
            Self::Decode => write!(f, "remote-auth enrollment bundle decoding failed"),
        }
    }
}

impl std::error::Error for EnrollmentBundleError {}

/// One wrapped private-epoch bundle handed to a remote-auth device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedEpochMaterial {
    pub persona_id: PersonaId,
    pub epoch_id: KeyEpochId,
    /// The wrap algorithm/version tag. v1 keeps this stringly-typed so the
    /// storage seam can land before the concrete wrap mechanism hardens.
    pub wrap_format: String,
    /// Opaque wrapped key bytes. The pairing flow fills these later.
    pub wrapped_key: Vec<u8>,
}

/// The signed remote-auth grant payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceGrantPayload {
    pub schema_version: u32,
    pub device_id: DeviceId,
    pub delegator_pubkey: DevicePublicKey,
    pub delegatee_pubkey: DevicePublicKey,
    pub issued_at_ms: u64,
    #[serde(default)]
    pub expires_at_ms: Option<u64>,
    /// The persona set this device may act for.
    #[serde(default)]
    pub personas: Vec<PersonaId>,
    /// Named scope atoms. Examples: `identity.act`, `private.read`,
    /// `transport.egress`.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Named attenuation atoms. Examples: `no-subdelegation`,
    /// `sync-membership-only`.
    #[serde(default)]
    pub attenuations: Vec<String>,
    /// The wrapped private-lane epoch material the device needs to read
    /// already-issued private content without receiving the master seed.
    #[serde(default)]
    pub wrapped_private_epochs: Vec<WrappedEpochMaterial>,
}

impl DeviceGrantPayload {
    /// Build a remote-auth payload with no expiry, attenuation, or wrapped key
    /// material yet. The pairing flow fills those later.
    pub fn new_remote_auth(
        device_id: DeviceId,
        delegator_pubkey: DevicePublicKey,
        delegatee_pubkey: DevicePublicKey,
        issued_at_ms: u64,
    ) -> Self {
        Self {
            schema_version: DEVICE_GRANT_SCHEMA_VERSION,
            device_id,
            delegator_pubkey,
            delegatee_pubkey,
            issued_at_ms,
            expires_at_ms: None,
            personas: Vec::new(),
            scopes: Vec::new(),
            attenuations: Vec::new(),
            wrapped_private_epochs: Vec::new(),
        }
    }
}

/// Stored signature bytes over the canonical CBOR payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceGrantSignature(pub Vec<u8>);

impl DeviceGrantSignature {
    pub fn to_signature(&self) -> Result<Ed25519Signature, DeviceGrantError> {
        let bytes: [u8; 64] = self
            .0
            .as_slice()
            .try_into()
            .map_err(|_| DeviceGrantError::InvalidSignatureLength)?;
        Ok(Ed25519Signature::from_bytes(&bytes))
    }
}

impl From<Ed25519Signature> for DeviceGrantSignature {
    fn from(value: Ed25519Signature) -> Self {
        Self(value.to_bytes().to_vec())
    }
}

/// One signed device grant stored at `identity/grants/<device_id>.cbor`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedDeviceGrant {
    pub payload: DeviceGrantPayload,
    pub signature: DeviceGrantSignature,
}

/// Inputs for issuing and persisting one remote-auth device grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAuthGrantSpec {
    pub device_id: DeviceId,
    pub delegatee_pubkey: DevicePublicKey,
    pub label: String,
    pub exposure: DeviceExposure,
    pub issued_at_ms: u64,
    pub expires_at_ms: Option<u64>,
    pub personas: Vec<PersonaId>,
    pub scopes: Vec<String>,
    pub attenuations: Vec<String>,
    pub wrapped_private_epochs: Vec<WrappedEpochMaterial>,
}

/// One plaintext private-epoch bundle the delegator wraps during pairing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateEpochPlaintext {
    pub persona_id: PersonaId,
    pub epoch_id: KeyEpochId,
    pub epoch_secret: Vec<u8>,
}

/// Parameters the delegator chooses before showing a QR or typed pairing code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAuthPairingTicketRequest {
    pub issued_at_ms: u64,
    pub expires_at_ms: Option<u64>,
    pub personas: Vec<PersonaId>,
    pub scopes: Vec<String>,
    pub attenuations: Vec<String>,
}

/// QR / clipboard transported pairing ticket the new device scans before it
/// sends back its device identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAuthPairingTicket {
    pub schema_version: u32,
    pub ticket_id: Uuid,
    pub issued_at_ms: u64,
    #[serde(default)]
    pub expires_at_ms: Option<u64>,
    #[serde(default)]
    pub personas: Vec<PersonaId>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub attenuations: Vec<String>,
    pub pairing_secret: [u8; REMOTE_AUTH_PAIRING_SECRET_LEN],
}

/// The new device's response after scanning the ticket and minting its own
/// device identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAuthPairingResponse {
    pub device_id: DeviceId,
    pub delegatee_pubkey: DevicePublicKey,
    pub label: String,
    pub exposure: DeviceExposure,
}

/// Typed handoff bundle the delegator gives the delegatee after accepting a
/// remote-auth pairing response: the signed grant plus the persona wallet roots
/// that let the new device restore those personas' public state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAuthEnrollmentBundle {
    pub schema_version: u32,
    /// The pairing ticket this bundle completes, when the host wants the
    /// delegatee to recover the cached PAKE-derived wrapping key path.
    #[serde(default)]
    pub ticket_id: Option<Uuid>,
    pub grant: SignedDeviceGrant,
    #[serde(default)]
    pub persona_wallets: Vec<PersonaWalletManifest>,
}

/// A remote-auth issuance request where pairing still holds plaintext epoch
/// material and the shared PAKE secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairedRemoteAuthGrantSpec {
    pub device_id: DeviceId,
    pub delegatee_pubkey: DevicePublicKey,
    pub label: String,
    pub exposure: DeviceExposure,
    pub issued_at_ms: u64,
    pub expires_at_ms: Option<u64>,
    pub personas: Vec<PersonaId>,
    pub scopes: Vec<String>,
    pub attenuations: Vec<String>,
    pub pairing_secret: Vec<u8>,
    pub private_epochs: Vec<PrivateEpochPlaintext>,
}

/// Deterministic pairing-derived material both sides compute from the same
/// shared PAKE secret and transcript identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAuthPairingMaterial {
    pub wrapping_key: [u8; 32],
    pub short_auth_string: String,
}

/// Summary of one remote-auth device revocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAuthRevocationOutcome {
    pub device_id: DeviceId,
    pub already_revoked: bool,
    pub rotated_personas: Vec<PersonaId>,
}

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

/// Build the typed enrollment bundle a delegator hands to a newly accepted
/// remote-auth device.
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
    let grant = load_signed_device_grant(data_root, device_id)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("signed device grant missing for {}", device_id.as_uuid()),
        )
    })?;
    match verify_device_grant(&grant)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?
    {
        true => {}
        false => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "signed device grant failed signature verification",
            ));
        }
    }
    let mut persona_wallets = Vec::with_capacity(grant.payload.personas.len());
    for &persona in &grant.payload.personas {
        let wallet = load_persona_wallet(data_root, persona)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("persona wallet missing for {}", persona.as_uuid()),
            )
        })?;
        persona_wallets.push(wallet);
    }
    Ok(RemoteAuthEnrollmentBundle {
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

fn remote_auth_pairing_transcript(
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

fn derive_pairing_key_from_transcript(context: &str, transcript: &[u8]) -> [u8; 32] {
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

fn wrapped_epoch_aad(persona_id: PersonaId, epoch_id: KeyEpochId) -> Vec<u8> {
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

fn encode_payload(payload: &DeviceGrantPayload) -> Result<Vec<u8>, DeviceGrantError> {
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

/// Stable content hash of the signed grant envelope bytes.
pub fn device_grant_ref(grant: &SignedDeviceGrant) -> Result<Hash, DeviceGrantError> {
    let bytes = encode_signed_device_grant(grant)?;
    Ok(Hash::of(bytes.as_slice()))
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
pub fn save_signed_device_grant(data_root: &Path, grant: &SignedDeviceGrant) -> io::Result<Hash> {
    let bytes = encode_signed_device_grant(grant)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    save_device_grant(data_root, grant.payload.device_id, &bytes)?;
    device_grant_ref(grant).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The on-disk path of a signed grant for `device_id`.
pub fn signed_device_grant_path(data_root: &Path, device_id: DeviceId) -> std::path::PathBuf {
    device_grant_path(data_root, device_id)
}

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
    issue_remote_auth_device_grant_from_pairing(
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
    )
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

fn install_remote_auth_enrollment_bundle_inner(
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

    let grant_ref = save_signed_device_grant(data_root, &bundle.grant)?;
    for wallet in &bundle.persona_wallets {
        save_persona_wallet(data_root, wallet)?;
    }
    if !bundle.grant.payload.wrapped_private_epochs.is_empty() {
        let wrapping_key = wrapping_key.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "remote-auth enrollment bundle carries wrapped private epochs; install needs the pairing-derived wrapping key",
            )
        })?;
        restore_wrapped_private_epochs(data_root, &bundle.grant, wrapping_key)?;
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
        &bundle.grant.payload.personas,
        bundle.grant.payload.device_id,
        grant_ref,
    )?;
    Ok(())
}

/// Revoke one delegated remote-auth device, clear its active persona wallet
/// slots, and rotate future-write private epochs when the grant carried
/// `private.read`.
pub fn revoke_remote_auth_device(
    data_root: &Path,
    device_id: DeviceId,
) -> io::Result<RemoteAuthRevocationOutcome> {
    let grant = load_signed_device_grant(data_root, device_id)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("signed device grant missing for {}", device_id.as_uuid()),
        )
    })?;
    match verify_device_grant(&grant)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?
    {
        true => {}
        false => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "signed device grant failed signature verification",
            ));
        }
    }

    let mut roster = load_device_roster(data_root)?.unwrap_or_else(DeviceRoster::new);
    let device = roster
        .devices
        .iter()
        .find(|record| record.device_id == device_id)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("device roster missing {}", device_id.as_uuid()),
            )
        })?;
    if device.mode == DeviceMode::Copy {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "device {} is a copy-mode device; revoke by rotating the master root",
                device_id.as_uuid()
            ),
        ));
    }

    let already_revoked = roster.revoked.contains(&device_id);
    if !already_revoked {
        roster.revoked.push(device_id);
        save_device_roster(data_root, &roster)?;
    }
    let mut identity_wallet = load_identity_wallet(data_root)?.unwrap_or_default();
    identity_wallet.device_roster_ref = Some(device_roster_ref(&roster)?);
    save_identity_wallet(data_root, &identity_wallet)?;

    let rotated_personas = revoke_persona_grant_access(data_root, &grant)?;
    Ok(RemoteAuthRevocationOutcome {
        device_id,
        already_revoked,
        rotated_personas,
    })
}

fn restore_wrapped_private_epochs(
    data_root: &Path,
    grant: &SignedDeviceGrant,
    wrapping_key: [u8; 32],
) -> io::Result<()> {
    for wrapped in &grant.payload.wrapped_private_epochs {
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

fn remote_auth_capability_slot_id(device_id: DeviceId) -> String {
    format!("device-grant:{}", device_id.as_uuid())
}

fn revoke_persona_grant_access(
    data_root: &Path,
    grant: &SignedDeviceGrant,
) -> io::Result<Vec<PersonaId>> {
    let slot_id = remote_auth_capability_slot_id(grant.payload.device_id);
    let private_read = grant
        .payload
        .scopes
        .iter()
        .any(|scope| scope == "private.read");
    let mut rotated_personas = Vec::new();
    for &persona in &grant.payload.personas {
        let mut wallet = load_persona_wallet(data_root, persona)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("persona wallet missing for {}", persona.as_uuid()),
            )
        })?;
        let mut changed = false;
        if let Some(existing) = wallet
            .capability_slots
            .iter_mut()
            .find(|slot| slot.slot_id == slot_id)
        {
            if existing.grant_ref.take().is_some() {
                changed = true;
            }
        }

        let should_rotate = private_read
            && grant
                .payload
                .wrapped_private_epochs
                .iter()
                .filter(|epoch| epoch.persona_id == persona)
                .any(|epoch| wallet.private_epoch_head == epoch.epoch_id);
        let next_epoch = if should_rotate {
            let next_epoch = KeyEpochId::new();
            wallet.private_epoch_head = next_epoch;
            rotated_personas.push(persona);
            changed = true;
            Some(next_epoch)
        } else {
            None
        };

        if changed {
            save_persona_wallet(data_root, &wallet)?;
        }
        if let Some(next_epoch) = next_epoch {
            ensure_persona_epoch_bridge(data_root, persona, next_epoch)?;
        }
    }
    Ok(rotated_personas)
}

fn upsert_persona_capability_slots(
    data_root: &Path,
    personas: &[PersonaId],
    device_id: DeviceId,
    grant_ref: Hash,
) -> io::Result<()> {
    let slot_id = remote_auth_capability_slot_id(device_id);
    for &persona in personas {
        let mut wallet = load_persona_wallet(data_root, persona)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("persona wallet missing for {}", persona.as_uuid()),
            )
        })?;
        if let Some(existing) = wallet
            .capability_slots
            .iter_mut()
            .find(|slot| slot.slot_id == slot_id)
        {
            existing.grant_ref = Some(grant_ref);
        } else {
            wallet.capability_slots.push(CapabilitySlotRef {
                slot_id: slot_id.clone(),
                grant_ref: Some(grant_ref),
            });
        }
        save_persona_wallet(data_root, &wallet)?;
    }
    Ok(())
}

fn validate_remote_auth_spec(data_root: &Path, spec: &RemoteAuthGrantSpec) -> io::Result<()> {
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

fn validate_paired_remote_auth_spec(
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

fn validate_remote_auth_enrollment_bundle(
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

fn upsert_remote_auth_device_record(
    roster: &mut DeviceRoster,
    spec: &RemoteAuthGrantSpec,
    grant_ref: Hash,
) {
    if let Some(existing) = roster
        .devices
        .iter_mut()
        .find(|record| record.device_id == spec.device_id)
    {
        existing.device_pubkey = spec.delegatee_pubkey;
        existing.label = spec.label.clone();
        existing.mode = DeviceMode::RemoteAuth;
        existing.exposure = spec.exposure;
        existing.grant_ref = Some(grant_ref);
        return;
    }
    roster.devices.push(DeviceRecord {
        device_id: spec.device_id,
        device_pubkey: spec.delegatee_pubkey,
        label: spec.label.clone(),
        mode: DeviceMode::RemoteAuth,
        exposure: spec.exposure,
        grant_ref: Some(grant_ref),
    });
}

fn upsert_grant_index(wallet: &mut IdentityWalletManifest, device_id: DeviceId, grant_ref: Hash) {
    if let Some(existing) = wallet
        .grant_index
        .iter_mut()
        .find(|known| known.device_id == device_id)
    {
        existing.grant_ref = Some(grant_ref);
        return;
    }
    wallet.grant_index.push(DeviceGrantRef {
        device_id,
        grant_ref: Some(grant_ref),
    });
}

fn upsert_local_remote_auth_record(
    roster: &mut DeviceRoster,
    local: &LocalDeviceIdentity,
    grant_ref: Hash,
) {
    if let Some(existing) = roster
        .devices
        .iter_mut()
        .find(|record| record.device_id == local.device_id)
    {
        existing.device_pubkey = local.public_key();
        existing.label = local.label.clone();
        existing.mode = DeviceMode::RemoteAuth;
        existing.exposure = DeviceExposure::HiddenClient;
        existing.grant_ref = Some(grant_ref);
        return;
    }
    roster.devices.push(DeviceRecord {
        device_id: local.device_id,
        device_pubkey: local.public_key(),
        label: local.label.clone(),
        mode: DeviceMode::RemoteAuth,
        exposure: DeviceExposure::HiddenClient,
        grant_ref: Some(grant_ref),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::{IdentityProvider, InMemoryProvider};
    use uuid::Uuid;

    fn temp_data_root(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mere-wallet-grant-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn fixture_device() -> DeviceId {
        DeviceId::from_uuid(Uuid::from_u128(0xaaa1))
    }

    fn fixture_persona() -> PersonaId {
        PersonaId::from_uuid(Uuid::from_u128(0xaaa2))
    }

    fn fixture_epoch() -> KeyEpochId {
        KeyEpochId(Uuid::from_u128(0xaaa3))
    }

    fn second_epoch() -> KeyEpochId {
        KeyEpochId(Uuid::from_u128(0xaaa5))
    }

    fn second_persona() -> PersonaId {
        PersonaId::from_uuid(Uuid::from_u128(0xaaa4))
    }

    fn delegator() -> Ed25519Keypair {
        InMemoryProvider::from_seed([3; 32])
            .derive_keypair(b"wallet-grant-delegator")
            .unwrap()
    }

    fn delegatee() -> Ed25519Keypair {
        InMemoryProvider::from_seed([4; 32])
            .derive_keypair(b"wallet-grant-delegatee")
            .unwrap()
    }

    fn sample_payload() -> DeviceGrantPayload {
        let delegator = delegator();
        let delegatee = delegatee();
        let mut payload = DeviceGrantPayload::new_remote_auth(
            fixture_device(),
            DevicePublicKey::from(delegator.public_key()),
            DevicePublicKey::from(delegatee.public_key()),
            1_700_000_001,
        );
        payload.expires_at_ms = Some(1_800_000_001);
        payload.personas.push(fixture_persona());
        payload.scopes = vec!["identity.act".into(), "private.read".into()];
        payload.attenuations = vec!["no-subdelegation".into()];
        payload.wrapped_private_epochs.push(WrappedEpochMaterial {
            persona_id: fixture_persona(),
            epoch_id: fixture_epoch(),
            wrap_format: "xchacha20poly1305-v1".into(),
            wrapped_key: vec![0xde, 0xad, 0xbe, 0xef],
        });
        payload
    }

    fn sample_remote_auth_spec() -> RemoteAuthGrantSpec {
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
            wrapped_private_epochs: vec![WrappedEpochMaterial {
                persona_id: fixture_persona(),
                epoch_id: fixture_epoch(),
                wrap_format: "xchacha20poly1305-v1".into(),
                wrapped_key: vec![0xde, 0xad, 0xbe, 0xef],
            }],
        }
    }

    fn sample_paired_remote_auth_spec() -> PairedRemoteAuthGrantSpec {
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

    fn sample_pairing_ticket_request() -> RemoteAuthPairingTicketRequest {
        RemoteAuthPairingTicketRequest {
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(1_800_000_001),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
        }
    }

    fn sample_pairing_response() -> RemoteAuthPairingResponse {
        RemoteAuthPairingResponse {
            device_id: fixture_device(),
            delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
            label: "Pocket relay".into(),
            exposure: DeviceExposure::ExposedEgress,
        }
    }

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
    fn revoke_remote_auth_device_clears_slots_and_rotates_future_write_epochs() {
        let root = temp_data_root("remote-auth-revoke");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        crate::wallet_store::ensure_wallet_state(&root, second_persona(), "Studio PC").unwrap();

        let first_epoch = crate::wallet_store::load_current_private_epoch(&root, fixture_persona())
            .unwrap()
            .expect("first persona epoch bridge should exist");
        let second_epoch = crate::wallet_store::load_current_private_epoch(&root, second_persona())
            .unwrap()
            .expect("second persona epoch bridge should exist");
        let spec = RemoteAuthGrantSpec {
            device_id: fixture_device(),
            delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
            label: "Pocket relay".into(),
            exposure: DeviceExposure::ExposedEgress,
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona(), second_persona()],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
            wrapped_private_epochs: vec![
                wrap_private_epoch_material(
                    fixture_persona(),
                    first_epoch.epoch_id,
                    &first_epoch.epoch_secret,
                    [21; 32],
                )
                .unwrap(),
                wrap_private_epoch_material(
                    second_persona(),
                    second_epoch.epoch_id,
                    &second_epoch.epoch_secret,
                    [22; 32],
                )
                .unwrap(),
            ],
        };
        let grant = issue_remote_auth_device_grant(&root, &spec).unwrap();
        let grant_ref = device_grant_ref(&grant).unwrap();

        let outcome = revoke_remote_auth_device(&root, spec.device_id).unwrap();
        assert!(!outcome.already_revoked);
        assert_eq!(outcome.rotated_personas.len(), 2);
        assert!(outcome.rotated_personas.contains(&fixture_persona()));
        assert!(outcome.rotated_personas.contains(&second_persona()));

        let roster = crate::wallet_store::load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        assert!(roster.revoked.contains(&spec.device_id));

        let identity_wallet = crate::wallet_store::load_identity_wallet(&root)
            .unwrap()
            .expect("identity wallet should exist");
        assert_eq!(
            identity_wallet.device_roster_ref,
            Some(crate::wallet_store::device_roster_ref(&roster).unwrap())
        );
        assert!(
            identity_wallet.grant_index.iter().any(
                |known| known.device_id == spec.device_id && known.grant_ref == Some(grant_ref)
            )
        );

        for (persona, old_epoch) in [
            (fixture_persona(), first_epoch.epoch_id),
            (second_persona(), second_epoch.epoch_id),
        ] {
            let wallet = crate::wallet_store::load_persona_wallet(&root, persona)
                .unwrap()
                .expect("persona wallet should exist");
            assert_ne!(wallet.private_epoch_head, old_epoch);
            assert!(wallet.capability_slots.iter().any(|slot| {
                slot.slot_id == format!("device-grant:{}", spec.device_id.as_uuid())
                    && slot.grant_ref.is_none()
            }));

            let rotated = crate::wallet_store::load_current_private_epoch(&root, persona)
                .unwrap()
                .expect("rotated epoch should be staged");
            assert_eq!(rotated.epoch_id, wallet.private_epoch_head);
        }

        let err = build_remote_auth_enrollment_bundle(&root, spec.device_id).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn revoke_remote_auth_device_is_idempotent_once_rotation_has_landed() {
        let root = temp_data_root("remote-auth-revoke-idempotent");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let current_epoch =
            crate::wallet_store::load_current_private_epoch(&root, fixture_persona())
                .unwrap()
                .expect("epoch bridge should exist");
        let spec = RemoteAuthGrantSpec {
            device_id: fixture_device(),
            delegatee_pubkey: DevicePublicKey::from(delegatee().public_key()),
            label: "Pocket relay".into(),
            exposure: DeviceExposure::ExposedEgress,
            issued_at_ms: 1_700_000_001,
            expires_at_ms: Some(4_102_444_800_000),
            personas: vec![fixture_persona()],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
            wrapped_private_epochs: vec![
                wrap_private_epoch_material(
                    fixture_persona(),
                    current_epoch.epoch_id,
                    &current_epoch.epoch_secret,
                    [23; 32],
                )
                .unwrap(),
            ],
        };
        issue_remote_auth_device_grant(&root, &spec).unwrap();

        let first = revoke_remote_auth_device(&root, spec.device_id).unwrap();
        let rotated_head = crate::wallet_store::load_persona_wallet(&root, fixture_persona())
            .unwrap()
            .expect("persona wallet should exist")
            .private_epoch_head;
        let second = revoke_remote_auth_device(&root, spec.device_id).unwrap();
        let second_head = crate::wallet_store::load_persona_wallet(&root, fixture_persona())
            .unwrap()
            .expect("persona wallet should exist")
            .private_epoch_head;

        assert!(!first.already_revoked);
        assert_eq!(first.rotated_personas, vec![fixture_persona()]);
        assert!(second.already_revoked);
        assert!(second.rotated_personas.is_empty());
        assert_eq!(second_head, rotated_head);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn revoke_remote_auth_device_rejects_copy_mode_devices() {
        let root = temp_data_root("remote-auth-revoke-copy");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        let copy_device = DeviceId::new();
        let mut payload = sample_payload();
        payload.device_id = copy_device;
        let grant = issue_device_grant(&delegator(), payload).unwrap();
        save_signed_device_grant(&root, &grant).unwrap();

        let mut roster = crate::wallet_store::load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        roster.devices.push(DeviceRecord {
            device_id: copy_device,
            device_pubkey: DevicePublicKey::from(delegatee().public_key()),
            label: "Laptop clone".into(),
            mode: DeviceMode::Copy,
            exposure: DeviceExposure::ExposedEgress,
            grant_ref: None,
        });
        crate::wallet_store::save_device_roster(&root, &roster).unwrap();

        let err = revoke_remote_auth_device(&root, copy_device).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

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

    #[test]
    fn remote_auth_enrollment_bundle_round_trips_through_cbor() {
        let chain_root =
            crate::wallet_store::derive_persona_chain_root([21; 32], fixture_persona()).unwrap();
        let bundle = RemoteAuthEnrollmentBundle {
            schema_version: REMOTE_AUTH_ENROLLMENT_BUNDLE_SCHEMA_VERSION,
            ticket_id: Some(Uuid::from_u128(0xfeed)),
            grant: issue_device_grant(&delegator(), sample_payload()).unwrap(),
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

        let restored_grant = load_signed_device_grant(&delegatee_root, local.device_id)
            .unwrap()
            .expect("delegatee grant should persist");
        let grant_ref = device_grant_ref(&restored_grant).unwrap();
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
