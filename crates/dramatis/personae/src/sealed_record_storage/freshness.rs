//! External freshness evidence for authoritative sealed-record storage.
//!
//! The record directory and this ledger deliberately have different roots.
//! Restoring an older record directory while preserving the ledger is then
//! detectable even though every restored ciphertext still authenticates.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::IdentityError;

use super::save_json_atomic;

const LEDGER_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct RecordRevision {
    pub(super) generation: u64,
    pub(super) digest: [u8; 32],
    pub(super) deleted: bool,
}

#[derive(Serialize, Deserialize)]
struct LedgerEntry {
    version: u8,
    record_id: String,
    current: RecordRevision,
    pending: Option<RecordRevision>,
    mac: [u8; 32],
}

#[derive(Serialize)]
struct UnsignedLedgerEntry<'a> {
    version: u8,
    record_id: &'a str,
    current: RecordRevision,
    pending: Option<RecordRevision>,
}

#[derive(Clone)]
pub(super) struct FileFreshnessLedger {
    inner: Arc<FileFreshnessLedgerInner>,
}

struct FileFreshnessLedgerInner {
    root: PathBuf,
    key: Zeroizing<[u8; 32]>,
}

impl FileFreshnessLedger {
    pub(super) fn open(root: PathBuf, key: [u8; 32]) -> Result<Self, IdentityError> {
        std::fs::create_dir_all(&root).map_err(|error| {
            IdentityError::Backend(format!("create freshness ledger {root:?}: {error}"))
        })?;
        Ok(Self {
            inner: Arc::new(FileFreshnessLedgerInner {
                root,
                key: Zeroizing::new(key),
            }),
        })
    }

    pub(super) fn reconcile(
        &self,
        record_aad: &str,
        observed: RecordRevision,
        legacy_or_absent: bool,
    ) -> Result<RecordRevision, IdentityError> {
        let record_id = record_id(record_aad);
        let Some(mut entry) = self.load(&record_id)? else {
            if !legacy_or_absent {
                return Err(rollback_error(record_aad, "freshness evidence is missing"));
            }
            self.save(&record_id, observed, None)?;
            return Ok(observed);
        };

        match entry.pending {
            Some(pending) if observed == pending => {
                entry.current = pending;
                entry.pending = None;
                self.save(&record_id, entry.current, None)?;
                Ok(pending)
            }
            Some(_) if observed == entry.current => {
                entry.pending = None;
                self.save(&record_id, entry.current, None)?;
                Ok(entry.current)
            }
            Some(_) => Err(rollback_error(
                record_aad,
                "record matches neither the committed nor prepared revision",
            )),
            None if observed == entry.current => Ok(entry.current),
            None => Err(rollback_error(
                record_aad,
                "record does not match the latest committed revision",
            )),
        }
    }

    pub(super) fn prepare(
        &self,
        record_aad: &str,
        current: RecordRevision,
        next: RecordRevision,
    ) -> Result<(), IdentityError> {
        if next.generation != current.generation.saturating_add(1) {
            return Err(IdentityError::Backend(format!(
                "sealed record {record_aad:?} proposed non-consecutive generation {} after {}",
                next.generation, current.generation
            )));
        }
        let record_id = record_id(record_aad);
        let entry = self.load(&record_id)?.ok_or_else(|| {
            IdentityError::Backend(format!(
                "sealed record {record_aad:?} has no initialized freshness evidence"
            ))
        })?;
        if entry.current != current || entry.pending.is_some() {
            return Err(rollback_error(
                record_aad,
                "freshness evidence changed before the update began",
            ));
        }
        self.save(&record_id, current, Some(next))
    }

    pub(super) fn commit(
        &self,
        record_aad: &str,
        next: RecordRevision,
    ) -> Result<(), IdentityError> {
        let record_id = record_id(record_aad);
        let entry = self.load(&record_id)?.ok_or_else(|| {
            IdentityError::Backend(format!(
                "sealed record {record_aad:?} lost its freshness evidence during commit"
            ))
        })?;
        if entry.pending != Some(next) {
            return Err(rollback_error(
                record_aad,
                "prepared revision changed before commit",
            ));
        }
        self.save(&record_id, next, None)
    }

    fn load(&self, record_id: &str) -> Result<Option<LedgerEntry>, IdentityError> {
        let path = self.path(record_id);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(IdentityError::Backend(format!(
                    "read freshness evidence {path:?}: {error}"
                )));
            }
        };
        let entry: LedgerEntry = serde_json::from_slice(&bytes).map_err(|error| {
            IdentityError::Backend(format!("parse freshness evidence {path:?}: {error}"))
        })?;
        if entry.version != LEDGER_VERSION || entry.record_id != record_id {
            return Err(IdentityError::Backend(format!(
                "freshness evidence {path:?} has an unsupported identity or version"
            )));
        }
        let expected = self.mac(record_id, entry.current, entry.pending)?;
        if !constant_time_eq(&entry.mac, &expected) {
            return Err(IdentityError::Backend(format!(
                "freshness evidence {path:?} failed authentication"
            )));
        }
        Ok(Some(entry))
    }

    fn save(
        &self,
        record_id: &str,
        current: RecordRevision,
        pending: Option<RecordRevision>,
    ) -> Result<(), IdentityError> {
        let entry = LedgerEntry {
            version: LEDGER_VERSION,
            record_id: record_id.to_string(),
            current,
            pending,
            mac: self.mac(record_id, current, pending)?,
        };
        save_json_atomic(&self.path(record_id), &entry)
    }

    fn mac(
        &self,
        record_id: &str,
        current: RecordRevision,
        pending: Option<RecordRevision>,
    ) -> Result<[u8; 32], IdentityError> {
        let unsigned = UnsignedLedgerEntry {
            version: LEDGER_VERSION,
            record_id,
            current,
            pending,
        };
        let bytes = serde_json::to_vec(&unsigned).map_err(|error| {
            IdentityError::Backend(format!("encode freshness evidence: {error}"))
        })?;
        Ok(*blake3::keyed_hash(&self.inner.key, &bytes).as_bytes())
    }

    fn path(&self, record_id: &str) -> PathBuf {
        self.inner
            .root
            .join("records")
            .join(format!("{record_id}.json"))
    }
}

pub(super) fn revision(bytes: Option<&[u8]>, generation: u64, deleted: bool) -> RecordRevision {
    let digest = bytes
        .map(|bytes| *blake3::hash(bytes).as_bytes())
        .unwrap_or([0; 32]);
    RecordRevision {
        generation,
        digest,
        deleted,
    }
}

fn record_id(record_aad: &str) -> String {
    blake3::hash(record_aad.as_bytes()).to_hex().to_string()
}

fn rollback_error(record_aad: &str, detail: &str) -> IdentityError {
    IdentityError::Backend(format!(
        "sealed record rollback detected for {record_aad:?}: {detail}"
    ))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (&left, &right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

pub(super) fn roots_are_separate(records: &Path, ledger: &Path) -> bool {
    !ledger.starts_with(records) && !records.starts_with(ledger)
}
