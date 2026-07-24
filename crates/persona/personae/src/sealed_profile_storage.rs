//! [`SealedProfileStorage`] — profile storage over sealed records.
//!
//! Fills the `OsKeychainStorage` role from
//! [`mere/design_docs/mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](../../../../../../design_docs/mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md)
//! §3.2: the desktop-default [`crate::vault::IdentityStorage`] backend whose
//! unlock rides OS-protected local storage rather than a passphrase. It
//! composes two existing layers instead of adding cryptography:
//!
//! - [`crate::SealedRecordStorage`] holds one sealed record per profile
//!   (ChaCha20-Poly1305, record path bound as AEAD associated data).
//! - [`crate::startup_unlock`] supplies the 32-byte root key for the
//!   `AutoOs` mode (DPAPI-wrapped on Windows; other platforms report
//!   honest absence rather than falling back to plaintext).
//!
//! The unlock ceremony stays outside, mirroring `SealedRecordStorage`:
//! [`SealedProfileStorage::open_with_key`] takes an already-unlocked root,
//! and [`SealedProfileStorage::open_auto_os`] is the convenience that
//! sources it from the auto-unlock ladder, returning `Ok(None)` where that
//! backend does not exist yet.
//!
//! ## Layout
//!
//! One directory, one record per profile at `profiles/<blake3(id)>.json`,
//! plus the `AutoOs` root at `auto-unlock-root.json` when `open_auto_os`
//! manages the key. Profile ids are hashed for the filename (ids are
//! user-chosen strings; hashing keeps them filesystem-safe), and the id
//! itself is stored inside the sealed record.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::profile_wire::{PlaintextProfile, plaintext_to_slot, slot_to_plaintext};
use crate::sealed_record_storage::SealedRecordStorage;
use crate::vault::{IdentityStorage, Profile, ProfileId, ProfileSummary};
use crate::{Ed25519Keypair, IdentityError};

const PROFILE_DIR: &str = "profiles";
const AUTO_UNLOCK_ROOT_FILE: &str = "auto-unlock-root.json";

/// One profile's sealed record. The id rides inside the plaintext so a
/// directory listing can recover it (filenames are hashes).
#[derive(Debug, Serialize, Deserialize)]
struct SealedProfileRecord {
    id: String,
    profile: PlaintextProfile,
}

/// Sealed-record-backed [`IdentityStorage`].
///
/// See the module docs for the layering. Send + Sync because the record
/// store holds only a path and a zeroizing key.
pub struct SealedProfileStorage {
    records: SealedRecordStorage,
    root: PathBuf,
}

impl SealedProfileStorage {
    /// Open profile storage rooted at `root` with an already-unlocked
    /// 32-byte root key (any key ladder: auto-unlock, passphrase-wrapped
    /// vault root, test fixture).
    pub fn open_with_key(root: impl Into<PathBuf>, key: [u8; 32]) -> Self {
        let root = root.into();
        Self {
            records: SealedRecordStorage::open_with_key(&root, key),
            root,
        }
    }

    /// Open profile storage rooted at `root`, sourcing the key from the
    /// [`crate::startup_unlock`] `AutoOs` ladder (DPAPI on Windows).
    ///
    /// Returns `Ok(None)` on platforms where the auto-unlock backend is
    /// not implemented, so callers surface a degraded state (or fall back
    /// to [`crate::PassphraseEncryptedStorage`]) instead of silently
    /// storing plaintext.
    pub fn open_auto_os(root: impl Into<PathBuf>) -> Result<Option<Self>, IdentityError> {
        let root: PathBuf = root.into();
        let Some(key) =
            crate::startup_unlock::load_or_create_auto_unlock_root(root.join(AUTO_UNLOCK_ROOT_FILE))?
        else {
            return Ok(None);
        };
        Ok(Some(Self::open_with_key(root, key)))
    }

    fn record_path(id: &ProfileId) -> String {
        let hash = blake3::hash(id.0.as_bytes());
        format!("{PROFILE_DIR}/{}.json", hash.to_hex())
    }

    fn profiles_dir(&self) -> PathBuf {
        self.root.join(PROFILE_DIR)
    }
}

impl IdentityStorage for SealedProfileStorage {
    fn load_profile(&self, id: &ProfileId) -> Result<Profile, IdentityError> {
        let record: SealedProfileRecord = self
            .records
            .load_record(Self::record_path(id))?
            .ok_or_else(|| IdentityError::Backend(format!("profile not found: {:?}", id)))?;
        if record.id != id.0 {
            return Err(IdentityError::Backend(format!(
                "profile record id mismatch: asked {:?}, stored {:?}",
                id.0, record.id
            )));
        }
        let mut slots = std::collections::HashMap::with_capacity(record.profile.slots.len());
        for s in &record.profile.slots {
            let (k, slot) = plaintext_to_slot(s);
            slots.insert(k, slot);
        }
        Ok(Profile {
            id: id.clone(),
            display_name: record.profile.display_name,
            master: Ed25519Keypair::from_seed(record.profile.master_seed),
            slots,
        })
    }

    fn save_profile(&self, profile: &Profile) -> Result<(), IdentityError> {
        let record = SealedProfileRecord {
            id: profile.id.0.clone(),
            profile: PlaintextProfile {
                display_name: profile.display_name.clone(),
                master_seed: profile.master.to_seed(),
                slots: profile
                    .slots
                    .iter()
                    .map(|(k, s)| slot_to_plaintext(k, s))
                    .collect(),
            },
        };
        self.records
            .save_record(Self::record_path(&profile.id), &record)
    }

    fn delete_profile(&self, id: &ProfileId) -> Result<(), IdentityError> {
        self.records.delete_record(Self::record_path(id))
    }

    fn list_profiles(&self) -> Result<Vec<ProfileSummary>, IdentityError> {
        let dir = self.profiles_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(IdentityError::Backend(format!(
                    "list profiles dir {:?}: {err}",
                    dir
                )));
            }
        };
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry
                .map_err(|err| IdentityError::Backend(format!("read dir entry: {err}")))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.ends_with(".json") {
                continue;
            }
            let record: SealedProfileRecord = self
                .records
                .load_record(format!("{PROFILE_DIR}/{name}"))?
                .ok_or_else(|| {
                    IdentityError::Backend(format!("profile record vanished during list: {name}"))
                })?;
            out.push(ProfileSummary {
                id: ProfileId(record.id),
                display_name: record.profile.display_name,
                slot_count: record.profile.slots.len(),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{
        CredentialLineage, IdentitySlot, IdentityVault, ProtocolKey, SecretBytes, UnlockTier,
    };
    use tempfile::tempdir;

    fn ssh_slot() -> IdentitySlot {
        IdentitySlot::Direct {
            kind: "ssh".to_string(),
            payload: SecretBytes::new(vec![0x5a; 64]),
            lineage: CredentialLineage::LocallyDerived,
            unlock_tier: UnlockTier::Session,
        }
    }

    #[test]
    fn save_and_load_round_trips_through_sealed_records() {
        let dir = tempdir().unwrap();
        let storage = SealedProfileStorage::open_with_key(dir.path(), [0x11; 32]);

        let id = ProfileId("personal".to_string());
        let mut profile = Profile::new(id.clone(), "personal", Ed25519Keypair::from_seed([7; 32]));
        profile
            .slots
            .insert(ProtocolKey::new("ssh", Some("laptop".into())), ssh_slot());
        storage.save_profile(&profile).unwrap();

        let loaded = storage.load_profile(&id).unwrap();
        assert_eq!(loaded.display_name, "personal");
        assert_eq!(
            loaded.master.public_key().to_bytes(),
            profile.master.public_key().to_bytes()
        );
        assert!(loaded.has_slot(&ProtocolKey::new("ssh", Some("laptop".into()))));
    }

    #[test]
    fn survives_reopen_with_the_same_key() {
        let dir = tempdir().unwrap();
        let id = ProfileId("p".to_string());
        {
            let storage = SealedProfileStorage::open_with_key(dir.path(), [0x22; 32]);
            let mut profile = Profile::new(id.clone(), "p", Ed25519Keypair::from_seed([5; 32]));
            profile
                .slots
                .insert(ProtocolKey::new("ssh", None), ssh_slot());
            storage.save_profile(&profile).unwrap();
        }
        let storage = SealedProfileStorage::open_with_key(dir.path(), [0x22; 32]);
        let loaded = storage.load_profile(&id).unwrap();
        assert!(loaded.has_slot(&ProtocolKey::new("ssh", None)));
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let dir = tempdir().unwrap();
        let id = ProfileId("p".to_string());
        {
            let storage = SealedProfileStorage::open_with_key(dir.path(), [0x33; 32]);
            storage
                .save_profile(&Profile::new(
                    id.clone(),
                    "p",
                    Ed25519Keypair::from_seed([1; 32]),
                ))
                .unwrap();
        }
        let storage = SealedProfileStorage::open_with_key(dir.path(), [0x44; 32]);
        assert!(storage.load_profile(&id).is_err());
    }

    #[test]
    fn list_recovers_ids_from_hashed_filenames() {
        let dir = tempdir().unwrap();
        let storage = SealedProfileStorage::open_with_key(dir.path(), [0x55; 32]);
        for (id, name) in [("work", "Work"), ("personal", "Personal")] {
            storage
                .save_profile(&Profile::new(
                    ProfileId(id.to_string()),
                    name,
                    Ed25519Keypair::from_seed([9; 32]),
                ))
                .unwrap();
        }
        let mut listed = storage.list_profiles().unwrap();
        listed.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, ProfileId("personal".into()));
        assert_eq!(listed[1].id, ProfileId("work".into()));
        assert_eq!(listed[1].display_name, "Work");
    }

    #[test]
    fn awkward_profile_ids_are_filesystem_safe() {
        // Ids with separators / traversal shapes must round-trip because
        // filenames are hashes, not the id text.
        let dir = tempdir().unwrap();
        let storage = SealedProfileStorage::open_with_key(dir.path(), [0x66; 32]);
        let id = ProfileId("../weird/../id with spaces".to_string());
        storage
            .save_profile(&Profile::new(
                id.clone(),
                "weird",
                Ed25519Keypair::from_seed([2; 32]),
            ))
            .unwrap();
        let loaded = storage.load_profile(&id).unwrap();
        assert_eq!(loaded.display_name, "weird");
    }

    #[test]
    fn delete_profile_removes_the_record() {
        let dir = tempdir().unwrap();
        let storage = SealedProfileStorage::open_with_key(dir.path(), [0x77; 32]);
        let id = ProfileId("p".to_string());
        storage
            .save_profile(&Profile::new(
                id.clone(),
                "p",
                Ed25519Keypair::from_seed([4; 32]),
            ))
            .unwrap();
        storage.delete_profile(&id).unwrap();
        assert!(storage.load_profile(&id).is_err());
        assert_eq!(storage.list_profiles().unwrap().len(), 0);
    }

    #[test]
    fn vault_works_against_sealed_profile_storage() {
        let dir = tempdir().unwrap();
        let storage = SealedProfileStorage::open_with_key(dir.path(), [0x88; 32]);
        let profile = Profile::new(
            ProfileId("p".into()),
            "p",
            Ed25519Keypair::from_seed([9; 32]),
        );
        let mut vault = IdentityVault::with_profile(storage, profile);
        vault
            .add_slot(ProtocolKey::new("ssh", None), ssh_slot())
            .unwrap();

        let storage2 = SealedProfileStorage::open_with_key(dir.path(), [0x88; 32]);
        let loaded = storage2.load_profile(&ProfileId("p".into())).unwrap();
        assert!(loaded.has_slot(&ProtocolKey::new("ssh", None)));
    }

    #[cfg(windows)]
    #[test]
    fn open_auto_os_round_trips_on_windows() {
        let dir = tempdir().unwrap();
        let id = ProfileId("p".to_string());
        {
            let storage = SealedProfileStorage::open_auto_os(dir.path())
                .unwrap()
                .expect("windows has the AutoOs backend");
            storage
                .save_profile(&Profile::new(
                    id.clone(),
                    "p",
                    Ed25519Keypair::from_seed([6; 32]),
                ))
                .unwrap();
        }
        let storage = SealedProfileStorage::open_auto_os(dir.path())
            .unwrap()
            .expect("windows has the AutoOs backend");
        let loaded = storage.load_profile(&id).unwrap();
        assert_eq!(loaded.display_name, "p");
    }
}
