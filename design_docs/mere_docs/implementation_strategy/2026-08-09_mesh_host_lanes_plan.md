# Mesh Host Lanes Plan

**Date**: 2026-08-09

**Status**: Implemented through the Burn 0.22.0-pre.2 production row. **H0, H1,
H2, and the lease-bound remote adapter have landed.** Stable Burn 0.22
repinning and its clean package receipt remain release-gated.

The remote gate landed on 2026-08-23 in mere `7fb07225`, after the pre.2 source
re-audit corrected the close seam and the client/server identity rules. The
adapter mounts Burn on Distillery's existing p2panda/Iroh endpoint, admits only
the job poster under the exact live host-supervised lease, and closes reserved
or active matching sessions before owner reclaim is authored. P2panda's exact
external-ALPN seam landed first in its `main` as `9f2c2a01`.

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
| ~~**H0: Host supervisor**~~ | **Done 2026-08-09** — `mere-mesh-host`. Non-blocking in-flight jobs, real heartbeat state, cancellation before revoke, correct leased completion |
| ~~**H1: Blob location and delivery**~~ | **Done 2026-08-10** — `DeviceAttested` + `TransportCourier`. Mesh author resolves to a transport endpoint through an existing master-signed attestation; disjoint-store two-host receipt passes |
| ~~**H2: Retention safety**~~ | **Done 2026-08-12** — fail-closed checkpoint rules plus Distillery's owner-controlled sweep; shared hashes and cross-subsystem custody remain protected |
| **Burn stable closure** | **Prerelease row executed 2026-08-20**; repeat dependency, backend, and package closure on stable 0.22 when published |
| ~~**Remote adapter**~~ | **Done 2026-08-23** — signed job-poster claim, exact live lease/resource/device admission, shared endpoint, targeted stop-before-reclaim receipt |

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

**What it had to get right**, each of which was a live bug in the example:

1. Execution runs off the decision loop, so a tick can heartbeat, reclaim, or
   claim while a job is running.
2. `LeaseProgress` comes from the running job's control handle, not a constant.
3. Owner reclaim cancels the run *and then* authors `LeaseRevokedByOwner`.
4. A leased job completes with `JobCompletedUnderLease` naming its lease.
   `JobDoneV2` on a leased job is refused by the fold, so getting this wrong is
   silent no-progress rather than corruption — which is worse to debug.
5. `CheckpointClass::NonInterruptible` honours `reclaim_grace_ms` before a hard
   cancel.

### Landed 2026-08-09 — `crates/mesh/host` (`mere-mesh-host`)

`MeshHost::tick` reaps finished runs, abandons any lease it no longer holds,
escalates an overdue reclaim, then takes at most one new action. It never
awaits execution. The OS enters through exactly two seams, `Clock` and
`ConditionSource`, so a receipt can drive it with a clock it controls.

Every tick returns the [`Step`]s it took, rather than logging them, so a caller
can see what the supervisor actually did instead of inferring it. `AwaitingStop`
and `Reclaimed { stopped_at }` are separate steps precisely because the ordering
in (3) is the property worth proving: no revoke exists on the wire while the run
is still stopping, and the revoke that follows reports how far the work had got
when it let go.

**A sixth requirement, found while building it.** A device can grant itself a
lease on a board that has not caught up, win *locally*, and start work — then
lose to a peer's earlier grant when the facts arrive. The protocol handles that
correctly (the fold picks one winner), but the loser must notice: a completion
under a lease that did not survive is dropped by every peer, so the run is
wasted and reporting it would be a lie. `still_held` is the check, run every
tick against every in-flight lease; a lost lease cancels its run and authors
nothing, because a lease this device does not hold is not its to release. The
same check covers expiry and a revoke authored elsewhere.

**Receipts.** `tests/supervised_reclaim.rs` runs two supervised hosts over real
p2panda-net: one claims, grants itself a lease, runs, heartbeats real progress,
has its device reclaimed mid-run, stops, and the other finishes under epoch 1 —
with nobody authoring a lease event by hand. `still_held` is unit-tested
directly against a folded board where two grants race.

**Not done here.** Blob delivery (H1): a worker still needs the inputs in its
own space, and the supervisor reports a miss as
`ReleaseReason::InputUnavailable` rather than as an unreliable worker.

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

### Landed 2026-08-10

**The binding already existed.** Neither proposal above was needed as new
machinery. `personae` already mints a `DerivedKeyAttestation`: a statement,
signed *by the master*, that a given derived key belongs to it. What was
missing was only publication. `MeshEvent::DeviceAttested` carries one on the
mesh, and the board folds a `DeviceDirectory` out of them — self-authenticating,
no new record format, nothing to get wrong.

Two rules make it safe, checked at the store before mutation and again in the
fold: the master signature must verify over `MESH_AUTHOR_SALT`, and **the
attested key must be the operation's own author**. Without the second rule
anyone could publish a perfectly valid attestation *about somebody else* and
aim the ring's blob fetches at a device of their choosing.

**One store, not two.** A host's blob space is `mere-transport`'s `BlobStore`
directly (`TransportBlobSpace` implements the mesh's `BlobSource`/`BlobSink`
over it). The first sketch had a muniment space beside the iroh-blobs one, which
would have meant copying every blob twice and deciding forever which copy was
real. Because it is the store the router already serves, staging a blob makes it
fetchable, the namespace reads out of it, and pulling one in is `fetch_from`
against the same store. A unit test pins the thing this rests on: iroh-blobs and
`BlobRef` agree on what a blob is called.

**Who to ask.** The poster first — it named the bytes, so it had them — then
every other attested device, because a peer that already fetched it is just as
good a source. Never this device.

`LeaseActivity { Fetching, Preparing, Running, Checkpointing }` landed on
`LeaseProgress` with `#[serde(default)]` and skip-when-`Running`, so a snapshot
written before H1 still hashes to its committed bytes.

**Not done here.** A courier failure is deliberately not fatal: the run proceeds
and fails on the missing input, which is reported as
`ReleaseReason::InputUnavailable`. Retry policy, parallel fetch, and partial
transfer resume are not implemented — the lane works, it is not yet tuned.

---

---

## 4. H2: retention safety

Two distinct holes, one fail-closed rule to start.

**Live leases versus prefix pruning.** A retention checkpoint that prunes the
claim operations a later lease epoch depends on stops that epoch validating:
the fold can no longer prove the grant's author was the eligible winner.

**Blob lifetime.** Checkpoint erasure removes M1's *inline* inputs from
operation bodies. A V2 input or output is a blob with its own lifetime, and
`RetentionEffect::BlobCollected` sat in the vocabulary with nothing behind it.

### Landed 2026-08-10

**Refuse, on both paths.** `JobBoardSnapshot::live_leases(at_ms, policy)` names
the jobs whose lease is live, and a checkpoint carrying any of them is refused
with `CheckpointError::LiveLease`. The check runs in `build_checkpoint`, so a
host does not author something its peers will reject, *and* in
`validate_checkpoint`, so one device cannot push a stranding frontier onto
everybody else.

It needed no new clock and no board: a checkpoint already carries its own
`at_ms` and its own snapshot, so a peer validating somebody else's checkpoint
can see the problem with nothing but the thing in front of it. The observation
policy lives on `MeshRetentionPolicy` as `lease`, because "how long must a
lease's claim history be kept" is a retention question.

A mesh with a live lease therefore cannot checkpoint until it ends. That is
bounded — leases expire by their own signed window, at most 24h — and it is the
deliberately unclever answer. Advancing the frontier selectively around a live
lease is still the optimisation, still deferred, still wants its own receipt.

**Blobs are kept through convergence, not merely completion.**
`collectable_blobs` returns a job's inputs and committed output only once the
job is terminal *in an accepted checkpoint*. The distinction is the whole point:
a local board saying `Committed` is this device's own opinion, formed the moment
it folded its own result, and dropping bytes on that basis would let a worker
throw away a job's inputs before the poster had ever seen the answer. A
checkpoint is the mesh's opinion. A device with no checkpoint collects nothing.

`MeshStore::collectable_blobs` began as the query. The 2026-08-12 Distillery
slice completed the host side: `Distillery::maintain` authors the checkpoint,
queries the safe set, releases this mesh's stable transport-store tags when the
owner enables collection, and reports `RetentionEffect::BlobCollected`.
Collection defaults off.

The real sweep exposed two rules the original H2 query had missed. First, a
content-addressed hash may be shared: a post-checkpoint or unfinished job that
reuses an older terminal job's input protects those bytes. The query therefore
checks the current replay tail as well as the accepted checkpoint and emits a
deduplicated set. Second, physical storage is shared across domains. Mesh tags
are scoped by mesh id and hash; removing one leaves an Eidetic, other-mesh, or
other-subsystem tag intact. A collecting `iroh-blobs` store removes the bytes
only after the last custody tag is gone.

---

## 5. Burn migration

The chosen `v0.22.0-pre.2` row migrated ESP and Quint on 2026-08-20, retired the
old `cubecl-wgpu` compatibility patch, and re-proved the native, wasm, WGPU,
real-device, and ESP package boundaries. Stable 0.22 is still unavailable, so
the final repin and clean package receipt remain release-gated. Owned by the
[Burn 0.22 migration plan](2026-08-09_burn_0_22_migration_plan.md).

The remote adapter uses this exact production prerelease row. Stable
publication closure must still repeat the dependency and receipt matrix; the
prerelease success is not that receipt.

---

## 6. Remote adapter

Landed 2026-08-23.

> **Named in error.** Earlier drafts of this section called the credential a
> `RemoteTicket`. No such type exists in `burn-remote 0.22.0-pre.1`; the real
> surface is `PeerAuthorizer` + an opaque `Vec<u8>` credential. The design is
> [lease-bound remote sessions](../technical_architecture/2026-08-10_lease_bound_remote_sessions.md),
> originally written against the source on 2026-08-10 and corrected against
> pre.2 during implementation.

**Burn Remote authorizes a session at admission.** M3 owner reclaim requires
terminating a session that was *already* authorized — the whole point of reclaim
is that permission granted a minute ago stops applying the moment the human
wants their GPU back. Admission-time authorization cannot express that.

The landed adapter supplies both:

1. a **lease-bound credential** — the session's authority is the lease, so it
   expires when the lease does; and
2. a **targeted close hook** — a way to end one server session on revoke.

The vendored patch reserves the session before application authorization, then
lets the pump observe a targeted server-close signal. That reservation matters:
without it, reclaim could inspect the session list in the instant after
authorization but before worker binding and miss the session. `p2panda-net`
also gained `accept_raw`, because its ordinary application ALPNs are deliberately
network-id salted while Burn peers require the literal `burn/remote/1` ALPN.
No second endpoint, router, or transport identity was introduced.

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
- **2026-08-09 (H0)**: `mere-mesh-host` landed. The gate's five requirements
  were all met, and building it surfaced a sixth that no amount of planning had
  turned up: a supervisor must keep checking that it *still holds* the lease its
  run is bound to, because a grant made on a stale board can win locally and
  lose globally. That check (`still_held`) is now the first thing every tick
  does after reaping. Two-host receipt over real transport; 7 tests green;
  clippy clean; largest file 531 lines.
- **2026-08-10 (H1)**: blob delivery landed, and the plan's framing was half
  wrong in a useful way. Discovery *was* the hard part, as recorded — but the
  fix needed no new record: personae's `DerivedKeyAttestation` was already
  exactly the authenticated author-to-master binding, and only wanted
  publishing. The genuinely new decision was collapsing the two blob stores
  into one, which the plan had not noticed was a choice at all. Disjoint-store
  two-host receipt over real transport; 124 tests green across both crates.
- **2026-08-10 (H2)**: retention safety landed, unusually close to as planned —
  the fail-closed instinct paid off twice. Putting the refusal on the *accept*
  path as well as the build path came free, because a checkpoint already carries
  everything needed to judge it (its own `at_ms` and snapshot), which meant no
  new clock and no board lookup. The one judgement call was defining
  "converged" as "terminal in an accepted checkpoint" rather than "terminal on
  my board": the second would let a worker drop a job's inputs before the poster
  had seen the answer. 117 mesh tests green.
- **2026-08-12 (H2 host completion / Distillery v0)**: Distillery became the
  first real `mere-mesh-host` consumer. Its authority drives supervisor ticks
  and an explicit owner-governed checkpoint/collection operation. The sweep
  added stable mesh-scoped custody tags to `mere-transport`, collecting disk
  and memory store modes, a current-tail shared-hash guard, and a real joined
  mesh receipt that executes `mesh.blake3/v1`, retains through completion, then
  releases only after an accepted checkpoint. Views, the resident process,
  Burn migration, and Burn Remote remain later gates.
- **2026-08-12 (upstream gate check)**: crates.io now exposes Burn and Burn
  Remote `0.22.0-pre.2`. Stable 0.22 remains unpublished, so the settled
  production-migration gate stays closed. The remote-session design was read
  against pre.1 at that checkpoint; its pre.2 revalidation landed on
  2026-08-23.
- **2026-08-20 (prerelease migration)**: Mark reopened the production gate for
  the chosen prerelease row. ESP migrated first and Quint second. The workspace
  now resolves one wgpu 30.0.0 and one libsqlite3-sys 0.38.2, the old
  `cubecl-wgpu` backport retired, and the native, wasm, WGPU, real-device, and
  ESP package receipts passed. Stable repinning remains open.
- **2026-08-23 (remote adapter)**: pre.2 was re-audited and the last executable
  gate landed. The pure mesh claim binds mesh, job, lease, epoch, job poster,
  server transport identity, device index, and exact resource; the authorizer
  also requires the claim's job/lease to be the active host run. Distillery
  mounts Burn on its shared p2panda endpoint and owner reclaim closes every
  matching reserved or active session before the revoke fact. The receipts
  prove a real remote tensor round trip, live refusal rules, stop-before-reclaim
  ordering, and client error rather than a hang. Mere `7fb07225`; p2panda
  `9f2c2a01`. Stable Burn publication closure is the only gate left open.
