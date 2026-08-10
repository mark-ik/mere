# Mesh Lease Scheduler Plan

**Date**: 2026-06-30

**Status**: **Complete 2026-08-09 as the lease protocol floor.** Re-scoped and
then executed the same day, directly after M2. All seven done conditions carry
a receipt.

Read "complete" narrowly. What landed is the *protocol*: the lease algebra
(envelope, epochs, claim eligibility, clock-free fold, time-indexed
projection), the policy vocabulary, and the control seam — all of it library,
all of it tested. What did **not** land is a host that uses it. No shipped
binary supervises in-flight work: `mesh-peer` blocks on execution, so it
declares `DevicePolicy::unsupervised` and the worker refuses to hand it leased
jobs at all. A device that cannot heartbeat while working and cannot stop on
demand must not take a lease, and M3 enforces that rather than pretending
otherwise.

The supervisor, and the lanes M2 and M3 both leaned on, are gates H0-H2 of the
[mesh host lanes plan](../../mere_docs/implementation_strategy/2026-08-09_mesh_host_lanes_plan.md).

**Related**:
[`2026-06-04_resource_coordination_brief.md`](../../mere_docs/research/2026-06-04_resource_coordination_brief.md),
[`2026-06-30_personal_mesh_substrate_m2_plan.md`](2026-06-30_personal_mesh_substrate_m2_plan.md),
[`2026-08-09_burn_0_22_migration_plan.md`](../../mere_docs/implementation_strategy/2026-08-09_burn_0_22_migration_plan.md),
[`2026-06-12_mesh_m1_plan.md`](../2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md)

M3 makes lending an owned device socially safe. It records lease lifecycle as
replicated facts, but each device enforces its owner's settings locally. Owner
reclaim is a clean handoff, never worker dishonesty.

---

## 1. Core rule and ownership

Owned hardware keeps absolute foreground priority. If the user cannot take a
GPU, CPU, storage budget, or network budget back, the personal mesh has become
a cluster scheduler wearing local-first clothes.

- **Mesh wire** records lease, heartbeat, release, revoke, and completion
  facts.
- **The deterministic fold** retains those facts without consulting a wall
  clock or the OS.
- **A time-indexed projection** answers whether a lease is live at an explicit
  observation time.
- **The host scheduler** reads local settings and activity, chooses whether to
  offer resources, supplies execution control, and authors reclaim.
- **Resource adapters** cooperate with cancellation and checkpoints. They do
  not decide foreground policy or classify their own reliability.

Replicated grant state plus local enforcement is the invariant. M3 does not
introduce an online central broker.

---

## 2. M3a: settle lease authority and time

The earlier plan said a missed heartbeat could simply become `LeaseLapsed` in
the fold. That is incomplete: a pure fold has no current time, and authored
timestamps alone cannot silently become trustworthy clocks.

Before adding events, settle and test these rules:

1. Who may issue a lease and extend it?
2. Which signed fact establishes its duration or expiry?
3. What bounded clock skew is accepted?
4. Who may author reassignment after expiry?
5. Which evidence distinguishes an expired lease from a temporarily unseen
   heartbeat?

The initial personal-mesh recommendation was job-author lease authority: claims
are proposals; the job author grants a lease to the deterministic winning
claim. **That recommendation was rejected at the gate on 2026-08-09**, for the
reason this section anticipated: it requires the author to be online *after*
claims arrive, and online again to reassign. The ordinary personal-mesh shape
is "post from the laptop, close the lid", under which no job ever starts.

### The settled rules

**Authority is a signed envelope, exercised once.** The job author signs
`LeaseTerms { max_duration_ms, heartbeat_ms, miss_allowance }` into the spec at
post time. The deterministic claim winner for an epoch then authors its own
`LeaseGranted` — valid only inside that envelope. Author authority survives the
author going offline; a holder still cannot invent its own window.

1. **Who may issue a lease, and extend it?** The epoch's claim winner issues it
   to itself, bounded by the author's envelope. **Extension is not in M3**: a
   lease runs its signed window, and a job that needs longer is posted with
   longer terms. When a genuinely long-running resource arrives, extension
   becomes a heartbeat-carried bump under the same ceiling — not a new
   authority.
2. **Which signed fact establishes duration or expiry?** `LeaseGranted`'s own
   `granted_at_ms` / `expires_at_ms`, admissible only when the difference fits
   `terms.max_duration_ms`.
3. **What clock skew is accepted?** An observer-side `LeasePolicy::max_skew_ms`
   (default 60s), applied *only* in the projection and always in the holder's
   favour: it widens the live window at both ends and hides a grant or end fact
   dated further ahead than the slack.
4. **Who may author reassignment after expiry?** Nobody in particular. The next
   epoch's claim winner grants itself, exactly as epoch 0 did. The gate is
   `granted_at_ms >= the previous epoch's signed end` — one signed value against
   another. No privileged reassigner, no online broker.
5. **What distinguishes expiry from an unseen heartbeat?** Two projections that
   are deliberately not the same thing. `LapseReason::Expired` is a fact: the
   signed window is over. `LapseReason::HeartbeatSilence` is an *observation*:
   nothing heard for `heartbeat_ms × miss_allowance`, which may equally mean
   this device is behind on sync. Both reopen the job; only the first says
   anything about the holder.

Three shape corrections fell out of building it:

- `LeaseGranted` carries **no** `holder` and **no** `lease` field. The holder is
  the operation's author and the lease id is the operation's hash, so neither
  is forgeable into a body.
- It carries no `checkpoint` either: `JobSpec::checkpoint` already says what
  happens on interruption, and a second copy would be a second source of truth.
- The checkpoint classes keep M2's `Restart` / `Resumable` names and gain
  `NonInterruptible`, rather than renaming to this plan's provisional
  `Interruptible` / `Checkpointable`. Renaming would change the encoded variant
  strings inside already-signed V2 specs, which §3 forbids.

---

## 3. M3b: versioned lease facts and projection

Add lease-era variants without changing the serialized M1/M2 variants:

```rust
LeaseGranted { job, lease, holder, expires_at_ms, heartbeat_ms, checkpoint, at_ms }
LeaseHeartbeat { job, lease, progress, checkpoint, at_ms }
LeaseReleased { job, lease, reason, at_ms }
LeaseRevokedByOwner { job, lease, reason, at_ms }
JobCompletedUnderLease { job, lease, output, at_ms }
```

Names remain provisional; the facts do not. A lease id binds every heartbeat,
checkpoint, revoke, and output to one grant. A completion from the wrong holder,
an expired/revoked lease, or a prior lease is rejected before mutation.

> **As shipped** (2026-08-09), `LeaseGranted` is
> `{ job, epoch, granted_at_ms, expires_at_ms }`. The `holder`, `lease`,
> `heartbeat_ms`, and `checkpoint` fields sketched above were all dropped: the
> first two are derived from the operation itself and so cannot be forged, and
> the last two already live in the author's signed `LeaseTerms` / `JobSpec`. See
> §2's shape corrections.

The fold stores the latest admissible facts. A projection such as
`board.at(observed_at_ms)` classifies:

- `Posted`: no granted lease;
- `Leased`: grant exists and is live at the supplied time;
- `Revoked`: the owner explicitly reclaimed it;
- `Released`: the holder returned it cleanly;
- `Lapsed`: the projection time is beyond the authorized heartbeat/expiry
  window; and
- `Done`: an admissible leased completion exists.

Calling the fold twice over the same facts must return the same retained state.
Changing only the explicit observation time may change the live projection.
Tests must cover boundary times and clock-skew policy.

Reliability distinguishes causes:

- owner revoke and policy shutdown carry no worker penalty;
- clean release carries no penalty;
- lapse may become reliability evidence later; and
- invalid completion is an admission failure, not a completed job.

---

## 4. M3c: host policy and execution control

Device policy is configurable host state:

- idle and foreground-activity thresholds;
- battery and thermal floors/ceilings;
- network class and bandwidth cap;
- quiet hours;
- maximum concurrent jobs;
- allowed resource ids;
- checkpoint classes this device will accept; and
- later, per-ring limits.

The host observes the OS and produces a policy snapshot/capability
advertisement. `mere-mesh` must not query the OS. ESP must not choose rendering
versus compute priority. The scheduler supplies M2's execution-control handle
and may request cancel, yield, or checkpoint.

Policy changes affect new offers immediately. Active work follows its declared
checkpoint class:

- `Interruptible`: cancel and re-dispatch from the start;
- `Checkpointable`: request a checkpoint, then cancel and re-dispatch from that
  boundary; and
- `NonInterruptible`: finish or fail within a user-configurable maximum grace
  window.

Owner reclaim is allowed regardless of class. A grace window changes how the
handoff occurs, not who has authority.

---

## 5. M3d: executable reclaim receipt

Use a deterministic delayed test resource before a GPU resource. It must expose
multiple cooperative cancellation points and an optional checkpoint boundary.

The receipt runs two workers:

1. Both race to claim a job; one receives the lease.
2. The winner heartbeats and makes observable progress.
3. Its owner reclaims the device through the host policy path.
4. Execution stops, a clean owner-revoke fact is authored, and reliability is
   unchanged.
5. The other worker receives a new lease and completes.
6. A separate run drops heartbeats without owner revoke and projects `Lapsed`.
7. The board and receipt distinguish those two histories.

Only after this receipt may a long-running Burn/WGPU or Burn Remote resource
join the registry.

---

## 6. Storage and uptime are a follow-on

Storage can reuse lease ids, owner reclaim, and policy snapshots, but its proof
is continued encrypted retrievability rather than compute completion. The
earlier `BestEffort`, `Checkpointed`, `UptimeWindow`, and `Replicated` service
classes remain valid research vocabulary. They are not implemented in M3.

Spin storage service classes into their own plan when a real replicated-blob
consumer is selected. That plan owns possession challenges, repair, replica
counts, and reliability effects.

---

## 7. Carried forward

M2 and M3 both leaned on lanes the mesh does not provide, and both said so
rather than pretending otherwise. All of it now lives in one place: the
[mesh host lanes plan](../../mere_docs/implementation_strategy/2026-08-09_mesh_host_lanes_plan.md), which owns
peer-to-peer blob delivery, blob retention, retention versus live leases,
portable checkpoints, tolerant verification, and reliability accounting.

---

## 8. Burn Remote prior art and gate

As of 2026-08-09, `burn-remote` is published only as `0.22.0-pre.1`. Its iroh
transport, client/server split, `RemoteTicket`, and authorization callback still
fit the intended compute adapter. It can mount on an application-owned iroh
Router, while Mere retains job authorization, lease lifecycle, and owner
reclaim.

The installed Rust 1.97.1 exceeds Burn 0.22's Rust 1.95 requirement. Release
stability and API migration, rather than the toolchain, remain the gate. The
separate Burn 0.22 plan owns that migration. A prerelease probe may inspect the
Router and authorization seams after M2/M3, but it does not widen production
dependencies or publish a public remote feature.

---

## 9. Non-goals and stop rule

M3 does not implement kith authorization, economic standing, public resource
banking, storage repair, Burn migration, remote tensor transport, or training.

Stop after the delayed-resource owner-reclaim and lapse receipts. The next
serial gate is the stable Burn migration; Burn Remote follows it as another
resource adapter rather than another scheduler.

## 10. Done conditions

Each line names the receipt that closed it. Paths are under
`crates/mesh/mesh/`.

- **Lease authority, expiry, skew, and reassignment rules are explicit and
  executable.** §2 above records the answers; `lease::tests` executes them
  (`the_chain_accepts_one_grant_per_epoch_from_the_claim_winner`,
  `a_grant_from_a_non_winner_is_refused`,
  `a_grant_wider_than_the_signed_envelope_is_refused`,
  `the_next_epoch_may_not_start_before_the_previous_one_ended`), and
  `projection::tests::skew_widens_both_boundaries_in_the_holders_favour` pins
  the skew rule.
- **Folded facts are deterministic; time-dependent state requires an explicit
  observation time.** `tests/lease_receipts.rs::lease_facts_fold_identically_in_every_order`
  rotates the whole log and then moves only the clock;
  `projection::tests::the_same_facts_change_phase_only_with_the_observation_time`
  and `silence_and_expiry_boundaries_are_exact` cover the boundaries.
- **Wrong-holder, stale-lease, revoked-lease, and late outputs are rejected.**
  `lease::tests::facts_from_the_wrong_author_or_outside_the_window_are_ignored`,
  `the_earliest_end_wins_and_a_completion_closes_the_job`, and the fold's
  refusal to accept a completion whose output breaks the signed grant
  (`fold::GatheredJobLeases::resolve`).
- **Host settings are configurable and OS-free below the host boundary.**
  `policy::tests::each_configured_limit_names_the_reason_it_withholds` and
  `quiet_hours_wrap_midnight`; `DeviceConditions` is host-supplied and nothing
  in `mere-mesh` reads a clock, a battery, or a window manager.
- **Cooperative cancellation and checkpoint control reach a real resource.**
  `mesh.delayed/v1` (`resources/delayed.rs`) with
  `a_checkpoint_request_stops_at_a_boundary_and_says_where` and
  `a_cancel_after_a_checkpoint_request_still_throws_the_work_away`;
  `resource::tests::signals_escalate_and_never_walk_back`.
- **Owner revoke and heartbeat lapse produce distinct convergent receipts.**
  `tests/lease_receipts.rs::owner_revoke_and_heartbeat_silence_are_distinguishable_histories`
  continues one shared opening two ways, and
  `sync::tests::an_owner_reclaim_converges_and_the_other_peer_finishes_the_job`
  proves the reclaim arrives at the other peer *as a reclaim* over real
  p2panda-net.
- **A second worker completes after reclaim or lapse.**
  `tests/lease_receipts.rs::owner_reclaim_hands_the_job_to_a_second_worker`
  runs the plan's steps 1-5 end to end against the real registry, with the
  first device's run actually cancelled mid-flight.

## 11. Progress

- **2026-06-30**: split out of the resource-coordination brief around lease
  lifecycle, owner reclaim, device policy, and storage service classes.
- **2026-07-03**: recorded Burn Remote over iroh as compute prior art.
- **2026-08-09**: split replicated lease facts from host enforcement; exposed
  the pure-fold/current-time contradiction; added an explicit lease-authority
  decision gate, versioned events, time-indexed projection, cooperative
  cancellation, and a real owner-reclaim receipt; deferred storage classes;
  refreshed Burn Remote to the current `0.22.0-pre.1` gate.
- **2026-08-09 (execution)**: M3a-M3d landed in `crates/mesh/mesh`. New modules
  `lease`, `projection`, `policy`, `fold`, `resources/delayed`; five lease
  variants beside the frozen M1/M2 ones; `Job` gained a clock-free
  `LeaseRecord` and its eligible-claimant list; `JobControl` gained escalating
  signals and run-reported progress. 108 lib tests plus 3 integration receipts
  green, clippy clean on the new code, every file back under the 600-LOC
  ceiling (largest non-test body: `board.rs` at 480, after the gather half moved
  to `fold.rs`).

  Structural learnings, per the implementation-feedback rule:

  - **A one-sided eligibility window is a real hole, and a test found it.** The
    first cut made a claim eligible for epoch N whenever `at_ms >= boundary(N)`.
    Epoch 0's boundary is 0, so a device claiming *much later* could become the
    epoch-0 winner retroactively and invalidate a lease already running. The
    window is closed at both ends now — `[previous epoch's end, this grant's own
    granted_at_ms]` — so a holder can only ever narrow the field against itself,
    never widen it. This is the rule to re-read first when kith widens the ring.
  - **"Full" is not "stop".** The first `DevicePolicy` returned `AtCapacity`
    from the same function the worker uses for *both* "take new work?" and "keep
    running work?", so a device would have reclaimed its own job the instant it
    accepted it. Capacity is a separate question now, and the asymmetry is the
    point: a reason to refuse is not automatically a reason to abandon.
  - **Progress belongs to the run, not the scheduler.** `LeaseProgress` is
    written by the adapter through `JobControl::report` and read by the host
    through the handle, so a heartbeat cannot claim progress that did not
    happen. That fell out of wanting the receipt to heartbeat *observed* work
    rather than a number the test chose.
  - **The unforgeable fields are the ones that are not fields.** `LeaseGranted`
    names no holder and no lease id: both are derived from the operation
    itself. Every lease rule that matters is then a comparison between things an
    author could not have lied about independently.
  - **Retention and live leases do not mix yet.** A checkpoint that prunes the
    claim operations a later epoch depends on will stop that epoch validating.
    Recorded in `fold.rs` and carried forward rather than papered over.

  Deferred into the [mesh host lanes plan](../../mere_docs/implementation_strategy/2026-08-09_mesh_host_lanes_plan.md):
  portable checkpoints (a checkpoint is local until a blob lane can carry it),
  retention interaction with live leases, and reliability accounting — plus M2's
  three, which move there from §7 of this plan.
