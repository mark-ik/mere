//! Sealed-record storage for small typed secret records.
//!
//! This backend complements the profile-oriented [`crate::vault`] storage with a
//! record-oriented store that seals one typed serde value per path. It owns the
//! sealed-record wire format and authenticated-encryption mechanics, but not the
//! unlock ceremony: callers provide the already-unlocked 32-byte record root key.
//!
//! That split is deliberate. Some callers will eventually source this root from
//! the passphrase-unlocked vault root, while others need a seed-derived wrapping
//! key for syncable records. One backend, different key ladders.

use std::path::{Component, Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::IdentityError;

const SEALED_RECORD_FORMAT_VERSION: u8 = 1;
const NONCE_LEN: usize = 12;

#[derive(Debug, Serialize, Deserialize)]
struct SealedRecordEnvelope {
    version: u8,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

/// Directory-backed sealed-record storage.
///
/// Each relative record path maps to one sealed JSON envelope on disk. The
/// record path itself is bound into AEAD associated data so ciphertext cannot be
/// transparently copied to a different logical record id.
pub struct SealedRecordStorage {
    root: PathBuf,
    key: Zeroizing<[u8; 32]>,
}

impl SealedRecordStorage {
    /// Open a sealed-record store rooted at `root`, using an already-unlocked
    /// 32-byte root key.
    pub fn open_with_key(root: impl Into<PathBuf>, key: [u8; 32]) -> Self {
        Self {
            root: root.into(),
            key: Zeroizing::new(key),
        }
    }

    /// Load one typed record, or `None` when absent.
    pub fn load_record<T>(&self, relative: impl AsRef<Path>) -> Result<Option<T>, IdentityError>
    where
        T: DeserializeOwned,
    {
        let (path, aad) = resolve_record_path(&self.root, relative.as_ref())?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(IdentityError::Backend(format!(
                    "read sealed record {:?}: {err}",
                    path
                )));
            }
        };
        let envelope: SealedRecordEnvelope = serde_json::from_slice(&bytes).map_err(|err| {
            IdentityError::Backend(format!("parse sealed record {:?}: {err}", path))
        })?;
        if envelope.version != SEALED_RECORD_FORMAT_VERSION {
            return Err(IdentityError::Backend(format!(
                "unsupported sealed-record version {} at {:?}",
                envelope.version, path
            )));
        }
        if envelope.nonce.len() != NONCE_LEN {
            return Err(IdentityError::Backend(format!(
                "sealed record {:?} has nonce length {}, expected {}",
                path,
                envelope.nonce.len(),
                NONCE_LEN
            )));
        }
        let cipher = ChaCha20Poly1305::new(Key::from_slice(self.key.as_ref()));
        let nonce = Nonce::from_slice(&envelope.nonce);
        let plaintext = cipher
            .decrypt(
                nonce,
                Payload {
                    msg: envelope.ciphertext.as_slice(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| IdentityError::Backend(format!("decrypt sealed record {:?}", path)))?;
        let value = serde_json::from_slice(&plaintext).map_err(|err| {
            IdentityError::Backend(format!("decode sealed record plaintext {:?}: {err}", path))
        })?;
        Ok(Some(value))
    }

    /// Save one typed record atomically.
    pub fn save_record<T>(&self, relative: impl AsRef<Path>, value: &T) -> Result<(), IdentityError>
    where
        T: Serialize,
    {
        let (path, aad) = resolve_record_path(&self.root, relative.as_ref())?;
        let plaintext = serde_json::to_vec(value).map_err(|err| {
            IdentityError::Backend(format!("encode sealed record {:?}: {err}", path))
        })?;
        let nonce = random_bytes(NONCE_LEN);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(self.key.as_ref()));
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plaintext.as_slice(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|err| {
                IdentityError::Backend(format!("encrypt sealed record {:?}: {err}", path))
            })?;
        let envelope = SealedRecordEnvelope {
            version: SEALED_RECORD_FORMAT_VERSION,
            nonce,
            ciphertext,
        };
        save_json_atomic(&path, &envelope)
    }

    /// Delete one record. Missing files are ignored.
    pub fn delete_record(&self, relative: impl AsRef<Path>) -> Result<(), IdentityError> {
        let (path, _) = resolve_record_path(&self.root, relative.as_ref())?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(IdentityError::Backend(format!(
                "delete sealed record {:?}: {err}",
                path
            ))),
        }
    }
}

fn resolve_record_path(root: &Path, relative: &Path) -> Result<(PathBuf, String), IdentityError> {
    let mut normalized = PathBuf::new();
    let mut aad = String::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                if !aad.is_empty() {
                    aad.push('/');
                }
                aad.push_str(&part.to_string_lossy());
                normalized.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(IdentityError::Backend(format!(
                    "sealed record path must be relative and traversal-free: {:?}",
                    relative
                )));
            }
        }
    }
    if aad.is_empty() {
        return Err(IdentityError::Backend(
            "sealed record path must not be empty".to_string(),
        ));
    }
    Ok((root.join(normalized), aad))
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
        .map_err(|err| IdentityError::Backend(format!("serialize {:?}: {err}", path)))?;
    let parent = path.parent().ok_or_else(|| {
        IdentityError::Backend(format!("sealed record path has no parent: {:?}", path))
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|err| IdentityError::Backend(format!("create dir {:?}: {err}", parent)))?;
    let tmp = tempfile_in_dir(parent)?;
    std::fs::write(&tmp, &bytes)
        .map_err(|err| IdentityError::Backend(format!("write tmp {:?}: {err}", tmp)))?;
    std::fs::rename(&tmp, path).map_err(|err| {
        IdentityError::Backend(format!("rename tmp {:?} -> {:?}: {err}", tmp, path))
    })?;
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
    Ok(dir.join(format!(".sealed-record-{now}-{hex}.tmp")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct SampleRecord {
        label: String,
        bytes: Vec<u8>,
    }

    #[test]
    fn round_trips_a_typed_record() {
        let dir = tempdir().unwrap();
        let store = SealedRecordStorage::open_with_key(dir.path(), [0x11; 32]);
        let value = SampleRecord {
            label: "Pocket Meerkat".into(),
            bytes: vec![1, 2, 3, 4],
        };

        store
            .save_record("identity/test-record.json", &value)
            .unwrap();
        let restored = store
            .load_record::<SampleRecord>("identity/test-record.json")
            .unwrap()
            .unwrap();
        assert_eq!(restored, value);
    }

    #[test]
    fn ciphertext_is_bound_to_the_record_path() {
        let dir = tempdir().unwrap();
        let store = SealedRecordStorage::open_with_key(dir.path(), [0x22; 32]);
        let value = SampleRecord {
            label: "Studio PC".into(),
            bytes: vec![9, 8, 7],
        };

        store.save_record("identity/source.json", &value).unwrap();
        let source = dir.path().join("identity/source.json");
        let copied = dir.path().join("identity/copied.json");
        std::fs::create_dir_all(copied.parent().unwrap()).unwrap();
        std::fs::copy(&source, &copied).unwrap();

        let err = store
            .load_record::<SampleRecord>("identity/copied.json")
            .unwrap_err();
        assert!(
            err.to_string().contains("decrypt sealed record"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn stored_file_is_not_plaintext_json_of_the_record() {
        let dir = tempdir().unwrap();
        let store = SealedRecordStorage::open_with_key(dir.path(), [0x33; 32]);
        let value = SampleRecord {
            label: "Tablet".into(),
            bytes: vec![0xaa; 32],
        };

        store
            .save_record("identity/plaintext-check.json", &value)
            .unwrap();
        let bytes = std::fs::read(dir.path().join("identity/plaintext-check.json")).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("Tablet"));
        assert!(!text.contains("\"label\":\"Tablet\""));
    }
}
