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

## Prior art: burn-remote over iroh (noted 2026-07-03)

Burn's `burn-remote` gained iroh as its **primary transport** (tracel-ai/burn
PR #5111, on main 2026-07; unreleased — burn 0.21 is what mere pins in
`intel/embed` / `eidetic-search` / `aether`). Before any bespoke compute-worker
protocol is written for this mesh, evaluate it: the shape matches this plan
family point-for-point.

- **The compute half of the mesh, ready-made**: a peer advertises devices; a
  client holds `Device::remote_iroh(&node, peer, idx)` and tensor ops execute
  remotely. Their `p2p-remote-training` example is the personal-fleet picture.
- **Authorization is the tessera/kith seam**: `RemoteTicket` carries an
  `EndpointAddr` plus *opaque credential bytes*; the compute peer's
  `PeerAuthorizer` callback verifies them — signature format, expiry, and fleet
  membership are explicitly application concerns. A tessera receipt or kith
  capability grant plugs in without burn knowing either exists.
- **Composes onto murm's endpoint**: burn registers as an ALPN on an
  application-owned `iroh::protocol::Router` (`accept(BURN_REMOTE_ALPN, ..)`),
  so it mounts on the same endpoint/identity/relay policy Mere already runs —
  no second identity, no second connection pool. "Applications own the endpoint
  configuration" is their stated design.
- **Peer-to-peer tensor movement uses short-lived capabilities**: transfers go
  source-peer → destination-peer directly (not via the client), gated by a
  random short-lived capability bound to the destination's authenticated
  endpoint identity, download-count-limited.
- **Also reopens the deferred geist/serving lane**: their `remote-inference-web`
  example (wasm client, remote compute peer) is the right shape for the no-JIT
  browser lane — though browser-side transport is websocket, not iroh, so the
  web story is transport-asymmetric for now.

Caveats: freshly landed (expect API churn before it stabilizes in a release);
burn is a heavy tree to widen beyond the current ndarray-only embed pin; and
lease semantics (this plan's core) stay ours — burn gives execution + authz
hooks, not owner-reclaim or heartbeat policy.

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
- **2026-07-03** - Added the burn-remote-over-iroh prior-art section (Mark
  flagged tracel-ai/burn PR #5111): the compute-worker protocol + authz seam
  this mesh would otherwise hand-roll. Watch for the post-0.21 burn release.
