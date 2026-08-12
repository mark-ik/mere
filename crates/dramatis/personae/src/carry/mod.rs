// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The carry model: the wallet record types a persona travels with.
//!
//! Folded in from mere's `session-runtime::wallet_store` per the 2026-07-08
//! founding ruling (executed 2026-08-10; see mere's wallet carry fold-in
//! plan). This module is the *model* only: record types, content refs, and
//! derivation. It knows no filesystem, reads no policy, and holds no seal
//! seam; mere's `session-runtime` keeps the store adapter (paths, sealed
//! records, bootstrap), and the epoch seal seam stays there because it
//! implements eidetic's `PayloadSealer`.
//!
//! # Content refs
//!
//! Wallet manifests address grants, rosters, and eidetic roots by content
//! hash. Here that is [`CarryRef`], a self-contained multihash-display
//! newtype (`blake3:<hex>`) whose serialized form is byte-identical to the
//! `eidetic::Hash` string form these records historically stored. The digest
//! is BLAKE3-256 computed by [`CarryRef::of`]; nothing on disk changed in the
//! fold-in, and a pinned-vector test on the mere side keeps the two reprs
//! honest.

use std::collections::BTreeMap;

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Ed25519Keypair, Ed25519PublicKey, IdentityError, IdentityProvider, InMemoryProvider, PersonaId,
};

/// Wallet schema version stamped into every carry record.
pub const WALLET_SCHEMA_VERSION: u32 = 1;

mod refs;

pub use refs::{CarryHashFn, CarryRef, CarryRefParseError};

// ── Identity-level records ───────────────────────────────────────────────────

/// Stable device id within the identity-level roster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub Uuid);

impl DeviceId {
    /// Mint a fresh device id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing uuid.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Borrow the underlying uuid.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for DeviceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Opaque key-epoch id for persona-private material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyEpochId(pub Uuid);

impl KeyEpochId {
    /// Mint a fresh epoch id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for KeyEpochId {
    fn default() -> Self {
        Self::new()
    }
}

/// Stored public key bytes for a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DevicePublicKey(pub [u8; 32]);

impl DevicePublicKey {
    /// Convert to the runtime identity type.
    pub fn to_public_key(self) -> Result<Ed25519PublicKey, IdentityError> {
        Ed25519PublicKey::from_bytes(&self.0)
    }
}

impl From<Ed25519PublicKey> for DevicePublicKey {
    fn from(value: Ed25519PublicKey) -> Self {
        Self(value.to_bytes())
    }
}

/// Stored chain-root bytes for a persona's standing lineage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PersonaChainRoot(pub [u8; 32]);

/// How a device was enrolled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceMode {
    /// Full seed copy. Revocation means master rotation.
    Copy,
    /// Delegated device with wrapped private-lane epoch material.
    #[default]
    RemoteAuth,
}

/// Whether a device is hidden behind another egress or intentionally exposed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceExposure {
    /// No public listener; dials outward only.
    #[default]
    HiddenClient,
    /// Reachable and allowed to serve as egress/availability anchor.
    ExposedEgress,
}

/// How identity recovery is expected to work for this wallet root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryPolicy {
    /// Seed recovery plus device handover are both valid v1 flows.
    #[default]
    SeedAndDeviceHandover,
    /// Seed phrase only; device handover disabled.
    SeedPhraseOnly,
}

/// One persona known to the identity-level wallet root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaWalletRef {
    /// The persona this wallet ref points at.
    pub persona_id: PersonaId,
}

/// One device grant tracked from the identity-level wallet root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceGrantRef {
    /// The granted device.
    pub device_id: DeviceId,
    /// Content ref of the stored signed grant, when one exists.
    #[serde(default)]
    pub grant_ref: Option<CarryRef>,
}

/// Identity-level wallet manifest: device fabric, recovery posture, and the known personas.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityWalletManifest {
    /// Wallet schema version stamped at write time.
    pub schema_version: u32,
    /// Content ref of the current device roster, when one exists.
    #[serde(default)]
    pub device_roster_ref: Option<CarryRef>,
    /// The recovery posture for this wallet root.
    #[serde(default)]
    pub recovery_policy: RecoveryPolicy,
    /// The personas known to this wallet root.
    #[serde(default)]
    pub personas: Vec<PersonaWalletRef>,
    /// Grant refs indexed by device.
    #[serde(default)]
    pub grant_index: Vec<DeviceGrantRef>,
}

impl Default for IdentityWalletManifest {
    fn default() -> Self {
        Self {
            schema_version: WALLET_SCHEMA_VERSION,
            device_roster_ref: None,
            recovery_policy: RecoveryPolicy::default(),
            personas: Vec::new(),
            grant_index: Vec::new(),
        }
    }
}

// ── Persona-level records ────────────────────────────────────────────────────

/// References into the encrypted lane for one persona.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivateRoots {
    /// The primary private eidetic root, when one exists.
    #[serde(default)]
    pub primary_root: Option<CarryRef>,
    /// Additional typed roots keyed by their logical role.
    #[serde(default)]
    pub typed_roots: BTreeMap<String, CarryRef>,
    /// Optional restore cursor / checkpoint for faster restore.
    #[serde(default)]
    pub restore_cursor: Option<CarryRef>,
}

/// References into the cleartext / public lane for one persona.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRoots {
    /// The primary public eidetic root, when one exists.
    #[serde(default)]
    pub primary_root: Option<CarryRef>,
    /// Additional typed roots keyed by their logical role.
    #[serde(default)]
    pub typed_roots: BTreeMap<String, CarryRef>,
}

/// One persona-scoped capability slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySlotRef {
    /// Stable slot identifier within the persona.
    pub slot_id: String,
    /// Content ref of the slot's grant, when one exists.
    #[serde(default)]
    pub grant_ref: Option<CarryRef>,
}

/// Persona-level wallet manifest: refs and epoch history for one persona.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaWalletManifest {
    /// Wallet schema version stamped at write time.
    pub schema_version: u32,
    /// The persona this manifest belongs to.
    pub persona_id: PersonaId,
    /// The persona's standing chain root.
    pub chain_root: PersonaChainRoot,
    /// The current private key epoch.
    pub private_epoch_head: KeyEpochId,
    /// Content ref of the sealed epoch history, when one exists.
    #[serde(default)]
    pub epoch_history_ref: Option<CarryRef>,
    /// References into the encrypted lane.
    #[serde(default)]
    pub private_roots: PrivateRoots,
    /// References into the cleartext / public lane.
    #[serde(default)]
    pub public_roots: PublicRoots,
    /// Persona-scoped capability slots.
    #[serde(default)]
    pub capability_slots: Vec<CapabilitySlotRef>,
}

impl PersonaWalletManifest {
    /// Build a new persona wallet manifest with empty refs and slots.
    pub fn new(
        persona_id: PersonaId,
        chain_root: PersonaChainRoot,
        private_epoch_head: KeyEpochId,
    ) -> Self {
        Self {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id,
            chain_root,
            private_epoch_head,
            epoch_history_ref: None,
            private_roots: PrivateRoots::default(),
            public_roots: PublicRoots::default(),
            capability_slots: Vec::new(),
        }
    }
}

/// One plaintext private epoch currently staged in the temporary host bridge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivateEpochRecord {
    /// The epoch this secret belongs to.
    pub epoch_id: KeyEpochId,
    /// The staged plaintext epoch secret.
    pub epoch_secret: Vec<u8>,
}

/// Transitional per-persona plaintext epoch bridge for pairing-time wrapping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaEpochBridge {
    /// Wallet schema version stamped at write time.
    pub schema_version: u32,
    /// The persona whose epochs are staged here.
    pub persona_id: PersonaId,
    /// The staged epochs, oldest first.
    #[serde(default)]
    pub epochs: Vec<PrivateEpochRecord>,
}

impl PersonaEpochBridge {
    /// Build an empty bridge for one persona under the current schema.
    pub fn new(persona_id: PersonaId) -> Self {
        Self {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id,
            epochs: Vec::new(),
        }
    }
}

// ── Device fabric ────────────────────────────────────────────────────────────

/// Identity-wide device roster.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRoster {
    /// Wallet schema version stamped at write time.
    pub schema_version: u32,
    /// The enrolled devices.
    #[serde(default)]
    pub devices: Vec<DeviceRecord>,
    /// Devices revoked from the fabric.
    #[serde(default)]
    pub revoked: Vec<DeviceId>,
}

impl DeviceRoster {
    /// Build an empty roster under the current schema.
    pub fn new() -> Self {
        Self {
            schema_version: WALLET_SCHEMA_VERSION,
            devices: Vec::new(),
            revoked: Vec::new(),
        }
    }
}

/// One enrolled device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRecord {
    /// The device's stable roster id.
    pub device_id: DeviceId,
    /// The device's public key.
    pub device_pubkey: DevicePublicKey,
    /// Human-readable device label.
    pub label: String,
    /// How the device was enrolled.
    pub mode: DeviceMode,
    /// The device's exposure posture.
    pub exposure: DeviceExposure,
    /// Content ref of the device's stored grant, when one exists.
    #[serde(default)]
    pub grant_ref: Option<CarryRef>,
}

/// The local host's delegated-device identity bridge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalDeviceIdentity {
    /// Wallet schema version stamped at write time.
    pub schema_version: u32,
    /// This host's device id.
    pub device_id: DeviceId,
    /// The delegated device's key seed.
    pub device_seed: [u8; 32],
    /// Human-readable device label.
    pub label: String,
}

impl LocalDeviceIdentity {
    /// Build a new local delegated-device identity from a fresh device id and seed.
    pub fn new(device_id: DeviceId, device_seed: [u8; 32], label: String) -> Self {
        Self {
            schema_version: WALLET_SCHEMA_VERSION,
            device_id,
            device_seed,
            label,
        }
    }

    /// Reconstruct the delegated device's keypair from the stored seed.
    pub fn keypair(&self) -> Ed25519Keypair {
        Ed25519Keypair::from_seed(self.device_seed)
    }

    /// The delegated device's public key.
    pub fn public_key(&self) -> DevicePublicKey {
        DevicePublicKey::from(self.keypair().public_key())
    }
}

/// One retained remote-auth wrapping key keyed by delegated device id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAuthWrappingKeyRecord {
    /// The delegated device this wrapping key serves.
    pub device_id: DeviceId,
    /// The pairing ticket that minted the key, when known.
    #[serde(default)]
    pub ticket_id: Option<Uuid>,
    /// The retained wrapping key.
    pub wrapping_key: [u8; 32],
}

/// Transitional identity-level bridge of remote-auth wrapping keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAuthWrappingKeyBridge {
    /// Wallet schema version stamped at write time.
    pub schema_version: u32,
    /// The retained wrapping keys.
    #[serde(default)]
    pub keys: Vec<RemoteAuthWrappingKeyRecord>,
}

impl RemoteAuthWrappingKeyBridge {
    /// Build an empty bridge under the current schema.
    pub fn new() -> Self {
        Self {
            schema_version: WALLET_SCHEMA_VERSION,
            keys: Vec::new(),
        }
    }
}

impl Default for RemoteAuthWrappingKeyBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ── Derivation ───────────────────────────────────────────────────────────────

/// The canonical persona derivation salt: `BLAKE3("persona" || persona_id)`.
pub fn persona_wallet_salt(persona: PersonaId) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(b"persona");
    hasher.update(persona.as_uuid().as_bytes());
    *hasher.finalize().as_bytes()
}

/// Derive the current persona chain root from the shared master seed.
pub fn derive_persona_chain_root(
    master_seed: [u8; 32],
    persona: PersonaId,
) -> Result<PersonaChainRoot, IdentityError> {
    let provider = InMemoryProvider::from_seed(master_seed);
    let keypair = provider.derive_keypair(&persona_wallet_salt(persona))?;
    Ok(PersonaChainRoot(keypair.public_key().to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // BLAKE3-256 of the empty input, from the BLAKE3 reference test vectors.
    const EMPTY_BLAKE3: &str = "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

    #[test]
    fn carry_ref_display_is_fn_colon_hex() {
        let r = CarryRef::of(b"");
        assert_eq!(r.to_string(), format!("blake3:{EMPTY_BLAKE3}"));
    }

    #[test]
    fn carry_ref_serializes_as_its_display_string() {
        let r = CarryRef::of(b"carry");
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, format!("\"{r}\""));
        let back: CarryRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn carry_ref_parses_its_own_display_form() {
        let r = CarryRef::of(b"round-trip");
        assert_eq!(r.to_string().parse::<CarryRef>().unwrap(), r);
    }

    #[test]
    fn carry_ref_rejects_malformed_strings() {
        assert!("no-colon".parse::<CarryRef>().is_err());
        assert!("sha256:00".parse::<CarryRef>().is_err());
        assert!(
            format!("blake3:{}", "0".repeat(63))
                .parse::<CarryRef>()
                .is_err()
        );
        assert!("blake3:zz".parse::<CarryRef>().is_err());
    }

    #[test]
    fn manifest_defaults_carry_the_schema_version() {
        let m = IdentityWalletManifest::default();
        assert_eq!(m.schema_version, WALLET_SCHEMA_VERSION);
        assert!(m.device_roster_ref.is_none());
    }

    #[test]
    fn manifest_json_round_trips_with_refs() {
        let mut m = IdentityWalletManifest::default();
        m.device_roster_ref = Some(CarryRef::of(b"roster"));
        m.grant_index.push(DeviceGrantRef {
            device_id: DeviceId::new(),
            grant_ref: Some(CarryRef::of(b"grant")),
        });
        let json = serde_json::to_string(&m).unwrap();
        let back: IdentityWalletManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn device_public_key_round_trips_through_identity_type() {
        let keypair = Ed25519Keypair::from_seed([7u8; 32]);
        let stored = DevicePublicKey::from(keypair.public_key());
        assert_eq!(stored.to_public_key().unwrap(), keypair.public_key());
    }
}
