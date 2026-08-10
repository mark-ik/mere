# Mesh Host Lanes Plan

**Date**: 2026-08-09

**Status**: Open. Spun out of M2 and M3 on completion, so their honest gaps have
a tracked home rather than living in archived plans.

**Related**:
[`2026-06-30_personal_mesh_substrate_m2_plan.md`](../../archive_docs/2026-08-09_completed_plans/2026-06-30_personal_mesh_substrate_m2_plan.md),
[`2026-06-30_mesh_lease_scheduler_plan.md`](../../archive_docs/2026-08-09_completed_plans/2026-06-30_mesh_lease_scheduler_plan.md),
[`2026-06-30_kith_capability_sharing_plan.md`](2026-06-30_kith_capability_sharing_plan.md),
[`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md)

M2 gave the mesh a bounded execution substrate; M3 made lending a device
socially safe. Both were finished on their own terms, and both leaned on things
the mesh does not provide. This plan owns those things.

The unifying shape: **`mere-mesh` moves signed facts, and assumes a host moves
bytes.** Operations replicate over LogSync; blobs do not. Every item below is
either a consequence of that split or a piece of accounting the mesh refused to
invent before it had a consumer.

---

## 1. Blob delivery (the largest gap)

A `JobSpec` names inputs by content address. A worker that has never seen those
bytes cannot run the job, and today the honest failure is
`NamespaceError::MissingBlob`. The M2 two-peer receipt stages the input on both
devices deliberately and says so.

What a lane has to settle:

- **Who asks.** A worker that has won a claim, before or after granting itself a
  lease? Fetching before the grant wastes bandwidth on races; fetching after
  burns lease time on transfer.
- **Who serves.** The job author is the obvious holder, but a third device that
  already fetched the blob is a better source and the mesh knows who they are.
- **What it costs the lease.** Transfer time inside a lease window is time not
  spent computing. Either the envelope accounts for it or the heartbeat has to
  distinguish "fetching" from "working" — the second is probably right, since
  `LeaseProgress` already has room to say so.
- **What a failure means.** `ReleaseReason::InputUnavailable` exists precisely so
  a scheduler does not read a missing blob as worker unreliability. Whatever the
  lane does, that distinction must survive it.

Candidates already in the family: the native-drop exporter
(`mesh::drop_export`), and a request path over the same p2panda endpoint the
sync lane uses. Do not invent a third transport.

## 2. Blob retention

Checkpoint erasure removes M1's *inline* inputs from operation bodies. A V2
input or output is a blob with its own lifetime, and
`RetentionEffect::BlobCollected` sits in the vocabulary with nothing behind it.

Owner reclaim and device policy are the natural place to decide when a device
stops holding a job's bytes — the same settings that say what a device will lend
should say what it will keep.

## 3. Retention versus live leases

A retention checkpoint that prunes the claim operations a later lease epoch
depends on will stop that epoch validating: the fold can no longer prove the
grant's author was the eligible winner. Recorded in `fold.rs`.

The likely fix is that `build_checkpoint` must not advance its frontier past a
job whose lease is still live — but "still live" needs an observation time, and
checkpoint construction is currently clock-free. Settle that before shipping
retention on a mesh that lends.

## 4. Portable checkpoints

`CheckpointClass::Resumable` and `resource::Checkpoint` are real: an adapter can
be asked to stop at a boundary and can say which one it reached, and
`LeaseProgress::checkpoint_held` carries that to other devices. What does *not*
exist is resuming somewhere else — the checkpoint is bytes on the interrupted
device, so the next epoch's worker starts from nothing.

This is a consequence of §1, not an independent problem. It closes when a blob
lane can carry a checkpoint the way it carries an input.

## 5. Tolerant verification

`registry::verify_output` re-runs and compares bytes for
`VerificationClass::ExactBytes`, and returns `Verdict::NotCheckable` for
anything weaker, because an element-wise tolerance needs a decoder only the
resource has. The first resource that cannot claim exact bytes — a GPU or remote
adapter, most likely — brings that decoder with it.

**Do not add the comparator before its consumer.** A tolerance nobody has
measured is a number someone will trust.

## 6. Reliability accounting

M3 settled the *causes* and deliberately implemented none of the consequences:

- owner revoke and policy shutdown carry no worker penalty;
- clean release carries no penalty;
- lapse may become reliability evidence later; and
- invalid completion is an admission failure, not a completed job.

The trap is `LapseReason::HeartbeatSilence`, which is an observation about the
*observer's* contact, not a fact about the holder. Any standing derived from
lapses has to survive one device being behind on sync, or it will punish the
quiet member of a healthy ring. Reliability probably belongs with kith, where
standing first has a purpose.

---

## 7. Non-goals

This plan does not own economic standing, bounty escrow, public verification
markets, storage service classes, Burn migration, or remote tensor transport.
Those have their own plans or their own gates.

## 8. Done conditions

- A worker can run a job whose inputs it has never held, and the receipt proves
  it across two devices with disjoint blob spaces.
- A missing input is still distinguishable from an unreliable worker after the
  lane exists.
- A device's retention settings decide when it stops holding a job's bytes.
- Checkpointing a mesh with a live lease is either safe or refused, explicitly.
- Whatever verification class a non-exact resource declares, a re-run can judge
  it.

## 9. Progress

- **2026-08-09**: founded on the day M2 and M3 both landed, to hold the five
  gaps they named honestly rather than let them decay inside archived plans.
