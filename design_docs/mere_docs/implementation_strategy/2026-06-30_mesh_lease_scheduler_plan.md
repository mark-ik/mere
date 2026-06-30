# Mesh Lease Scheduler Plan

**Date**: 2026-06-30  
**Status**: Planned after M2 substrate.  
**Related**:
[`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md),
[`2026-06-30_personal_mesh_substrate_m2_plan.md`](2026-06-30_personal_mesh_substrate_m2_plan.md),
[`../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md`](../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md)

The scheduler is the owner-priority layer. It decides when this device may offer
work, when a lease is still alive, and how a job recovers when a worker drops or
the owner takes the machine back.

---

## Core Rule

Owned hardware keeps foreground priority. A local owner reclaim is a clean
handoff event. It must not look like worker dishonesty or a reputation lapse.

This rule is load-bearing because it is what makes lending personal machines
socially safe. If the user cannot take their GPU back instantly, the mesh becomes
a cluster scheduler wearing local-first clothes.

---

## Events

M1 has `JobPosted`, `JobClaimed`, and `JobDone`. M3 adds lease semantics:

```rust
pub enum MeshEvent {
    JobPosted { spec: JobSpec, nonce: u64, at_ms: u64 },
    LeaseClaimed { job: JobId, lease: LeaseSpec, at_ms: u64 },
    LeaseHeartbeat { job: JobId, lease: LeaseId, at_ms: u64 },
    LeaseReleased { job: JobId, lease: LeaseId, at_ms: u64 },
    LeaseRevokedByOwner { job: JobId, lease: LeaseId, reason: ReclaimReason, at_ms: u64 },
    JobDone { job: JobId, lease: LeaseId, output: JobOutput, at_ms: u64 },
}
```

`LeaseLapsed` can be derived by the fold from missed heartbeats, or authored by a
coordinator projection. The board should not need an online broker to notice a
dead lease.

---

## Fold Semantics

The board should distinguish:

- `Posted`: no valid lease
- `Leased`: a valid lease exists and heartbeats are current
- `Done`: the lease holder returned an accepted result
- `Revoked`: the owner cleanly reclaimed the resource
- `Lapsed`: the worker missed the heartbeat window

Reassign is allowed after `Revoked` or `Lapsed`, but policy differs:

- `Revoked` carries no penalty.
- `Lapsed` may feed reliability standing later.
- `Done` closes the job unless the verification layer rejects it.

---

## Device Policy

Device policy must be settings, not hardcoded defaults:

- idle threshold
- foreground activity detector
- battery floor
- thermal ceiling
- network class and bandwidth cap
- quiet hours
- maximum concurrent jobs
- allowed resource kinds
- per-ring limits once kith sharing exists
- checkpoint class allowed on this device

The host owns these settings. The mesh crate should receive a policy snapshot or
capability advertisement, not query the OS directly.

---

## Checkpoint Classes

Every job declares one:

- `Interruptible`: kill and re-dispatch from the start.
- `Checkpointable`: resume from a saved boundary.
- `NonInterruptible`: finish or fail; owner reclaim may defer until the policy's
  maximum grace window.

M2 can record the class. M3 makes it operational.

---

## Storage And Uptime Classes

Storage sharing should reuse the same lease vocabulary, but it has a different
done-condition from compute. Compute asks whether a result was produced. Storage
asks whether encrypted content stayed retrievable across checkpoints.

Uptime is therefore a service class, not a universal mesh rule:

- `BestEffort`: peer stores while online; no standing loss for absence.
- `Checkpointed`: peer answers periodic possession/retrieval checks.
- `UptimeWindow`: peer commits to policy windows such as nights, weekends, or
  "while plugged in".
- `Replicated`: repair starts when available copies drop below policy.

The storage layer should emit checkpoint facts that the board can fold:

```rust
pub enum StorageCheckpoint {
    Stored { blob: BlobRef, lease: LeaseId, at_ms: u64 },
    ChallengeIssued { blob: BlobRef, nonce: Vec<u8>, at_ms: u64 },
    ChallengeAnswered { blob: BlobRef, proof: Hash, at_ms: u64 },
    RepairRequested { blob: BlobRef, reason: RepairReason, at_ms: u64 },
}
```

The same owner-priority rule still applies. If a laptop reclaims its disk or
goes offline outside its promised class, that is not worker dishonesty. If it
misses a promised checkpoint, that becomes reliability evidence for the storage
lane.

---

## Done Conditions

- A worker that stops heartbeating is reassigned.
- A worker whose owner reclaims resources is cleanly canceled and reassigned.
- The board exposes the difference between owner revoke and worker lapse.
- Lease/heartbeat defaults are configurable.
- A test covers two workers racing, one winning, then lapsing, then the other
  completing the job.

## Progress

- **2026-06-30** - Split out of the merged resource-coordination brief. Scope
  narrowed to lease lifecycle, owner reclaim, and device policy.
- **2026-06-30** - Added storage checkpoint and optional uptime classes so
  storage reliability does not leak into the default compute lease rule.
