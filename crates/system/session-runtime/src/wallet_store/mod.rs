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

//!
//! Split 2026-08-10 (wallet carry fold-in plan, W2): the carry *model* now
//! lives in `personae::carry` and this tree is the store adapter, one module
//! per concern. The re-exports below keep every existing consumer path
//! (`session_runtime::wallet_store::*`) spelled the same.

mod bootstrap;
mod devices;
mod epochs;
mod io;
mod manifests;
mod paths;
mod secrets;
#[cfg(test)]
mod test_support;

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
// ── Carry model (moved to personae 2026-08-10) ───────────────────────────────
//
// The wallet record types, content refs, and derivation live in
// `identity::carry` (the fold-in the 2026-07-08 personae founding promised;
// see design_docs/mere_docs/implementation_strategy/
// 2026-08-10_wallet_carry_foldin_plan.md). This module keeps the store
// adapter: path layout, device-settings policy, sealed-record wiring, and
// bootstrap. The re-exports keep every existing consumer path
// (`session_runtime::wallet_store::*`) compiling unchanged.
pub use identity::carry::{
    CapabilitySlotRef, CarryHashFn, CarryRef, CarryRefParseError, DeviceExposure, DeviceGrantRef,
    DeviceId, DeviceMode, DevicePublicKey, DeviceRecord, DeviceRoster, IdentityWalletManifest,
    KeyEpochId, LocalDeviceIdentity, PersonaChainRoot, PersonaEpochBridge, PersonaWalletManifest,
    PersonaWalletRef, PrivateEpochRecord, PrivateRoots, PublicRoots, RecoveryPolicy,
    RemoteAuthWrappingKeyBridge, RemoteAuthWrappingKeyRecord, WALLET_SCHEMA_VERSION,
    derive_persona_chain_root, persona_wallet_salt,
};

pub use bootstrap::{WalletBootstrapMode, bootstrap_wallet_state, ensure_wallet_state};
pub use devices::{
    ensure_local_device_identity, load_device_grant, load_local_device_identity,
    load_remote_auth_wrapping_key_bridge, save_device_grant, save_local_device_identity,
    save_remote_auth_wrapping_key_bridge,
};
pub use epochs::{
    ensure_persona_epoch_bridge, load_current_private_epoch, load_persona_epoch_bridge,
    save_persona_epoch_bridge, stage_persona_private_epoch,
};
pub use manifests::{
    device_roster_ref, load_device_roster, load_identity_wallet, load_persona_wallet,
    save_device_roster, save_identity_wallet, save_persona_wallet,
};
pub use paths::{
    device_grant_path, device_roster_path, identity_auto_unlock_root_path, identity_dir,
    identity_grants_dir, identity_seed_path, identity_wallet_path, list_personas,
    local_device_identity_path, persona_epoch_bridge_path, persona_wallet_path,
    remote_auth_wrapping_keys_path,
};
pub use secrets::{
    identity_seed_locked_at_startup, load_identity_seed, relock_wallet_after_manual_unlock,
    save_identity_seed, unlock_wallet_with_auto_os, wallet_local_secrets_locked,
};

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use identity::{IdentityProvider, InMemoryProvider, PersonaId};

    /// The fold-in's disk-format invariant: `CarryRef` must serialize exactly
    /// as `eidetic::Hash` always did, digest and framing both, or every
    /// stored manifest ref on every machine silently stops resolving.
    #[test]
    fn carry_ref_repr_matches_eidetic_hash_repr() {
        for input in [b"".as_slice(), b"roster bytes", b"grant envelope"] {
            let hash = eidetic::Hash::of(input);
            let carry = CarryRef::of(input);
            assert_eq!(carry.to_string(), hash.to_string());
            assert_eq!(
                serde_json::to_string(&carry).unwrap(),
                serde_json::to_string(&hash).unwrap()
            );
            let reparsed: CarryRef =
                serde_json::from_str(&serde_json::to_string(&hash).unwrap()).unwrap();
            assert_eq!(reparsed, carry);
        }
    }

    #[test]
    fn device_public_key_round_trips_through_identity_type() {
        let provider = InMemoryProvider::from_seed([9u8; 32]);
        let stored = DevicePublicKey::from(provider.master_public_key());
        let restored = stored.to_public_key().unwrap();
        assert_eq!(restored.to_bytes(), provider.master_public_key().to_bytes());
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
}
