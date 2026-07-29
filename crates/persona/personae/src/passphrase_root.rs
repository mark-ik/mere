//! Passphrase-wrapped vault root — the cross-platform unlock backend.
//!
//! The sealed-record store ([`crate::sealed_record_storage`]) unlocks from an
//! already-obtained 32-byte vault root. [`crate::startup_unlock`] obtains that
//! root by wrapping a random key with the OS store (DPAPI today, Windows only).
//! This module is the second wrapper over the *same kind of root*: an
//! Argon2id-derived key-encryption key seals the vault root under a user
//! passphrase, so any platform has a user-held root of trust, and the
//! declared-but-unimplemented [`StartupUnlockMode::Prompt`] mode has a backend.
//!
//! [`StartupUnlockMode::Prompt`]: crate::StartupUnlockMode::Prompt
//!
//! ## The dual-wrapper model
//!
//! A vault root is a random 32 bytes. The OS-store wrapper and the passphrase
//! wrapper are two independent seals over that one root, so a device can carry
//! both: the passphrase as the root of trust, the OS store as silent
//! convenience. Enrolling a passphrase after the OS store already minted a root
//! must **re-wrap that existing root**, never mint a new one, or records sealed
//! under the old root become unreadable. [`save_passphrase_root`] takes the root
//! as an explicit input for exactly this reason; [`change_passphrase`] re-wraps
//! in place.
//!
//! ## Crypto
//!
//! - **Argon2id** derives the KEK from passphrase + per-file salt (the same KDF
//!   as [`crate::passphrase_storage`], via `derive_kek`).
//! - **ChaCha20-Poly1305** seals the 32-byte root, with a context string bound
//!   as associated data so the ciphertext cannot be repurposed as some other
//!   record.
//! - The root never touches disk unsealed; intermediate plaintext is held in
//!   [`Zeroizing`] buffers.

use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
 
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::IdentityError;
use crate::passphrase_storage::derive_kek;

const PASSPHRASE_ROOT_FILE_VERSION: u8 = 1;
const ARGON2_SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

/// Backend tag stored in the file, mirroring the OS-store wrapper's `backend`
/// field so a root's provenance is legible on disk.
pub const PASSPHRASE_ROOT_BACKEND: &str = "argon2id-chacha20poly1305-v1";

/// Associated data binding the seal to its purpose, so a wrapped-root envelope
/// cannot be swapped in for a different sealed record.
const PASSPHRASE_ROOT_AAD: &[u8] = b"mere.identity.vault-root.passphrase.v1";

/// A passphrase-sealed vault root, as written to disk.
///
/// Fields are private: obtain one from [`wrap_vault_root`] or from disk via
/// [`load_passphrase_root`]. The wrapped root is opaque ciphertext; the salt and
/// nonce are public inputs, safe to store in the clear.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassphraseWrappedRoot {
    version: u8,
    backend: String,
    salt: Vec<u8>,
    nonce: Vec<u8>,
    wrapped_root: Vec<u8>,
}

/// Seal a 32-byte vault root under `passphrase` with a fresh salt and nonce.
pub fn wrap_vault_root(
    root: &[u8; 32],
    passphrase: &[u8],
) -> Result<PassphraseWrappedRoot, IdentityError> {
    let salt = random_bytes(ARGON2_SALT_LEN);
    let kek = derive_kek(passphrase, &salt)?;
    let nonce = random_bytes(NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(kek.as_ref()));
    let wrapped_root = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: root.as_slice(),
                aad: PASSPHRASE_ROOT_AAD,
            },
        )
        .map_err(|err| IdentityError::Backend(format!("wrap vault root: {err}")))?;
    Ok(PassphraseWrappedRoot {
        version: PASSPHRASE_ROOT_FILE_VERSION,
        backend: PASSPHRASE_ROOT_BACKEND.to_string(),
        salt,
        nonce,
        wrapped_root,
    })
}

/// Recover the 32-byte vault root from a sealed envelope with the passphrase
/// that produced it. A wrong passphrase, a tampered envelope, or a corrupt file
/// all surface as an authentication failure.
pub fn unwrap_vault_root(
    file: &PassphraseWrappedRoot,
    passphrase: &[u8],
) -> Result<[u8; 32], IdentityError> {
    if file.version != PASSPHRASE_ROOT_FILE_VERSION {
        return Err(IdentityError::Backend(format!(
            "unsupported passphrase vault-root version {}",
            file.version
        )));
    }
    if file.nonce.len() != NONCE_LEN {
        return Err(IdentityError::Backend(format!(
            "passphrase vault root has nonce length {}, expected {NONCE_LEN}",
            file.nonce.len()
        )));
    }
    let kek = derive_kek(passphrase, &file.salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(kek.as_ref()));
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(&file.nonce),
                Payload {
                    msg: file.wrapped_root.as_slice(),
                    aad: PASSPHRASE_ROOT_AAD,
                },
            )
            .map_err(|_| {
                IdentityError::Backend("incorrect passphrase or corrupt vault root".to_string())
            })?,
    );
    let root: [u8; 32] = plaintext.as_slice().try_into().map_err(|_| {
        IdentityError::Backend("vault root did not decrypt to 32 bytes".to_string())
    })?;
    Ok(root)
}

/// Whether a passphrase-wrapped vault root file already exists at `path`.
pub fn passphrase_root_exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().is_file()
}

/// Seal `root` under `passphrase` and write it atomically to `path`.
///
/// The root is an explicit input so callers re-wrap an existing root (e.g. one
/// the OS-store wrapper already minted) rather than accidentally minting a new
/// one and orphaning already-sealed records.
pub fn save_passphrase_root(
    path: impl AsRef<Path>,
    root: &[u8; 32],
    passphrase: &[u8],
) -> Result<(), IdentityError> {
    let file = wrap_vault_root(root, passphrase)?;
    save_json_atomic(path.as_ref(), &file)
}

/// Load and unseal the vault root at `path`, or `None` when the file is absent.
pub fn load_passphrase_root(
    path: impl AsRef<Path>,
    passphrase: &[u8],
) -> Result<Option<[u8; 32]>, IdentityError> {
    let path = path.as_ref();
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|err| {
        IdentityError::Backend(format!("read passphrase vault root {path:?}: {err}"))
    })?;
    let file: PassphraseWrappedRoot = serde_json::from_slice(&bytes).map_err(|err| {
        IdentityError::Backend(format!("parse passphrase vault root {path:?}: {err}"))
    })?;
    unwrap_vault_root(&file, passphrase).map(Some)
}

/// Re-wrap the vault root at `path` from `old_passphrase` to `new_passphrase`,
/// preserving the root itself (a fresh salt and nonce are minted). Fails if no
/// wrapped root exists or the old passphrase is wrong.
pub fn change_passphrase(
    path: impl AsRef<Path>,
    old_passphrase: &[u8],
    new_passphrase: &[u8],
) -> Result<(), IdentityError> {
    let path = path.as_ref();
    let root = Zeroizing::new(load_passphrase_root(path, old_passphrase)?.ok_or_else(|| {
        IdentityError::Backend(format!(
            "no passphrase-wrapped vault root at {path:?} to re-key"
        ))
    })?);
    save_passphrase_root(path, &root, new_passphrase)
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).expect("OS randomness available");
    buf
}

fn save_json_atomic<T>(path: &Path, value: &T) -> Result<(), IdentityError>
where
    T: Serialize,
{
    let bytes = serde_json::to_vec(value)
        .map_err(|err| IdentityError::Backend(format!("serialize {path:?}: {err}")))?;
    let parent = path.parent().ok_or_else(|| {
        IdentityError::Backend(format!(
            "passphrase vault-root path has no parent: {path:?}"
        ))
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|err| IdentityError::Backend(format!("create dir {parent:?}: {err}")))?;
    let tmp = tempfile_in_dir(parent)?;
    std::fs::write(&tmp, &bytes)
        .map_err(|err| IdentityError::Backend(format!("write tmp {tmp:?}: {err}")))?;
    std::fs::rename(&tmp, path)
        .map_err(|err| IdentityError::Backend(format!("rename tmp {tmp:?} -> {path:?}: {err}")))?;
    Ok(())
}

fn tempfile_in_dir(dir: &Path) -> Result<PathBuf, IdentityError> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| IdentityError::Backend(format!("time: {err}")))?
        .as_nanos();
    let mut rand = [0u8; 8];
    getrandom::fill(&mut rand).expect("OS randomness available");
    let mut hex = String::with_capacity(16);
    for byte in rand {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok(dir.join(format!(".passphrase-root-{now}-{hex}.tmp")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sealed_record_storage::SealedRecordStorage;
    use tempfile::tempdir;

    #[test]
    fn wrap_then_unwrap_round_trips() {
        let root = [7u8; 32];
        let file = wrap_vault_root(&root, b"correct horse battery staple").unwrap();
        let recovered = unwrap_vault_root(&file, b"correct horse battery staple").unwrap();
        assert_eq!(recovered, root);
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let file = wrap_vault_root(&[9u8; 32], b"right").unwrap();
        let err = unwrap_vault_root(&file, b"wrong").unwrap_err();
        assert!(
            err.to_string().contains("incorrect passphrase"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn save_and_load_round_trips_through_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("identity/vault-root.pass.json");
        let root = [0x33u8; 32];

        assert!(!passphrase_root_exists(&path));
        save_passphrase_root(&path, &root, b"hunter2").unwrap();
        assert!(passphrase_root_exists(&path));

        let loaded = load_passphrase_root(&path, b"hunter2").unwrap().unwrap();
        assert_eq!(loaded, root);
    }

    #[test]
    fn load_absent_root_is_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nope.json");
        assert_eq!(load_passphrase_root(&path, b"whatever").unwrap(), None);
    }

    #[test]
    fn change_passphrase_preserves_root_and_rejects_the_old_one() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault-root.pass.json");
        let root = [0x5cu8; 32];
        save_passphrase_root(&path, &root, b"old-pass").unwrap();

        change_passphrase(&path, b"old-pass", b"new-pass").unwrap();

        // The same root comes back under the new passphrase...
        assert_eq!(
            load_passphrase_root(&path, b"new-pass").unwrap().unwrap(),
            root
        );
        // ...and the old passphrase no longer opens it.
        assert!(load_passphrase_root(&path, b"old-pass").is_err());
    }

    #[test]
    fn two_passphrases_wrap_the_same_root() {
        // Models the dual-wrapper design: one root, independently sealed twice
        // (here two passphrases; in production one is the OS-store wrapper).
        let root = [0xa5u8; 32];
        let a = wrap_vault_root(&root, b"passphrase-a").unwrap();
        let b = wrap_vault_root(&root, b"passphrase-b").unwrap();
        assert_eq!(unwrap_vault_root(&a, b"passphrase-a").unwrap(), root);
        assert_eq!(unwrap_vault_root(&b, b"passphrase-b").unwrap(), root);
    }

    #[test]
    fn wrapped_file_does_not_contain_the_raw_root() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault-root.pass.json");
        let root = [0xabu8; 32];
        save_passphrase_root(&path, &root, b"pp").unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(
            !bytes.windows(root.len()).any(|w| w == root),
            "raw root bytes appeared in the on-disk file"
        );
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let mut file = wrap_vault_root(&[1u8; 32], b"pp").unwrap();
        file.wrapped_root[0] ^= 0xff;
        assert!(unwrap_vault_root(&file, b"pp").is_err());
    }

    #[test]
    fn passphrase_root_opens_a_sealed_record_store() {
        // The point of the whole slice: a passphrase-unlocked root is a valid
        // vault root that the sealed-record store can seal and unseal under.
        let dir = tempdir().unwrap();
        let root_path = dir.path().join("identity/vault-root.pass.json");
        let mut root = [0u8; 32];
        getrandom::fill(&mut root).expect("OS randomness available");
        save_passphrase_root(&root_path, &root, b"open sesame").unwrap();

        let unlocked = load_passphrase_root(&root_path, b"open sesame")
            .unwrap()
            .unwrap();
        let store = SealedRecordStorage::open_with_key(dir.path(), unlocked);
        store
            .save_record("identity/master.seed", &[42u8; 32])
            .unwrap();
        let restored: [u8; 32] = store.load_record("identity/master.seed").unwrap().unwrap();
        assert_eq!(restored, [42u8; 32]);
    }
}
