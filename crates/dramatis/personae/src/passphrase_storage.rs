//! [`PassphraseEncryptedStorage`] — passphrase-encrypted on-disk vault.
//!
//! Phase 2C v0 production-grade backend per
//! [`mere/design_docs/mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](../../../../../../design_docs/mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md)
//! §3.2.
//!
//! ## Wire format
//!
//! All profiles for one user live in a single JSON file. The file is
//! serde-shaped JSON of [`EncryptedFile`]: per-profile records with
//! ChaCha20-Poly1305 ciphertext + nonce + Argon2id parameters.
//!
//! ## Crypto
//!
//! - **Argon2id** derives a 32-byte KEK from passphrase + per-file salt.
//!   Memory-hard; resists GPU-accelerated brute-force.
//! - **ChaCha20-Poly1305** authenticated encryption per profile. A
//!   per-profile nonce is generated at save-time. The plaintext is the
//!   serde JSON of the in-memory profile shape.
//! - **The passphrase derives a *KEK* (key-encryption key); all profiles
//!   in the same file share the KEK.** Using a per-profile passphrase
//!   would force the user to remember N passphrases — bad UX. If a user
//!   wants partitioned profiles (work / personal), they can use two
//!   different files.
//!
//! ## Limitations (v0)
//!
//! - **Single file per backend instance.** Multi-file vaults are out of
//!   scope here; the user can construct multiple `PassphraseEncryptedStorage`
//!   over different paths to partition.
//! - **Whole-file rewrites on every save.** Atomic writes via tempfile
//!   + rename. Acceptable for the expected vault sizes (kilobytes).
//! - **No re-encrypt on passphrase change.** Add `change_passphrase`
//!   when a UX surface needs it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::profile_wire::{PlaintextProfile, plaintext_to_slot, slot_to_plaintext};
use crate::vault::{IdentityStorage, Profile, ProfileId, ProfileSummary};
use crate::{Ed25519Keypair, IdentityError};

const ARGON2_SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const FILE_VERSION: u8 = 1;

/// On-disk file format. JSON-shaped.
#[derive(Debug, Serialize, Deserialize)]
struct EncryptedFile {
    /// File-format version. Bump if the wire shape changes.
    version: u8,
    /// Argon2id salt — single salt per file, since all profiles share
    /// the same KEK derived from the passphrase.
    salt: Vec<u8>,
    /// Per-profile encrypted records.
    profiles: HashMap<String, EncryptedProfile>,
}

/// One profile's encrypted record.
#[derive(Debug, Serialize, Deserialize)]
struct EncryptedProfile {
    /// Per-profile ChaCha20-Poly1305 nonce.
    nonce: Vec<u8>,
    /// Authenticated ciphertext.
    ciphertext: Vec<u8>,
}

/// Argon2id-derived KEK.
///
/// Shared with [`crate::passphrase_root`] so the passphrase profile vault and
/// the passphrase-wrapped vault root derive their key-encryption keys through
/// one KDF configuration (the "one unlock ladder" rule): same Argon2id
/// parameters, same zeroize discipline.
pub(crate) fn derive_kek(
    passphrase: &[u8],
    salt: &[u8],
) -> Result<Zeroizing<[u8; 32]>, IdentityError> {
    let argon = Argon2::default();
    let mut kek = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(passphrase, salt, kek.as_mut())
        .map_err(|e| IdentityError::Backend(format!("argon2 derive: {e}")))?;
    Ok(kek)
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).expect("OS randomness available");
    buf
}

/// Passphrase-encrypted on-disk storage backend.
///
/// Holds the file path + the in-memory KEK derived from the user's
/// passphrase at construction time. KEK lives in a [`Zeroizing<[u8; 32]>`]
/// and zeros on drop.
pub struct PassphraseEncryptedStorage {
    path: PathBuf,
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    /// Argon2 salt for this file. Reused across saves of the same file
    /// (re-rolling would force a re-derive on every save).
    salt: Vec<u8>,
    /// Cached KEK; the file's salt + the original passphrase produced it.
    kek: Zeroizing<[u8; 32]>,
}

impl PassphraseEncryptedStorage {
    /// Open or create a passphrase-encrypted vault file at `path`.
    ///
    /// If the file does not exist, it is created with a fresh random
    /// salt and zero profiles. If the file exists, the salt is read
    /// from it and the supplied passphrase is verified by attempting
    /// to decrypt one of the profiles (or trusted-empty if no
    /// profiles).
    ///
    /// **Note**: with zero profiles, an incorrect passphrase is
    /// indistinguishable from a correct one — the file just appears
    /// empty. The first `save_profile` call commits the KEK to a
    /// decryption-checkable state.
    #[tracing::instrument(level = "info", skip_all, name = "passphrase_storage.open")]
    pub fn open(path: impl Into<PathBuf>, passphrase: &[u8]) -> Result<Self, IdentityError> {
        let path: PathBuf = path.into();
        let (salt, kek, _existing) = if path.exists() {
            let bytes = std::fs::read(&path)
                .map_err(|e| IdentityError::Backend(format!("read vault file: {e}")))?;
            let file: EncryptedFile = serde_json::from_slice(&bytes)
                .map_err(|e| IdentityError::Backend(format!("parse vault file: {e}")))?;
            if file.version != FILE_VERSION {
                return Err(IdentityError::Backend(format!(
                    "unsupported vault file version {}",
                    file.version
                )));
            }
            let kek = derive_kek(passphrase, &file.salt)?;
            // Verify against an existing profile if any.
            if let Some((_, p)) = file.profiles.iter().next() {
                let cipher = ChaCha20Poly1305::new(&Key::try_from(&kek.as_ref()[..]).expect("fixed-length key material"));
                let nonce = &Nonce::try_from(&p.nonce[..]).expect("fixed-length key material");
                cipher
                    .decrypt(nonce, p.ciphertext.as_slice())
                    .map_err(|_| IdentityError::Backend("incorrect passphrase".to_string()))?;
            }
            (file.salt, kek, file.profiles)
        } else {
            let salt = random_bytes(ARGON2_SALT_LEN);
            let kek = derive_kek(passphrase, &salt)?;
            (salt, kek, HashMap::new())
        };

        Ok(Self {
            path,
            inner: Arc::new(Mutex::new(Inner { salt, kek })),
        })
    }

    fn load_file(&self) -> Result<EncryptedFile, IdentityError> {
        if !self.path.exists() {
            let salt = self.inner.lock().unwrap().salt.clone();
            return Ok(EncryptedFile {
                version: FILE_VERSION,
                salt,
                profiles: HashMap::new(),
            });
        }
        let bytes = std::fs::read(&self.path)
            .map_err(|e| IdentityError::Backend(format!("read vault file: {e}")))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| IdentityError::Backend(format!("parse vault file: {e}")))
    }

    fn save_file(&self, file: &EncryptedFile) -> Result<(), IdentityError> {
        let bytes = serde_json::to_vec(file)
            .map_err(|e| IdentityError::Backend(format!("serialize vault file: {e}")))?;
        // Atomic write via temp + rename to avoid corruption on partial
        // writes. Same dir as final to keep the rename atomic on the
        // same filesystem.
        let dir = self.path.parent().ok_or_else(|| {
            IdentityError::Backend(format!("vault path has no parent: {:?}", self.path))
        })?;
        let tmp = tempfile_in_dir(dir)?;
        std::fs::write(&tmp, &bytes)
            .map_err(|e| IdentityError::Backend(format!("write tmp: {e}")))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| IdentityError::Backend(format!("rename tmp: {e}")))?;
        Ok(())
    }
}

fn tempfile_in_dir(dir: &Path) -> Result<PathBuf, IdentityError> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| IdentityError::Backend(format!("time: {e}")))?
        .as_nanos();
    let mut rand = [0u8; 8];
    getrandom::fill(&mut rand).expect("OS randomness available");
    // Render `rand` as lowercase hex without pulling in the `hex` crate.
    let mut hex = String::with_capacity(16);
    for b in rand {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{:02x}", b);
    }
    let suffix = format!(".vault-{}-{}.tmp", n, hex);
    Ok(dir.join(suffix))
}

impl IdentityStorage for PassphraseEncryptedStorage {
    fn load_profile(&self, id: &ProfileId) -> Result<Profile, IdentityError> {
        let file = self.load_file()?;
        let entry = file
            .profiles
            .get(&id.0)
            .ok_or_else(|| IdentityError::Backend(format!("profile not found: {:?}", id)))?;
        let kek = self.inner.lock().unwrap().kek.clone();
        let cipher = ChaCha20Poly1305::new(&Key::try_from(&kek.as_ref()[..]).expect("fixed-length key material"));
        let nonce = &Nonce::try_from(&entry.nonce[..]).expect("fixed-length key material");
        let plaintext_bytes = cipher
            .decrypt(nonce, entry.ciphertext.as_slice())
            .map_err(|_| IdentityError::Backend("decrypt profile failed".to_string()))?;
        let plain: PlaintextProfile = serde_json::from_slice(&plaintext_bytes)
            .map_err(|e| IdentityError::Backend(format!("decode plaintext: {e}")))?;

        let mut slots = HashMap::with_capacity(plain.slots.len());
        for s in &plain.slots {
            let (k, slot) = plaintext_to_slot(s);
            slots.insert(k, slot);
        }
        Ok(Profile {
            id: id.clone(),
            display_name: plain.display_name,
            master: Ed25519Keypair::from_seed(plain.master_seed),
            slots,
        })
    }

    fn save_profile(&self, profile: &Profile) -> Result<(), IdentityError> {
        let plain = PlaintextProfile {
            display_name: profile.display_name.clone(),
            master_seed: profile.master.to_seed(),
            slots: profile
                .slots
                .iter()
                .map(|(k, s)| slot_to_plaintext(k, s))
                .collect(),
        };
        let plaintext_bytes = serde_json::to_vec(&plain)
            .map_err(|e| IdentityError::Backend(format!("encode plaintext: {e}")))?;

        let kek = self.inner.lock().unwrap().kek.clone();
        let cipher = ChaCha20Poly1305::new(&Key::try_from(&kek.as_ref()[..]).expect("fixed-length key material"));
        let nonce_bytes = random_bytes(NONCE_LEN);
        let nonce = &Nonce::try_from(&nonce_bytes[..]).expect("fixed-length key material");
        let ciphertext = cipher
            .encrypt(nonce, plaintext_bytes.as_slice())
            .map_err(|e| IdentityError::Backend(format!("encrypt profile: {e}")))?;

        let mut file = self.load_file()?;
        file.profiles.insert(
            profile.id.0.clone(),
            EncryptedProfile {
                nonce: nonce_bytes,
                ciphertext,
            },
        );
        self.save_file(&file)
    }

    fn delete_profile(&self, id: &ProfileId) -> Result<(), IdentityError> {
        let mut file = self.load_file()?;
        file.profiles.remove(&id.0);
        self.save_file(&file)
    }

    fn list_profiles(&self) -> Result<Vec<ProfileSummary>, IdentityError> {
        let file = self.load_file()?;
        let kek = self.inner.lock().unwrap().kek.clone();
        let cipher = ChaCha20Poly1305::new(&Key::try_from(&kek.as_ref()[..]).expect("fixed-length key material"));
        let mut out = Vec::with_capacity(file.profiles.len());
        for (id_str, entry) in &file.profiles {
            // Decrypt to get display_name + slot count. This is N
            // decrypts per list call, fine for the expected profile
            // counts (handful) but worth caching if it ever matters.
            let nonce = &Nonce::try_from(&entry.nonce[..]).expect("fixed-length key material");
            let bytes = cipher
                .decrypt(nonce, entry.ciphertext.as_slice())
                .map_err(|_| IdentityError::Backend("decrypt for list failed".to_string()))?;
            let plain: PlaintextProfile = serde_json::from_slice(&bytes)
                .map_err(|e| IdentityError::Backend(format!("decode plaintext: {e}")))?;
            out.push(ProfileSummary {
                id: ProfileId(id_str.clone()),
                display_name: plain.display_name,
                slot_count: plain.slots.len(),
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

    fn nostr_slot() -> IdentitySlot {
        IdentitySlot::Direct {
            kind: "nostr".to_string(),
            payload: SecretBytes::new(vec![0x42; 32]),
            lineage: CredentialLineage::LocallyGeneratedExternallyRegistered,
            unlock_tier: UnlockTier::Session,
        }
    }

    fn matrix_slot() -> IdentitySlot {
        IdentitySlot::Bootstrap {
            kind: "matrix".to_string(),
            bootstrap: SecretBytes::new(b"login-flow-seed".to_vec()),
            state_dir: PathBuf::from("/tmp/matrix-state"),
            lineage: CredentialLineage::LocallyGeneratedExternallyRegistered,
            unlock_tier: UnlockTier::ShortTtl { idle_seconds: 900 },
        }
    }

    #[test]
    fn save_and_load_round_trips_through_passphrase_encrypt() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault.json");
        let storage =
            PassphraseEncryptedStorage::open(&path, b"correct horse battery staple").unwrap();

        let id = ProfileId("personal".to_string());
        let mut profile = Profile::new(id.clone(), "personal", Ed25519Keypair::from_seed([7; 32]));
        profile
            .slots
            .insert(ProtocolKey::new("nostr", None), nostr_slot());
        profile.slots.insert(
            ProtocolKey::new("matrix", Some("homeserver-1".into())),
            matrix_slot(),
        );

        storage.save_profile(&profile).unwrap();

        let loaded = storage.load_profile(&id).unwrap();
        assert_eq!(loaded.display_name, "personal");
        assert_eq!(
            loaded.master.public_key().to_bytes(),
            profile.master.public_key().to_bytes()
        );
        assert_eq!(loaded.slots.len(), 2);
        assert!(loaded.has_slot(&ProtocolKey::new("nostr", None)));
        assert!(loaded.has_slot(&ProtocolKey::new("matrix", Some("homeserver-1".into()))));
    }

    #[test]
    fn wrong_passphrase_on_existing_file_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault.json");

        // First open + save with the right passphrase.
        let storage = PassphraseEncryptedStorage::open(&path, b"right").unwrap();
        let mut profile = Profile::new(
            ProfileId("p".into()),
            "p",
            Ed25519Keypair::from_seed([5; 32]),
        );
        profile
            .slots
            .insert(ProtocolKey::new("nostr", None), nostr_slot());
        storage.save_profile(&profile).unwrap();
        drop(storage);

        // Reopen with wrong passphrase — must error during open, not
        // silently produce a different KEK.
        let res = PassphraseEncryptedStorage::open(&path, b"wrong");
        match res {
            Err(IdentityError::Backend(msg)) => {
                assert!(msg.contains("incorrect passphrase"), "got: {msg}");
            }
            Err(e) => panic!("unexpected error variant: {e}"),
            Ok(_) => panic!("expected error, got Ok"),
        }
    }

    #[test]
    fn correct_passphrase_on_existing_file_succeeds() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault.json");

        let storage = PassphraseEncryptedStorage::open(&path, b"hunter2").unwrap();
        let mut profile = Profile::new(
            ProfileId("p".into()),
            "p",
            Ed25519Keypair::from_seed([5; 32]),
        );
        profile
            .slots
            .insert(ProtocolKey::new("nostr", None), nostr_slot());
        storage.save_profile(&profile).unwrap();
        drop(storage);

        let storage2 = PassphraseEncryptedStorage::open(&path, b"hunter2").unwrap();
        let loaded = storage2.load_profile(&ProfileId("p".into())).unwrap();
        assert_eq!(loaded.display_name, "p");
    }

    #[test]
    fn delete_profile_removes_from_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault.json");
        let storage = PassphraseEncryptedStorage::open(&path, b"pp").unwrap();
        let id = ProfileId("p".into());
        storage
            .save_profile(&Profile::new(
                id.clone(),
                "p",
                Ed25519Keypair::from_seed([1; 32]),
            ))
            .unwrap();
        storage.delete_profile(&id).unwrap();
        let summary = storage.list_profiles().unwrap();
        assert_eq!(summary.len(), 0);
    }

    #[test]
    fn vault_works_against_passphrase_storage() {
        // End-to-end: IdentityVault wraps PassphraseEncryptedStorage and
        // round-trips slots through the encrypted file.
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault.json");
        let storage = PassphraseEncryptedStorage::open(&path, b"pp").unwrap();

        let profile = Profile::new(
            ProfileId("p".into()),
            "p",
            Ed25519Keypair::from_seed([9; 32]),
        );
        let mut vault = IdentityVault::with_profile(storage, profile);
        vault
            .add_slot(ProtocolKey::new("nostr", None), nostr_slot())
            .unwrap();

        // Re-open from the same file.
        let storage2 = PassphraseEncryptedStorage::open(&path, b"pp").unwrap();
        let loaded = storage2.load_profile(&ProfileId("p".into())).unwrap();
        assert!(loaded.has_slot(&ProtocolKey::new("nostr", None)));
    }

    #[test]
    fn ciphertext_bytes_do_not_contain_plaintext_secrets() {
        // Light-touch sanity check: the on-disk file should not
        // contain the plaintext slot bytes.
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault.json");
        let storage = PassphraseEncryptedStorage::open(&path, b"pp").unwrap();
        let mut profile = Profile::new(
            ProfileId("p".into()),
            "test-display-name-not-secret",
            Ed25519Keypair::from_seed([3; 32]),
        );
        // Use a distinctive marker as the slot payload.
        profile.slots.insert(
            ProtocolKey::new("test", None),
            IdentitySlot::Direct {
                kind: "test".to_string(),
                payload: SecretBytes::new(b"DISTINCTIVE_SECRET_MARKER".to_vec()),
                lineage: CredentialLineage::LocallyDerived,
                unlock_tier: UnlockTier::Session,
            },
        );
        storage.save_profile(&profile).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        // Display name is plaintext-encoded inside the ciphertext, so
        // it should NOT appear directly. Same for the secret marker.
        assert!(
            !bytes
                .windows(b"DISTINCTIVE_SECRET_MARKER".len())
                .any(|w| w == b"DISTINCTIVE_SECRET_MARKER")
        );
        assert!(
            !bytes
                .windows(b"test-display-name-not-secret".len())
                .any(|w| w == b"test-display-name-not-secret")
        );
    }
}
