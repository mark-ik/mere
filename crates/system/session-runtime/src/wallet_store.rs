// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Identity-level and persona-level wallet storage for the carry layer.
//!
//! This is the first storage slice of the persona wallet plan:
//!
//! ```text
//! <data_root>/
//! ├── identity/
//! │   ├── wallet.json
//! │   ├── device-roster.json
//! │   └── grants/
//! │       └── <device_id>.cbor
//! └── personas/
//!     └── <persona_id>/
//!         ├── persona.json
//!         ├── wallet.json
//!         ├── vault/
//!         ├── settings/
//!         └── engine-profiles/
//! ```
//!
//! The split is deliberate:
//!
//! - `identity/` owns the one device fabric authored once for all personas.
//! - `personas/<id>/wallet.json` owns only persona-scoped refs and epoch history.
//!
//! This module owns the file layout and typed serde shapes. Pairing, wrapped-key
//! semantics, and transport resolution layer on top.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::{fs, io};

use blake3::Hasher;
use eidetic::Hash;
use identity::{
    Ed25519Keypair, Ed25519PublicKey, IdentityError, IdentityProvider, InMemoryProvider, PersonaId,
    SealedRecordStorage, StartupUnlockMode, load_or_create_auto_unlock_root,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engine_profile_store::PERSONAS_DIR;
use crate::settings_store;

/// Top-level directory holding identity-scoped wallet state.
pub const IDENTITY_DIR: &str = "identity";
/// Identity-level wallet manifest filename under [`IDENTITY_DIR`].
pub const IDENTITY_WALLET_FILENAME: &str = "wallet.json";
/// Persona-level wallet manifest filename under `personas/<id>/`.
pub const PERSONA_WALLET_FILENAME: &str = "wallet.json";
/// Device roster filename under [`IDENTITY_DIR`].
pub const DEVICE_ROSTER_FILENAME: &str = "device-roster.json";
/// Directory under [`IDENTITY_DIR`] holding per-device grant payloads.
pub const IDENTITY_GRANTS_DIR: &str = "grants";
/// Transitional master-seed bridge under [`IDENTITY_DIR`].
///
/// This keeps the host on one identity root until the passphrase-encrypted
/// vault handoff replaces the raw 32-byte seed file with a sealed local record.
pub const IDENTITY_SEED_FILENAME: &str = "master.seed";
/// Local auto-unlock root under [`IDENTITY_DIR`].
///
/// `StartupUnlockMode::AutoOs` uses this OS-protected local root to unlock
/// device-local sealed records such as `identity/master.seed`.
pub const IDENTITY_AUTO_UNLOCK_ROOT_FILENAME: &str = "vault-root.auto.json";
/// Transitional delegated-device identity bridge under [`IDENTITY_DIR`].
///
/// Remote-auth pairing needs a stable device id + keypair before the OS-keychain
/// backend exists. This file is still the typed host seam, but the local secret
/// bytes now ride the same sealed-record backend as the local seed.
pub const LOCAL_DEVICE_IDENTITY_FILENAME: &str = "local-device.json";
/// Transitional remote-auth wrapping-key bridge under [`IDENTITY_DIR`].
///
/// Pairing-backed delegated devices need a retained per-device wrapping key so
/// later private-epoch rotation can refresh their signed grant without rerunning
/// the whole pairing ceremony. This file is a temporary host seam that later
/// encrypted key history should replace.
pub const REMOTE_AUTH_WRAPPING_KEYS_FILENAME: &str = "remote-auth-wrapping-keys.json";
/// Transitional per-persona private-epoch bridge under `personas/<id>/`.
///
/// Remote-auth pairing needs plaintext private-epoch material to wrap into a
/// `private.read` grant before the encrypted-at-rest lane exists. This file is
/// still the typed host seam, but the secret bytes now migrate behind the
/// sealed-record backend while epoch-history storage remains in transition.
pub const PERSONA_EPOCH_BRIDGE_FILENAME: &str = "private-epoch-bridge.json";
/// Current schema version for wallet and roster files.
pub const WALLET_SCHEMA_VERSION: u32 = 1;

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

/// Opaque key-epoch id for persona-private material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyEpochId(pub Uuid);

impl KeyEpochId {
    /// Mint a fresh epoch id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
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
    pub persona_id: PersonaId,
}

/// One device grant tracked from the identity-level wallet root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceGrantRef {
    pub device_id: DeviceId,
    #[serde(default)]
    pub grant_ref: Option<Hash>,
}

/// Identity-level wallet manifest: device fabric, recovery posture, and the known personas.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityWalletManifest {
    pub schema_version: u32,
    #[serde(default)]
    pub device_roster_ref: Option<Hash>,
    #[serde(default)]
    pub recovery_policy: RecoveryPolicy,
    #[serde(default)]
    pub personas: Vec<PersonaWalletRef>,
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

/// References into the encrypted lane for one persona.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivateRoots {
    /// The primary private eidetic root, when one exists.
    #[serde(default)]
    pub primary_root: Option<Hash>,
    /// Additional typed roots keyed by their logical role.
    #[serde(default)]
    pub typed_roots: BTreeMap<String, Hash>,
    /// Optional restore cursor / checkpoint for faster restore.
    #[serde(default)]
    pub restore_cursor: Option<Hash>,
}

/// References into the cleartext / public lane for one persona.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRoots {
    /// The primary public eidetic root, when one exists.
    #[serde(default)]
    pub primary_root: Option<Hash>,
    /// Additional typed roots keyed by their logical role.
    #[serde(default)]
    pub typed_roots: BTreeMap<String, Hash>,
}

/// One persona-scoped capability slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySlotRef {
    pub slot_id: String,
    #[serde(default)]
    pub grant_ref: Option<Hash>,
}

/// Persona-level wallet manifest: refs and epoch history for one persona.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaWalletManifest {
    pub schema_version: u32,
    pub persona_id: PersonaId,
    pub chain_root: PersonaChainRoot,
    pub private_epoch_head: KeyEpochId,
    #[serde(default)]
    pub epoch_history_ref: Option<Hash>,
    #[serde(default)]
    pub private_roots: PrivateRoots,
    #[serde(default)]
    pub public_roots: PublicRoots,
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
    pub epoch_id: KeyEpochId,
    pub epoch_secret: Vec<u8>,
}

/// Transitional per-persona plaintext epoch bridge for pairing-time wrapping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaEpochBridge {
    pub schema_version: u32,
    pub persona_id: PersonaId,
    #[serde(default)]
    pub epochs: Vec<PrivateEpochRecord>,
}

impl PersonaEpochBridge {
    pub fn new(persona_id: PersonaId) -> Self {
        Self {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id,
            epochs: Vec::new(),
        }
    }
}

/// Identity-wide device roster.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRoster {
    pub schema_version: u32,
    #[serde(default)]
    pub devices: Vec<DeviceRecord>,
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
    pub device_id: DeviceId,
    pub device_pubkey: DevicePublicKey,
    pub label: String,
    pub mode: DeviceMode,
    pub exposure: DeviceExposure,
    #[serde(default)]
    pub grant_ref: Option<Hash>,
}

/// The local host's delegated-device identity bridge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalDeviceIdentity {
    pub schema_version: u32,
    pub device_id: DeviceId,
    pub device_seed: [u8; 32],
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
    pub device_id: DeviceId,
    #[serde(default)]
    pub ticket_id: Option<Uuid>,
    pub wrapping_key: [u8; 32],
}

/// Transitional identity-level bridge of remote-auth wrapping keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAuthWrappingKeyBridge {
    pub schema_version: u32,
    #[serde(default)]
    pub keys: Vec<RemoteAuthWrappingKeyRecord>,
}

impl RemoteAuthWrappingKeyBridge {
    pub fn new() -> Self {
        Self {
            schema_version: WALLET_SCHEMA_VERSION,
            keys: Vec::new(),
        }
    }
}

/// Which wallet bootstrap posture the current data root resolved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletBootstrapMode {
    /// A copy-style wallet root with a shared identity seed is live.
    CopySeeded,
    /// A sealed local secret exists, but the current startup-unlock mode left it
    /// locked at launch, so copy/delegated state must not be clobbered.
    Locked,
    /// A delegated-device identity exists locally, but enrollment has not yet
    /// installed persona wallet state.
    DelegatedPending,
    /// Delegated-device wallet state is already installed and must not be
    /// overwritten by copy-style seed bootstrap.
    DelegatedEnrolled,
}

/// `<data_root>/identity/`
pub fn identity_dir(data_root: &Path) -> PathBuf {
    data_root.join(IDENTITY_DIR)
}

/// `<data_root>/identity/wallet.json`
pub fn identity_wallet_path(data_root: &Path) -> PathBuf {
    identity_dir(data_root).join(IDENTITY_WALLET_FILENAME)
}

/// `<data_root>/identity/device-roster.json`
pub fn device_roster_path(data_root: &Path) -> PathBuf {
    identity_dir(data_root).join(DEVICE_ROSTER_FILENAME)
}

/// `<data_root>/identity/grants/`
pub fn identity_grants_dir(data_root: &Path) -> PathBuf {
    identity_dir(data_root).join(IDENTITY_GRANTS_DIR)
}

/// `<data_root>/identity/master.seed`
pub fn identity_seed_path(data_root: &Path) -> PathBuf {
    identity_dir(data_root).join(IDENTITY_SEED_FILENAME)
}

/// `<data_root>/identity/local-device.json`
pub fn local_device_identity_path(data_root: &Path) -> PathBuf {
    identity_dir(data_root).join(LOCAL_DEVICE_IDENTITY_FILENAME)
}

/// `<data_root>/identity/remote-auth-wrapping-keys.json`
pub fn remote_auth_wrapping_keys_path(data_root: &Path) -> PathBuf {
    identity_dir(data_root).join(REMOTE_AUTH_WRAPPING_KEYS_FILENAME)
}

/// `<data_root>/identity/vault-root.auto.json`
pub fn identity_auto_unlock_root_path(data_root: &Path) -> PathBuf {
    identity_dir(data_root).join(IDENTITY_AUTO_UNLOCK_ROOT_FILENAME)
}

fn io_backend_error(err: IdentityError) -> io::Error {
    io::Error::other(err.to_string())
}

fn runtime_manual_unlocks() -> &'static Mutex<HashSet<PathBuf>> {
    static MANUAL_UNLOCKS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    MANUAL_UNLOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn runtime_manual_unlock_active(data_root: &Path) -> bool {
    runtime_manual_unlocks()
        .lock()
        .map(|roots| roots.contains(data_root))
        .unwrap_or(false)
}

fn set_runtime_manual_unlock(data_root: &Path, active: bool) {
    let Ok(mut roots) = runtime_manual_unlocks().lock() else {
        return;
    };
    if active {
        roots.insert(data_root.to_path_buf());
    } else {
        roots.remove(data_root);
    }
}

fn wallet_startup_unlock_mode(data_root: &Path) -> StartupUnlockMode {
    settings_store::load_settings(data_root)
        .ok()
        .flatten()
        .map(|settings| settings.startup_unlock_mode)
        .unwrap_or_default()
}

fn looks_like_sealed_record(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    object.contains_key("version")
        && object.contains_key("nonce")
        && object.contains_key("ciphertext")
}

fn wallet_local_secret_store(data_root: &Path) -> io::Result<Option<SealedRecordStorage>> {
    if runtime_manual_unlock_active(data_root) {
        return load_or_create_auto_unlock_root(identity_auto_unlock_root_path(data_root))
            .map(|root| root.map(|key| SealedRecordStorage::open_with_key(data_root, key)))
            .map_err(io_backend_error);
    }
    match wallet_startup_unlock_mode(data_root) {
        StartupUnlockMode::AutoOs => {
            load_or_create_auto_unlock_root(identity_auto_unlock_root_path(data_root))
                .map(|root| root.map(|key| SealedRecordStorage::open_with_key(data_root, key)))
                .map_err(io_backend_error)
        }
        StartupUnlockMode::Prompt | StartupUnlockMode::Locked => Ok(None),
    }
}

fn wallet_secret_record_root_key(seed: [u8; 32]) -> [u8; 32] {
    blake3::derive_key(
        "mere.session_runtime.wallet_store.secret_records.v1",
        seed.as_slice(),
    )
}

fn wallet_secret_store_from_seed(data_root: &Path, seed: [u8; 32]) -> SealedRecordStorage {
    SealedRecordStorage::open_with_key(data_root, wallet_secret_record_root_key(seed))
}

fn wallet_secret_store(data_root: &Path) -> io::Result<Option<SealedRecordStorage>> {
    Ok(load_identity_seed(data_root)?.map(|seed| wallet_secret_store_from_seed(data_root, seed)))
}

fn wallet_persona_secret_store(data_root: &Path) -> io::Result<Option<SealedRecordStorage>> {
    if let Some(store) = wallet_secret_store(data_root)? {
        return Ok(Some(store));
    }
    if load_local_device_identity(data_root)?.is_some() {
        return wallet_local_secret_store(data_root);
    }
    Ok(None)
}

/// `<data_root>/identity/grants/<device_id>.cbor`
pub fn device_grant_path(data_root: &Path, device_id: DeviceId) -> PathBuf {
    identity_grants_dir(data_root).join(format!("{}.cbor", device_id.as_uuid()))
}

/// `<data_root>/personas/<persona_id>/wallet.json`
pub fn persona_wallet_path(data_root: &Path, persona: PersonaId) -> PathBuf {
    data_root
        .join(PERSONAS_DIR)
        .join(persona.as_uuid().to_string())
        .join(PERSONA_WALLET_FILENAME)
}

/// `<data_root>/personas/<persona_id>/private-epoch-bridge.json`
pub fn persona_epoch_bridge_path(data_root: &Path, persona: PersonaId) -> PathBuf {
    data_root
        .join(PERSONAS_DIR)
        .join(persona.as_uuid().to_string())
        .join(PERSONA_EPOCH_BRIDGE_FILENAME)
}

/// Load the identity wallet manifest, or `None` when absent.
pub fn load_identity_wallet(data_root: &Path) -> io::Result<Option<IdentityWalletManifest>> {
    load_json_optional(&identity_wallet_path(data_root))
}

/// Save the identity wallet manifest atomically.
pub fn save_identity_wallet(data_root: &Path, wallet: &IdentityWalletManifest) -> io::Result<()> {
    save_json_atomic(&identity_wallet_path(data_root), wallet)
}

/// Load the device roster, or `None` when absent.
pub fn load_device_roster(data_root: &Path) -> io::Result<Option<DeviceRoster>> {
    load_json_optional(&device_roster_path(data_root))
}

/// Save the device roster atomically.
pub fn save_device_roster(data_root: &Path, roster: &DeviceRoster) -> io::Result<()> {
    save_json_atomic(&device_roster_path(data_root), roster)
}

/// Stable content hash of a device roster's on-disk JSON bytes.
pub fn device_roster_ref(roster: &DeviceRoster) -> io::Result<Hash> {
    let bytes = json_pretty_bytes(roster)?;
    Ok(Hash::of(bytes.as_slice()))
}

/// Load one persona wallet manifest, or `None` when absent.
pub fn load_persona_wallet(
    data_root: &Path,
    persona: PersonaId,
) -> io::Result<Option<PersonaWalletManifest>> {
    load_json_optional(&persona_wallet_path(data_root, persona))
}

/// Save one persona wallet manifest atomically.
pub fn save_persona_wallet(data_root: &Path, wallet: &PersonaWalletManifest) -> io::Result<()> {
    save_json_atomic(&persona_wallet_path(data_root, wallet.persona_id), wallet)
}

/// Load one persona epoch bridge, or `None` when absent.
pub fn load_persona_epoch_bridge(
    data_root: &Path,
    persona: PersonaId,
) -> io::Result<Option<PersonaEpochBridge>> {
    let path = persona_epoch_bridge_path(data_root, persona);
    if !path.is_file() {
        return Ok(None);
    }
    let relative = path
        .strip_prefix(data_root)
        .expect("wallet path under data root");
    let secret_store = wallet_persona_secret_store(data_root)?;
    if let Some(store) = &secret_store {
        match store.load_record::<PersonaEpochBridge>(relative) {
            Ok(bridge) => return Ok(bridge),
            Err(_) => {}
        }
    }
    if secret_store.is_none() && looks_like_sealed_record(&path) {
        return Ok(None);
    }
    let legacy = load_json_optional(&path)?;
    if let (Some(bridge), Some(_)) = (&legacy, &secret_store) {
        save_persona_epoch_bridge(data_root, bridge)?;
    }
    Ok(legacy)
}

/// Save one persona epoch bridge atomically.
pub fn save_persona_epoch_bridge(data_root: &Path, bridge: &PersonaEpochBridge) -> io::Result<()> {
    let path = persona_epoch_bridge_path(data_root, bridge.persona_id);
    if let Some(store) = wallet_persona_secret_store(data_root)? {
        return store
            .save_record(
                path.strip_prefix(data_root)
                    .expect("wallet path under data root"),
                bridge,
            )
            .map_err(io_backend_error);
    }
    save_json_atomic(&path, bridge)
}

/// Load the opaque grant payload for one device, or `None` when absent.
pub fn load_device_grant(data_root: &Path, device_id: DeviceId) -> io::Result<Option<Vec<u8>>> {
    let path = device_grant_path(data_root, device_id);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Save the opaque grant payload for one device atomically.
pub fn save_device_grant(data_root: &Path, device_id: DeviceId, bytes: &[u8]) -> io::Result<()> {
    let path = device_grant_path(data_root, device_id);
    save_bytes_atomic(&path, bytes)
}

/// Load the shared master seed, or `None` when the bridge file is absent.
pub fn load_identity_seed(data_root: &Path) -> io::Result<Option<[u8; 32]>> {
    let path = identity_seed_path(data_root);
    if !path.is_file() {
        return Ok(None);
    }
    let relative = path
        .strip_prefix(data_root)
        .expect("wallet path under data root");
    let local_store = wallet_local_secret_store(data_root)?;
    if let Some(store) = &local_store {
        match store.load_record::<[u8; 32]>(relative) {
            Ok(Some(seed)) => return Ok(Some(seed)),
            Ok(None) => return Ok(None),
            Err(_) => {}
        }
    }
    if local_store.is_none() && looks_like_sealed_record(&path) {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let seed = <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("identity seed at {path:?} is neither a sealed record nor 32 raw bytes"),
        )
    })?;
    if local_store.is_some() {
        save_identity_seed(data_root, seed)?;
    }
    Ok(Some(seed))
}

/// Whether the startup seed record exists but is currently inaccessible because
/// the install stayed locked at launch.
pub fn identity_seed_locked_at_startup(data_root: &Path) -> bool {
    let path = identity_seed_path(data_root);
    path.is_file()
        && looks_like_sealed_record(&path)
        && load_identity_seed(data_root).ok().flatten().is_none()
}

/// Whether any device-local sealed wallet secret is still unavailable this launch.
pub fn wallet_local_secrets_locked(data_root: &Path) -> bool {
    identity_seed_locked_at_startup(data_root) || local_device_identity_locked_at_startup(data_root)
}

/// Explicitly unlock device-local sealed wallet secrets with the local OS store for this
/// session only, without changing the persisted startup policy.
pub fn unlock_wallet_with_auto_os(data_root: &Path) -> io::Result<bool> {
    let Some(_) = load_or_create_auto_unlock_root(identity_auto_unlock_root_path(data_root))
        .map_err(io_backend_error)?
    else {
        return Ok(false);
    };
    set_runtime_manual_unlock(data_root, true);
    let unlocked = load_identity_seed(data_root)?.is_some()
        || load_local_device_identity(data_root)?.is_some();
    if !unlocked {
        set_runtime_manual_unlock(data_root, false);
    }
    Ok(unlocked)
}

/// Clear the session-scoped explicit unlock override so later secret loads respect the
/// persisted startup policy again.
pub fn relock_wallet_after_manual_unlock(data_root: &Path) {
    set_runtime_manual_unlock(data_root, false);
}

/// Save the shared master seed atomically.
pub fn save_identity_seed(data_root: &Path, seed: [u8; 32]) -> io::Result<()> {
    let path = identity_seed_path(data_root);
    if let Some(store) = wallet_local_secret_store(data_root)? {
        return store
            .save_record(
                path.strip_prefix(data_root)
                    .expect("wallet path under data root"),
                &seed,
            )
            .map_err(io_backend_error);
    }
    save_bytes_atomic(&path, &seed)
}

/// Load the local delegated-device identity, or `None` when absent.
pub fn load_local_device_identity(data_root: &Path) -> io::Result<Option<LocalDeviceIdentity>> {
    let path = local_device_identity_path(data_root);
    if !path.is_file() {
        return Ok(None);
    }
    let relative = path
        .strip_prefix(data_root)
        .expect("wallet path under data root");
    let local_store = wallet_local_secret_store(data_root)?;
    if let Some(store) = &local_store {
        match store.load_record::<LocalDeviceIdentity>(relative) {
            Ok(identity) => return Ok(identity),
            Err(_) => {}
        }
    }
    if local_store.is_none() && looks_like_sealed_record(&path) {
        return Ok(None);
    }
    let legacy = load_json_optional(&path)?;
    if let (Some(identity), Some(_)) = (&legacy, &local_store) {
        save_local_device_identity(data_root, identity)?;
    }
    Ok(legacy)
}

fn local_device_identity_locked_at_startup(data_root: &Path) -> bool {
    let path = local_device_identity_path(data_root);
    path.is_file()
        && looks_like_sealed_record(&path)
        && load_local_device_identity(data_root)
            .ok()
            .flatten()
            .is_none()
}

/// Save the local delegated-device identity atomically.
pub fn save_local_device_identity(
    data_root: &Path,
    identity: &LocalDeviceIdentity,
) -> io::Result<()> {
    let path = local_device_identity_path(data_root);
    if let Some(store) = wallet_local_secret_store(data_root)? {
        return store
            .save_record(
                path.strip_prefix(data_root)
                    .expect("wallet path under data root"),
                identity,
            )
            .map_err(io_backend_error);
    }
    save_json_atomic(&path, identity)
}

/// Load the retained remote-auth wrapping-key bridge, or `None` when absent.
pub fn load_remote_auth_wrapping_key_bridge(
    data_root: &Path,
) -> io::Result<Option<RemoteAuthWrappingKeyBridge>> {
    let path = remote_auth_wrapping_keys_path(data_root);
    if !path.is_file() {
        return Ok(None);
    }
    if let Some(store) = wallet_secret_store(data_root)? {
        match store.load_record(
            path.strip_prefix(data_root)
                .expect("wallet path under data root"),
        ) {
            Ok(bridge) => return Ok(bridge),
            Err(err) => {
                if let Some(bridge) = load_json_optional(&path)? {
                    save_remote_auth_wrapping_key_bridge(data_root, &bridge)?;
                    return Ok(Some(bridge));
                }
                return Err(io_backend_error(err));
            }
        }
    }
    if looks_like_sealed_record(&path) {
        return Ok(None);
    }
    load_json_optional(&path)
}

/// Save the retained remote-auth wrapping-key bridge atomically.
pub fn save_remote_auth_wrapping_key_bridge(
    data_root: &Path,
    bridge: &RemoteAuthWrappingKeyBridge,
) -> io::Result<()> {
    let store = wallet_secret_store(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "identity seed missing; cannot seal remote-auth wrapping keys",
        )
    })?;
    let path = remote_auth_wrapping_keys_path(data_root);
    store
        .save_record(
            path.strip_prefix(data_root)
                .expect("wallet path under data root"),
            bridge,
        )
        .map_err(io_backend_error)
}

/// Ensure a stable delegated-device identity exists for this data root.
pub fn ensure_local_device_identity(
    data_root: &Path,
    device_label: &str,
) -> io::Result<LocalDeviceIdentity> {
    if let Some(identity) = load_local_device_identity(data_root)? {
        return Ok(identity);
    }
    let seed = InMemoryProvider::random().master_keypair().to_seed();
    let identity = LocalDeviceIdentity::new(DeviceId::new(), seed, device_label.to_string());
    save_local_device_identity(data_root, &identity)?;
    Ok(identity)
}

/// Ensure the temporary plaintext epoch bridge contains `epoch_id`.
pub fn ensure_persona_epoch_bridge(
    data_root: &Path,
    persona: PersonaId,
    epoch_id: KeyEpochId,
) -> io::Result<PersonaEpochBridge> {
    let mut bridge = load_persona_epoch_bridge(data_root, persona)?
        .unwrap_or_else(|| PersonaEpochBridge::new(persona));
    if !bridge.epochs.iter().any(|epoch| epoch.epoch_id == epoch_id) {
        bridge.epochs.push(PrivateEpochRecord {
            epoch_id,
            epoch_secret: InMemoryProvider::random()
                .master_keypair()
                .to_seed()
                .to_vec(),
        });
        save_persona_epoch_bridge(data_root, &bridge)?;
    }
    Ok(bridge)
}

/// Stage a known plaintext private epoch in the temporary host bridge.
pub fn stage_persona_private_epoch(
    data_root: &Path,
    persona: PersonaId,
    epoch_id: KeyEpochId,
    epoch_secret: &[u8],
) -> io::Result<PersonaEpochBridge> {
    let mut bridge = load_persona_epoch_bridge(data_root, persona)?
        .unwrap_or_else(|| PersonaEpochBridge::new(persona));
    let mut changed = false;
    match bridge
        .epochs
        .iter_mut()
        .find(|epoch| epoch.epoch_id == epoch_id)
    {
        Some(existing) => {
            if existing.epoch_secret != epoch_secret {
                existing.epoch_secret = epoch_secret.to_vec();
                changed = true;
            }
        }
        None => {
            bridge.epochs.push(PrivateEpochRecord {
                epoch_id,
                epoch_secret: epoch_secret.to_vec(),
            });
            changed = true;
        }
    }
    if changed {
        save_persona_epoch_bridge(data_root, &bridge)?;
    }
    Ok(bridge)
}

/// Load the current plaintext private epoch for `persona`, if the temporary
/// host bridge has one matching the wallet's `private_epoch_head`.
pub fn load_current_private_epoch(
    data_root: &Path,
    persona: PersonaId,
) -> io::Result<Option<PrivateEpochRecord>> {
    let Some(wallet) = load_persona_wallet(data_root, persona)? else {
        return Ok(None);
    };
    let Some(bridge) = load_persona_epoch_bridge(data_root, persona)? else {
        return Ok(None);
    };
    Ok(bridge
        .epochs
        .into_iter()
        .find(|epoch| epoch.epoch_id == wallet.private_epoch_head))
}

/// Bootstrap wallet state without clobbering a delegated-device install.
///
/// Fresh roots still seed the copy-style identity bridge. Once a delegated
/// device has either minted its local identity or installed a remote-auth
/// enrollment bundle, startup must preserve that state instead of creating
/// `identity/master.seed` and a copy-mode roster entry.
pub fn bootstrap_wallet_state(
    data_root: &Path,
    persona: PersonaId,
    device_label: &str,
) -> io::Result<WalletBootstrapMode> {
    if load_identity_seed(data_root)?.is_some() {
        ensure_wallet_state(data_root, persona, device_label)?;
        return Ok(WalletBootstrapMode::CopySeeded);
    }
    if identity_seed_locked_at_startup(data_root) {
        return Ok(WalletBootstrapMode::Locked);
    }
    if load_identity_wallet(data_root)?.is_some() {
        return Ok(WalletBootstrapMode::DelegatedEnrolled);
    }
    if load_local_device_identity(data_root)?.is_some() {
        return Ok(WalletBootstrapMode::DelegatedPending);
    }
    if local_device_identity_locked_at_startup(data_root) {
        return Ok(WalletBootstrapMode::Locked);
    }
    ensure_wallet_state(data_root, persona, device_label)?;
    Ok(WalletBootstrapMode::CopySeeded)
}

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

/// Ensure the shared identity root and one persona wallet exist, seeding the
/// minimal carry-layer files on first launch.
///
/// The bridge seed is currently a raw 32-byte file under `identity/`; the
/// encrypted persona vault remains the follow-up that supersedes it.
pub fn ensure_wallet_state(
    data_root: &Path,
    persona: PersonaId,
    device_label: &str,
) -> io::Result<[u8; 32]> {
    let seed = match load_identity_seed(data_root)? {
        Some(seed) => seed,
        None => {
            let seed = InMemoryProvider::random().master_keypair().to_seed();
            save_identity_seed(data_root, seed)?;
            seed
        }
    };

    let mut identity_wallet = load_identity_wallet(data_root)?.unwrap_or_default();
    if !identity_wallet
        .personas
        .iter()
        .any(|known| known.persona_id == persona)
    {
        identity_wallet.personas.push(PersonaWalletRef {
            persona_id: persona,
        });
    }

    let roster = match load_device_roster(data_root)? {
        Some(roster) => roster,
        None => {
            let provider = InMemoryProvider::from_seed(seed);
            let roster = DeviceRoster {
                schema_version: WALLET_SCHEMA_VERSION,
                devices: vec![DeviceRecord {
                    device_id: DeviceId::new(),
                    device_pubkey: DevicePublicKey::from(provider.master_public_key()),
                    label: device_label.to_string(),
                    mode: DeviceMode::Copy,
                    exposure: DeviceExposure::HiddenClient,
                    grant_ref: None,
                }],
                revoked: Vec::new(),
            };
            save_device_roster(data_root, &roster)?;
            roster
        }
    };

    let persona_wallet = match load_persona_wallet(data_root, persona)? {
        Some(wallet) => wallet,
        None => {
            let chain_root = derive_persona_chain_root(seed, persona)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let wallet = PersonaWalletManifest::new(persona, chain_root, KeyEpochId::new());
            save_persona_wallet(data_root, &wallet)?;
            wallet
        }
    };
    ensure_persona_epoch_bridge(data_root, persona, persona_wallet.private_epoch_head)?;

    identity_wallet.device_roster_ref = Some(device_roster_ref(&roster)?);
    save_identity_wallet(data_root, &identity_wallet)?;
    Ok(seed)
}

fn load_json_optional<T>(path: &Path) -> io::Result<Option<T>>
where
    T: for<'de> Deserialize<'de>,
{
    match fs::read_to_string(path) {
        Ok(text) => {
            let value = serde_json::from_str(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Ok(Some(value))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn save_json_atomic<T>(path: &Path, value: &T) -> io::Result<()>
where
    T: Serialize,
{
    let json = json_pretty_bytes(value)?;
    save_bytes_atomic(path, &json)
}

fn json_pretty_bytes<T>(value: &T) -> io::Result<Vec<u8>>
where
    T: Serialize,
{
    serde_json::to_vec_pretty(value).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn save_bytes_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path has no parent: {path:?}"),
        )
    })?;
    fs::create_dir_all(dir)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("tmp");
    let tmp = path.with_extension(format!("{ext}.tmp"));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_store::{PersistedSettings, save_settings};
    use identity::{IdentityProvider, InMemoryProvider};

    fn temp_data_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mere-wallet-store-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn fixture_persona() -> PersonaId {
        PersonaId::from_uuid(Uuid::from_u128(0x1111))
    }

    fn fixture_device() -> DeviceId {
        DeviceId::from_uuid(Uuid::from_u128(0x2222))
    }

    fn fixture_chain_root() -> PersonaChainRoot {
        PersonaChainRoot([7u8; 32])
    }

    fn fixture_epoch() -> KeyEpochId {
        KeyEpochId(Uuid::from_u128(0x3333))
    }

    #[test]
    fn identity_paths_compose_under_identity_root() {
        let root = Path::new("/data");
        assert_eq!(identity_dir(root), Path::new("/data").join("identity"));
        assert_eq!(
            identity_wallet_path(root),
            Path::new("/data").join("identity").join("wallet.json")
        );
        assert_eq!(
            identity_seed_path(root),
            Path::new("/data").join("identity").join("master.seed")
        );
        assert_eq!(
            local_device_identity_path(root),
            Path::new("/data")
                .join("identity")
                .join("local-device.json")
        );
        assert_eq!(
            device_roster_path(root),
            Path::new("/data")
                .join("identity")
                .join("device-roster.json")
        );
        assert_eq!(
            device_grant_path(root, fixture_device()),
            Path::new("/data")
                .join("identity")
                .join("grants")
                .join(format!("{}.cbor", fixture_device().as_uuid()))
        );
    }

    #[test]
    fn persona_wallet_path_composes_under_persona_root() {
        let path = persona_wallet_path(Path::new("/data"), fixture_persona());
        let expected = Path::new("/data")
            .join("personas")
            .join(fixture_persona().as_uuid().to_string())
            .join("wallet.json");
        assert_eq!(path, expected);
    }

    #[test]
    fn device_public_key_round_trips_through_identity_type() {
        let provider = InMemoryProvider::from_seed([9u8; 32]);
        let stored = DevicePublicKey::from(provider.master_public_key());
        let restored = stored.to_public_key().unwrap();
        assert_eq!(restored.to_bytes(), provider.master_public_key().to_bytes());
    }

    #[test]
    fn missing_wallet_files_return_none() {
        let root = temp_data_root("missing");
        assert!(load_identity_wallet(&root).unwrap().is_none());
        assert!(load_device_roster(&root).unwrap().is_none());
        assert!(load_local_device_identity(&root).unwrap().is_none());
        assert!(
            load_persona_wallet(&root, fixture_persona())
                .unwrap()
                .is_none()
        );
        assert!(
            load_device_grant(&root, fixture_device())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn identity_wallet_round_trips() {
        let root = temp_data_root("identity");
        let wallet = IdentityWalletManifest {
            device_roster_ref: Some(Hash::of(b"roster")),
            personas: vec![PersonaWalletRef {
                persona_id: fixture_persona(),
            }],
            grant_index: vec![DeviceGrantRef {
                device_id: fixture_device(),
                grant_ref: Some(Hash::of(b"grant")),
            }],
            ..IdentityWalletManifest::default()
        };
        save_identity_wallet(&root, &wallet).unwrap();
        let restored = load_identity_wallet(&root).unwrap().unwrap();
        assert_eq!(restored, wallet);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn device_roster_round_trips() {
        let root = temp_data_root("roster");
        let provider = InMemoryProvider::from_seed([4u8; 32]);
        let roster = DeviceRoster {
            devices: vec![DeviceRecord {
                device_id: fixture_device(),
                device_pubkey: DevicePublicKey::from(provider.master_public_key()),
                label: "home-server".to_string(),
                mode: DeviceMode::RemoteAuth,
                exposure: DeviceExposure::ExposedEgress,
                grant_ref: Some(Hash::of(b"grant")),
            }],
            revoked: vec![DeviceId::from_uuid(Uuid::from_u128(0x4444))],
            ..DeviceRoster::new()
        };
        save_device_roster(&root, &roster).unwrap();
        let restored = load_device_roster(&root).unwrap().unwrap();
        assert_eq!(restored, roster);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn persona_wallet_round_trips() {
        let root = temp_data_root("persona");
        let mut wallet =
            PersonaWalletManifest::new(fixture_persona(), fixture_chain_root(), fixture_epoch());
        wallet.epoch_history_ref = Some(Hash::of(b"epochs"));
        wallet.private_roots.primary_root = Some(Hash::of(b"private-root"));
        wallet
            .private_roots
            .typed_roots
            .insert("eidetic".to_string(), Hash::of(b"typed-private"));
        wallet.public_roots.primary_root = Some(Hash::of(b"public-root"));
        wallet.capability_slots.push(CapabilitySlotRef {
            slot_id: "cluster-read".to_string(),
            grant_ref: Some(Hash::of(b"cap")),
        });
        save_persona_wallet(&root, &wallet).unwrap();
        let restored = load_persona_wallet(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, wallet);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn persona_epoch_bridge_round_trips() {
        let root = temp_data_root("epoch-bridge");
        let bridge = PersonaEpochBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id: fixture_persona(),
            epochs: vec![PrivateEpochRecord {
                epoch_id: fixture_epoch(),
                epoch_secret: vec![0x44; 32],
            }],
        };
        save_persona_epoch_bridge(&root, &bridge).unwrap();
        let restored = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn copy_seeded_persona_epoch_bridge_round_trips_as_a_sealed_record() {
        let root = temp_data_root("epoch-bridge-sealed-copy");
        save_identity_seed(&root, [0x44; 32]).unwrap();
        let bridge = PersonaEpochBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id: fixture_persona(),
            epochs: vec![PrivateEpochRecord {
                epoch_id: fixture_epoch(),
                epoch_secret: b"known-private-epoch".to_vec(),
            }],
        };

        save_persona_epoch_bridge(&root, &bridge).unwrap();
        let restored = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);

        let text = fs::read_to_string(persona_epoch_bridge_path(&root, fixture_persona())).unwrap();
        assert!(!text.contains("\"epoch_secret\":["));
        assert!(!text.contains("known-private-epoch"));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn delegated_persona_epoch_bridge_round_trips_as_a_sealed_record() {
        let root = temp_data_root("epoch-bridge-sealed-delegated");
        ensure_local_device_identity(&root, "Pocket Meerkat").unwrap();
        let bridge = PersonaEpochBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id: fixture_persona(),
            epochs: vec![PrivateEpochRecord {
                epoch_id: fixture_epoch(),
                epoch_secret: b"known-private-epoch".to_vec(),
            }],
        };

        save_persona_epoch_bridge(&root, &bridge).unwrap();
        let restored = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);

        let text = fs::read_to_string(persona_epoch_bridge_path(&root, fixture_persona())).unwrap();
        assert!(!text.contains("\"epoch_secret\":["));
        assert!(!text.contains("known-private-epoch"));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn loading_a_legacy_plaintext_persona_epoch_bridge_migrates_it_to_a_sealed_record() {
        let root = temp_data_root("epoch-bridge-migrate");
        save_identity_seed(&root, [0x51; 32]).unwrap();
        let bridge = PersonaEpochBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            persona_id: fixture_persona(),
            epochs: vec![PrivateEpochRecord {
                epoch_id: fixture_epoch(),
                epoch_secret: b"known-private-epoch".to_vec(),
            }],
        };
        save_json_atomic(
            &persona_epoch_bridge_path(&root, fixture_persona()),
            &bridge,
        )
        .unwrap();

        let restored = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);

        let text = fs::read_to_string(persona_epoch_bridge_path(&root, fixture_persona())).unwrap();
        assert!(!text.contains("\"epoch_secret\":["));
        assert!(!text.contains("known-private-epoch"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn opaque_device_grant_round_trips() {
        let root = temp_data_root("grant");
        let bytes = vec![0xa1, 0x62, 0x6f, 0x6b];
        save_device_grant(&root, fixture_device(), &bytes).unwrap();
        let restored = load_device_grant(&root, fixture_device()).unwrap().unwrap();
        assert_eq!(restored, bytes);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn identity_seed_round_trips() {
        let root = temp_data_root("seed");
        let seed = [0x55; 32];
        save_identity_seed(&root, seed).unwrap();
        assert_eq!(load_identity_seed(&root).unwrap(), Some(seed));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn loading_a_legacy_raw_seed_migrates_it_to_a_sealed_record() {
        let root = temp_data_root("seed-migrate");
        let seed = [0x7a; 32];
        save_bytes_atomic(&identity_seed_path(&root), &seed).unwrap();

        let restored = load_identity_seed(&root).unwrap().unwrap();
        assert_eq!(restored, seed);
        assert!(identity_auto_unlock_root_path(&root).is_file());

        let text = fs::read_to_string(identity_seed_path(&root)).unwrap();
        assert!(text.contains("ciphertext"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_device_identity_round_trips() {
        let root = temp_data_root("local-device");
        let identity = LocalDeviceIdentity::new(fixture_device(), [0x33; 32], "Tablet".into());
        save_local_device_identity(&root, &identity).unwrap();
        let restored = load_local_device_identity(&root).unwrap().unwrap();
        assert_eq!(restored, identity);
        assert_eq!(
            restored.public_key(),
            DevicePublicKey::from(restored.keypair().public_key())
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn loading_a_legacy_local_device_identity_migrates_it_to_a_sealed_record() {
        let root = temp_data_root("local-device-migrate");
        let identity = LocalDeviceIdentity::new(fixture_device(), [0x33; 32], "Tablet".into());
        save_json_atomic(&local_device_identity_path(&root), &identity).unwrap();

        let restored = load_local_device_identity(&root).unwrap().unwrap();
        assert_eq!(restored, identity);
        assert!(identity_auto_unlock_root_path(&root).is_file());

        let text = fs::read_to_string(local_device_identity_path(&root)).unwrap();
        assert!(!text.contains("\"label\": \"Tablet\""));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn remote_auth_wrapping_key_bridge_round_trips_as_a_sealed_record() {
        let root = temp_data_root("wrapping-keys");
        save_identity_seed(&root, [0x66; 32]).unwrap();
        let bridge = RemoteAuthWrappingKeyBridge {
            schema_version: WALLET_SCHEMA_VERSION,
            keys: vec![RemoteAuthWrappingKeyRecord {
                device_id: fixture_device(),
                ticket_id: Some(Uuid::from_u128(0xfeed)),
                wrapping_key: [0x5a; 32],
            }],
        };

        save_remote_auth_wrapping_key_bridge(&root, &bridge).unwrap();
        let restored = load_remote_auth_wrapping_key_bridge(&root)
            .unwrap()
            .unwrap();
        assert_eq!(restored, bridge);

        let text = fs::read_to_string(remote_auth_wrapping_keys_path(&root)).unwrap();
        assert!(!text.contains("\"wrapping_key\""));
        assert!(!text.contains("feed"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn persona_wallet_salt_matches_known_vector() {
        let salt = persona_wallet_salt(PersonaId::default_persona());
        assert_eq!(
            hex::encode(salt),
            "670ad53661cebddd4356f5fe407cf5452691ed29413fa708a841985f317eebde"
        );
    }

    #[test]
    fn derived_chain_root_matches_provider_derivation() {
        let seed = [0x21; 32];
        let persona = fixture_persona();
        let provider = InMemoryProvider::from_seed(seed);
        let manual = provider
            .derive_keypair(&persona_wallet_salt(persona))
            .unwrap()
            .public_key()
            .to_bytes();
        let derived = derive_persona_chain_root(seed, persona).unwrap();
        assert_eq!(derived, PersonaChainRoot(manual));
    }

    #[test]
    fn ensure_wallet_state_seeds_identity_and_persona_files() {
        let root = temp_data_root("bootstrap");
        let persona = fixture_persona();
        let seed = ensure_wallet_state(&root, persona, "Studio PC").unwrap();

        assert_eq!(load_identity_seed(&root).unwrap(), Some(seed));
        let identity_wallet = load_identity_wallet(&root)
            .unwrap()
            .expect("identity wallet should exist");
        assert_eq!(
            identity_wallet.personas,
            vec![PersonaWalletRef {
                persona_id: persona
            }]
        );
        let roster = load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        assert_eq!(roster.devices.len(), 1, "one local device is seeded");
        assert_eq!(roster.devices[0].label, "Studio PC");
        assert_eq!(roster.devices[0].mode, DeviceMode::Copy);
        let persona_wallet = load_persona_wallet(&root, persona)
            .unwrap()
            .expect("persona wallet should exist");
        assert_eq!(persona_wallet.persona_id, persona);
        assert_eq!(
            persona_wallet.chain_root,
            derive_persona_chain_root(seed, persona).unwrap()
        );
        let epoch = load_current_private_epoch(&root, persona)
            .unwrap()
            .expect("current private epoch should exist");
        assert_eq!(epoch.epoch_id, persona_wallet.private_epoch_head);
        assert_eq!(epoch.epoch_secret.len(), 32);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_wallet_state_is_idempotent_for_existing_seeded_files() {
        let root = temp_data_root("bootstrap-idempotent");
        let persona = fixture_persona();
        let first = ensure_wallet_state(&root, persona, "Studio PC").unwrap();
        let second = ensure_wallet_state(&root, persona, "Other Label").unwrap();
        assert_eq!(first, second, "bootstrap reuses the same master seed");
        let roster = load_device_roster(&root)
            .unwrap()
            .expect("roster should exist");
        assert_eq!(
            roster.devices.len(),
            1,
            "bootstrap does not duplicate the device"
        );
        assert_eq!(
            roster.devices[0].label, "Studio PC",
            "bootstrap leaves the seeded device record intact"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_local_device_identity_is_idempotent() {
        let root = temp_data_root("ensure-local-device");
        let first = ensure_local_device_identity(&root, "Phone").unwrap();
        let second = ensure_local_device_identity(&root, "Other Label").unwrap();
        assert_eq!(first, second);
        assert_eq!(second.label, "Phone");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_persona_epoch_bridge_is_idempotent_for_the_same_epoch() {
        let root = temp_data_root("ensure-epoch-bridge");
        let first = ensure_persona_epoch_bridge(&root, fixture_persona(), fixture_epoch()).unwrap();
        let second =
            ensure_persona_epoch_bridge(&root, fixture_persona(), fixture_epoch()).unwrap();
        assert_eq!(first, second);
        assert_eq!(second.epochs.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stage_persona_private_epoch_replaces_existing_secret() {
        let root = temp_data_root("stage-epoch-bridge");
        ensure_persona_epoch_bridge(&root, fixture_persona(), fixture_epoch()).unwrap();

        stage_persona_private_epoch(
            &root,
            fixture_persona(),
            fixture_epoch(),
            b"known-private-epoch",
        )
        .unwrap();

        let bridge = load_persona_epoch_bridge(&root, fixture_persona())
            .unwrap()
            .expect("bridge should exist");
        assert_eq!(bridge.epochs.len(), 1);
        assert_eq!(bridge.epochs[0].epoch_id, fixture_epoch());
        assert_eq!(
            bridge.epochs[0].epoch_secret,
            b"known-private-epoch".to_vec()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bootstrap_wallet_state_seeds_a_fresh_copy_root() {
        let root = temp_data_root("bootstrap-copy");
        let mode = bootstrap_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::CopySeeded);
        assert!(load_identity_seed(&root).unwrap().is_some());
        assert!(load_identity_wallet(&root).unwrap().is_some());
        assert!(load_local_device_identity(&root).unwrap().is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bootstrap_wallet_state_preserves_a_pending_delegated_identity() {
        let root = temp_data_root("bootstrap-delegated-pending");
        let local = ensure_local_device_identity(&root, "Pocket Meerkat").unwrap();

        let mode = bootstrap_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::DelegatedPending);
        assert!(load_identity_seed(&root).unwrap().is_none());
        assert_eq!(load_local_device_identity(&root).unwrap(), Some(local));
        assert!(load_identity_wallet(&root).unwrap().is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bootstrap_wallet_state_preserves_an_enrolled_delegated_wallet() {
        let root = temp_data_root("bootstrap-delegated-enrolled");
        let persona = fixture_persona();
        save_identity_wallet(
            &root,
            &IdentityWalletManifest {
                schema_version: WALLET_SCHEMA_VERSION,
                device_roster_ref: None,
                recovery_policy: RecoveryPolicy::default(),
                personas: vec![PersonaWalletRef {
                    persona_id: persona,
                }],
                grant_index: Vec::new(),
            },
        )
        .unwrap();

        let mode = bootstrap_wallet_state(&root, persona, "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::DelegatedEnrolled);
        assert!(load_identity_seed(&root).unwrap().is_none());
        assert_eq!(
            load_identity_wallet(&root)
                .unwrap()
                .expect("identity wallet should persist")
                .personas,
            vec![PersonaWalletRef {
                persona_id: persona
            }]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn bootstrap_wallet_state_reports_locked_when_copy_seed_is_sealed_but_startup_stays_locked() {
        let root = temp_data_root("bootstrap-locked-copy");
        let persona = fixture_persona();
        ensure_wallet_state(&root, persona, "Studio PC").unwrap();
        save_settings(
            &root,
            &PersistedSettings {
                startup_unlock_mode: StartupUnlockMode::Locked,
                ..PersistedSettings::default()
            },
        )
        .unwrap();

        let mode = bootstrap_wallet_state(&root, persona, "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::Locked);
        assert!(identity_seed_locked_at_startup(&root));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn bootstrap_wallet_state_reports_locked_when_delegated_identity_is_sealed_but_startup_stays_locked()
     {
        let root = temp_data_root("bootstrap-locked-delegated");
        let persona = fixture_persona();
        ensure_local_device_identity(&root, "Pocket Meerkat").unwrap();
        save_settings(
            &root,
            &PersistedSettings {
                startup_unlock_mode: StartupUnlockMode::Prompt,
                ..PersistedSettings::default()
            },
        )
        .unwrap();

        let mode = bootstrap_wallet_state(&root, persona, "Studio PC").unwrap();

        assert_eq!(mode, WalletBootstrapMode::Locked);

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn explicit_auto_os_unlock_opens_a_locked_seed_without_changing_startup_mode() {
        let root = temp_data_root("explicit-auto-os-unlock");
        let persona = fixture_persona();
        ensure_wallet_state(&root, persona, "Studio PC").unwrap();
        save_settings(
            &root,
            &PersistedSettings {
                startup_unlock_mode: StartupUnlockMode::Locked,
                ..PersistedSettings::default()
            },
        )
        .unwrap();

        assert!(wallet_local_secrets_locked(&root));
        assert_eq!(load_identity_seed(&root).unwrap(), None);

        assert!(unlock_wallet_with_auto_os(&root).unwrap());
        assert!(!wallet_local_secrets_locked(&root));
        assert!(load_identity_seed(&root).unwrap().is_some());
        assert_eq!(
            settings_store::load_settings(&root)
                .unwrap()
                .unwrap_or_default()
                .startup_unlock_mode,
            StartupUnlockMode::Locked
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn relock_after_manual_unlock_hides_the_seed_again_under_locked_startup() {
        let root = temp_data_root("manual-relock");
        let persona = fixture_persona();
        ensure_wallet_state(&root, persona, "Studio PC").unwrap();
        save_settings(
            &root,
            &PersistedSettings {
                startup_unlock_mode: StartupUnlockMode::Prompt,
                ..PersistedSettings::default()
            },
        )
        .unwrap();

        assert!(unlock_wallet_with_auto_os(&root).unwrap());
        assert!(load_identity_seed(&root).unwrap().is_some());

        relock_wallet_after_manual_unlock(&root);

        assert!(wallet_local_secrets_locked(&root));
        assert_eq!(load_identity_seed(&root).unwrap(), None);

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(not(windows))]
    #[test]
    fn explicit_auto_os_unlock_reports_unavailable_without_platform_support() {
        let root = temp_data_root("explicit-auto-os-unlock-unsupported");
        assert!(!unlock_wallet_with_auto_os(&root).unwrap());
        let _ = fs::remove_dir_all(&root);
    }
}
