//! A muniment-backed p2panda operation store.
//!
//! [`MunimentStore`] implements the three traits p2panda-net's `LogSync`
//! reconciles against, [`OperationStore`], [`LogStore`] and [`TopicStore`], over
//! a muniment [`Backend`]. One adapter, so a moot's log, a murmur and mesh's
//! job queue can all sit on the same store family (redb on desktop, IndexedDB +
//! OPFS in the browser) instead of a hand-rolled backend each.
//!
//! `LogSync` needs only `LogStore` + `TopicStore`, but the full trio is provided
//! so the adapter is a drop-in for p2panda's own `SqliteStore`. The `?Send`
//! backend is no obstacle: `LogSync` drives the store from a single-threaded
//! ractor actor, so the handle must be `Send` (it is, over a `Send` backend) but
//! its method futures need not be.
//!
//! ## Key schema
//!
//! muniment exposes a flat string key space. Three namespaces live in it:
//!
//! - `log/<author>/<log>/<seq>` holds the operation, CBOR `(Header, body)`. The
//!   author and log id are hex; `seq` is a zero-padded 16-hex sequence number so
//!   keys sort in log order and a single [`scan`](Backend::scan) walks a log.
//! - `op/<hash>` points at the `log/...` key above, so a lookup by operation id
//!   finds the one blob without a scan.
//! - `topic/<topic>/<author>/<log>` records a topic association, value empty.
//!
//! The log id segment is `hex(CBOR(log_id))`, which works for any `L: LogId`
//! (`u64` for direct exchange and mesh's per-space logs, `[u8; 32]` and friends
//! alike). The
//! trailing `/` after it keeps one log's key range from bleeding into another's
//! whose encoding shares a prefix.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::marker::PhantomData;

use muniment::{Backend, StoreError, WriteOp};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Extensions, Hash, Header, LogId, Operation, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use proofs::{BlobRef, DigestAlg};

/// Result of one domain-driven content collection pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlobGcReport {
    pub examined: u64,
    pub collected: u64,
}

/// One already-authorized operation prepared for an atomic batch insert.
pub(crate) struct IndexedOperation<'a, E, L> {
    pub topic: &'a Topic,
    pub operation: &'a Operation<E>,
    pub log_id: &'a L,
    pub prune_before_current: bool,
    pub erase_payloads: &'a [Hash],
}

struct PlannedOperation<E> {
    pointer: String,
    log_key: String,
    log_prefix: String,
    payload_ref: Option<String>,
    seq_num: u64,
    header: Header<E>,
    has_body: bool,
}

/// A p2panda operation store over a muniment [`Backend`].
///
/// Generic over the backend `B` and the operation extensions `E`. Cheap to clone
/// (over a `Clone` backend), so a handle can be handed to `LogSync`.
pub struct MunimentStore<B, E> {
    backend: B,
    // `fn() -> E` marks the extension type without owning it, keeping the handle
    // `Send`/`Sync` and covariant regardless of `E`.
    _ext: PhantomData<fn() -> E>,
}

impl<B: Clone, E> Clone for MunimentStore<B, E> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            _ext: PhantomData,
        }
    }
}

impl<B, E> MunimentStore<B, E> {
    /// Wrap a backend.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            _ext: PhantomData,
        }
    }

    /// The backend this store reads and writes through.
    pub fn backend(&self) -> &B {
        &self.backend
    }
}

// ── key helpers ───────────────────────────────────────────────────────────────

/// The `op/<hash>` pointer key for an operation id.
fn op_ptr(hash: &Hash) -> String {
    format!("op/{}", hash.to_hex())
}

/// The log id key segment: `hex(CBOR(log_id))`, canonical for a given value.
fn log_seg<L: LogId>(log_id: &L) -> Result<String, StoreError> {
    let bytes = encode_cbor(log_id).map_err(codec)?;
    Ok(hex::encode(bytes))
}

/// The `log/<author>/<log>/` prefix a single log's entries share.
fn log_prefix<L: LogId>(author: &VerifyingKey, log_id: &L) -> Result<String, StoreError> {
    Ok(format!("log/{}/{}/", author.to_hex(), log_seg(log_id)?))
}

/// An exclusive upper bound for scanning a log prefix. Sequence suffixes are
/// lowercase hex (`0`..`f`); `g` sorts above them all and below any longer
/// sibling prefix (whose next char would be a hex digit, not `/`).
fn scan_end(prefix: &str) -> String {
    format!("{prefix}g")
}

/// Parse the sequence number back out of a `log/.../<seq>` key.
fn seq_from_key(key: &str, prefix: &str) -> Result<u64, StoreError> {
    let seq = key
        .strip_prefix(prefix)
        .ok_or_else(|| StoreError::Codec("log key missing its prefix".into()))?;
    u64::from_str_radix(seq, 16).map_err(codec)
}

/// The `topic/<topic>/<author>/<log>` association key.
fn topic_key<L: LogId>(
    topic: &Topic,
    author: &VerifyingKey,
    log_id: &L,
) -> Result<String, StoreError> {
    Ok(format!(
        "topic/{}/{}/{}",
        topic.to_hex(),
        author.to_hex(),
        log_seg(log_id)?
    ))
}

fn payload_ref_prefix(payload_hash: &Hash) -> String {
    format!("payload-ref/{}/", payload_hash.to_hex())
}

fn payload_ref_key(payload_hash: &Hash, operation_hash: &Hash) -> String {
    format!(
        "{}{}",
        payload_ref_prefix(payload_hash),
        operation_hash.to_hex()
    )
}

pub(crate) fn pending_payload_key(payload_hash: &Hash) -> String {
    format!("pending-payload/{}", payload_hash.to_hex())
}

/// Whether `seq` falls in the half-open sync range. `after` is exclusive (and,
/// when absent, includes sequence 0); `until` is inclusive.
fn in_range(seq: u64, after: Option<u64>, until: Option<u64>) -> bool {
    after.is_none_or(|a| seq > a) && until.is_none_or(|u| seq <= u)
}

fn codec(err: impl std::fmt::Display) -> StoreError {
    StoreError::Codec(err.to_string())
}

fn record_write(writes: &mut BTreeMap<String, Option<Vec<u8>>>, write: WriteOp) {
    match write {
        WriteOp::Put { key, value } => {
            writes.insert(key, Some(value));
        }
        WriteOp::Delete { key } => {
            writes.insert(key, None);
        }
    }
}

// ── operation encode / decode ─────────────────────────────────────────────────

/// Encode an operation as the CBOR `(Header, Option<body-bytes>)` blob stored at
/// its log key.
fn encode_op<E: Extensions>(op: &Operation<E>) -> Result<Vec<u8>, StoreError> {
    let body = op.body.as_ref().map(|b| b.to_bytes());
    encode_cbor(&(&op.header, &body)).map_err(codec)
}

/// Decode a stored blob back into an operation, recomputing its id from the
/// header.
fn decode_op<E: Extensions>(bytes: &[u8]) -> Result<Operation<E>, StoreError> {
    let (header, body): (Header<E>, Option<Vec<u8>>) = decode_cbor(bytes).map_err(codec)?;
    Ok(Operation {
        hash: header.hash(),
        header,
        body: body.map(Body::from),
    })
}

// ── OperationStore ────────────────────────────────────────────────────────────

impl<B, E, L> OperationStore<Operation<E>, Hash, L> for MunimentStore<B, E>
where
    B: Backend,
    E: Extensions,
    L: LogId,
{
    type Error = StoreError;

    async fn insert_operation(
        &self,
        id: &Hash,
        operation: &Operation<E>,
        log_id: &L,
    ) -> Result<bool, StoreError> {
        let ptr = op_ptr(id);
        // Insert-or-ignore: an operation id already present is a no-op.
        if self.backend.get(&ptr).await?.is_some() {
            return Ok(false);
        }
        let prefix = log_prefix(&operation.header.verifying_key, log_id)?;
        let log_key = format!("{prefix}{:016x}", operation.header.seq_num);
        let blob = encode_op(operation)?;
        // The blob and its pointer land together so a reader never sees one
        // without the other.
        self.backend
            .apply(&{
                let mut writes = vec![
                    WriteOp::Put {
                        key: log_key.clone(),
                        value: blob,
                    },
                    WriteOp::Put {
                        key: ptr,
                        value: log_key.clone().into_bytes(),
                    },
                ];
                if let Some(payload_hash) = operation.header.payload_hash.as_ref() {
                    writes.push(WriteOp::Put {
                        key: payload_ref_key(payload_hash, &operation.hash),
                        value: log_key.into_bytes(),
                    });
                }
                writes
            })
            .await?;
        Ok(true)
    }

    // These forward to the inherent twins above (inherent resolution wins), which
    // hold the real logic and stay callable without pinning `L`.

    async fn get_operation(&self, id: &Hash) -> Result<Option<Operation<E>>, StoreError> {
        self.get_operation(id).await
    }

    async fn get_operation_tx(&self, id: &Hash) -> Result<Option<Operation<E>>, StoreError> {
        self.get_operation(id).await
    }

    async fn has_operation(&self, id: &Hash) -> Result<bool, StoreError> {
        self.has_operation(id).await
    }

    async fn has_operation_tx(&self, id: &Hash) -> Result<bool, StoreError> {
        self.has_operation(id).await
    }

    async fn delete_operation(&self, id: &Hash) -> Result<bool, StoreError> {
        self.delete_operation(id).await
    }

    async fn delete_operation_payload(&self, id: &Hash) -> Result<bool, StoreError> {
        self.delete_operation_payload(id).await
    }
}

/// Inherent twins of the id-keyed [`OperationStore`] methods.
///
/// Those methods key by operation hash, never by log id, yet their trait
/// signatures still carry the `L` type parameter, so a bare `store.get_operation(id)`
/// can't infer `L`. These same-name inherent methods shadow the trait ones for
/// direct calls (inherent resolution wins), so callers and the trait impl below
/// both reach them without pinning a log-id type.
impl<B, E> MunimentStore<B, E>
where
    B: Backend,
    E: Extensions,
{
    /// Atomically associate an author's log with a topic and insert one operation.
    ///
    /// The topic association, log entry, and hash pointer reach the backend in
    /// one [`Backend::apply`] batch. Domain stores should prefer this helper over
    /// manually composing [`TopicStore::associate`] and
    /// [`OperationStore::insert_operation`].
    pub async fn insert_indexed_operation<L: LogId>(
        &self,
        topic: &Topic,
        operation: &Operation<E>,
        log_id: &L,
    ) -> Result<bool, StoreError> {
        let (inserted, _) = self
            .insert_indexed_operation_with_prune(topic, operation, log_id, false)
            .await?;
        Ok(inserted)
    }

    /// Atomically index several admitted operations and caller-supplied trailing
    /// writes.
    ///
    /// The processor performs policy and continuity checks first. This method
    /// rechecks that every operation is still absent, then submits all topic,
    /// log, pointer, staging-cleanup, and receipt writes in one backend batch.
    pub(crate) async fn apply_indexed_operation_batch<L: LogId>(
        &self,
        operations: &[IndexedOperation<'_, E, L>],
        leading_writes: &[WriteOp],
        trailing_writes: &[WriteOp],
    ) -> Result<(), StoreError> {
        let mut pointers = BTreeSet::new();
        let mut writes: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        let mut planned: BTreeMap<String, PlannedOperation<E>> = BTreeMap::new();
        for write in leading_writes {
            record_write(&mut writes, write.clone());
        }
        for entry in operations {
            let pointer = op_ptr(&entry.operation.hash);
            if !pointers.insert(pointer.clone()) {
                return Err(codec("atomic operation batch contains a duplicate id"));
            }
            if self.backend.get(&pointer).await?.is_some() {
                return Err(codec(
                    "operation store changed after atomic batch validation",
                ));
            }
            let topic = topic_key(
                entry.topic,
                &entry.operation.header.verifying_key,
                entry.log_id,
            )?;
            let prefix = log_prefix(&entry.operation.header.verifying_key, entry.log_id)?;
            let log_key = format!("{prefix}{:016x}", entry.operation.header.seq_num);

            if entry.prune_before_current {
                let (live_prune, _) = self
                    .prefix_prune_writes(
                        &entry.operation.header.verifying_key,
                        entry.log_id,
                        entry.operation.header.seq_num,
                    )
                    .await?;
                for write in live_prune {
                    record_write(&mut writes, write);
                }
                let planned_prefix: Vec<_> = planned
                    .iter()
                    .filter(|(_, operation)| {
                        operation.log_prefix == prefix
                            && operation.seq_num < entry.operation.header.seq_num
                    })
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in planned_prefix {
                    if let Some(operation) = planned.remove(&id) {
                        writes.insert(operation.pointer, None);
                        writes.insert(operation.log_key, None);
                        if let Some(payload_ref) = operation.payload_ref {
                            writes.insert(payload_ref, None);
                        }
                    }
                }
            }

            for id in entry.erase_payloads {
                let id_hex = id.to_hex();
                if let Some(operation) = planned.get_mut(&id_hex) {
                    if let Some(payload_ref) = operation.payload_ref.as_ref() {
                        writes.insert(payload_ref.clone(), None);
                    }
                    if operation.has_body {
                        writes.insert(
                            operation.log_key.clone(),
                            Some(
                                encode_cbor(&(&operation.header, &None::<Vec<u8>>))
                                    .map_err(codec)?,
                            ),
                        );
                        operation.has_body = false;
                    }
                    continue;
                }
                let payload_pointer = op_ptr(id);
                if matches!(writes.get(&payload_pointer), Some(None)) {
                    continue;
                }
                let Some(payload_log_key) = self.log_key_for(id).await? else {
                    continue;
                };
                let payload_blob = match writes.get(&payload_log_key) {
                    Some(Some(bytes)) => Some(bytes.clone()),
                    Some(None) => None,
                    None => self.backend.get(&payload_log_key).await?,
                };
                let Some(payload_blob) = payload_blob else {
                    continue;
                };
                let (header, body): (Header<E>, Option<Vec<u8>>) =
                    decode_cbor(&payload_blob[..]).map_err(codec)?;
                if let Some(payload_hash) = header.payload_hash.as_ref() {
                    writes.insert(payload_ref_key(payload_hash, id), None);
                }
                if body.is_some() {
                    writes.insert(
                        payload_log_key.clone(),
                        Some(encode_cbor(&(&header, &None::<Vec<u8>>)).map_err(codec)?),
                    );
                }
            }

            writes.insert(topic, Some(Vec::new()));
            writes.insert(log_key.clone(), Some(encode_op(entry.operation)?));
            writes.insert(pointer.clone(), Some(log_key.clone().into_bytes()));
            let payload_ref = entry
                .operation
                .header
                .payload_hash
                .as_ref()
                .map(|payload_hash| {
                    (
                        payload_ref_key(payload_hash, &entry.operation.hash),
                        log_key.clone().into_bytes(),
                    )
                });
            if let Some((key, value)) = payload_ref.as_ref() {
                writes.insert(key.clone(), Some(value.clone()));
            }
            planned.insert(
                entry.operation.hash.to_hex(),
                PlannedOperation {
                    pointer,
                    log_key,
                    log_prefix: prefix,
                    payload_ref: payload_ref.map(|(key, _)| key),
                    seq_num: entry.operation.header.seq_num,
                    header: entry.operation.header.clone(),
                    has_body: entry.operation.body.is_some(),
                },
            );
        }
        for write in trailing_writes {
            record_write(&mut writes, write.clone());
        }
        let writes: Vec<_> = writes
            .into_iter()
            .map(|(key, value)| match value {
                Some(value) => WriteOp::Put { key, value },
                None => WriteOp::Delete { key },
            })
            .collect();
        self.backend.apply(&writes).await
    }

    /// Atomically index one operation and optionally remove its preceding log.
    pub(crate) async fn insert_indexed_operation_with_prune<L: LogId>(
        &self,
        topic: &Topic,
        operation: &Operation<E>,
        log_id: &L,
        prune_before_current: bool,
    ) -> Result<(bool, u64), StoreError> {
        let (inserted, pruned, _) = self
            .insert_indexed_operation_with_effects(
                topic,
                operation,
                log_id,
                prune_before_current,
                &[],
            )
            .await?;
        Ok((inserted, pruned))
    }

    /// Atomically index one operation, remove an authorized prefix, and erase
    /// authorized payloads while retaining their signed headers.
    pub(crate) async fn insert_indexed_operation_with_effects<L: LogId>(
        &self,
        topic: &Topic,
        operation: &Operation<E>,
        log_id: &L,
        prune_before_current: bool,
        erase_payloads: &[Hash],
    ) -> Result<(bool, u64, u64), StoreError> {
        let ptr = op_ptr(&operation.hash);
        if self.backend.get(&ptr).await?.is_some() {
            return Ok((false, 0, 0));
        }
        let topic = topic_key(topic, &operation.header.verifying_key, log_id)?;
        let prefix = log_prefix(&operation.header.verifying_key, log_id)?;
        let log_key = format!("{prefix}{:016x}", operation.header.seq_num);
        let blob = encode_op(operation)?;
        let (mut writes, pruned_entries) = if prune_before_current {
            self.prefix_prune_writes(
                &operation.header.verifying_key,
                log_id,
                operation.header.seq_num,
            )
            .await?
        } else {
            (Vec::new(), 0)
        };
        let mut erased_payloads = 0;
        for id in erase_payloads {
            let Some(payload_log_key) = self.log_key_for(id).await? else {
                continue;
            };
            let Some(payload_blob) = self.backend.get(&payload_log_key).await? else {
                continue;
            };
            let (header, body): (Header<E>, Option<Vec<u8>>) =
                decode_cbor(&payload_blob[..]).map_err(codec)?;
            if let Some(payload_hash) = header.payload_hash.as_ref() {
                writes.push(WriteOp::Delete {
                    key: payload_ref_key(payload_hash, id),
                });
            }
            if body.is_none() {
                continue;
            }
            let stripped = encode_cbor(&(&header, &None::<Vec<u8>>)).map_err(codec)?;
            writes.push(WriteOp::Put {
                key: payload_log_key.clone(),
                value: stripped,
            });
            erased_payloads += 1;
        }
        writes.extend([
            WriteOp::Put {
                key: topic,
                value: Vec::new(),
            },
            WriteOp::Put {
                key: log_key.clone(),
                value: blob,
            },
            WriteOp::Put {
                key: ptr,
                value: log_key.clone().into_bytes(),
            },
        ]);
        if let Some(payload_hash) = operation.header.payload_hash.as_ref() {
            writes.push(WriteOp::Put {
                key: payload_ref_key(payload_hash, &operation.hash),
                value: log_key.into_bytes(),
            });
        }
        self.backend.apply(&writes).await?;
        Ok((true, pruned_entries, erased_payloads))
    }

    async fn prefix_prune_writes<L: LogId>(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        until: u64,
    ) -> Result<(Vec<WriteOp>, u64), StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        let mut writes = Vec::new();
        let mut pruned = 0;
        for key in keys {
            if seq_from_key(&key, &prefix)? >= until {
                continue;
            }
            if let Some(blob) = self.backend.get(&key).await? {
                let (header, _): (Header<E>, Option<Vec<u8>>) =
                    decode_cbor(&blob[..]).map_err(codec)?;
                let operation_hash = header.hash();
                writes.push(WriteOp::Delete {
                    key: op_ptr(&operation_hash),
                });
                if let Some(payload_hash) = header.payload_hash.as_ref() {
                    writes.push(WriteOp::Delete {
                        key: payload_ref_key(payload_hash, &operation_hash),
                    });
                }
            }
            writes.push(WriteOp::Delete { key });
            pruned += 1;
        }
        Ok((writes, pruned))
    }

    /// Resolve an operation id to its `log/...` key via the `op/<hash>` pointer.
    async fn log_key_for(&self, id: &Hash) -> Result<Option<String>, StoreError> {
        match self.backend.get(&op_ptr(id)).await? {
            Some(bytes) => Ok(Some(String::from_utf8(bytes).map_err(codec)?)),
            None => Ok(None),
        }
    }

    /// Fetch an operation by id.
    pub async fn get_operation(&self, id: &Hash) -> Result<Option<Operation<E>>, StoreError> {
        let Some(log_key) = self.log_key_for(id).await? else {
            return Ok(None);
        };
        match self.backend.get(&log_key).await? {
            Some(blob) => Ok(Some(decode_op(&blob)?)),
            None => Ok(None),
        }
    }

    /// Whether an operation id is present.
    pub async fn has_operation(&self, id: &Hash) -> Result<bool, StoreError> {
        Ok(self.backend.get(&op_ptr(id)).await?.is_some())
    }

    /// Delete an operation and its log entry.
    pub async fn delete_operation(&self, id: &Hash) -> Result<bool, StoreError> {
        let ptr = op_ptr(id);
        let Some(log_key) = self.log_key_for(id).await? else {
            return Ok(false);
        };
        let mut writes = vec![
            WriteOp::Delete { key: ptr },
            WriteOp::Delete {
                key: log_key.clone(),
            },
        ];
        if let Some(blob) = self.backend.get(&log_key).await? {
            let (header, _): (Header<E>, Option<Vec<u8>>) =
                decode_cbor(&blob[..]).map_err(codec)?;
            if let Some(payload_hash) = header.payload_hash.as_ref() {
                writes.push(WriteOp::Delete {
                    key: payload_ref_key(payload_hash, id),
                });
            }
        }
        self.backend.apply(&writes).await?;
        Ok(true)
    }

    /// Drop an operation's payload, keeping its header (and so its log entry).
    pub async fn delete_operation_payload(&self, id: &Hash) -> Result<bool, StoreError> {
        let Some(log_key) = self.log_key_for(id).await? else {
            return Ok(false);
        };
        let Some(blob) = self.backend.get(&log_key).await? else {
            return Ok(false);
        };
        let (header, _body): (Header<E>, Option<Vec<u8>>) =
            decode_cbor(&blob[..]).map_err(codec)?;
        let stripped = encode_cbor(&(&header, &None::<Vec<u8>>)).map_err(codec)?;
        let mut writes = vec![WriteOp::Put {
            key: log_key,
            value: stripped,
        }];
        if let Some(payload_hash) = header.payload_hash.as_ref() {
            writes.push(WriteOp::Delete {
                key: payload_ref_key(payload_hash, id),
            });
        }
        self.backend.apply(&writes).await?;
        Ok(true)
    }

    /// Read a payload chunk-set retained until its signed header arrives.
    pub(crate) async fn pending_payload(
        &self,
        payload_hash: &Hash,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        self.backend.get(&pending_payload_key(payload_hash)).await
    }

    /// Prepare writes that attach verified bytes to every eligible retained
    /// header naming this payload hash.
    ///
    /// Retention erasure removes the `payload-ref` key, so a later chunk cannot
    /// resurrect an intentionally erased body.
    pub(crate) async fn payload_attachment_writes(
        &self,
        payload_hash: &Hash,
        bytes: &[u8],
    ) -> Result<(Vec<WriteOp>, u64), StoreError> {
        if Hash::digest(bytes) != *payload_hash {
            return Err(codec("payload chunk-set has a false digest"));
        }
        let mut writes = Vec::new();
        let mut attached = 0;
        for reference_key in self.backend.list(&payload_ref_prefix(payload_hash)).await? {
            let Some(log_key) = self.backend.get(&reference_key).await? else {
                writes.push(WriteOp::Delete { key: reference_key });
                continue;
            };
            let log_key = String::from_utf8(log_key).map_err(codec)?;
            let Some(blob) = self.backend.get(&log_key).await? else {
                writes.push(WriteOp::Delete { key: reference_key });
                continue;
            };
            let (header, body): (Header<E>, Option<Vec<u8>>) =
                decode_cbor(&blob[..]).map_err(codec)?;
            if header.payload_hash.as_ref() != Some(payload_hash) {
                return Err(codec("payload reference points at another digest"));
            }
            if header.payload_size != bytes.len() as u64 {
                return Err(codec("payload bytes do not match the signed size"));
            }
            if body.is_none() {
                writes.push(WriteOp::Put {
                    key: log_key,
                    value: encode_cbor(&(&header, &Some(bytes.to_vec()))).map_err(codec)?,
                });
                attached += 1;
            }
        }
        Ok((writes, attached))
    }

    /// Attach verified payload bytes to eligible retained headers atomically.
    ///
    /// A removed payload-reference key is the retention barrier: this returns
    /// zero instead of recreating a body that policy has erased.
    pub(crate) async fn attach_payload(
        &self,
        payload_hash: &Hash,
        bytes: &[u8],
    ) -> Result<u64, StoreError> {
        let (writes, attached) = self.payload_attachment_writes(payload_hash, bytes).await?;
        if !writes.is_empty() {
            self.backend.apply(&writes).await?;
        }
        Ok(attached)
    }

    /// Prepare one verified content-addressed blob write for an import batch.
    pub(crate) fn imported_blob_write(
        blob_hash: [u8; 32],
        bytes: &[u8],
    ) -> Result<WriteOp, StoreError> {
        if *blake3::hash(bytes).as_bytes() != blob_hash {
            return Err(codec("blob chunk-set has a false digest"));
        }
        Ok(WriteOp::Put {
            key: format!("blob/{}", hex::encode(blob_hash)),
            value: bytes.to_vec(),
        })
    }

    /// Store content-addressed bytes in the shared muniment backend.
    pub async fn put_blob(&self, bytes: &[u8]) -> Result<BlobRef, StoreError> {
        let reference = BlobRef::blake3(bytes);
        let digest = hex::encode(reference.digest.as_32().map_err(codec)?);
        self.backend.put(&format!("blob/{digest}"), bytes).await?;
        Ok(reference)
    }

    /// Fetch content-addressed bytes after checking the typed reference.
    pub async fn get_blob(&self, reference: &BlobRef) -> Result<Option<Vec<u8>>, StoreError> {
        if reference.digest.alg != DigestAlg::Blake3 {
            return Err(codec("muniment blobs require a BLAKE3 reference"));
        }
        let digest = hex::encode(reference.digest.as_32().map_err(codec)?);
        let bytes = self.backend.get(&format!("blob/{digest}")).await?;
        match bytes {
            Some(bytes) if reference.verifies(&bytes) => Ok(Some(bytes)),
            Some(_) => Err(codec("stored blob does not match its typed reference")),
            None => Ok(None),
        }
    }

    /// Collect blobs absent from the domain-supplied retained reference set.
    ///
    /// Replication owns the atomic delete mechanics. The domain remains
    /// responsible for tracing every live-state and checkpoint reference.
    pub async fn collect_unreferenced_blobs<'a>(
        &self,
        retained: impl IntoIterator<Item = &'a BlobRef>,
    ) -> Result<BlobGcReport, StoreError> {
        let mut retained_keys = BTreeSet::new();
        for reference in retained {
            if reference.digest.alg != DigestAlg::Blake3 {
                return Err(codec("muniment blobs require a BLAKE3 reference"));
            }
            retained_keys.insert(format!(
                "blob/{}",
                hex::encode(reference.digest.as_32().map_err(codec)?)
            ));
        }
        let keys = self.backend.list("blob/").await?;
        let writes: Vec<_> = keys
            .iter()
            .filter(|key| !retained_keys.contains(*key))
            .cloned()
            .map(|key| WriteOp::Delete { key })
            .collect();
        self.backend.apply(&writes).await?;
        Ok(BlobGcReport {
            examined: keys.len() as u64,
            collected: writes.len() as u64,
        })
    }

    /// The number of distinct operations stored (one `op/<hash>` pointer each).
    pub async fn operation_count(&self) -> Result<usize, StoreError> {
        Ok(self.backend.list("op/").await?.len())
    }

    /// Whether the store holds no operations.
    pub async fn is_empty(&self) -> Result<bool, StoreError> {
        Ok(self.operation_count().await? == 0)
    }
}

// ── LogStore ──────────────────────────────────────────────────────────────────

impl<B, E, L> LogStore<Operation<E>, VerifyingKey, L, u64, Hash> for MunimentStore<B, E>
where
    B: Backend,
    E: Extensions,
    L: LogId,
{
    type Error = StoreError;

    async fn get_latest_entry(
        &self,
        author: &VerifyingKey,
        log_id: &L,
    ) -> Result<Option<Operation<E>>, StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        match keys.last() {
            Some(key) => match self.backend.get(key).await? {
                Some(blob) => Ok(Some(decode_op(&blob)?)),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }

    async fn get_latest_entry_tx(
        &self,
        author: &VerifyingKey,
        log_id: &L,
    ) -> Result<Option<Operation<E>>, StoreError> {
        self.get_latest_entry(author, log_id).await
    }

    async fn get_log_heights(
        &self,
        author: &VerifyingKey,
        logs: &[L],
    ) -> Result<Option<BTreeMap<L, u64>>, StoreError> {
        let mut heights = BTreeMap::new();
        for log_id in logs {
            let prefix = log_prefix(author, log_id)?;
            let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
            if let Some(key) = keys.last() {
                heights.insert(log_id.clone(), seq_from_key(key, &prefix)?);
            }
        }
        if heights.is_empty() {
            Ok(None)
        } else {
            Ok(Some(heights))
        }
    }

    async fn get_log_size(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<u64>,
        until: Option<u64>,
    ) -> Result<Option<(u64, u64)>, StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        let mut count = 0;
        let mut bytes = 0;
        for key in &keys {
            if !in_range(seq_from_key(key, &prefix)?, after, until) {
                continue;
            }
            if let Some(blob) = self.backend.get(key).await? {
                let (header, _): (Header<E>, Option<Vec<u8>>) =
                    decode_cbor(&blob[..]).map_err(codec)?;
                bytes += header.to_bytes().len() as u64 + header.payload_size;
                count += 1;
            }
        }
        Ok(Some((count, bytes)))
    }

    async fn get_log_entries(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<u64>,
        until: Option<u64>,
    ) -> Result<Option<Vec<(Operation<E>, Vec<u8>)>>, StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        let mut entries = Vec::new();
        for key in &keys {
            if !in_range(seq_from_key(key, &prefix)?, after, until) {
                continue;
            }
            if let Some(blob) = self.backend.get(key).await? {
                let op = decode_op::<E>(&blob)?;
                let header = op.header.to_bytes();
                entries.push((op, header));
            }
        }
        if entries.is_empty() {
            Ok(None)
        } else {
            Ok(Some(entries))
        }
    }

    async fn prune_entries(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        until: &u64,
    ) -> Result<u64, StoreError> {
        let (writes, pruned) = self.prefix_prune_writes(author, log_id, *until).await?;
        self.backend.apply(&writes).await?;
        Ok(pruned)
    }
}

// ── TopicStore ────────────────────────────────────────────────────────────────

impl<B, E, L> TopicStore<Topic, VerifyingKey, L> for MunimentStore<B, E>
where
    B: Backend,
    E: Extensions,
    L: LogId,
{
    type Error = StoreError;

    async fn associate(
        &self,
        topic: &Topic,
        author: &VerifyingKey,
        data_id: &L,
    ) -> Result<bool, StoreError> {
        let key = topic_key(topic, author, data_id)?;
        if self.backend.get(&key).await?.is_some() {
            return Ok(false);
        }
        self.backend.put(&key, b"").await?;
        Ok(true)
    }

    async fn remove(
        &self,
        topic: &Topic,
        author: &VerifyingKey,
        data_id: &L,
    ) -> Result<bool, StoreError> {
        let key = topic_key(topic, author, data_id)?;
        if self.backend.get(&key).await?.is_none() {
            return Ok(false);
        }
        self.backend.delete(&key).await?;
        Ok(true)
    }

    async fn resolve(&self, topic: &Topic) -> Result<BTreeMap<VerifyingKey, Vec<L>>, StoreError> {
        let prefix = format!("topic/{}/", topic.to_hex());
        let keys = self.backend.list(&prefix).await?;
        let mut out: BTreeMap<VerifyingKey, Vec<L>> = BTreeMap::new();
        for key in keys {
            let rest = key
                .strip_prefix(&prefix)
                .ok_or_else(|| StoreError::Codec("topic key missing its prefix".into()))?;
            let (author_hex, log_hex) = rest
                .split_once('/')
                .ok_or_else(|| StoreError::Codec("malformed topic key".into()))?;
            let author = VerifyingKey::try_from(hex::decode(author_hex).map_err(codec)?.as_slice())
                .map_err(codec)?;
            let log_id: L =
                decode_cbor(&hex::decode(log_hex).map_err(codec)?[..]).map_err(codec)?;
            out.entry(author).or_default().push(log_id);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muniment::MemoryBackend;
    use p2panda_core::{Body, Header, Operation, SigningKey};

    type Ext = ();

    /// Compile-time proof the adapter meets `LogSync`'s store bound: the two sync
    /// traits it reconciles against, plus `Clone + Send + 'static` over a real
    /// backend. If any part regresses, this stops compiling.
    fn _log_sync_ready<S>()
    where
        S: LogStore<Operation<Ext>, VerifyingKey, u64, u64, Hash>
            + TopicStore<Topic, VerifyingKey, u64>
            + Clone
            + Send
            + 'static,
    {
    }

    #[allow(dead_code)]
    fn _assert_ready() {
        _log_sync_ready::<MunimentStore<MemoryBackend, Ext>>();
    }

    /// A signed operation for one author's log at `seq`, chained onto `backlink`.
    fn make_op(
        sk: &SigningKey,
        seq: u64,
        backlink: Option<Hash>,
        payload: &[u8],
    ) -> Operation<Ext> {
        let body = Body::new(payload);
        let mut header = Header::<Ext> {
            version: 1,
            verifying_key: sk.verifying_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0.into(),
            seq_num: seq,
            backlink,
            extensions: (),
        };
        header.sign(sk);
        Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        }
    }

    #[test]
    fn round_trips_and_orders_a_log() {
        pollster::block_on(async {
            let store = MunimentStore::<_, Ext>::new(MemoryBackend::new());
            let sk = SigningKey::generate();
            let author = sk.verifying_key();
            let log_id = 0u64;

            let op0 = make_op(&sk, 0, None, b"zero");
            let op1 = make_op(&sk, 1, Some(op0.hash), b"one");

            // Insert-or-ignore: the first insert takes, the repeat is a no-op.
            assert!(
                store
                    .insert_operation(&op0.hash, &op0, &log_id)
                    .await
                    .unwrap()
            );
            assert!(
                !store
                    .insert_operation(&op0.hash, &op0, &log_id)
                    .await
                    .unwrap()
            );
            assert!(
                store
                    .insert_operation(&op1.hash, &op1, &log_id)
                    .await
                    .unwrap()
            );

            // OperationStore: fetch by id round-trips header and body.
            let got = store.get_operation(&op0.hash).await.unwrap().unwrap();
            assert_eq!(got.hash, op0.hash);
            assert_eq!(got.body.unwrap().to_bytes(), b"zero");
            assert!(store.has_operation(&op1.hash).await.unwrap());
            assert!(!store.has_operation(&Hash::digest(b"absent")).await.unwrap());

            // LogStore: latest is the highest seq; entries return in seq order.
            let latest = store
                .get_latest_entry(&author, &log_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(latest.hash, op1.hash);

            let entries = store
                .get_log_entries(&author, &log_id, None, None)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].0.hash, op0.hash);
            assert_eq!(entries[1].0.hash, op1.hash);
            // The second tuple element is the encoded header.
            assert_eq!(entries[0].1, op0.header.to_bytes());

            let heights = store
                .get_log_heights(&author, &[log_id])
                .await
                .unwrap()
                .unwrap();
            assert_eq!(heights.get(&log_id), Some(&1));

            let (count, _bytes) = store
                .get_log_size(&author, &log_id, None, None)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(count, 2);

            // Range: after seq 0 leaves only op1.
            let tail = store
                .get_log_entries(&author, &log_id, Some(0), None)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(tail.len(), 1);
            assert_eq!(tail[0].0.hash, op1.hash);
        });
    }

    #[test]
    fn associates_and_resolves_a_topic() {
        pollster::block_on(async {
            let store = MunimentStore::<_, Ext>::new(MemoryBackend::new());
            let sk = SigningKey::generate();
            let author = sk.verifying_key();
            let topic = Topic::random();
            let log_id = 7u64;

            assert!(store.associate(&topic, &author, &log_id).await.unwrap());
            assert!(!store.associate(&topic, &author, &log_id).await.unwrap());

            let resolved = store.resolve(&topic).await.unwrap();
            assert_eq!(resolved.get(&author), Some(&vec![log_id]));

            assert!(store.remove(&topic, &author, &log_id).await.unwrap());
            let empty: BTreeMap<VerifyingKey, Vec<u64>> = store.resolve(&topic).await.unwrap();
            assert!(empty.is_empty());
        });
    }

    #[test]
    fn prunes_below_a_sequence() {
        pollster::block_on(async {
            let store = MunimentStore::<_, Ext>::new(MemoryBackend::new());
            let sk = SigningKey::generate();
            let author = sk.verifying_key();
            let log_id = 0u64;

            let op0 = make_op(&sk, 0, None, b"zero");
            let op1 = make_op(&sk, 1, Some(op0.hash), b"one");
            let op2 = make_op(&sk, 2, Some(op1.hash), b"two");
            for op in [&op0, &op1, &op2] {
                store.insert_operation(&op.hash, op, &log_id).await.unwrap();
            }

            // Prune below seq 2: op0 and op1 go, op2 stays.
            let pruned = store.prune_entries(&author, &log_id, &2).await.unwrap();
            assert_eq!(pruned, 2);
            assert!(!store.has_operation(&op0.hash).await.unwrap());
            assert!(store.has_operation(&op2.hash).await.unwrap());

            let remaining = store
                .get_log_entries(&author, &log_id, None, None)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(remaining.len(), 1);
            assert_eq!(remaining[0].0.hash, op2.hash);
        });
    }

    #[test]
    fn blob_collection_keeps_only_domain_traced_references() {
        pollster::block_on(async {
            let store = MunimentStore::<_, Ext>::new(MemoryBackend::new());
            let keep = store.put_blob(b"live checkpoint").await.unwrap();
            let collect = store.put_blob(b"expired payload").await.unwrap();
            assert_eq!(
                store.get_blob(&collect).await.unwrap(),
                Some(b"expired payload".to_vec())
            );

            let report = store.collect_unreferenced_blobs([&keep]).await.unwrap();
            assert_eq!(
                report,
                BlobGcReport {
                    examined: 2,
                    collected: 1,
                }
            );
            assert_eq!(
                store.get_blob(&keep).await.unwrap(),
                Some(b"live checkpoint".to_vec())
            );
            assert_eq!(store.get_blob(&collect).await.unwrap(), None);
        });
    }
}
