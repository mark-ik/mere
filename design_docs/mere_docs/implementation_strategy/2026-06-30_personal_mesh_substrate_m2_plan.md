# Personal Mesh Substrate M2 Plan

**Date**: 2026-06-30  
**Status**: Active next slice.  
**Related**:
[`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md),
[`../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md`](../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md),
[`2026-06-03_actor_constellation_plan.md`](2026-06-03_actor_constellation_plan.md),
[`../research/2026-06-24_local_models_harness_brief.md`](../research/2026-06-24_local_models_harness_brief.md)

M2 turns the M1 proof into a real substrate. The goal is not a market, not kith
sharing, and not sharded models. The goal is a job namespace and one useful
resource adapter.

---

## Current State

`crates/mesh` has the M1 core:

- signed `MeshEvent` operations over LogSync
- deterministic `JobBoard` fold
- `JobPosted`, `JobClaimed`, `JobDone`
- `Echo` and `Blake3` job kinds
- a `mesh-peer` two-machine rehearsal path

That proves transport and convergence. It does not prove the product contract.
M1 jobs still carry inline payloads and ambient assumptions.

---

## M2 Contract

Add a namespace-shaped job spec before adding more work kinds.

```rust
pub struct JobSpec {
    pub kind: MeshJobKind,
    pub namespace: JobNamespace,
    pub requirements: DeviceRequirements,
    pub determinism: DeterminismClass,
    pub checkpoint: CheckpointClass,
    pub verification: VerificationHint,
}

pub struct JobNamespace {
    pub inputs: Vec<BlobRef>,
    pub weights: Vec<BlobRef>,
    pub scratch: ScratchGrant,
    pub output: OutputSink,
    pub metrics: Option<MetricsSink>,
    pub host_calls: Vec<HostCallGrant>,
    pub privacy: PrivacyClass,
}
```

These names are design placeholders. The important part is the boundary: a job
names only what its namespace grants. The sandbox contains code; the namespace
limits what code can reach.

---

## MeshResource Seam

The adapter seam should sit above wire/store/sync and below the host actor.

```rust
pub trait MeshResource {
    fn kind(&self) -> MeshJobKind;
    fn can_run(&self, device: &DeviceCaps, spec: &JobSpec) -> bool;
    fn prepare(&self, spec: &JobSpec) -> Result<PreparedJob, MeshResourceError>;
    fn run(&self, job: PreparedJob) -> Result<JobOutput, MeshResourceError>;
}
```

Rules:

- Adding a new resource must not change LogSync or board convergence.
- The board folds claims and results; adapters execute only after the host has
  selected a valid action.
- Blob-backed inputs arrive here, not as a special case in `wire.rs`.
- Device capabilities are facts the host advertises, not claims the asker trusts
  blindly.

---

## First Adapter

Prefer an embeddings batch with the deterministic `hashed` provider before a
GPU/Burn adapter. It is product-useful, already near the `intel/embed` seam, and
can be tested without device-specific GPU behavior.

Candidate done-condition:

1. Laptop posts an embedding batch over blob-backed inputs.
2. Workstation claims it because it advertises the adapter.
3. Result lands as a content-addressed output.
4. A deterministic local rerun verifies the result.
5. Adding the adapter does not touch `JobBoard::fold`.

Burn-wgpu should wait until device policy and owner reclaim exist. Otherwise the
first real adapter will smuggle scheduler policy into the resource layer.

---

## Non-Goals

- Kith grants
- Bounty escrow
- Public verification markets
- Wasmtime arbitrary compute
- Petals-style model hosting
- Training

Those lanes depend on this contract; they do not belong inside it.

## Done Conditions

- `JobSpec` and `JobNamespace` are serializable, signed through the existing mesh
  operation path, and covered by round-trip tests.
- `MeshResource` exists as an adapter seam with one real adapter.
- M1 `Echo` / `Blake3` still run through a compatibility path or minimal adapter.
- The two-peer sync test covers a nontrivial namespace-backed job.
- The code has one place where resource kinds are registered.

## Progress

- **2026-06-30** - Split out of the merged resource-coordination brief. Scope
  narrowed to the substrate slice: namespace manifest plus first adapter.
