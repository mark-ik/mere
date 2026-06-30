# Resource Coordination Map

**Date**: 2026-06-04  
**Status**: Split map as of 2026-06-30. The original merged mechanics brief had
grown into five plans: personal mesh substrate, lease scheduling, capability
sharing, bounty economy, and communal compute. This file is now the routing
document. The old 2026-06-03 source briefs remain archived in
[`../../archive_docs/2026-06-04_resource_coordination_merge/`](../../archive_docs/2026-06-04_resource_coordination_merge/),
and M1 completion remains archived in
[`../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md`](../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md).

**Correction folded in 2026-06-30**: old prose that treated Rhai or Rune as the
active outer-ring sandbox is stale. Read the security split this way:

- Rhai remains Mere's local command/scripting language: the omnibar shell,
  block-evaluator lane, and scriptable graph/field rules can still use it.
  The correction only removes Rhai as the security boundary for arbitrary
  outer-ring compute.
- Browser nodes run trusted host orchestration plus constrained declarative policy
  and verdict logic.
- ML jobs run the worker's trusted kernels over namespace-bounded untrusted data.
- Native helpers may offer arbitrary untrusted compute only as fuel-metered
  Wasmtime jobs with a deterministic profile.
- Raw native binaries are outside the mesh contract.

**Capability correction folded in 2026-06-30**: live object-capability references
and offline signed receipts are different tools.

- Live references are good for connected interaction: actor handles, limited
  command/mod authority, and job capabilities.
- Offline receipts are required for p2p folds: grants, revocations, moot epochs,
  role roots, inclusion proofs, results, and verifier signatures.
- The mesh should not pretend a live authorization check explains all future
  replicas. Every cross-device/cross-moot fact that matters later needs signed
  evidence.
- Spritely/OCapN-shaped work is useful prior art for this split, but this map
  does not adopt it as a stack choice.

**Commitment correction folded in 2026-06-30**: do not bake Merkle trees into
the social model. The shared interface is
[`../implementation_strategy/2026-06-30_commitment_proof_interface_plan.md`](../implementation_strategy/2026-06-30_commitment_proof_interface_plan.md).
It treats Merkle, sparse Merkle, MMR, accumulators, vector commitments, and
private set-cardinality proofs as backends behind typed commitments and typed
proofs. P2panda still owns operation identity, signatures, per-author log order,
topics, and LogSync. Commitment roots live inside event bodies and projections.

---

## 1. Personal Mesh Substrate

**Authority**:
[`../implementation_strategy/2026-06-30_personal_mesh_substrate_m2_plan.md`](../implementation_strategy/2026-06-30_personal_mesh_substrate_m2_plan.md)

**Owns**: `crates/mesh`, the signed job operation grammar, the deterministic
job board, `JobSpec`, `JobNamespace`, and the `MeshResource` adapter seam.

**Current state**: M1 is done. One owned device can post a job, another can claim
it, execute `Echo` / `Blake3`, and return the result over LogSync. The crate does
not yet model real resources, blob-backed inputs, grants, leases, heartbeats,
preemption, or verification tiers.

**Next done-condition**: one useful nontrivial job runs through a namespace
manifest and a resource adapter without changing the board/sync core.

---

## 2. Lease And Scheduler Semantics

**Authority**:
[`../implementation_strategy/2026-06-30_mesh_lease_scheduler_plan.md`](../implementation_strategy/2026-06-30_mesh_lease_scheduler_plan.md)

**Owns**: owner priority, idle routing, revocable leases, heartbeat/reassign,
checkpoint classes, device policy, and clean owner reclamation.

**Invariant**: owned hardware always has local foreground priority. Owner reclaim
is a clean cancellation event, not a worker failure or reputation lapse.

**Next done-condition**: a dropped or reclaimed worker is reassigned, and the
board distinguishes `LeaseRevokedByOwner` from `LeaseLapsed`.

---

## 3. Capability Sharing

**Authority**:
[`../implementation_strategy/2026-06-30_kith_capability_sharing_plan.md`](../implementation_strategy/2026-06-30_kith_capability_sharing_plan.md)

**Related**:
[`../implementation_strategy/2026-06-26_federation_interop_plan.md`](../implementation_strategy/2026-06-26_federation_interop_plan.md),
[`../implementation_strategy/2026-05-10_graph_cluster_namespaces_brief.md`](../implementation_strategy/2026-05-10_graph_cluster_namespaces_brief.md),
[`2026-05-14_capability_gate_catalogue_brief.md`](2026-05-14_capability_gate_catalogue_brief.md)

**Owns**: kith/kin grants, revocation, expiry, namespace caps, and claim
validation. This is still permissioned sharing, not an economy.

**Next done-condition**: a friend can claim only a T0/T1 job covered by a valid
grant, the board ignores unauthorized claims, and revocation prevents new leases.

---

## 4. Bounty And Verification Economy

**Authority**:
[`../../moothold_docs/implementation_strategy/2026-06-30_bounty_verification_economy_plan.md`](../../moothold_docs/implementation_strategy/2026-06-30_bounty_verification_economy_plan.md)

**Related**:
[`../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md),
[`../implementation_strategy/2026-06-06_moot_constitution_brief.md`](../implementation_strategy/2026-06-06_moot_constitution_brief.md),
[`../../archive_docs/2026-06-09_completed_plans/2026-06-02_tessera_plan.md`](../../archive_docs/2026-06-09_completed_plans/2026-06-02_tessera_plan.md)

**Owns**: result specs, bounty posts/claims/submissions, escrow, T0-T3
verification, verdict commit-reveal, tessera/credit movement, and lapse
accounting.

**Boundary**: the economy engages only once work crosses the trust boundary.
Personal devices and granted kith run through permission and scheduler policy.

---

## 5. Communal Compute And Model Hosting

**Authority**:
[`2026-06-10_communal_compute_tiers_brief.md`](2026-06-10_communal_compute_tiers_brief.md)

**Related**:
[`2026-06-24_local_models_harness_brief.md`](2026-06-24_local_models_harness_brief.md),
[`2026-05-10_geist_models_brief.md`](2026-05-10_geist_models_brief.md)

**Owns**: volunteer-computing lessons, time-bank policy presets, moot/moothold
compute commons, BOINC-shaped data queues, LCZero-style data/eval contribution
loops, and late Petals-shaped async model hosting.

**Boundary**: communal hosting is downstream of the mesh, lease scheduler, kith
grant path, and bounty/verifier grammar. Untrusted communal training remains a
research frontier, not a near-term milestone.

---

## Shared Invariants

- The mesh is Plan-9-shaped: a job sees only the namespace assembled from its
  grants, inputs, outputs, scratch, metrics, weights, and allowed host calls.
- Grant, lease, revocation, heartbeat, and result facts are signed events on the
  p2p substrate; every device enforces them locally.
- Live capability references govern connected actors and runtime mod/job handles;
  signed receipts govern replay, audit, tessera, and disconnected replicas.
- P2panda operation hashes and app-level commitment digests are distinct. Do not
  use a Merkle/MMR root as a p2panda log head or operation id.
- Syncthing is the product feel for owned devices, not the architecture.
- BOINC is the batch-validation reference, not the personal-device UX.
- Kubernetes, Ray, Golem, Akash, and Bacalhau are outer references. Their
  scheduler or market shapes should not leak into the personal ring.
- Communal model hosting is async and late. Interactive WAN sharding and
  untrusted training stay off the main path.

---

## Milestone Map

1. **M1 personal job round-trip** - done 2026-06-12.
2. **M2 resource adapter + namespace manifest** - active next slice.
3. **M3 lease, heartbeat, reclaim, reassign** - scheduler slice.
4. **M4 kith capability sharing** - first trust-widening slice.
5. **M5 bounty grammar and deterministic verification** - economy data model.
6. **M6 storage audit and repair** - storage bounty lane.
7. **M7 communal queue** - BOINC-shaped data/eval loop.
8. **M8 async model hosting** - Petals-shaped, not backbone.

## Progress

- **2026-06-30** - Split the merged resource-coordination plan into five
  authorities and retired stale Rhai/Rune outer-ring sandbox wording while
  preserving Rhai as Mere's local command/scripting lane. This file now routes
  work rather than owning every mechanic.
- **2026-06-30** - Added the live-capability/offline-receipt distinction and
  routed tessera-style epoch evidence to the moothold economy lane.
- **2026-06-30** - Added the typed commitment/proof interface and clarified that
  p2panda operation identity remains below application proof roots.
