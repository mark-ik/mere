# Commitment Proof Interface Plan

**Date**: 2026-06-30  
**Status**: Shared type interface landed 2026-07-13; proof backends remain planned.
**Related**:
[`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md),
[`2026-06-30_kith_capability_sharing_plan.md`](2026-06-30_kith_capability_sharing_plan.md),
[`2026-06-30_mesh_lease_scheduler_plan.md`](../../archive_docs/2026-08-09_completed_plans/2026-06-30_mesh_lease_scheduler_plan.md),
[`../../moothold_docs/implementation_strategy/2026-06-30_bounty_verification_economy_plan.md`](../../moothold_docs/implementation_strategy/2026-06-30_bounty_verification_economy_plan.md)

This plan owns the proof interface for moot epochs, tessera receipts, kith
grants, storage checkpoints, and mesh result evidence. The interface is shaped
around what a proof means, not around one tree implementation.

---

## Decision

Do not expose a `MerkleTree` as the model. Expose typed commitments and typed
proofs:

```rust
pub enum CommitmentScheme {
    MerkleV1,
    SparseMerkleV1,
    MmrV1,
    AccumulatorV1,
    VectorCommitmentV1,
    PsiCardinalityV1,
}

pub enum CommitmentDomain {
    MootAdmins,
    MootMods,
    MootStakeholders,
    MootMembers,
    TesseraReceipts,
    StorageChunks,
    StorageCheckpoints,
    MeshJobResults,
    DelegationSet,
}

pub struct Digest {
    pub alg: DigestAlg,
    pub bytes: Vec<u8>,
}

pub enum DigestAlg {
    Blake3,
    P2pandaOperation,
    Sha256,
    William3,
}

pub struct Commitment {
    pub scheme: CommitmentScheme,
    pub domain: CommitmentDomain,
    pub version: u16,
    pub digest: Digest,
}
```

The `DigestAlg` names the digest family. The `CommitmentScheme` names how the
digest should be interpreted as a proof root. This prevents three meanings from
colliding:

- p2panda operation hashes identify signed operations and their log order
- commitment digests identify application-level proof roots
- blob/content digests address stored bytes

---

## Proof Types

```rust
pub struct InclusionProof {
    pub commitment: Commitment,
    pub subject: Digest,
    pub proof_bytes: Vec<u8>,
}

pub struct NonInclusionProof {
    pub commitment: Commitment,
    pub subject: Digest,
    pub proof_bytes: Vec<u8>,
}

pub struct AppendProof {
    pub previous: Commitment,
    pub next: Commitment,
    pub appended_range: Range<u64>,
    pub proof_bytes: Vec<u8>,
}

pub struct SetRelationProof {
    pub left: Commitment,
    pub right: Commitment,
    pub relation: SetRelation,
    pub proof_bytes: Vec<u8>,
}

pub enum SetRelation {
    IsMember,
    IsNotMember,
    Subset,
    OverlapCountAtLeast(u64),
    ThresholdSigned(u16),
}
```

Callers ask for facts:

```rust
verify_membership(proof, subject, commitment) -> bool
verify_non_membership(proof, subject, commitment) -> bool
verify_append(proof, previous, next) -> bool
verify_overlap(proof, old_epoch, new_epoch) -> SetRelationResult
```

They do not walk a tree directly.

---

## Default Scheme Map

Start conservative:

- role rosters: `SparseMerkleV1`
- tessera receipt logs: `MmrV1`
- storage chunks: `MerkleV1`
- storage checkpoint logs: `MmrV1`
- mesh result batches: `MerkleV1` or `MmrV1` depending on append pressure
- private membership overlap: defer to `PsiCardinalityV1`

The first build can implement only `MerkleV1`, `SparseMerkleV1`, and `MmrV1`.
The others stay enum slots until scale or privacy pressure justifies them.

---

## P2panda Boundary

The commitment interface is p2panda-compliant only if it remains application
payload/projection metadata.

P2panda owns:

- operation authorship and signatures
- per-author append-only log sequence and backlinks
- topics and LogSync replication
- operation identity through the operation hash

Mere owns:

- moot epoch commitments
- role and membership proofs
- tessera receipt witnesses
- storage checkpoint witnesses
- mesh result and verifier evidence

So a p2panda operation should carry a logical event body such as:

```rust
pub enum MootEvent {
    EpochDeclared {
        epoch: u64,
        constitution: Digest,
        role_commitments: Vec<RoleCommitment>,
    },
    WorkReceiptIssued {
        receipt: WorkReceipt,
    },
    GrantIssued {
        grant: MeshGrant,
    },
    GrantRevoked {
        grant_id: GrantId,
        epoch: u64,
    },
}
```

The p2panda operation proves who authored the event and where it sits in that
author's log. The commitment proof proves a claim inside the event's social or
application meaning.

Do not:

- replace p2panda operation hashes with commitment digests
- treat an MMR or Merkle root as the p2panda log head
- put scheme-specific proof logic into operation header extensions
- use application proofs as a substitute for p2panda signature or backlink
  validation

---

## Done Conditions

- `MootEpochHeader` stores role commitments, not raw `*_root: Hash` fields.
- Work receipts carry typed `RoleWitness` and optional `DelegationProof`.
- Kith grants can name a delegation commitment without assuming a Merkle tree.
- Storage checkpoints distinguish chunk commitments from checkpoint-log
  commitments.
- p2panda wire docs say that commitments live in event bodies and projections,
  not operation headers.

## Progress

- **2026-06-30** - Created after reviewing Merkle alternatives. The plan keeps
  Merkle as the first backend while preserving MMR, sparse Merkle, accumulator,
  vector-commitment, and private-set-cardinality lanes behind one typed proof
  surface.
- **2026-07-13** - Landed the neutral `proofs` crate with typed `Digest`,
  `Commitment`, `CommitmentScheme`, `CommitmentDomain`, and `BlobRef`. Added
  `DigestV1` for direct commitments to canonical bytes so callers do not make a
  false Merkle claim. Mesh retention checkpoints now bind both a content
  reference and a `StorageCheckpoints` commitment while keeping their p2panda
  operation identity separate. Scheme-specific proof verification remains.
