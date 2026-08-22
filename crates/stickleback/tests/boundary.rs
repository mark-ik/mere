//! Boundary fixtures for the promoted replication contract.
//!
//! These exercise the four generic extension points named in the Stickleback
//! promotion plan's S0 slice, plus the derivation constants that must survive
//! the rename. They use the public surface only and a neutral test domain
//! ("ledger"), so they prove the crate composes for a domain that is not Murm,
//! Mesh, or Moot. Nothing here may reference a real domain crate.

use muniment::MemoryBackend;
use p2panda_core::{Body, Header, Operation, SigningKey, Topic};
use p2panda_store::logs::LogStore;
use proofs::Digest;
use serde::{Deserialize, Serialize};

use stickleback::{
    Admission, CheckpointAuthority, DropExportProfile, DropLimits, MunimentStore, OperationPolicy,
    OperationProcessor, ProcessOutcome, ReceiptPeer, Reject, StoreTarget, export_topic_operations,
    import_plain_drop, write_plain_drop,
};

/// The neutral domain's operation extensions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct LedgerExt {
    space: u8,
}

/// A neutral domain policy: entries belong to space 7 and must carry a body.
#[derive(Clone, Copy)]
struct LedgerPolicy;

impl OperationPolicy<LedgerExt> for LedgerPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<LedgerExt>) -> Result<Admission<u64>, Reject> {
        if operation.header.extensions.space != 7 {
            return Err(Reject::new("wrong-space", "entry addresses another ledger"));
        }
        if operation.body.is_none() {
            return Err(Reject::new("missing-body", "ledger entries require a body"));
        }
        Ok(Admission::keep(StoreTarget::new(Topic::from([7; 32]), 0)))
    }
}

/// The neutral domain's retention trust root: one signer set at one revision.
struct LedgerAuthority {
    signers: Vec<[u8; 32]>,
    revision: Digest,
}

impl CheckpointAuthority for LedgerAuthority {
    fn authority_revision(&self) -> Digest {
        self.revision.clone()
    }

    fn permits_checkpoint(&self, author: [u8; 32], named_revision: &Digest) -> bool {
        *named_revision == self.revision && self.signers.contains(&author)
    }
}

fn entry(
    key: &SigningKey,
    space: u8,
    seq: u32,
    backlink: Option<p2panda_core::Hash>,
) -> Operation<LedgerExt> {
    let body = Body::from_bytes(format!("entry-{seq}").as_bytes());
    // p2panda 0.7.1 made the header's CBOR cache, size and digest private
    // and folded signing into the builder: `build` encodes, signs and
    // caches the digest in one step, so the struct-literal + `sign` pair
    // has no equivalent. `body` sets payload_size and payload_hash.
    let header = Header::builder()
        .body(body.as_bytes())
        .seq_num(seq)
        .backlink(backlink)
        .build(key, LedgerExt { space });
    Operation {
        hash: header.hash(),
        header,
        body: Some(body),
    }
}

fn processor() -> OperationProcessor<MemoryBackend, LedgerExt, LedgerPolicy> {
    OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), LedgerPolicy)
}

/// Extension point 1: a domain policy decides admission *before* the store is
/// mutated, and a refusal leaves no trace.
#[test]
fn policy_accepts_and_rejects_before_storage() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let processor = processor();

        let accepted = entry(&key, 7, 0, None);
        assert!(matches!(
            processor.process(&accepted).await.unwrap(),
            ProcessOutcome::Inserted { .. }
        ));
        assert_eq!(processor.store().operation_count().await.unwrap(), 1);

        // Addressed to another ledger: refused, and the store is untouched.
        let foreign = entry(&key, 9, 0, None);
        let rejected = processor.process(&foreign).await;
        assert!(rejected.is_err(), "foreign-space entry must be refused");
        assert_eq!(
            processor.store().operation_count().await.unwrap(),
            1,
            "a refused entry must not mutate the store"
        );
        assert!(
            !processor
                .store()
                .has_operation(&foreign.hash)
                .await
                .unwrap(),
            "refused entry must be absent"
        );
    });
}

/// Extension point 2: locally authored and remotely received operations reach
/// storage through the same processor and land in the same store. There is no
/// second insertion path.
#[test]
fn local_and_received_operations_share_one_store() {
    pollster::block_on(async {
        let processor = processor();

        // Locally authored.
        let local_key = SigningKey::generate();
        let local = entry(&local_key, 7, 0, None);
        assert!(matches!(
            processor.process(&local).await.unwrap(),
            ProcessOutcome::Inserted { .. }
        ));

        // Received from a peer over the drain: same processor, same store.
        let peer_key = SigningKey::generate();
        let received = entry(&peer_key, 7, 0, None);
        assert!(matches!(
            processor.process(&received).await.unwrap(),
            ProcessOutcome::Inserted { .. }
        ));

        assert_eq!(processor.store().operation_count().await.unwrap(), 2);
        assert!(processor.store().has_operation(&local.hash).await.unwrap());
        assert!(
            processor
                .store()
                .has_operation(&received.hash)
                .await
                .unwrap()
        );

        // Re-offering a received operation is idempotent, not a second insert.
        assert_eq!(
            processor.process(&received).await.unwrap(),
            ProcessOutcome::Duplicate
        );
        assert_eq!(processor.store().operation_count().await.unwrap(), 2);
    });
}

/// Extension point 3: destructive retention runs only behind a domain-supplied
/// authority decision. Replication does not infer the right to prune.
///
/// The gate itself lives in the domain -- this crate declares the trait and
/// supplies the erasure mechanics, and never calls `permits_checkpoint` on its
/// own. This fixture is the composed seam: the same prune is refused for an
/// unauthorized author and a stale revision, and performed for an authorized
/// one.
#[test]
fn checkpoint_authority_gates_destructive_retention() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let author = key.verifying_key();
        let authority = LedgerAuthority {
            signers: vec![*author.as_bytes()],
            revision: Digest::blake3(b"ledger authority revision 1"),
        };

        let processor = processor();
        let first = entry(&key, 7, 0, None);
        let second = entry(&key, 7, 1, Some(first.hash));
        let third = entry(&key, 7, 2, Some(second.hash));
        for operation in [&first, &second, &third] {
            processor.process(operation).await.unwrap();
        }
        let log_id = 0u64;

        // An unauthorized author may not checkpoint: no prune is attempted.
        let stranger = [3u8; 32];
        assert!(!authority.permits_checkpoint(stranger, &authority.authority_revision()));

        // Neither may an authorized author naming a stale revision.
        let stale = Digest::blake3(b"ledger authority revision 0");
        assert!(!authority.permits_checkpoint(*author.as_bytes(), &stale));
        assert_eq!(
            processor.store().operation_count().await.unwrap(),
            3,
            "no refused checkpoint may erase history"
        );

        // The authorized signer at the current revision may prune.
        assert!(authority.permits_checkpoint(*author.as_bytes(), &authority.authority_revision()));
        let pruned =
            LogStore::<Operation<LedgerExt>, _, u64, u32, p2panda_core::Hash>::prune_entries(
                processor.store(),
                &author,
                &log_id,
                &2,
            )
            .await
            .unwrap();
        assert_eq!(pruned, 2);
        assert!(!processor.store().has_operation(&first.hash).await.unwrap());
        assert!(processor.store().has_operation(&third.hash).await.unwrap());
    });
}

/// Extension point 4: a native drop imports through the same processor, so an
/// offline carrier cannot bypass domain policy.
#[test]
fn native_drop_import_passes_through_the_processor() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let source = processor();
        let first = entry(&key, 7, 0, None);
        let second = entry(&key, 7, 1, Some(first.hash));
        source.process(&first).await.unwrap();
        source.process(&second).await.unwrap();

        let records = export_topic_operations::<_, _, u64>(
            source.store(),
            &Topic::from([7; 32]),
            DropExportProfile::default(),
        )
        .await
        .unwrap();
        let mut drop_bytes = Vec::new();
        write_plain_drop(&mut drop_bytes, &records, DropLimits::default()).unwrap();

        // A fresh holder accepts the carried operations through its policy.
        let sink = processor();
        let report = import_plain_drop(drop_bytes.as_slice(), DropLimits::default(), &sink)
            .await
            .unwrap();
        assert_eq!(sink.store().operation_count().await.unwrap(), 2);
        assert_eq!(
            report.accepted, 2,
            "both carried operations must be admitted"
        );

        // Re-importing the same drop is idempotent.
        import_plain_drop(drop_bytes.as_slice(), DropLimits::default(), &sink)
            .await
            .unwrap();
        assert_eq!(
            sink.store().operation_count().await.unwrap(),
            2,
            "drop import must not duplicate operations"
        );
    });
}

/// The promoted crate carries derivation constants whose inputs are part of the
/// stored format, not the package name. `ReceiptPeer` scopes muniment keys
/// through a blake3 KDF whose context string contains "murm"; renaming that
/// string would silently repoint every stored peer receipt.
///
/// This vector freezes the derivation across the promotion. If it fails after
/// a rename, the rename changed stored keys and must be reverted -- the plan's
/// stop rule.
#[test]
fn receipt_peer_scope_survives_the_promotion() {
    let peer = ReceiptPeer::from_authenticated_identity(b"stickleback-s0-fixed-identity");
    assert_eq!(
        hex::encode(peer.0),
        "3cb68dac6b318c71c04b526070017a84da3fe77f5bef063b39d1cea06c74a8e8",
        "ReceiptPeer derivation changed: the KDF context string must not be \
         renamed during the promotion"
    );
}
