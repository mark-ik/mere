# Mesh Lease Scheduler Plan

**Date**: 2026-06-30

**Status**: Planned immediately after the M2 namespace/resource receipt;
re-scoped 2026-08-09 into replicated lease facts and host enforcement.

**Related**:
[`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md),
[`2026-06-30_personal_mesh_substrate_m2_plan.md`](2026-06-30_personal_mesh_substrate_m2_plan.md),
[`2026-08-09_burn_0_22_migration_plan.md`](2026-08-09_burn_0_22_migration_plan.md),
[`../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md`](../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md)

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

The initial personal-mesh recommendation is job-author lease authority: claims
are proposals; the job author grants a lease to the deterministic winning
claim, including holder, duration/expiry, heartbeat interval, checkpoint class,
and a unique lease id. The author need not remain online while the lease runs,
but reassignment waits for an authorized expiry/revoke projection rather than
trusting a worker's self-declared clock.

If implementation finds that this makes disconnected progress unusable, stop
and record a different authority rule. Do not bury a distributed-clock choice
inside `JobBoard::fold`.

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

## 7. Burn Remote prior art and gate

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

## 8. Non-goals and stop rule

M3 does not implement kith authorization, economic standing, public resource
banking, storage repair, Burn migration, remote tensor transport, or training.

Stop after the delayed-resource owner-reclaim and lapse receipts. The next
serial gate is the stable Burn migration; Burn Remote follows it as another
resource adapter rather than another scheduler.

## 9. Done conditions

- Lease authority, expiry, skew, and reassignment rules are explicit and
  executable.
- Folded facts are deterministic; time-dependent state requires an explicit
  observation time.
- Wrong-holder, stale-lease, revoked-lease, and late outputs are rejected.
- Host settings are configurable and OS-free below the host boundary.
- Cooperative cancellation and checkpoint control reach a real resource.
- Owner revoke and heartbeat lapse produce distinct convergent receipts.
- A second worker completes after reclaim or lapse.

## 10. Progress

- **2026-06-30**: split out of the resource-coordination brief around lease
  lifecycle, owner reclaim, device policy, and storage service classes.
- **2026-07-03**: recorded Burn Remote over iroh as compute prior art.
- **2026-08-09**: split replicated lease facts from host enforcement; exposed
  the pure-fold/current-time contradiction; added an explicit lease-authority
  decision gate, versioned events, time-indexed projection, cooperative
  cancellation, and a real owner-reclaim receipt; deferred storage classes;
  refreshed Burn Remote to the current `0.22.0-pre.1` gate.
