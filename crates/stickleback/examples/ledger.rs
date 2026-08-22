//! A tiny neutral domain over Stickleback.
//!
//! "Ledger" is not one of Stickleback's real consumers -- it exists to show the
//! whole contract a domain has to supply, and nothing more:
//!
//! 1. an extensions type carried in every operation header,
//! 2. an [`OperationPolicy`] that authorizes and addresses an operation before
//!    it reaches storage,
//! 3. a [`CheckpointAuthority`], if the domain prunes history.
//!
//! Stickleback owns the rest: structural verification, log continuity,
//! idempotence, the atomic indexed write, and the erasure mechanics.
//!
//! Run with `cargo run --example ledger -p stickleback`.

use muniment::MemoryBackend;
use p2panda_core::Topic;
use p2panda_core::{Body, Header, Operation, SigningKey};
use proofs::Digest;
use serde::{Deserialize, Serialize};
use stickleback::{
    Admission, CheckpointAuthority, MunimentStore, OperationPolicy, OperationProcessor,
    ProcessOutcome, Reject, StoreTarget,
};

/// What this domain carries in an operation header. A real domain would put its
/// space id, addressing, and any capability reference here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct LedgerExt {
    /// Which ledger this entry belongs to.
    space: u8,
}

/// The domain's admission rule. This is the only place authorization lives:
/// Stickleback has already checked the signature, header, and body hash, and
/// will not store anything this rejects.
struct LedgerPolicy {
    space: u8,
}

impl OperationPolicy<LedgerExt> for LedgerPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<LedgerExt>) -> Result<Admission<u64>, Reject> {
        if operation.header.extensions.space != self.space {
            return Err(Reject::new("wrong-space", "entry addresses another ledger"));
        }
        if operation.body.is_none() {
            return Err(Reject::new("missing-body", "ledger entries require a body"));
        }
        // Address the entry: which topic replicates it, and which per-author log
        // it belongs to. Returning `keep` retains the preceding history.
        Ok(Admission::keep(StoreTarget::new(
            Topic::from([self.space; 32]),
            0,
        )))
    }
}

/// The domain's retention trust root. Stickleback never infers the right to
/// prune from transport access or visible membership -- it asks this.
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

fn main() {
    pollster::block_on(async {
        let key = SigningKey::generate();

        // One processor over one store is the whole ingress path. Locally
        // authored operations and operations received from a peer both go
        // through it -- there is deliberately no second way in.
        let processor = OperationProcessor::new(
            MunimentStore::new(MemoryBackend::new()),
            LedgerPolicy { space: 7 },
        );

        let first = entry(&key, 7, 0, None);
        let second = entry(&key, 7, 1, Some(first.hash));
        for operation in [&first, &second] {
            match processor.process(operation).await.unwrap() {
                ProcessOutcome::Inserted { .. } => println!("accepted {}", operation.hash),
                other => println!("unexpected outcome: {other:?}"),
            }
        }

        // The same operation again is idempotent, not a second insert.
        let repeat = processor.process(&second).await.unwrap();
        println!("re-offering the same entry: {repeat:?}");

        // An entry addressed to another ledger is refused before storage.
        let foreign = entry(&key, 9, 0, None);
        match processor.process(&foreign).await {
            Err(error) => println!("refused before storage: {error}"),
            Ok(outcome) => println!("unexpectedly stored: {outcome:?}"),
        }
        println!(
            "stored entries: {}",
            processor.store().operation_count().await.unwrap()
        );

        // Destructive retention is gated by the domain, not by Stickleback.
        let authority = LedgerAuthority {
            signers: vec![*key.verifying_key().as_bytes()],
            revision: Digest::blake3(b"ledger authority revision 1"),
        };
        let stranger = [3u8; 32];
        println!(
            "stranger may checkpoint: {}",
            authority.permits_checkpoint(stranger, &authority.authority_revision())
        );
        println!(
            "ledger signer may checkpoint: {}",
            authority.permits_checkpoint(
                *key.verifying_key().as_bytes(),
                &authority.authority_revision()
            )
        );
    });
}
