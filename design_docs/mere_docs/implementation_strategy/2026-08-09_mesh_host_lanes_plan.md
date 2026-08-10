# Mesh Host Lanes Plan

**Date**: 2026-08-09

**Status**: Open. Re-cut 2026-08-09 after review, around a supervisor gate the
first draft was missing entirely.

**Related**:
[`2026-06-30_personal_mesh_substrate_m2_plan.md`](../../archive_docs/2026-08-09_completed_plans/2026-06-30_personal_mesh_substrate_m2_plan.md),
[`2026-06-30_mesh_lease_scheduler_plan.md`](../../archive_docs/2026-08-09_completed_plans/2026-06-30_mesh_lease_scheduler_plan.md),
[`2026-08-09_burn_0_22_migration_plan.md`](2026-08-09_burn_0_22_migration_plan.md),
[`2026-06-30_kith_capability_sharing_plan.md`](2026-06-30_kith_capability_sharing_plan.md),
[`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md)

M2 and M3 are credible **library floors**: a bounded execution substrate and a
lease protocol, both tested, neither driven by a real host. This plan builds the
host, and then the two connections the floors quietly assume and nobody has
made yet:

- **mesh author identity → transport peer.** Mesh facts identify operation
  authors. `transport` addresses a `PeerID`. Nothing maps one to the other.
- **lease state → a running session.** M3 can revoke a lease. A Burn Remote
  session authorized at admission does not stop because a fact was signed.

The first draft of this plan listed five gaps and no supervisor, which put the
cart before the horse: every one of those gaps is reached *through* a host that
supervises running work.

---

## 1. Gates

| Gate | Required result |
|---|---|
| **H0: Host supervisor** | Non-blocking in-flight jobs, real heartbeat state, cancellation/reclaim, correct leased completion |
| **H1: Blob location and delivery** | Mesh author mapped to transport endpoint; existing iroh-blobs path used; disjoint-store two-host receipt |
| **H2: Retention safety** | Refuse checkpoint/prune when an observed live lease depends on the prefix; retain inputs and outputs through completion/convergence |
| **Burn migration** | Stable 0.22 dependency migration and existing backend baselines |
| **Remote adapter** | Lease-bound authorization plus targeted session revocation |

Serial. H0 first, because H1 and H2 are both things a supervisor does.

---

## 2. H0: the host supervisor

**The gap.** Nothing in the tree supervises work. `mesh-peer` awaits execution
inside its decision loop, keeps no in-flight map, ignores its cancellation
handle, cannot emit meaningful live progress, and would author an ordinary V2
completion where a leased job needs `JobCompletedUnderLease`. Its owner-reclaim
arm authored a revoke *without stopping the work*. As of this re-cut it declares
`DevicePolicy::unsupervised` and the worker refuses to give it leased jobs at
all — the honest state, not a fix.

**Where it lives.** Above `mere-mesh`, as a reusable host service. Not in
`mere-mesh` (which must stay OS-free and clock-free), not in an example, and not
owned by Turnstone — Turnstone is a good *first consumer*, but a second consumer
would then have to reach into an app. It owns:

- the in-flight job map and their `JobControlHandle`s;
- the `DeviceConditions` provider (the only thing here that touches the OS);
- the `ResourceRegistry` and blob access;
- the shared `P2pandaTransport`; and
- the authoring loop, so `next_action` runs on a tick that never blocks.

**What it must get right**, each of which is a live bug in the current example:

1. Execution runs off the decision loop, so a tick can heartbeat, reclaim, or
   claim while a job is running.
2. `LeaseProgress` comes from the running job's control handle, not a constant.
3. Owner reclaim cancels the run *and then* authors `LeaseRevokedByOwner` —
   in that order, and the receipt should assert the order.
4. A leased job completes with `JobCompletedUnderLease` naming its lease.
   `JobDoneV2` on a leased job is refused by the fold, so getting this wrong is
   silent no-progress rather than corruption — which is worse to debug.
5. `CheckpointClass::NonInterruptible` honours `reclaim_grace_ms` before a hard
   cancel.

**Receipt.** Two supervised hosts over real transport: one takes a lease and
starts a long job, its owner reclaims mid-run, the run actually stops, and the
other host finishes under a new epoch. That is the M3 receipt again, but driven
by the supervisor rather than by a test authoring events by hand.

---

## 3. H1: blob location and delivery

Operations replicate over LogSync; blobs do not. A worker that has never seen a
job's inputs cannot run it, and today the honest failure is
`NamespaceError::MissingBlob`.

**Use what exists.** `mere-transport`'s `BlobStore` wraps `iroh-blobs` and
already has `fetch_from(hash, peer: PeerID)`; `eidetic-iroh-fetcher` already
proves the two-node shape end to end. Do not add a second transport, a second
router, or a second blob store.

**The unresolved problem is discovery, not transfer.** Mesh facts identify
operation authors — and a mesh operation is signed by a *derived* key
(`derive_keypair(b"mesh-author")`), while `PeerID::from_public_key` takes the
master persona key. The two are different keys with no recoverable mapping, so
"fetch this blob from whoever posted the job" is currently unanswerable. Fix it
with one of:

- an **authenticated author-to-transport locator**: a signed record binding a
  mesh author key to a transport endpoint, published on the mesh itself; or
- a **trusted device-roster binding**: the own-devices ring already has a
  persona roster (see the family-shared identity work), so the mapping may
  simply be roster data rather than a new fact type.

Prefer the roster if it already carries both keys. Whichever wins, it must be
authenticated — an unauthenticated locator is an invitation to be told to fetch
from the wrong device.

**Grant first, then fetch under the lease.** Fetching before the grant wastes
bandwidth on races the device may lose. Fetching after means transfer time is
lease time, which the ring must be able to see — otherwise a device that is
downloading looks exactly like a device that has stalled.

That needs an explicit activity phase, because `LeaseProgress { done, total,
checkpoint_held }` cannot express "fetching" at all:

```rust
enum LeaseActivity { Fetching, Preparing, Running, Checkpointing }
```

Carried on the heartbeat beside the counters. Note this widens
`LeaseProgress`, which rides inside signed `LeaseHeartbeat` bodies — add the
field with `#[serde(default)]` and keep the pre-M3 encoding intact, the same
discipline M2 and M3 both used.

**A missing input is not unreliability.** `ReleaseReason::InputUnavailable`
exists precisely so a scheduler does not read a fetch failure as a bad worker.
Whatever the lane does, that distinction must survive it.

**Receipt.** Two hosts with **disjoint** blob stores: the poster holds the
input, the worker does not, and the job completes anyway.

---

## 4. H2: retention safety

Two distinct holes, one fail-closed rule to start.

**Live leases versus prefix pruning.** A retention checkpoint that prunes the
claim operations a later lease epoch depends on stops that epoch validating:
the fold can no longer prove the grant's author was the eligible winner.
Recorded in `fold.rs`.

**Initial rule: refuse, fail closed.** `build_checkpoint` refuses to advance
past a job whose lease is live at the supplied observation time. Checkpointing
already takes an `at_ms`, so this needs no new clock — only the projection the
board already exposes. Optimising the frontier so a busy mesh can still prune
comes later, and only with a receipt.

**Blob lifetime.** Checkpoint erasure removes M1's *inline* inputs from
operation bodies. A V2 input or output is a blob with its own lifetime, and
`RetentionEffect::BlobCollected` sits in the vocabulary with nothing behind it.
The rule to start from is retention *through* completion and convergence: a
device may drop a job's inputs once the result has committed and converged, and
not before. The same settings that say what a device will lend should say what
it will keep.

---

## 5. Burn migration

Release-gated, not toolchain-gated: the published line is still
`v0.22.0-pre.1`. Owned by the
[Burn 0.22 migration plan](2026-08-09_burn_0_22_migration_plan.md).

What can proceed now without waiting: baseline capture against the existing
backends, and B1 feature-fanout cleanup. A detached prerelease integration probe
is also fine — `crates/probes` is excluded from the workspace, so it cannot
widen production dependencies.

---

## 6. Remote adapter

The last gate, and the one with a real upstream question in it.

**Burn Remote authorizes a session at admission.** M3 owner reclaim requires
terminating a session that was *already* authorized — the whole point of reclaim
is that permission granted a minute ago stops applying the moment the human
wants their GPU back. Admission-time authorization cannot express that.

So the adapter needs two things:

1. a **lease-bound credential** — the session's authority is the lease, so it
   expires when the lease does; and
2. a **targeted close hook** — a way to end one server session on revoke.

If upstream exposes no targeted close, patch or contribute one. A second
endpoint or router is the wrong fix: Burn's protocol should mount on Mere's
existing transport authority, not stand up a parallel one with its own
lifetime.

---

## 7. Deferred until a consumer asks

These are real, and none of them blocks Burn:

- **Tolerant verification comparators.** `registry::verify_output` compares
  bytes for `VerificationClass::ExactBytes` and returns `NotCheckable`
  otherwise, because an element-wise tolerance needs a decoder only the
  resource has. Move it into the first non-exact remote resource slice. A
  tolerance nobody has measured is a number someone will trust.
- **Portable checkpoints.** `Resumable` and `resource::Checkpoint` are real: an
  adapter stops at a boundary and says which one. Resuming *elsewhere* needs the
  blob lane to carry the checkpoint. Require it only before claiming resumable
  remote execution.
- **Reliability and reputation accounting.** M3 settled the causes and
  implemented none of the consequences. The trap is
  `LapseReason::HeartbeatSilence`, which is an observation about the
  *observer's* contact rather than a fact about the holder — standing derived
  from lapses would punish the quiet member of a healthy ring. Belongs to the
  kith lane, where standing first has a purpose.

## 8. Non-goals

Economic standing, bounty escrow, public verification markets, storage service
classes, and remote tensor transport beyond the adapter gate above.

## 9. Done conditions

- A supervised host runs jobs off its decision loop, heartbeats real progress,
  stops a run on owner reclaim *before* authoring the revoke, and completes
  leased jobs under their lease.
- A worker runs a job whose inputs it has never held, proven across two hosts
  with disjoint blob stores.
- A mesh author key resolves to a transport endpoint through an authenticated
  binding.
- A missing input stays distinguishable from an unreliable worker.
- Checkpointing refuses to strand a live lease, and a device's retention
  settings decide when it drops a job's bytes.
- A revoked lease closes its remote session.

## 10. Progress

- **2026-08-09**: founded on the day M2 and M3 landed, to hold the gaps they
  named honestly.
- **2026-08-09 (re-cut)**: reordered around gates after review. The first draft
  had no host supervisor in it at all and treated blob delivery as the largest
  gap; the supervisor is prior to it. Also recorded the two missing connections
  explicitly (mesh author → transport peer, lease state → running session),
  named the existing `mere-transport` / `eidetic-iroh-fetcher` path rather than
  inviting a new one, added the `LeaseActivity` phase that `LeaseProgress`
  cannot currently express, made live-lease retention refusal fail-closed, and
  moved tolerant comparators, portable checkpoints, and reliability accounting
  behind their first real consumers.
