# Kith Capability Sharing Plan

**Date**: 2026-06-30  
**Status**: Planned after M2 substrate and M3 leases.  
**Related**:
[`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md),
[`2026-06-30_personal_mesh_substrate_m2_plan.md`](2026-06-30_personal_mesh_substrate_m2_plan.md),
[`2026-06-30_mesh_lease_scheduler_plan.md`](2026-06-30_mesh_lease_scheduler_plan.md),
[`2026-06-26_federation_interop_plan.md`](2026-06-26_federation_interop_plan.md),
[`2026-05-10_graph_cluster_namespaces_brief.md`](2026-05-10_graph_cluster_namespaces_brief.md)

Kith sharing is the first widening of the mesh. It is still permissioned and
free. The economy stays outside this ring.

---

## Boundary

Personal mesh:

- all peers are owned devices
- holding the mesh id is enough for M1
- scheduler policy is local

Kith mesh:

- a person grants another person limited job rights
- every claim must be validated against grant state
- revocation and expiry are first-class
- the job namespace is still the isolation primitive

Public/moot bounty work begins only after this boundary.

---

## Authority Model

The useful split from object-capability prior art is:

- **live authority**: an actor/reference can do only what the holder can reach
  right now
- **offline authority**: a signed grant/receipt lets another replica interpret a
  past event after the peers are disconnected

Kith sharing needs both. A live peer connection can hand a job actor a limited
capability reference, but the board still folds signed facts later. That means a
claim must carry enough evidence to answer: "was this subject allowed to make
this claim at the authored epoch?"

This keeps the social rule and the mechanical rule aligned. The user grants a
person limited rights; the mesh records a signed, replayable proof that the job
claim stayed inside those rights.

---

## Grant Shape

The minimum grant needs:

```rust
pub struct MeshGrant {
    pub grant_id: GrantId,
    pub issuer: PersonaKey,
    pub subject: PersonaKey,
    pub scope: NamespaceScope,
    pub allowed_job_kinds: Vec<MeshJobKind>,
    pub allowed_verification_tiers: Vec<VerificationTier>,
    pub resource_limits: ResourceLimits,
    pub lease_limits: LeaseLimits,
    pub not_before_ms: u64,
    pub expires_at_ms: u64,
    pub epoch: u64,
    pub delegation_root: Option<Hash>,
}
```

This may later map to Meadowcap-shaped path caps, UCAN-like tokens, zcap-shaped
delegations, OCapN-like live references, or p2panda-auth group facts. The
mesh-side requirement is simpler: the board must be able to decide whether a
claim was authorized at the time it was made.

---

## Claim Validation

Invalid claims should disappear from the folded board the same way foreign-mesh
ops do now.

A claim is valid only when:

- the claimant matches the grant subject or a permitted device key
- the job kind is allowed
- the requested namespace is within the grant scope
- the lease duration and resource class fit the grant
- the grant was active when the claim was authored
- no revocation at the same or later epoch blocks it
- any attached delegation proof resolves to the grant or role root named by the
  claim

This keeps enforcement local. Each device folds the same signed facts and reaches
the same board.

---

## Revocation

Revocation is a signed event, not an out-of-band UI action.

New claims after revocation are invalid. Active leases should receive a clean
cancel event. Whether the lease is allowed to finish within a grace window is a
grant policy setting.

Key rotation can come later. M1/M2 only need event-level revocation and expiry.

---

## Done Conditions

- One kith peer can claim a granted T0/T1 job.
- The same peer cannot claim an ungranted job kind.
- Revocation blocks new leases.
- Expiry blocks claims without deleting historical facts.
- The board ignores unauthorized claims deterministically.
- The host can show why a claim was rejected without inventing a second policy
  engine.

## Progress

- **2026-06-30** - Split out of the merged resource-coordination brief. Scope
  narrowed to kith grants and deterministic claim validation.
- **2026-06-30** - Added the authority split: live capability references for
  connected interaction, signed grants/proofs for offline board folding.
