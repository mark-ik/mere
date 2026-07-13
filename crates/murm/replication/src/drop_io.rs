//! Operation export and staged native-drop import.
//!
//! The codec verifies the entire carrier before this module decodes operations.
//! Import then preflights every operation for structural and domain admission
//! before committing the first one through the ordinary processor.

use std::collections::BTreeMap;
use std::io::Read;

use muniment::{Backend, StoreError};
use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Body, Extensions, Header, LogId, Operation, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;

use crate::drop::{
    DropLimits, DropProtector, DropRecord, NativeDropError, read_plain_drop, read_protected_drop,
};
use crate::{MunimentStore, OperationPolicy, OperationProcessor, ProcessError};

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

/// Per-drop operation import counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DropImportReport {
    pub accepted: u64,
    pub duplicate: u64,
    pub non_operation_records: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum DropIoError {
    #[error(transparent)]
    Drop(#[from] NativeDropError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("drop operation codec: {0}")]
    OperationCodec(String),
    #[error("drop export store: {0}")]
    Store(#[from] StoreError),
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
    let logs: BTreeMap<VerifyingKey, Vec<L>> = store.resolve(topic).await?;
    let mut records = Vec::new();
    for (author, log_ids) in logs {
        for log_id in log_ids {
            if let Some(entries) = store.get_log_entries(&author, &log_id, None, None).await? {
                records.extend(entries.into_iter().map(|(operation, _)| {
                    operation_record(&operation, profile.include_operation_bodies)
                }));
            }
        }
    }
    Ok(records)
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
    let (_, records) = read_plain_drop(reader, limits)?;
    import_records(records, processor).await
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
    let (_, records) = read_protected_drop(reader, limits, protector)?;
    import_records(records, processor).await
}

async fn import_records<B, E, P>(
    records: Vec<DropRecord>,
    processor: &OperationProcessor<B, E, P>,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
{
    let mut operations = Vec::new();
    let mut non_operation_records = 0;
    for record in &records {
        match decode_operation_record(record)? {
            Some(operation) => operations.push(operation),
            None => non_operation_records += 1,
        }
    }

    // Authorization and structural validity are all-or-nothing for this drop.
    for operation in &operations {
        processor.preflight(operation)?;
    }
    operations.sort_by_key(|operation| {
        (
            *operation.header.verifying_key.as_bytes(),
            operation.header.seq_num,
        )
    });

    let mut report = DropImportReport {
        non_operation_records,
        ..Default::default()
    };
    for operation in &operations {
        if processor.process(operation).await?.inserted() {
            report.accepted += 1;
        } else {
            report.duplicate += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drop::write_plain_drop;
    use crate::{Admission, Reject, StoreTarget};
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
            let second_report = import_plain_drop(
                std::io::Cursor::new(&bytes),
                DropLimits::default(),
                &processor,
            )
            .await
            .unwrap();
            assert_eq!(second_report.duplicate, 2);

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
}
