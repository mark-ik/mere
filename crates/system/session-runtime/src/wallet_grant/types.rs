// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The wire shapes: grant payloads and signatures, pairing tickets and
//! responses, enrollment bundles, and the specs the flows take as input.

use identity::carry::DeviceGrantSet;
use identity::delegation::SignedDelegationRevocation;

use super::WrappedEpochRecord;
use identity::{Ed25519Signature, PersonaId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    DEVICE_GRANT_SCHEMA_VERSION, DeviceExposure, DeviceGrantError, DeviceId, DevicePublicKey,
    KeyEpochId, PersonaWalletManifest, REMOTE_AUTH_PAIRING_SECRET_LEN,
};

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
    pub grant: DeviceGrantSet,
    /// The wrapped epoch records the grant's persona certificates authorise.
    ///
    /// Carried beside the certificates rather than inside them, so the
    /// delegatee receives key material without the capability statement having
    /// to be re-signed every time an epoch rotates.
    #[serde(default)]
    pub epochs: Vec<WrappedEpochRecord>,
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
    pub refreshed_devices: Vec<DeviceId>,
    /// The signed statements withdrawing every certificate the device held.
    ///
    /// Returned rather than only filed, because these are what travels: a
    /// caller hands them to peers, or to the radio itself if it can still be
    /// reached. The wallet's own copy is already folded into its ledger.
    pub statements: Vec<SignedDelegationRevocation>,
}
