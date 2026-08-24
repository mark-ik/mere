//! Focused proof that p2panda's prune law composes with the shared store.

use muniment::MemoryBackend;
use p2panda_core::prune::{PruneFlag, validate_prunable_backlink};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_stream::Processor;
use p2panda_stream::log_prune::{LogPrune, LogPruneArgs, LogPruneResult};
use serde::{Deserialize, Serialize};

use crate::{
    Admission, MunimentStore, OperationPolicy, OperationProcessor, ProcessError, ProcessOutcome,
    Reject, StoreTarget,
};

const SPACE: u64 = 7;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct LegacyExtension {
    #[serde(rename = "s")]
    space: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PrunableExtension {
    #[serde(rename = "s")]
    space: u64,
    #[serde(
        rename = "p",
        skip_serializing_if = "PruneFlag::is_not_set",
        default = "PruneFlag::default"
    )]
    prune_flag: PruneFlag,
}

#[derive(Clone, Copy)]
struct PrunePolicy {
    allow_prune: bool,
    prune_on_accept: bool,
}

impl OperationPolicy<PrunableExtension> for PrunePolicy {
    type LogId = u64;

    fn admit(
        &self,
        operation: &Operation<PrunableExtension>,
    ) -> Result<Admission<Self::LogId>, Reject> {
        if operation.header.extensions.space != SPACE {
            return Err(Reject::new(
                "wrong-space",
                "operation addresses another space",
            ));
        }
        if operation.header.extensions.prune_flag.is_set() && !self.allow_prune {
            return Err(Reject::new(
                "prune-not-authorized",
                "the current retention checkpoint does not authorize this prune point",
            ));
        }
        let target = StoreTarget::new(Topic::from([SPACE as u8; 32]), SPACE);
        if operation.header.extensions.prune_flag.is_set() && self.prune_on_accept {
            Ok(Admission::prune_before_current(target))
        } else {
            Ok(Admission::keep(target))
        }
    }
}

fn header<E>(signing_key: &SigningKey, extensions: E) -> Header<E>
where
    E: p2panda_core::Extensions,
{
    let body = Body::from_bytes(b"fixture");
    // p2panda 0.7.1 made the header's CBOR cache, size and digest private
    // and folded signing into the builder: `build` encodes, signs and
    // caches the digest in one step, so the struct-literal + `sign` pair
    // has no equivalent. `body` sets payload_size and payload_hash.
    let header = Header::builder()
        .body(body.as_bytes())
        .seq_num(0)
        .backlink(None)
        .build(signing_key, extensions);
    header
}

fn operation(
    signing_key: &SigningKey,
    seq_num: u32,
    backlink: Option<Hash>,
    prune: bool,
) -> Operation<PrunableExtension> {
    let body = Body::from_bytes(format!("event-{seq_num}").as_bytes());
    // p2panda 0.7.1 made the header's CBOR cache, size and digest private
    // and folded signing into the builder: `build` encodes, signs and
    // caches the digest in one step, so the struct-literal + `sign` pair
    // has no equivalent. `body` sets payload_size and payload_hash.
    let header = Header::builder()
        .body(body.as_bytes())
        .seq_num(seq_num)
        .backlink(backlink)
        .build(
            signing_key,
            PrunableExtension {
                space: SPACE,
                prune_flag: PruneFlag::new(prune),
            },
        );
    Operation {
        hash: header.hash(),
        header,
        body: Some(body),
    }
}

#[test]
fn default_prune_flag_preserves_pre_field_header() {
    let signing_key = SigningKey::from_bytes(&[0x42; 32]);
    let legacy = header(&signing_key, LegacyExtension { space: SPACE });
    let prunable = header(
        &signing_key,
        PrunableExtension {
            space: SPACE,
            prune_flag: PruneFlag::default(),
        },
    );

    assert_eq!(legacy.encode(), prunable.encode());
    assert_eq!(legacy.hash(), prunable.hash());
    assert_eq!(legacy.signature, prunable.signature);
}

#[test]
fn unapproved_prune_flag_leaves_the_store_unchanged() {
    pollster::block_on(async {
        let store = MunimentStore::new(MemoryBackend::new());
        let processor = OperationProcessor::new(
            store.clone(),
            PrunePolicy {
                allow_prune: false,
                prune_on_accept: false,
            },
        );
        let signing_key = SigningKey::generate();
        let prune_point = operation(&signing_key, 3, Some(Hash::digest(b"predecessor")), true);

        assert!(matches!(
            processor.process(&prune_point).await,
            Err(ProcessError::Rejected(Reject {
                code: "prune-not-authorized",
                ..
            }))
        ));
        assert!(store.is_empty().await.unwrap());
    });
}

#[test]
fn valid_prune_flag_removes_prefix_and_blocks_replay() {
    pollster::block_on(async {
        let store = MunimentStore::new(MemoryBackend::new());
        let processor = OperationProcessor::new(
            store.clone(),
            PrunePolicy {
                allow_prune: true,
                prune_on_accept: false,
            },
        );
        let signing_key = SigningKey::generate();
        let author = signing_key.verifying_key();

        let op_0 = operation(&signing_key, 0, None, false);
        let op_1 = operation(&signing_key, 1, Some(op_0.hash), false);
        let op_2 = operation(&signing_key, 2, Some(op_1.hash), false);
        let op_3 = operation(&signing_key, 3, Some(op_2.hash), true);

        for op in [&op_0, &op_1, &op_2, &op_3] {
            assert_eq!(
                processor.process(op).await.unwrap(),
                ProcessOutcome::Inserted {
                    pruned_entries: 0,
                    erased_payloads: 0,
                }
            );
        }

        let pruner: LogPrune<_, LogPruneArgs<VerifyingKey, u64, u32>, u64, PrunableExtension> =
            LogPrune::new(store.clone());
        pruner
            .process(LogPruneArgs::PruneEntriesUntil {
                author,
                log_id: SPACE,
                seq_num: 3,
            })
            .await
            .unwrap();
        let (_, result) = pruner.next().await.unwrap();
        assert_eq!(result, LogPruneResult::Pruned { num_entries: 3 });

        assert!(store.get_operation(&op_0.hash).await.unwrap().is_none());
        assert!(store.get_operation(&op_1.hash).await.unwrap().is_none());
        assert!(store.get_operation(&op_2.hash).await.unwrap().is_none());
        assert!(store.get_operation(&op_3.hash).await.unwrap().is_some());

        assert!(
            validate_prunable_backlink(
                None,
                &op_3.header,
                op_3.header.extensions.prune_flag.is_set(),
            )
            .is_ok()
        );
        assert!(matches!(
            processor.process(&op_0).await,
            Err(ProcessError::StaleOperation {
                seq_num: 0,
                frontier: 3
            })
        ));

        let entries = store
            .get_log_entries(&author, &SPACE, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0.hash, op_3.hash);
    });
}

#[test]
fn processor_commits_prune_point_and_prefix_in_one_batch() {
    pollster::block_on(async {
        let store = MunimentStore::new(MemoryBackend::new());
        let processor = OperationProcessor::new(
            store.clone(),
            PrunePolicy {
                allow_prune: true,
                prune_on_accept: true,
            },
        );
        let signing_key = SigningKey::generate();

        let op_0 = operation(&signing_key, 0, None, false);
        let op_1 = operation(&signing_key, 1, Some(op_0.hash), false);
        let op_2 = operation(&signing_key, 2, Some(op_1.hash), false);
        let op_3 = operation(&signing_key, 3, Some(op_2.hash), true);

        for op in [&op_0, &op_1, &op_2] {
            assert_eq!(
                processor.process(op).await.unwrap(),
                ProcessOutcome::Inserted {
                    pruned_entries: 0,
                    erased_payloads: 0,
                }
            );
        }
        assert_eq!(
            processor.process(&op_3).await.unwrap(),
            ProcessOutcome::Inserted {
                pruned_entries: 3,
                erased_payloads: 0,
            }
        );

        assert!(store.get_operation(&op_0.hash).await.unwrap().is_none());
        assert!(store.get_operation(&op_2.hash).await.unwrap().is_none());
        assert!(store.get_operation(&op_3.hash).await.unwrap().is_some());
        assert!(matches!(
            processor.process(&op_0).await,
            Err(ProcessError::StaleOperation {
                seq_num: 0,
                frontier: 3
            })
        ));

        let stale_flagged = operation(&signing_key, 2, Some(op_1.hash), true);
        assert!(matches!(
            processor.process(&stale_flagged).await,
            Err(ProcessError::StaleOperation {
                seq_num: 2,
                frontier: 3
            })
        ));
        assert_eq!(store.operation_count().await.unwrap(), 1);
    });
}

#[test]
fn processor_accepts_an_authorized_prune_point_without_its_predecessor() {
    pollster::block_on(async {
        let store = MunimentStore::new(MemoryBackend::new());
        let processor = OperationProcessor::new(
            store.clone(),
            PrunePolicy {
                allow_prune: true,
                prune_on_accept: true,
            },
        );
        let signing_key = SigningKey::generate();
        let prune_point = operation(&signing_key, 3, Some(Hash::digest(b"absent")), true);

        assert_eq!(
            processor.process(&prune_point).await.unwrap(),
            ProcessOutcome::Inserted {
                pruned_entries: 0,
                erased_payloads: 0,
            }
        );
        assert!(
            store
                .get_operation(&prune_point.hash)
                .await
                .unwrap()
                .is_some()
        );
    });
}
