# Athanor's steady-heat actor

**Date**: 2026-06-25
**Status**: P1 + P2 done (2026-07-01). Spun out of the [Alembic tail handoff](../../archive_docs/2026-06-30_completed_plans/2026-06-25_alembic_tail_and_audit_polish_handoff.md)
B1 (slice D's remainder). Architecture: [alembic memory + engrams](../technical_architecture/2026-06-09_alembic_memory_and_engrams.md).

## Goal

Schedule the forgetting pass **steady-heat** in the background (throttled at idle, yields to foreground),
replacing today's manual trigger. Then add Athanor's other passes (consolidation, facet extraction).
Stays inside eidetic **R0**: Athanor proposes, the host applies.

## What already exists (read against the code)

- **The pass logic** — `session-runtime/athanor.rs`: `propose_forgetting` (pure, reads a snapshot) +
  `apply_forgetting` (drops short-term cached blobs only; never graph truth, never engrams). Its module
  doc already says it is "the pure pass *logic*, separate from the actor that will schedule it." **This
  plan is that actor.**
- **The manual trigger** — `node_ops::run_forgetting_pass` calls propose+apply inline on the UI thread
  (the Alembic "forget stale recent now" row), reads the persisted `eviction_policy` (B4), and records the
  result into Steward (B2). The scheduler reuses this verb verbatim; it changes *when* it fires, not what.
- **The actor harness** — `armillary::{ActorHandle, Emitter, Wake, spawn}`. The canonical shape is
  `fetch.rs::spawn_fetcher(wake) -> (ActorHandle<FetchCommand>, Receiver<FetchUpdate>)`: an actor on its
  own thread, commanded via `ActorHandle`, emitting via a `Receiver` the host drains. `comms_host`,
  `content`, `find_worker` all follow it. Path B below mirrors it.
- **The idle hook** — `app_handler::about_to_wait` (the winit idle callback) currently only drains
  cross-window commands. It is where a host-side cadence (Path A) hangs, with `ControlFlow::WaitUntil`.

## Two shapes (the real choice)

**Path A — host-side idle cadence (cheap first slice).** No new thread. Track `last_forgetting: Option<Instant>`
and `last_activity: Instant` on the `Shell`; bump `last_activity` on window input. In `about_to_wait`: if
`now - last_activity >= IDLE_GRACE` **and** `last_forgetting` is `None`/older than `PASS_INTERVAL`, build
the primary window's ctx and call `run_forgetting_pass` once, stamp `last_forgetting`. Set
`ControlFlow::WaitUntil(next_check)` so the loop wakes to re-check with no input pending. Captures
"steady-heat, idle-throttled, yields to foreground" using the existing (light) verb on the UI thread —
forgetting reads a snapshot and drops a handful of blobs, so it does not need its own thread.

**Path B — armillary actor (the full version).** A `spawn_athanor(wake) -> (ActorHandle<AthanorCommand>,
Receiver<AthanorUpdate>)` mirroring `spawn_fetcher`: an off-thread actor owning the cadence and the
**heavy** passes. The R0 split means the actor cannot read the host's live graph, so forgetting either
stays a bare "tick" the host turns into `run_forgetting_pass`, or the host feeds it a snapshot to propose
against and drains the proposal back. The actor earns its thread when the heavy passes land (facet
extraction, consolidation chains), not for forgetting alone.

**Recommendation: A first, promote to B when the heavy passes arrive.** Forgetting is light; do not stand
up a thread + snapshot hand-off for it. The actor's reason to exist is the consolidation/facet work below.

## The other passes (Athanor's remit, decision-recorded)

- **Consolidation** — the dedup half is *already free* (content-addressing makes identical content one
  blob). The value is **relating version chains** (link successive engrams of the same material as a
  lineage). **Blocked until lineage exists, though:** the chain-relating reads
  `ProvenanceRecord.upstream`, which is `Vec::new()` in every crate today (verified — nothing populates
  it), and the content store is url-keyed (latest content, no retained version history). So there are *no
  chains to relate* until something produces lineage. The first producer is
  [engram compose/merge](../../archive_docs/2026-07-03_completed_plans/2026-06-25_engram_compose_merge_plan.md) (B7, which sets `upstream = source ids`)
  or an archive-on-refetch capture path. **Consolidation is downstream of B7**, not a standalone slice.
- **Facet extraction** — heavier (it reads content, possibly runs a local model — ties to the
  [local-models harness](../research/2026-06-24_local_models_harness_brief.md)). This is the pass that
  justifies Path B's off-thread actor. Later.

## Plan / done conditions

- **P1 (Path A).** Idle-cadence forgetting in `about_to_wait` (timers on `Shell`, `WaitUntil`,
  idle-detection); fires `run_forgetting_pass` steady-heat without a manual click. Done: with the app left
  idle past `PASS_INTERVAL`, a stale short-term node's cached content is evicted automatically and Steward's
  "Last forgetting" row (B2) updates; active use does not trigger a pass mid-interaction.
- **P2 (consolidation pass).** ✅ **DONE 2026-07-01.** `propose_consolidation`/`apply_consolidation` in
  `athanor.rs` relating version chains; driven by the same idle cadence as P1. Done: two engrams of the
  same material gain a lineage link.
- **P3 (Path B, when facet extraction lands).** `spawn_athanor` actor owns the cadence + heavy passes
  off-thread; forgetting/consolidation move behind it. Done: a heavy pass runs without dropping a frame.

## Gotchas

- **Chrome-hot.** P1 edits `app_handler.rs` + `main.rs` (the `Shell` struct) — both in Mark's concurrent
  set (`main.rs` was dirty 2026-06-25). Land P1 when his chrome work commits, or coordinate. (P2
  consolidation is `session-runtime`-only so it does *not* collide with the chrome-hot files — but it is
  **data-blocked** per *The other passes* above, so it is not a viable "do this while P1 waits"; B7
  unblocks it first.)
- **Thresholds are settings, not constants.** `IDLE_GRACE` + `PASS_INTERVAL` should be tunable (persona
  settings, like the eviction policy), not hardcoded picks — expose them once P1 proves the shape.
- **Which orrery.** Multi-window pools share orreries + the content store, so one pass on the primary ctx
  suffices; do not run per-window (it would re-proposes the same urls).
- Keep it R0: the cadence only changes *when* propose/apply run; it never drops graph truth or engrams.

## Progress

- 2026-06-25: Drafted from the handoff B1, verified against `athanor.rs` (the pure pass logic + its
  "separate actor" note), `run_forgetting_pass`, `about_to_wait`, and the `spawn_fetcher` actor shape.
  Not started; P1 waits on the chrome-hot `main.rs`/`app_handler.rs` set clearing.
- 2026-06-30: **P1 shipped.** `main.rs`/`app_handler.rs` were clear at landing time, so this went in
  without a coordination wait. New `app_handler/idle_forgetting.rs`: `Shell` gained `last_activity: Instant`
  (bumped by `note_activity`, called from `on_window_event` — any real input, not actor wakes) and
  `last_forgetting: Option<Instant>`; `about_to_wait` calls `maybe_run_idle_forgetting_pass` after `apply`,
  which fires `run_forgetting_pass` against the **primary** window's ctx once idle past `IDLE_GRACE`
  (120s), throttled to `PASS_INTERVAL` (900s). Constants, not settings yet, per the plan's own gotcha.
  **Deviation from the plan's literal text:** the plan suggested `ControlFlow::WaitUntil` so the loop
  "wakes to re-check with no input pending" — verified against the live code that nothing in the app sets
  `ControlFlow` anywhere (grepped clean), so winit's default `Poll` is already active and `about_to_wait`
  already ticks continuously; adding `WaitUntil` scheduling here would be a wider event-loop-cadence change
  outside this slice's scope, so P1 reads `Instant::now()` each tick instead (two comparisons, cheap) and
  leaves `ControlFlow` untouched. P2 (consolidation) stays blocked on B7's lineage per the plan; P3 (Path B
  actor) stays blocked on facet extraction landing.
- 2026-07-01: **P2 shipped**, unblocked now that B7 (engram compose) populates `upstream`. New
  `ConsolidationProposal` + `propose_consolidation`/`apply_consolidation` in `athanor.rs`: lists engram
  manifests (`provenance.upstream` rides on the manifest, so checking "already linked" needs no thaw),
  thaws the newest `CONSOLIDATION_CANDIDATE_CAP` (50) *`Generated`-origin* engrams once each to build their
  url sets, and proposes pairs whose url overlap is `>= SAME_MATERIAL_OVERLAP` (0.5, of the smaller set)
  and aren't already named together in some other manifest's `upstream`. Restricting candidates to
  `Generated` origin keeps composite-of-composite growth from running away — a `Derived` (already-
  consolidated) engram is never re-proposed as a fresh pairing target. **Applying composes the pair** —
  audited against eidetic (manifests are immutable once saved, no lighter-weight post-hoc link exists) —
  which is content-addressed, so re-linking an already-consolidated pair on a later pass is a safe no-op,
  not a duplicate; this is also why the "already linked" pre-filter matters (keeps a converged store from
  re-thawing/re-composing the same settled pairs every idle tick forever). `run_consolidation_pass` (new,
  in `export.rs` — `node_ops.rs` was already at the 600-LOC ceiling) runs right after `run_forgetting_pass`
  in the same `maybe_run_idle_forgetting_pass` gate, so one cadence drives both per the plan; records an
  `alembic.consolidate` diagnostic (Apparatus-visible), the same surface B2 uses for forgetting. 7 tests
  (4 new) in `athanor.rs`, all green. Not yet surfaced in Steward beyond the diagnostic — no "last
  consolidation" row like B2's forgetting one; a thin follow-on if it turns out to matter. P3 (Path B
  actor) still blocked on facet extraction landing.
