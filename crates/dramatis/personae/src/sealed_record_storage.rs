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

use std::fs::{File, TryLockError};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::IdentityError;

mod freshness;

use freshness::{FileFreshnessLedger, RecordRevision, revision, roots_are_separate};

const LEGACY_SEALED_RECORD_FORMAT_VERSION: u8 = 1;
const SEALED_RECORD_FORMAT_VERSION: u8 = 2;
const NONCE_LEN: usize = 12;

#[derive(Debug, Serialize, Deserialize)]
struct SealedRecordEnvelope {
    version: u8,
    #[serde(default)]
    generation: u64,
    #[serde(default)]
    deleted: bool,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

struct AuthoritativeStore {
    _lease: File,
    freshness: FileFreshnessLedger,
}

/// What an atomic record update should do after inspecting the current value.
pub enum SealedRecordChange<T> {
    /// Return the closure's result without writing the record.
    Keep,
    /// Seal and atomically replace the record before returning the result.
    Replace(T),
}

/// Directory-backed sealed-record storage.
///
/// Each relative record path maps to one sealed JSON envelope on disk. The
/// record path itself is bound into AEAD associated data so ciphertext cannot be
/// transparently copied to a different logical record id.
#[derive(Clone)]
pub struct SealedRecordStorage {
    root: PathBuf,
    key: Zeroizing<[u8; 32]>,
    update_lock: Arc<Mutex<()>>,
    authority: Option<Arc<AuthoritativeStore>>,
}

impl SealedRecordStorage {
    /// Open a sealed-record store rooted at `root`, using an already-unlocked
    /// 32-byte root key.
    pub fn open_with_key(root: impl Into<PathBuf>, key: [u8; 32]) -> Self {
        Self {
            root: root.into(),
            key: Zeroizing::new(key),
            update_lock: Arc::new(Mutex::new(())),
            authority: None,
        }
    }

    /// Claim exclusive process authority over a record directory and bind it
    /// to a separately rooted, keyed freshness ledger.
    ///
    /// Every clone retains the exclusive OS file lock. A second authority in
    /// this or another process fails immediately. The freshness ledger detects
    /// restoration of an older authenticated record directory as long as the
    /// ledger itself was not rolled back with it. Existing version-one records
    /// establish a generation-zero baseline on first authoritative access.
    pub fn claim_with_file_freshness(
        root: impl Into<PathBuf>,
        key: [u8; 32],
        freshness_root: impl Into<PathBuf>,
        freshness_key: [u8; 32],
    ) -> Result<Self, IdentityError> {
        let root = absolute_directory(root.into())?;
        let freshness_root = absolute_directory(freshness_root.into())?;
        if !roots_are_separate(&root, &freshness_root) {
            return Err(IdentityError::Backend(format!(
                "sealed records {root:?} and freshness evidence {freshness_root:?} need separate roots"
            )));
        }
        let lease_path = root.join(".personae-authority.lock");
        let lease = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lease_path)
            .map_err(|error| {
                IdentityError::Backend(format!(
                    "open sealed-record authority {lease_path:?}: {error}"
                ))
            })?;
        match lease.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(IdentityError::Backend(format!(
                    "sealed-record authority is already held for {root:?}"
                )));
            }
            Err(TryLockError::Error(error)) => {
                return Err(IdentityError::Backend(format!(
                    "claim sealed-record authority {lease_path:?}: {error}"
                )));
            }
        }
        Ok(Self {
            root,
            key: Zeroizing::new(key),
            update_lock: Arc::new(Mutex::new(())),
            authority: Some(Arc::new(AuthoritativeStore {
                _lease: lease,
                freshness: FileFreshnessLedger::open(freshness_root, freshness_key)?,
            })),
        })
    }

    /// Load one typed record, or `None` when absent.
    pub fn load_record<T>(&self, relative: impl AsRef<Path>) -> Result<Option<T>, IdentityError>
    where
        T: DeserializeOwned,
    {
        let _guard = self.lock_updates();
        self.load_record_unlocked(relative.as_ref())
    }

    fn load_record_unlocked<T>(&self, relative: &Path) -> Result<Option<T>, IdentityError>
    where
        T: DeserializeOwned,
    {
        let (path, aad) = resolve_record_path(&self.root, relative)?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                self.reconcile_freshness(&aad, revision(None, 0, true), true)?;
                return Ok(None);
            }
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
        if !matches!(
            envelope.version,
            LEGACY_SEALED_RECORD_FORMAT_VERSION | SEALED_RECORD_FORMAT_VERSION
        ) {
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
        let legacy = envelope.version == LEGACY_SEALED_RECORD_FORMAT_VERSION;
        let generation = if legacy { 0 } else { envelope.generation };
        self.reconcile_freshness(
            &aad,
            revision(Some(&bytes), generation, envelope.deleted),
            legacy,
        )?;
        if envelope.deleted {
            return Ok(None);
        }
        let cipher = ChaCha20Poly1305::new(
            &Key::try_from(&self.key.as_ref()[..]).expect("fixed-length key material"),
        );
        let nonce = &Nonce::try_from(&envelope.nonce[..]).expect("fixed-length key material");
        let encryption_aad = envelope_aad(&aad, envelope.version, generation, false);
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    nonce,
                    Payload {
                        msg: envelope.ciphertext.as_slice(),
                        aad: encryption_aad.as_bytes(),
                    },
                )
                .map_err(|_| IdentityError::Backend(format!("decrypt sealed record {:?}", path)))?,
        );
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
        let _guard = self.lock_updates();
        self.save_record_unlocked(relative.as_ref(), value)
    }

    fn save_record_unlocked<T>(&self, relative: &Path, value: &T) -> Result<(), IdentityError>
    where
        T: Serialize,
    {
        let (path, aad) = resolve_record_path(&self.root, relative)?;
        let current = self.current_revision(&path, &aad)?;
        let generation = current.generation.checked_add(1).ok_or_else(|| {
            IdentityError::Backend(format!("sealed record {aad:?} exhausted its generation"))
        })?;
        let plaintext = Zeroizing::new(serde_json::to_vec(value).map_err(|err| {
            IdentityError::Backend(format!("encode sealed record {:?}: {err}", path))
        })?);
        let nonce = random_bytes(NONCE_LEN);
        let cipher = ChaCha20Poly1305::new(
            &Key::try_from(&self.key.as_ref()[..]).expect("fixed-length key material"),
        );
        let encryption_aad = envelope_aad(&aad, SEALED_RECORD_FORMAT_VERSION, generation, false);
        let ciphertext = cipher
            .encrypt(
                &Nonce::try_from(&nonce[..]).expect("fixed-length key material"),
                Payload {
                    msg: plaintext.as_slice(),
                    aad: encryption_aad.as_bytes(),
                },
            )
            .map_err(|err| {
                IdentityError::Backend(format!("encrypt sealed record {:?}: {err}", path))
            })?;
        let envelope = SealedRecordEnvelope {
            version: SEALED_RECORD_FORMAT_VERSION,
            generation,
            deleted: false,
            nonce,
            ciphertext,
        };
        self.replace_authoritatively(&path, &aad, current, &envelope)
    }

    /// Read, inspect, and optionally replace one record under a lock shared by
    /// clones of this opened store.
    ///
    /// The replacement is visible only after its sealed file has been flushed
    /// and renamed. This is an in-process transaction boundary; independent
    /// store openings and rollback of the backing directory require a higher
    /// storage authority.
    pub fn update_record<T, R, E, F>(&self, relative: impl AsRef<Path>, update: F) -> Result<R, E>
    where
        T: DeserializeOwned + Serialize,
        E: From<IdentityError>,
        F: FnOnce(Option<T>) -> Result<(R, SealedRecordChange<T>), E>,
    {
        let _guard = self.lock_updates();
        let relative = relative.as_ref();
        let current = self.load_record_unlocked(relative).map_err(E::from)?;
        let (result, change) = update(current)?;
        if let SealedRecordChange::Replace(next) = change {
            self.save_record_unlocked(relative, &next)
                .map_err(E::from)?;
        }
        Ok(result)
    }

    /// Delete one record. Missing files are ignored.
    pub fn delete_record(&self, relative: impl AsRef<Path>) -> Result<(), IdentityError> {
        let _guard = self.lock_updates();
        let (path, aad) = resolve_record_path(&self.root, relative.as_ref())?;
        if self.authority.is_some() {
            let current = self.current_revision(&path, &aad)?;
            let generation = current.generation.checked_add(1).ok_or_else(|| {
                IdentityError::Backend(format!("sealed record {aad:?} exhausted its generation"))
            })?;
            let nonce = random_bytes(NONCE_LEN);
            let cipher = ChaCha20Poly1305::new(
                &Key::try_from(&self.key.as_ref()[..]).expect("fixed-length key material"),
            );
            let encryption_aad = envelope_aad(&aad, SEALED_RECORD_FORMAT_VERSION, generation, true);
            let ciphertext = cipher
                .encrypt(
                    &Nonce::try_from(&nonce[..]).expect("fixed-length key material"),
                    Payload {
                        msg: &[],
                        aad: encryption_aad.as_bytes(),
                    },
                )
                .map_err(|error| {
                    IdentityError::Backend(format!("seal tombstone for {path:?}: {error}"))
                })?;
            let envelope = SealedRecordEnvelope {
                version: SEALED_RECORD_FORMAT_VERSION,
                generation,
                deleted: true,
                nonce,
                ciphertext,
            };
            return self.replace_authoritatively(&path, &aad, current, &envelope);
        }
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(IdentityError::Backend(format!(
                "delete sealed record {:?}: {err}",
                path
            ))),
        }
    }

    fn lock_updates(&self) -> MutexGuard<'_, ()> {
        self.update_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn current_revision(&self, path: &Path, aad: &str) -> Result<RecordRevision, IdentityError> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let absent = revision(None, 0, true);
                return self.reconcile_freshness(aad, absent, true);
            }
            Err(error) => {
                return Err(IdentityError::Backend(format!(
                    "read sealed record {path:?}: {error}"
                )));
            }
        };
        let envelope: SealedRecordEnvelope = serde_json::from_slice(&bytes).map_err(|error| {
            IdentityError::Backend(format!("parse sealed record {path:?}: {error}"))
        })?;
        let legacy = envelope.version == LEGACY_SEALED_RECORD_FORMAT_VERSION;
        if !legacy && envelope.version != SEALED_RECORD_FORMAT_VERSION {
            return Err(IdentityError::Backend(format!(
                "unsupported sealed-record version {} at {path:?}",
                envelope.version
            )));
        }
        let generation = if legacy { 0 } else { envelope.generation };
        self.reconcile_freshness(
            aad,
            revision(Some(&bytes), generation, envelope.deleted),
            legacy,
        )
    }

    fn reconcile_freshness(
        &self,
        aad: &str,
        observed: RecordRevision,
        legacy_or_absent: bool,
    ) -> Result<RecordRevision, IdentityError> {
        match &self.authority {
            Some(authority) => authority
                .freshness
                .reconcile(aad, observed, legacy_or_absent),
            None => Ok(observed),
        }
    }

    fn replace_authoritatively<T: Serialize>(
        &self,
        path: &Path,
        aad: &str,
        current: RecordRevision,
        envelope: &T,
    ) -> Result<(), IdentityError> {
        let bytes = serde_json::to_vec(envelope)
            .map_err(|error| IdentityError::Backend(format!("serialize {path:?}: {error}")))?;
        let parsed: SealedRecordEnvelope = serde_json::from_slice(&bytes).map_err(|error| {
            IdentityError::Backend(format!("inspect sealed record {path:?}: {error}"))
        })?;
        let next = revision(Some(&bytes), parsed.generation, parsed.deleted);
        if let Some(authority) = &self.authority {
            authority.freshness.prepare(aad, current, next)?;
            save_bytes_atomic(path, &bytes)?;
            authority.freshness.commit(aad, next)
        } else {
            save_bytes_atomic(path, &bytes)
        }
    }
}

fn absolute_directory(path: PathBuf) -> Result<PathBuf, IdentityError> {
    std::fs::create_dir_all(&path)
        .map_err(|error| IdentityError::Backend(format!("create directory {path:?}: {error}")))?;
    path.canonicalize()
        .map_err(|error| IdentityError::Backend(format!("resolve directory {path:?}: {error}")))
}

fn envelope_aad(path_aad: &str, version: u8, generation: u64, deleted: bool) -> String {
    if version == LEGACY_SEALED_RECORD_FORMAT_VERSION {
        path_aad.to_string()
    } else {
        format!("personae/sealed-record/v{version}/{generation}/{deleted}/{path_aad}")
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
    save_bytes_atomic(path, &bytes)
}

fn save_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), IdentityError> {
    let parent = path.parent().ok_or_else(|| {
        IdentityError::Backend(format!("sealed record path has no parent: {:?}", path))
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|err| IdentityError::Backend(format!("create dir {:?}: {err}", parent)))?;
    let tmp = tempfile_in_dir(parent)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|err| IdentityError::Backend(format!("create tmp {:?}: {err}", tmp)))?;
    if let Err(err) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(&tmp);
        return Err(IdentityError::Backend(format!(
            "write and flush tmp {:?}: {err}",
            tmp
        )));
    }
    drop(file);
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(IdentityError::Backend(format!(
            "rename tmp {:?} -> {:?}: {err}",
            tmp, path
        )));
    }
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|err| IdentityError::Backend(format!("flush dir {:?}: {err}", parent)))?;
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

    #[derive(Debug, Serialize, Deserialize)]
    struct CounterRecord {
        value: u64,
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

    #[test]
    fn cloned_store_updates_are_one_load_modify_replace_transaction() {
        let dir = tempdir().unwrap();
        let store = SealedRecordStorage::open_with_key(dir.path(), [0x44; 32]);
        store
            .save_record("counter/value.json", &CounterRecord { value: 0 })
            .unwrap();

        let increment = |store: SealedRecordStorage| {
            std::thread::spawn(move || {
                for _ in 0..50 {
                    store
                        .update_record(
                            "counter/value.json",
                            |current: Option<CounterRecord>| -> Result<_, IdentityError> {
                                let mut current = current.expect("counter exists");
                                current.value += 1;
                                Ok(((), SealedRecordChange::Replace(current)))
                            },
                        )
                        .unwrap();
                }
            })
        };
        let left = increment(store.clone());
        let right = increment(store.clone());
        left.join().unwrap();
        right.join().unwrap();

        let restored = store
            .load_record::<CounterRecord>("counter/value.json")
            .unwrap()
            .unwrap();
        assert_eq!(restored.value, 100);
    }

    fn authoritative_store(root: &Path, freshness: &Path) -> SealedRecordStorage {
        SealedRecordStorage::claim_with_file_freshness(root, [0x55; 32], freshness, [0x56; 32])
            .unwrap()
    }

    #[test]
    fn authoritative_opening_is_exclusive_until_every_clone_drops() {
        let dir = tempdir().unwrap();
        let records = dir.path().join("records");
        let freshness = dir.path().join("freshness");
        let first = authoritative_store(&records, &freshness);
        let retained = first.clone();

        let error = SealedRecordStorage::claim_with_file_freshness(
            &records, [0x55; 32], &freshness, [0x56; 32],
        )
        .err()
        .expect("a second authority must be refused");
        assert!(error.to_string().contains("already held"));

        drop(first);
        assert!(
            SealedRecordStorage::claim_with_file_freshness(
                &records, [0x55; 32], &freshness, [0x56; 32],
            )
            .is_err()
        );
        drop(retained);
        authoritative_store(&records, &freshness);
    }

    #[test]
    fn external_freshness_evidence_rejects_an_older_valid_record() {
        let dir = tempdir().unwrap();
        let records = dir.path().join("records");
        let freshness = dir.path().join("freshness");
        let store = authoritative_store(&records, &freshness);
        let relative = "counter/value.json";
        let path = records.join(relative);
        store
            .save_record(relative, &CounterRecord { value: 1 })
            .unwrap();
        let older_valid_record = std::fs::read(&path).unwrap();
        store
            .save_record(relative, &CounterRecord { value: 2 })
            .unwrap();

        std::fs::write(&path, older_valid_record).unwrap();

        let error = store.load_record::<CounterRecord>(relative).unwrap_err();
        assert!(error.to_string().contains("rollback detected"));
    }

    #[test]
    fn authoritative_tombstone_rejects_resurrection_after_delete() {
        let dir = tempdir().unwrap();
        let records = dir.path().join("records");
        let freshness = dir.path().join("freshness");
        let store = authoritative_store(&records, &freshness);
        let relative = "identity/deleted.json";
        let path = records.join(relative);
        store
            .save_record(
                relative,
                &SampleRecord {
                    label: "Retired".into(),
                    bytes: vec![7; 16],
                },
            )
            .unwrap();
        let live_record = std::fs::read(&path).unwrap();

        store.delete_record(relative).unwrap();
        assert_eq!(store.load_record::<SampleRecord>(relative).unwrap(), None);
        std::fs::write(&path, live_record).unwrap();

        let error = store.load_record::<SampleRecord>(relative).unwrap_err();
        assert!(error.to_string().contains("rollback detected"));
    }
}
