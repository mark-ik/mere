# Bounty Verification Economy Plan

**Date**: 2026-06-30  
**Status**: Planned outer-ring architecture.  
**Related**:
[`../../mere_docs/research/2026-06-04_resource_coordination_brief.md`](../../mere_docs/research/2026-06-04_resource_coordination_brief.md),
[`../../mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](../../mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md),
[`../../mere_docs/implementation_strategy/2026-06-06_moot_constitution_brief.md`](../../mere_docs/implementation_strategy/2026-06-06_moot_constitution_brief.md),
[`../../archive_docs/2026-06-09_completed_plans/2026-06-02_tessera_plan.md`](../../archive_docs/2026-06-09_completed_plans/2026-06-02_tessera_plan.md)

The bounty economy is not the mesh. It is the verification and settlement layer
that engages when work crosses beyond owned devices and granted kith.

---

## Boundary

Mesh substrate:

- moves signed job facts
- runs jobs inside namespaces
- routes leases and results
- enforces local device policy

Bounty economy:

- defines the result being bought
- escrows credits
- chooses verification tier
- accounts for fulfilment, lapse, and settlement
- updates reciprocity credit and tessera standing

Keeping this boundary clean prevents the mesh crate from becoming a market
runtime.

---

## Event Grammar

The economy is a ledger projection over signed facts:

```rust
pub enum BountyEvent {
    BountyPosted { spec: ResultSpec, escrow: CreditAmount, tier: VerificationTier },
    BountyClaimed { bounty: BountyId, lease: LeaseId },
    Submission { bounty: BountyId, artifact: BlobRef },
    VerdictCommitted { bounty: BountyId, commitment: Hash },
    VerdictRevealed { bounty: BountyId, verdict: Verdict, nonce: Vec<u8> },
    Settlement { bounty: BountyId, winner: PersonaKey },
    CommitmentLapsed { bounty: BountyId, claimant: PersonaKey },
}
```

The event names are design placeholders. The important part is that settlement
is a fold, not a mutable account table.

---

## ResultSpec

`ResultSpec` is the contract:

- expected artifact type
- input namespace
- acceptance predicate
- verification tier
- timeout and heartbeat cadence
- deterministic or non-deterministic settlement mode
- privacy class
- allowed worker ring

For deterministic work, the artifact hash can settle most of the dispute. For
non-deterministic work, the spec must name verifier rules before work starts.

---

## Verification Tiers

- `T0`: self-verifying hash or deterministic local rerun.
- `T1`: trusted-ring verification, light redundancy, or asker acceptance.
- `T2`: quorum, spot checks, commit-reveal verdicts, and higher escrow.
- `T3`: expensive proofs, trusted hardware, or institutional/native helper lanes.

Browser nodes should stay in T0/T1 unless the result can be verified without
native helper machinery.

---

## Ledgers

Two ledgers remain separate:

- **Tessera**: non-buyable standing from real fulfilment and reliability.
- **Credits**: spendable reciprocity or funded-bounty units.

Credits can appear at the cash edge. Tessera cannot. Spending is bounded by
standing, and concord stays one-hop. The funded-bounty lane must not crowd out
commons reciprocity or buy governance.

---

## Moot Epoch Receipts

Tessera should bind help to the community state under which it happened. That
means receipts should name an epoch, not just a mutable moot id.

Minimum epoch header:

```rust
pub struct MootEpochHeader {
    pub moot_id: MootId,
    pub constitution_hash: Hash,
    pub policy_hash: Hash,
    pub epoch_number: u64,
    pub admin_root: Hash,
    pub mod_root: Hash,
    pub stakeholder_root: Hash,
    pub member_root: Hash,
}
```

The roots are not plain roster hashes if later proof is needed. Use Merkle roots,
vector commitments, or an accumulator shape so a receipt can carry an inclusion
proof without publishing the whole membership list.

Receipt shape:

```rust
pub struct WorkReceipt {
    pub bounty: BountyId,
    pub result_hash: Hash,
    pub executor: PersonaKey,
    pub verifier: Option<PersonaKey>,
    pub moot_epoch: MootEpochHeader,
    pub role_proof: InclusionProof,
    pub capability_proof: Option<DelegationProof>,
    pub signed_at_ms: u64,
}
```

Tessera then records "this persona helped under this moot epoch, role root,
capability proof, and result hash." Later value is a projection: current moot
policy can ask how many old members remain, whether the constitution changed, or
whether the signer belonged to admin, mods, stakeholders, or general members.
The historical receipt does not get rewritten.

---

## Done Conditions

- A deterministic bounty posts, claims, submits, verifies, and settles as a pure
  event fold.
- A wrong-hash submission is rejected.
- A missed heartbeat produces a lapse event distinct from owner reclaim.
- Verdict commit-reveal prevents a verifier from copying another verifier's
  answer.
- Credit and tessera deltas are derived from the event sequence.

## Progress

- **2026-06-30** - Split out of the merged resource-coordination brief and moved
  under `moothold_docs`, where the outer-ring economy belongs.
- **2026-06-30** - Added moot epoch receipts: tessera facts bind result,
  executor, role/capability proof, and epoch roots instead of relying on mutable
  membership labels.
