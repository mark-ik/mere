//! Operation export and staged native-drop import.
//!
//! The codec verifies the entire carrier before this module decodes operations.
//! Import then preflights every operation for structural and domain admission,
//! stages the verified records by semantic drop id, and commits operations,
//! authorized retention effects, stage cleanup, and a receipt in one batch.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use muniment::{Backend, StoreError, WriteOp};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Extensions, Hash, Header, LogId, Operation, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use serde::{Deserialize, Serialize};

use crate::drop::{
    DropId, DropLimits, DropProtector, DropRecord, DropWriteReceipt, NativeDropError,
    read_plain_drop, read_protected_drop, write_plain_drop, write_protected_drop,
};
use crate::store::pending_payload_key;
use crate::{
    DropReceipt, MunimentStore, OperationPolicy, OperationProcessor, ProcessError, ReceiptPeer,
};

/// Settings-controlled operation export selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropExportProfile {
    pub include_operation_bodies: bool,
}

impl Default for DropExportProfile {
    fn default() -> Self {
        Self {
            include_operation_bodies: true,
        }
    }
}

/// Domain-owned decision for one retained operation.
///
/// The domain maps its privacy classes and profile settings into this small
/// carrier-neutral vocabulary. Higher priorities are selected first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropExportDecision {
    Omit,
    Header { priority: u32 },
    Full { priority: u32 },
}

/// Privacy and priority authority for one export.
pub trait DropExportSelector<E: Extensions> {
    fn select(&self, operation: &Operation<E>) -> DropExportDecision;
}

impl<E: Extensions> DropExportSelector<E> for DropExportProfile {
    fn select(&self, _operation: &Operation<E>) -> DropExportDecision {
        if self.include_operation_bodies {
            DropExportDecision::Full { priority: 0 }
        } else {
            DropExportDecision::Header { priority: 0 }
        }
    }
}

/// Settings-controlled semantic record budget.
///
/// This counts canonical record bytes. Carrier framing and protection overhead
/// remain subject to [`DropLimits`] and the selected transport's own limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DropExportBudget {
    pub max_selected_record_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DropExportStats {
    pub selected_records: u64,
    pub selected_record_bytes: u64,
    pub policy_omitted: u64,
    pub budget_omitted: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropFileExportReport {
    pub drop: DropWriteReceipt,
    pub selection: DropExportStats,
}

/// Per-drop operation import counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropImportReport {
    pub drop_id: DropId,
    pub accepted: u64,
    pub duplicate: u64,
    pub hydrated_payloads: u64,
    pub non_operation_records: u64,
    pub receipt_hit: bool,
}

/// One verified drop corpus left staged after a continuity or storage failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StagedDrop {
    pub drop_id: DropId,
    pub record_count: u64,
}

// Keep the local marker schema stable. The semantic DropId remains in its key;
// the portable peer statement adds that id only when it leaves this device.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct StoredDropReceipt {
    operation_records: u64,
    non_operation_records: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum DropIoError {
    #[error(transparent)]
    Drop(#[from] NativeDropError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("drop operation codec: {0}")]
    OperationCodec(String),
    #[error("drop staging codec: {0}")]
    StagingCodec(String),
    #[error("native drop {0} is not staged")]
    MissingStage(String),
    #[error("native drop staging metadata does not match its namespace")]
    FalseStage,
    #[error("drop {kind} chunks for {digest} contain a gap or overlap at offset {offset}")]
    ChunkLayout {
        kind: &'static str,
        digest: String,
        offset: u64,
    },
    #[error("drop {kind} chunks for {digest} have a false digest")]
    FalseChunkDigest { kind: &'static str, digest: String },
    #[error("drop export store: {0}")]
    Store(#[from] StoreError),
    #[error("drop export record codec: {0}")]
    ExportCodec(String),
    #[error("drop export selected-byte count overflowed")]
    ExportSizeOverflow,
    #[error(transparent)]
    Receipt(#[from] crate::DropReceiptError),
}

#[derive(Default)]
struct AssembledContent {
    payloads: BTreeMap<[u8; 32], Vec<u8>>,
    blobs: BTreeMap<[u8; 32], Vec<u8>>,
}

/// Encode one p2panda operation as a native-drop record.
pub fn operation_record<E: Extensions>(operation: &Operation<E>, include_body: bool) -> DropRecord {
    DropRecord::Operation {
        header: operation.header.to_bytes(),
        inline_body: include_body
            .then(|| operation.body.as_ref().map(|body| body.to_bytes()))
            .flatten(),
    }
}

/// Decode one operation record while retaining header-only carriage.
pub fn decode_operation_record<E: Extensions>(
    record: &DropRecord,
) -> Result<Option<Operation<E>>, DropIoError> {
    let DropRecord::Operation {
        header,
        inline_body,
    } = record
    else {
        return Ok(None);
    };
    let decoded: Header<E> =
        decode_cbor(&header[..]).map_err(|error| DropIoError::OperationCodec(error.to_string()))?;
    if decoded.to_bytes() != *header {
        return Err(DropIoError::OperationCodec(
            "operation header is not canonically encoded".into(),
        ));
    }
    let hash = decoded.hash();
    Ok(Some(Operation {
        hash,
        header: decoded,
        body: inline_body.clone().map(Body::from),
    }))
}

/// Select every operation under one topic using the domain's configured body
/// policy. Retained-frontier filtering has already happened in the store.
pub async fn export_topic_operations<B, E, L>(
    store: &MunimentStore<B, E>,
    topic: &Topic,
    profile: DropExportProfile,
) -> Result<Vec<DropRecord>, DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
{
    let (records, _) = export_topic_operations_selected::<B, E, L, _>(
        store,
        topic,
        &profile,
        DropExportBudget::default(),
    )
    .await?;
    Ok(records)
}

/// Apply domain privacy and priority decisions, then fit selected records into
/// the configured semantic byte budget.
pub async fn export_topic_operations_selected<B, E, L, S>(
    store: &MunimentStore<B, E>,
    topic: &Topic,
    selector: &S,
    budget: DropExportBudget,
) -> Result<(Vec<DropRecord>, DropExportStats), DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
    S: DropExportSelector<E>,
{
    let logs: BTreeMap<VerifyingKey, Vec<L>> = store.resolve(topic).await?;
    let mut candidates = Vec::new();
    let mut stats = DropExportStats::default();
    let mut ordinal = 0u64;
    for (author, log_ids) in logs {
        for log_id in log_ids {
            if let Some(entries) = store.get_log_entries(&author, &log_id, None, None).await? {
                for (operation, _) in entries {
                    let decision = selector.select(&operation);
                    let (include_body, priority) = match decision {
                        DropExportDecision::Omit => {
                            stats.policy_omitted += 1;
                            ordinal += 1;
                            continue;
                        }
                        DropExportDecision::Header { priority } => (false, priority),
                        DropExportDecision::Full { priority } => (true, priority),
                    };
                    let record = operation_record(&operation, include_body);
                    let bytes = encode_cbor(&record)
                        .map_err(|error| DropIoError::ExportCodec(error.to_string()))?
                        .len() as u64;
                    candidates.push((Reverse(priority), ordinal, bytes, record));
                    ordinal += 1;
                }
            }
        }
    }
    candidates.sort_by_key(|(priority, ordinal, _, _)| (*priority, *ordinal));

    let mut records = Vec::new();
    for (_, _, bytes, record) in candidates {
        let next = stats
            .selected_record_bytes
            .checked_add(bytes)
            .ok_or(DropIoError::ExportSizeOverflow)?;
        if budget
            .max_selected_record_bytes
            .is_some_and(|maximum| next > maximum)
        {
            stats.budget_omitted += 1;
            continue;
        }
        stats.selected_record_bytes = next;
        stats.selected_records += 1;
        records.push(record);
    }
    Ok((records, stats))
}

/// Export one topic to a plaintext file for public data or explicit local use.
pub async fn export_plain_topic_file<B, E, L>(
    path: impl AsRef<Path>,
    store: &MunimentStore<B, E>,
    topic: &Topic,
    profile: DropExportProfile,
    limits: DropLimits,
) -> Result<DropWriteReceipt, DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
{
    let records = export_topic_operations::<B, E, L>(store, topic, profile).await?;
    let file = File::create(path).map_err(NativeDropError::Io)?;
    let mut writer = BufWriter::new(file);
    let receipt = write_plain_drop(&mut writer, &records, limits)?;
    writer.flush().map_err(NativeDropError::Io)?;
    writer.get_ref().sync_all().map_err(NativeDropError::Io)?;
    Ok(receipt)
}

/// Export one topic after domain privacy/priority selection.
pub async fn export_selected_plain_topic_file<B, E, L, S>(
    path: impl AsRef<Path>,
    store: &MunimentStore<B, E>,
    topic: &Topic,
    selector: &S,
    budget: DropExportBudget,
    limits: DropLimits,
) -> Result<DropFileExportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
    S: DropExportSelector<E>,
{
    let (records, selection) =
        export_topic_operations_selected::<B, E, L, S>(store, topic, selector, budget).await?;
    let file = File::create(path).map_err(NativeDropError::Io)?;
    let mut writer = BufWriter::new(file);
    let drop = write_plain_drop(&mut writer, &records, limits)?;
    writer.flush().map_err(NativeDropError::Io)?;
    writer.get_ref().sync_all().map_err(NativeDropError::Io)?;
    Ok(DropFileExportReport { drop, selection })
}

/// Export private data through domain selection and an injected protection
/// suite. Private export has no unfiltered convenience path.
pub async fn export_protected_topic_file<B, E, L, S, D>(
    path: impl AsRef<Path>,
    store: &MunimentStore<B, E>,
    topic: &Topic,
    selector: &S,
    budget: DropExportBudget,
    limits: DropLimits,
    protector: &D,
) -> Result<DropFileExportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
    S: DropExportSelector<E>,
    D: DropProtector,
{
    let (records, selection) =
        export_topic_operations_selected::<B, E, L, S>(store, topic, selector, budget).await?;
    let file = File::create(path).map_err(NativeDropError::Io)?;
    let mut writer = BufWriter::new(file);
    let drop = write_protected_drop(&mut writer, &records, limits, protector)?;
    writer.flush().map_err(NativeDropError::Io)?;
    writer.get_ref().sync_all().map_err(NativeDropError::Io)?;
    Ok(DropFileExportReport { drop, selection })
}

/// Verify a complete plaintext drop, preflight every operation, then apply the
/// corpus through the same processor used by local authoring and LogSync.
pub async fn import_plain_drop<B, E, P, R>(
    reader: R,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
    R: Read,
{
    let (read, records) = read_plain_drop(reader, limits)?;
    import_records(read.id, records, processor).await
}

/// Import a plaintext/public drop file through the shared operation processor.
pub async fn import_plain_drop_file<B, E, P>(
    path: impl AsRef<Path>,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
{
    let file = File::open(path).map_err(NativeDropError::Io)?;
    import_plain_drop(BufReader::new(file), limits, processor).await
}

/// Protected counterpart of [`import_plain_drop`].
pub async fn import_protected_drop<B, E, P, R, D>(
    reader: R,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
    protector: &D,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
    R: Read,
    D: DropProtector,
{
    let (read, records) = read_protected_drop(reader, limits, protector)?;
    import_records(read.id, records, processor).await
}

/// Import a protected drop file through its suite and the shared processor.
pub async fn import_protected_drop_file<B, E, P, D>(
    path: impl AsRef<Path>,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
    protector: &D,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
    D: DropProtector,
{
    let file = File::open(path).map_err(NativeDropError::Io)?;
    import_protected_drop(BufReader::new(file), limits, processor, protector).await
}

/// Read this device's durable completion receipt for peer exchange.
///
/// Receiving peers must keep exchanged receipts scoped to the authenticated
/// peer. They must not write them into the local `drop-receipt/` namespace.
pub async fn local_drop_receipt<B, E>(
    drop_id: DropId,
    store: &MunimentStore<B, E>,
) -> Result<Option<DropReceipt>, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let Some(bytes) = store.backend().get(&receipt_key(drop_id)).await? else {
        return Ok(None);
    };
    let stored: StoredDropReceipt =
        decode_cbor(&bytes[..]).map_err(|error| DropIoError::StagingCodec(error.to_string()))?;
    Ok(Some(DropReceipt::new(
        drop_id,
        stored.operation_records,
        stored.non_operation_records,
    )))
}

/// Remember an advisory receipt from one authenticated peer.
///
/// This namespace is deliberately separate from local import receipts. A peer
/// claim can suppress redundant sends to that same peer, but cannot make an
/// import local, authorize retention, or bypass the operation processor.
pub async fn store_peer_drop_receipt<B, E>(
    peer: ReceiptPeer,
    receipt: DropReceipt,
    store: &MunimentStore<B, E>,
) -> Result<(), DropIoError>
where
    B: Backend,
    E: Extensions,
{
    store
        .backend()
        .apply(&[WriteOp::Put {
            key: peer_receipt_key(peer, receipt.drop_id),
            value: receipt.to_canonical_bytes()?,
        }])
        .await?;
    Ok(())
}

/// Read one peer-scoped advisory receipt for resend coordination.
pub async fn peer_drop_receipt<B, E>(
    peer: ReceiptPeer,
    drop_id: DropId,
    store: &MunimentStore<B, E>,
) -> Result<Option<DropReceipt>, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let Some(bytes) = store
        .backend()
        .get(&peer_receipt_key(peer, drop_id))
        .await?
    else {
        return Ok(None);
    };
    let receipt = DropReceipt::from_canonical_bytes(&bytes)?;
    if receipt.drop_id != drop_id {
        return Err(DropIoError::StagingCodec(
            "peer drop receipt does not match its storage namespace".into(),
        ));
    }
    Ok(Some(receipt))
}

/// Remove every advisory receipt for one peer according to caller retention
/// settings or identity lifecycle.
pub async fn discard_peer_drop_receipts<B, E>(
    peer: ReceiptPeer,
    store: &MunimentStore<B, E>,
) -> Result<u64, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let keys = store.backend().list(&peer_receipt_prefix(peer)).await?;
    let writes: Vec<_> = keys
        .iter()
        .cloned()
        .map(|key| WriteOp::Delete { key })
        .collect();
    store.backend().apply(&writes).await?;
    Ok(keys.len() as u64)
}

/// List verified corpora awaiting retry or caller-directed disposal.
pub async fn list_staged_drops<B, E>(
    store: &MunimentStore<B, E>,
) -> Result<Vec<StagedDrop>, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let mut staged = Vec::new();
    for key in store.backend().list("drop-stage/").await? {
        if !key.ends_with("/meta") {
            continue;
        }
        let Some(bytes) = store.backend().get(&key).await? else {
            continue;
        };
        let (drop_id, record_count): (DropId, u64) = decode_cbor(&bytes[..])
            .map_err(|error| DropIoError::StagingCodec(error.to_string()))?;
        if key != stage_meta_key(drop_id) {
            return Err(DropIoError::FalseStage);
        }
        staged.push(StagedDrop {
            drop_id,
            record_count,
        });
    }
    staged.sort_by_key(|stage| stage.drop_id.0);
    Ok(staged)
}

/// Retry one verified staged corpus through current domain policy and frontier
/// state.
pub async fn resume_staged_drop<B, E, P>(
    drop_id: DropId,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
{
    let meta_key = stage_meta_key(drop_id);
    let Some(meta) = processor.store().backend().get(&meta_key).await? else {
        return Err(DropIoError::MissingStage(drop_hex(drop_id)));
    };
    let (stored_id, record_count): (DropId, u64) =
        decode_cbor(&meta[..]).map_err(|error| DropIoError::StagingCodec(error.to_string()))?;
    if stored_id != drop_id {
        return Err(DropIoError::FalseStage);
    }
    if record_count > limits.max_records {
        return Err(NativeDropError::Limit {
            limit: "record count",
            actual: record_count,
            maximum: limits.max_records,
        }
        .into());
    }
    let mut records = Vec::new();
    let prefix = stage_prefix(drop_id);
    for index in 0..record_count {
        let key = format!("{prefix}{index:016x}");
        let Some(bytes) = processor.store().backend().get(&key).await? else {
            return Err(DropIoError::FalseStage);
        };
        if bytes.len() as u64 > limits.max_record_bytes {
            return Err(NativeDropError::Limit {
                limit: "record bytes",
                actual: bytes.len() as u64,
                maximum: limits.max_record_bytes,
            }
            .into());
        }
        records.push(
            decode_cbor(&bytes[..])
                .map_err(|error| DropIoError::StagingCodec(error.to_string()))?,
        );
    }
    import_records(drop_id, records, processor).await
}

/// Remove a staged corpus selected by caller policy or settings.
pub async fn discard_staged_drop<B, E>(
    drop_id: DropId,
    store: &MunimentStore<B, E>,
) -> Result<u64, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let keys = store.backend().list(&stage_prefix(drop_id)).await?;
    let writes: Vec<_> = keys
        .iter()
        .cloned()
        .map(|key| WriteOp::Delete { key })
        .collect();
    store.backend().apply(&writes).await?;
    Ok(keys.len() as u64)
}

async fn import_records<B, E, P>(
    drop_id: DropId,
    records: Vec<DropRecord>,
    processor: &OperationProcessor<B, E, P>,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
{
    let receipt_key = receipt_key(drop_id);
    if let Some(bytes) = processor.store().backend().get(&receipt_key).await? {
        let receipt: StoredDropReceipt = decode_cbor(&bytes[..])
            .map_err(|error| DropIoError::StagingCodec(error.to_string()))?;
        discard_staged_drop(drop_id, processor.store()).await?;
        return Ok(DropImportReport {
            drop_id,
            accepted: 0,
            duplicate: receipt.operation_records,
            hydrated_payloads: 0,
            non_operation_records: receipt.non_operation_records,
            receipt_hit: true,
        });
    }

    let content = assemble_content(&records)?;
    let mut operations = Vec::new();
    let mut non_operation_records = 0;
    for record in &records {
        match decode_operation_record(record)? {
            Some(operation) => operations.push(operation),
            None => non_operation_records += 1,
        }
    }

    let mut consumed_payloads = BTreeSet::new();
    let mut consumed_pending = BTreeSet::new();
    for operation in &mut operations {
        let Some(payload_hash) = operation.header.payload_hash else {
            continue;
        };
        let digest = *payload_hash.as_bytes();
        if operation.body.is_some() {
            if content.payloads.contains_key(&digest) {
                consumed_payloads.insert(digest);
            }
            continue;
        }
        if let Some(bytes) = content.payloads.get(&digest) {
            operation.body = Some(Body::from(bytes.clone()));
            consumed_payloads.insert(digest);
            continue;
        }
        if let Some(bytes) = processor.store().pending_payload(&payload_hash).await? {
            if Hash::digest(&bytes) != payload_hash {
                return Err(DropIoError::FalseChunkDigest {
                    kind: "payload",
                    digest: payload_hash.to_hex(),
                });
            }
            operation.body = Some(Body::from(bytes));
            consumed_pending.insert(digest);
        }
    }

    // Authorization and structural validity are all-or-nothing and happen
    // before durable staging.
    for operation in &operations {
        processor.preflight(operation)?;
    }

    let mut leading_writes = Vec::new();
    for digest in consumed_pending {
        let payload_hash = Hash::from_bytes(digest);
        leading_writes.push(WriteOp::Delete {
            key: pending_payload_key(&payload_hash),
        });
    }
    for (digest, bytes) in &content.payloads {
        let payload_hash = Hash::from_bytes(*digest);
        let (writes, attached) = processor
            .store()
            .payload_attachment_writes(&payload_hash, bytes)
            .await?;
        leading_writes.extend(writes);
        let pending_key = pending_payload_key(&payload_hash);
        if consumed_payloads.contains(digest) || attached > 0 {
            leading_writes.push(WriteOp::Delete { key: pending_key });
        } else {
            leading_writes.push(WriteOp::Put {
                key: pending_key,
                value: bytes.clone(),
            });
        }
    }
    for (digest, bytes) in &content.blobs {
        leading_writes.push(MunimentStore::<B, E>::imported_blob_write(*digest, bytes)?);
    }

    let stage_keys = stage_records(drop_id, &records, processor.store().backend()).await?;
    let stored_receipt = StoredDropReceipt {
        operation_records: operations.len() as u64,
        non_operation_records,
    };
    let mut final_writes: Vec<_> = stage_keys
        .into_iter()
        .map(|key| WriteOp::Delete { key })
        .collect();
    final_writes.push(WriteOp::Put {
        key: receipt_key,
        value: encode_cbor(&stored_receipt)
            .map_err(|error| DropIoError::StagingCodec(error.to_string()))?,
    });
    let outcome = processor
        .process_batch_atomic(&operations, &leading_writes, &final_writes)
        .await?;
    Ok(DropImportReport {
        drop_id,
        accepted: outcome.inserted,
        duplicate: outcome.duplicate,
        hydrated_payloads: outcome.hydrated_payloads,
        non_operation_records,
        receipt_hit: false,
    })
}

fn drop_hex(drop_id: DropId) -> String {
    hex::encode(drop_id.0)
}

fn receipt_key(drop_id: DropId) -> String {
    format!("drop-receipt/{}", drop_hex(drop_id))
}

fn peer_receipt_prefix(peer: ReceiptPeer) -> String {
    format!("drop-peer-receipt/{}/", hex::encode(peer.0))
}

fn peer_receipt_key(peer: ReceiptPeer, drop_id: DropId) -> String {
    format!("{}{}", peer_receipt_prefix(peer), drop_hex(drop_id))
}

fn stage_prefix(drop_id: DropId) -> String {
    format!("drop-stage/{}/", drop_hex(drop_id))
}

fn stage_meta_key(drop_id: DropId) -> String {
    format!("{}meta", stage_prefix(drop_id))
}

fn assemble_content(records: &[DropRecord]) -> Result<AssembledContent, DropIoError> {
    let mut payload_chunks: BTreeMap<[u8; 32], Vec<(u64, &[u8])>> = BTreeMap::new();
    let mut blob_chunks: BTreeMap<[u8; 32], Vec<(u64, &[u8])>> = BTreeMap::new();
    for record in records {
        match record {
            DropRecord::PayloadChunk {
                payload_hash,
                offset,
                bytes,
            } => payload_chunks
                .entry(*payload_hash)
                .or_default()
                .push((*offset, bytes)),
            DropRecord::BlobChunk {
                blob_hash,
                offset,
                bytes,
            } => blob_chunks
                .entry(*blob_hash)
                .or_default()
                .push((*offset, bytes)),
            _ => {}
        }
    }
    let payloads = payload_chunks
        .into_iter()
        .map(|(digest, chunks)| {
            assemble_chunks("payload", digest, chunks).map(|bytes| (digest, bytes))
        })
        .collect::<Result<_, _>>()?;
    let blobs = blob_chunks
        .into_iter()
        .map(|(digest, chunks)| {
            assemble_chunks("blob", digest, chunks).map(|bytes| (digest, bytes))
        })
        .collect::<Result<_, _>>()?;
    Ok(AssembledContent { payloads, blobs })
}

fn assemble_chunks(
    kind: &'static str,
    digest: [u8; 32],
    mut chunks: Vec<(u64, &[u8])>,
) -> Result<Vec<u8>, DropIoError> {
    chunks.sort_by_key(|(offset, _)| *offset);
    let mut bytes = Vec::new();
    let mut expected = 0u64;
    for (offset, chunk) in chunks {
        if offset != expected {
            return Err(DropIoError::ChunkLayout {
                kind,
                digest: hex::encode(digest),
                offset,
            });
        }
        bytes.extend_from_slice(chunk);
        expected = expected
            .checked_add(chunk.len() as u64)
            .ok_or(DropIoError::ChunkLayout {
                kind,
                digest: hex::encode(digest),
                offset,
            })?;
    }
    if *blake3::hash(&bytes).as_bytes() != digest {
        return Err(DropIoError::FalseChunkDigest {
            kind,
            digest: hex::encode(digest),
        });
    }
    Ok(bytes)
}

async fn stage_records<B: Backend>(
    drop_id: DropId,
    records: &[DropRecord],
    backend: &B,
) -> Result<Vec<String>, DropIoError> {
    let prefix = stage_prefix(drop_id);
    let old_keys = backend.list(&prefix).await?;
    let mut writes: Vec<_> = old_keys
        .into_iter()
        .map(|key| WriteOp::Delete { key })
        .collect();
    let meta_key = stage_meta_key(drop_id);
    writes.push(WriteOp::Put {
        key: meta_key.clone(),
        value: encode_cbor(&(drop_id, records.len() as u64))
            .map_err(|error| DropIoError::StagingCodec(error.to_string()))?,
    });
    let mut staged_keys = vec![meta_key];
    for (index, record) in records.iter().enumerate() {
        let key = format!("{prefix}{index:016x}");
        writes.push(WriteOp::Put {
            key: key.clone(),
            value: encode_cbor(record)
                .map_err(|error| DropIoError::StagingCodec(error.to_string()))?,
        });
        staged_keys.push(key);
    }
    backend.apply(&writes).await?;
    Ok(staged_keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drop::write_plain_drop;
    use crate::{
        Admission, DropReceiptLimits, Reject, StoreTarget, read_drop_receipt, write_drop_receipt,
    };
    use muniment::MemoryBackend;
    use p2panda_core::{SigningKey, Topic};
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Ext {
        space: u8,
    }

    #[derive(Clone, Copy)]
    struct Policy;

    impl OperationPolicy<Ext> for Policy {
        type LogId = u64;

        fn admit(&self, operation: &Operation<Ext>) -> Result<Admission<u64>, Reject> {
            if operation.header.extensions.space != 7 {
                return Err(Reject::new("wrong-space", "not this space"));
            }
            Ok(Admission::keep(StoreTarget::new(Topic::from([7; 32]), 0)))
        }
    }

    struct RadioSelector;

    impl DropExportSelector<Ext> for RadioSelector {
        fn select(&self, operation: &Operation<Ext>) -> DropExportDecision {
            match operation.header.seq_num {
                0 => DropExportDecision::Omit,
                1 => DropExportDecision::Header { priority: 100 },
                _ => DropExportDecision::Full { priority: 10 },
            }
        }
    }

    #[derive(Clone)]
    struct EffectPolicy {
        erase: p2panda_core::Hash,
    }

    impl OperationPolicy<Ext> for EffectPolicy {
        type LogId = u64;

        fn admit(&self, operation: &Operation<Ext>) -> Result<Admission<u64>, Reject> {
            if operation.header.extensions.space != 7 {
                return Err(Reject::new("wrong-space", "not this space"));
            }
            let target = StoreTarget::new(Topic::from([7; 32]), 0);
            Ok(if operation.header.seq_num == 2 {
                Admission::prune_before_current(target).erasing_payloads([self.erase])
            } else {
                Admission::keep(target)
            })
        }
    }

    fn operation(
        key: &SigningKey,
        seq: u64,
        backlink: Option<p2panda_core::Hash>,
    ) -> Operation<Ext> {
        let body = Body::new(format!("event-{seq}").as_bytes());
        let mut header = Header {
            version: 1,
            verifying_key: key.verifying_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: seq.into(),
            seq_num: seq,
            backlink,
            extensions: Ext { space: 7 },
        };
        header.sign(key);
        Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        }
    }

    #[test]
    fn export_import_is_idempotent_and_uses_the_processor() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let first = operation(&key, 0, None);
            let second = operation(&key, 1, Some(first.hash));
            let records = vec![
                operation_record(&second, true),
                operation_record(&first, true),
            ];
            let mut bytes = Vec::new();
            write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();

            let processor =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
            let first_report = import_plain_drop(
                std::io::Cursor::new(&bytes),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert_eq!(first_report.accepted, 2);
            assert!(!first_report.receipt_hit);
            let second_report = import_plain_drop(
                std::io::Cursor::new(&bytes),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert_eq!(second_report.duplicate, 2);
            assert!(second_report.receipt_hit);
            assert_eq!(second_report.drop_id, first_report.drop_id);
            assert!(
                processor
                    .store()
                    .backend()
                    .list("drop-stage/")
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                processor
                    .store()
                    .backend()
                    .list("drop-receipt/")
                    .await
                    .unwrap()
                    .len(),
                1
            );

            let exported = export_topic_operations::<_, _, u64>(
                processor.store(),
                &Topic::from([7; 32]),
                DropExportProfile {
                    include_operation_bodies: false,
                },
            )
            .await
            .unwrap();
            assert_eq!(exported.len(), 2);
            assert!(exported.iter().all(|record| matches!(
                record,
                DropRecord::Operation {
                    inline_body: None,
                    ..
                }
            )));
        });
    }

    #[test]
    fn plaintext_file_moves_one_topic_between_fresh_profiles() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let first = operation(&key, 0, None);
            let second = operation(&key, 1, Some(first.hash));
            let source = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
            source.process(&first).await.unwrap();
            source.process(&second).await.unwrap();

            let path = std::env::temp_dir().join(format!(
                "murm-drop-file-{}-{}.meredrop",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let written = export_plain_topic_file::<_, _, u64>(
                &path,
                source.store(),
                &Topic::from([7; 32]),
                DropExportProfile::default(),
                DropLimits::default(),
            )
            .await
            .unwrap();
            assert!(written.total_bytes > 52);

            let destination =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
            let first_import = import_plain_drop_file(&path, DropLimits::default(), &destination)
                .await
                .unwrap();
            assert_eq!(first_import.drop_id, written.id);
            assert_eq!(first_import.accepted, 2);
            assert_eq!(destination.store().operation_count().await.unwrap(), 2);

            let repeated = import_plain_drop_file(&path, DropLimits::default(), &destination)
                .await
                .unwrap();
            assert!(repeated.receipt_hit);
            assert_eq!(repeated.duplicate, 2);
            std::fs::remove_file(path).unwrap();
        });
    }

    #[test]
    fn domain_selector_enforces_privacy_priority_and_radio_budget() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let first = operation(&key, 0, None);
            let second = operation(&key, 1, Some(first.hash));
            let third = operation(&key, 2, Some(second.hash));
            let source = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
            source.process(&first).await.unwrap();
            source.process(&second).await.unwrap();
            source.process(&third).await.unwrap();

            let (unbounded, unbounded_stats) = export_topic_operations_selected::<_, _, u64, _>(
                source.store(),
                &Topic::from([7; 32]),
                &RadioSelector,
                DropExportBudget::default(),
            )
            .await
            .unwrap();
            assert_eq!(unbounded_stats.policy_omitted, 1);
            assert_eq!(unbounded_stats.selected_records, 2);
            let high_priority_bytes = encode_cbor(&unbounded[0]).unwrap().len() as u64;
            let high_priority = decode_operation_record::<Ext>(&unbounded[0])
                .unwrap()
                .unwrap();
            assert_eq!(high_priority.header.seq_num, 1);
            assert!(high_priority.body.is_none());

            let path = std::env::temp_dir().join(format!(
                "murm-radio-drop-{}-{}.meredrop",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let report = export_selected_plain_topic_file::<_, _, u64, _>(
                &path,
                source.store(),
                &Topic::from([7; 32]),
                &RadioSelector,
                DropExportBudget {
                    max_selected_record_bytes: Some(high_priority_bytes),
                },
                DropLimits::default(),
            )
            .await
            .unwrap();
            assert_eq!(report.selection.selected_records, 1);
            assert_eq!(report.selection.policy_omitted, 1);
            assert_eq!(report.selection.budget_omitted, 1);

            let (_, selected) = read_plain_drop(
                BufReader::new(File::open(&path).unwrap()),
                DropLimits::default(),
            )
            .unwrap();
            assert_eq!(selected.len(), 1);
            assert_eq!(
                decode_operation_record::<Ext>(&selected[0])
                    .unwrap()
                    .unwrap()
                    .header
                    .seq_num,
                1
            );
            std::fs::remove_file(path).unwrap();
        });
    }

    #[test]
    fn completed_receipt_round_trips_and_stays_peer_scoped() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let operation = operation(&key, 0, None);
            let records = vec![operation_record(&operation, true)];
            let mut drop_bytes = Vec::new();
            let written =
                write_plain_drop(&mut drop_bytes, &records, DropLimits::default()).unwrap();

            let receiver =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
            import_plain_drop(
                std::io::Cursor::new(drop_bytes),
                DropLimits::default(),
                &receiver,
            )
            .await
            .unwrap();
            let local = local_drop_receipt(written.id, receiver.store())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(local.operation_records, 1);
            assert_eq!(local.non_operation_records, 0);

            let mut receipt_bytes = Vec::new();
            write_drop_receipt(&mut receipt_bytes, local, DropReceiptLimits::default()).unwrap();
            let exchanged =
                read_drop_receipt(&receipt_bytes[..], DropReceiptLimits::default()).unwrap();

            let sender =
                OperationProcessor::new(MunimentStore::<_, Ext>::new(MemoryBackend::new()), Policy);
            let peer = ReceiptPeer::from_authenticated_identity(b"receiver-session-identity");
            store_peer_drop_receipt(peer, exchanged, sender.store())
                .await
                .unwrap();
            assert_eq!(
                peer_drop_receipt(peer, written.id, sender.store())
                    .await
                    .unwrap(),
                Some(local)
            );
            assert_eq!(
                local_drop_receipt(written.id, sender.store())
                    .await
                    .unwrap(),
                None
            );
            assert_eq!(
                discard_peer_drop_receipts(peer, sender.store())
                    .await
                    .unwrap(),
                1
            );
        });
    }

    #[test]
    fn unauthorized_batch_is_rejected_before_any_operation_lands() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let allowed = operation(&key, 0, None);
            let denied_key = SigningKey::generate();
            let mut denied = operation(&denied_key, 0, None);
            denied.header.extensions.space = 8;
            denied.header.sign(&denied_key);
            denied.hash = denied.header.hash();
            let records = vec![
                operation_record(&allowed, true),
                operation_record(&denied, true),
            ];
            let mut bytes = Vec::new();
            write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
            let processor =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
            assert!(
                import_plain_drop(
                    std::io::Cursor::new(bytes),
                    DropLimits::default(),
                    &processor,
                )
                .await
                .is_err()
            );
            assert!(processor.store().is_empty().await.unwrap());
        });
    }

    #[test]
    fn payload_chunks_complete_a_header_in_the_same_drop() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let operation = operation(&key, 0, None);
            let body = operation.body.as_ref().unwrap().to_bytes();
            let digest = *operation.header.payload_hash.unwrap().as_bytes();
            let split = body.len() / 2;
            let records = vec![
                operation_record(&operation, false),
                DropRecord::PayloadChunk {
                    payload_hash: digest,
                    offset: split as u64,
                    bytes: body[split..].to_vec(),
                },
                DropRecord::PayloadChunk {
                    payload_hash: digest,
                    offset: 0,
                    bytes: body[..split].to_vec(),
                },
            ];
            let mut bytes = Vec::new();
            write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
            let processor =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);

            let report = import_plain_drop(
                std::io::Cursor::new(bytes),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert_eq!(report.accepted, 1);
            assert_eq!(
                processor
                    .store()
                    .get_operation(&operation.hash)
                    .await
                    .unwrap()
                    .unwrap()
                    .body
                    .unwrap()
                    .to_bytes(),
                body
            );
        });
    }

    #[test]
    fn payload_only_drop_waits_for_its_header() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let operation = operation(&key, 0, None);
            let body = operation.body.as_ref().unwrap().to_bytes();
            let payload_hash = operation.header.payload_hash.unwrap();
            let chunk = DropRecord::PayloadChunk {
                payload_hash: *payload_hash.as_bytes(),
                offset: 0,
                bytes: body.clone(),
            };
            let mut payload_drop = Vec::new();
            write_plain_drop(&mut payload_drop, &[chunk], DropLimits::default()).unwrap();
            let processor =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
            let payload_report = import_plain_drop(
                std::io::Cursor::new(payload_drop),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert_eq!(payload_report.accepted, 0);
            assert_eq!(
                processor
                    .store()
                    .backend()
                    .list("pending-payload/")
                    .await
                    .unwrap()
                    .len(),
                1
            );

            let mut header_drop = Vec::new();
            write_plain_drop(
                &mut header_drop,
                &[operation_record(&operation, false)],
                DropLimits::default(),
            )
            .unwrap();
            let header_report = import_plain_drop(
                std::io::Cursor::new(header_drop),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert_eq!(header_report.accepted, 1);
            assert!(
                processor
                    .store()
                    .backend()
                    .list("pending-payload/")
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                processor
                    .store()
                    .get_operation(&operation.hash)
                    .await
                    .unwrap()
                    .unwrap()
                    .body
                    .unwrap()
                    .to_bytes(),
                body
            );
        });
    }

    #[test]
    fn header_only_drop_accepts_its_payload_later() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let operation = operation(&key, 0, None);
            let body = operation.body.as_ref().unwrap().to_bytes();
            let payload_hash = operation.header.payload_hash.unwrap();
            let processor =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);

            let mut header_drop = Vec::new();
            write_plain_drop(
                &mut header_drop,
                &[operation_record(&operation, false)],
                DropLimits::default(),
            )
            .unwrap();
            import_plain_drop(
                std::io::Cursor::new(header_drop),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert!(
                processor
                    .store()
                    .get_operation(&operation.hash)
                    .await
                    .unwrap()
                    .unwrap()
                    .body
                    .is_none()
            );

            let mut payload_drop = Vec::new();
            write_plain_drop(
                &mut payload_drop,
                &[DropRecord::PayloadChunk {
                    payload_hash: *payload_hash.as_bytes(),
                    offset: 0,
                    bytes: body.clone(),
                }],
                DropLimits::default(),
            )
            .unwrap();
            import_plain_drop(
                std::io::Cursor::new(payload_drop),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert_eq!(
                processor
                    .store()
                    .get_operation(&operation.hash)
                    .await
                    .unwrap()
                    .unwrap()
                    .body
                    .unwrap()
                    .to_bytes(),
                body
            );
        });
    }

    #[test]
    fn blob_chunks_land_in_the_content_addressed_store() {
        pollster::block_on(async {
            let blob = b"native drop blob";
            let reference = proofs::BlobRef::blake3(blob);
            let digest = reference.digest.as_32().unwrap();
            let records = vec![
                DropRecord::BlobChunk {
                    blob_hash: digest,
                    offset: 7,
                    bytes: blob[7..].to_vec(),
                },
                DropRecord::BlobChunk {
                    blob_hash: digest,
                    offset: 0,
                    bytes: blob[..7].to_vec(),
                },
            ];
            let mut bytes = Vec::new();
            write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
            let processor =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
            import_plain_drop(
                std::io::Cursor::new(bytes),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert_eq!(
                processor.store().get_blob(&reference).await.unwrap(),
                Some(blob.to_vec())
            );
        });
    }

    #[test]
    fn continuity_failure_keeps_a_durable_stage_and_commits_nothing_live() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let first = operation(&key, 0, None);
            let wrong = operation(
                &key,
                1,
                Some(p2panda_core::Hash::digest(b"wrong predecessor")),
            );
            let records = vec![
                operation_record(&first, true),
                operation_record(&wrong, true),
            ];
            let mut bytes = Vec::new();
            write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
            let processor =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);

            assert!(matches!(
                import_plain_drop(
                    std::io::Cursor::new(bytes),
                    DropLimits::default(),
                    &processor,
                )
                .await,
                Err(DropIoError::Process(ProcessError::Backlink(_)))
            ));
            assert!(processor.store().is_empty().await.unwrap());
            assert_eq!(
                processor
                    .store()
                    .backend()
                    .list("drop-receipt/")
                    .await
                    .unwrap()
                    .len(),
                0
            );
            assert_eq!(
                processor
                    .store()
                    .backend()
                    .list("drop-stage/")
                    .await
                    .unwrap()
                    .len(),
                3
            );
            let staged = list_staged_drops(processor.store()).await.unwrap();
            assert_eq!(staged.len(), 1);
            assert_eq!(staged[0].record_count, 2);
            assert_eq!(
                discard_staged_drop(staged[0].drop_id, processor.store())
                    .await
                    .unwrap(),
                3
            );
            assert!(
                list_staged_drops(processor.store())
                    .await
                    .unwrap()
                    .is_empty()
            );
        });
    }

    #[test]
    fn staged_drop_resumes_after_its_predecessor_arrives() {
        pollster::block_on(async {
            let key = SigningKey::generate();
            let first = operation(&key, 0, None);
            let second = operation(&key, 1, Some(first.hash));
            let mut bytes = Vec::new();
            write_plain_drop(
                &mut bytes,
                &[operation_record(&second, true)],
                DropLimits::default(),
            )
            .unwrap();
            let processor =
                OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
            assert!(matches!(
                import_plain_drop(
                    std::io::Cursor::new(bytes),
                    DropLimits::default(),
                    &processor,
                )
                .await,
                Err(DropIoError::Process(ProcessError::MissingPredecessor {
                    seq_num: 1
                }))
            ));
            let staged = list_staged_drops(processor.store()).await.unwrap();
            assert_eq!(staged.len(), 1);

            processor.process(&first).await.unwrap();
            let report = resume_staged_drop(staged[0].drop_id, DropLimits::default(), &processor)
                .await
                .unwrap();
            assert_eq!(report.accepted, 1);
            assert!(
                list_staged_drops(processor.store())
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert!(
                processor
                    .store()
                    .get_operation(&second.hash)
                    .await
                    .unwrap()
                    .is_some()
            );
        });
    }

    #[test]
    fn effectful_drop_prunes_and_erases_in_the_same_receipted_commit() {
        pollster::block_on(async {
            let owner = SigningKey::generate();
            let first = operation(&owner, 0, None);
            let second = operation(&owner, 1, Some(first.hash));
            let victim_key = SigningKey::generate();
            let victim = operation(&victim_key, 0, None);
            let victim_body = victim.body.as_ref().unwrap().to_bytes();
            let victim_payload_hash = victim.header.payload_hash.unwrap();
            let backend = MemoryBackend::new();
            let processor = OperationProcessor::new(
                MunimentStore::new(backend),
                EffectPolicy { erase: victim.hash },
            );
            processor.process(&first).await.unwrap();
            processor.process(&second).await.unwrap();
            processor.process(&victim).await.unwrap();

            let checkpoint = operation(&owner, 2, Some(second.hash));
            let mut bytes = Vec::new();
            write_plain_drop(
                &mut bytes,
                &[operation_record(&checkpoint, true)],
                DropLimits::default(),
            )
            .unwrap();
            let report = import_plain_drop(
                std::io::Cursor::new(bytes),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();

            assert_eq!(report.accepted, 1);
            assert!(
                processor
                    .store()
                    .get_operation(&first.hash)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                processor
                    .store()
                    .get_operation(&second.hash)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                processor
                    .store()
                    .get_operation(&checkpoint.hash)
                    .await
                    .unwrap()
                    .is_some()
            );
            assert!(
                processor
                    .store()
                    .get_operation(&victim.hash)
                    .await
                    .unwrap()
                    .is_some_and(|operation| operation.body.is_none())
            );
            let mut late_payload_drop = Vec::new();
            write_plain_drop(
                &mut late_payload_drop,
                &[DropRecord::PayloadChunk {
                    payload_hash: *victim_payload_hash.as_bytes(),
                    offset: 0,
                    bytes: victim_body,
                }],
                DropLimits::default(),
            )
            .unwrap();
            import_plain_drop(
                std::io::Cursor::new(late_payload_drop),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert!(
                processor
                    .store()
                    .get_operation(&victim.hash)
                    .await
                    .unwrap()
                    .is_some_and(|operation| operation.body.is_none())
            );
            assert!(
                processor
                    .store()
                    .backend()
                    .list("drop-stage/")
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                processor
                    .store()
                    .backend()
                    .list("drop-receipt/")
                    .await
                    .unwrap()
                    .len(),
                2
            );
        });
    }
}
