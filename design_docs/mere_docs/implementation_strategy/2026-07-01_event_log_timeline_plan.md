# Event log + Timeline (Alembic slice E)

> **Superseded for implementation on 2026-08-03 by
> [Graph View Curation and Interaction](2026-08-03_graph_view_curation_and_interaction_plan.md).**
> `GraphJournal` now supplies the append-only attributed `CapturedDelta` replay
> substrate this plan proposed as a new `GraphMutation` log, and Cambium now
> supplies continuous pointer capture for the scrubber. The undo/restore
> distinctions below remain historical design authority; do not implement its
> obsolete log or discrete-slider seams.

**Date**: 2026-07-01
**Status**: Superseded for implementation; retained as a historical decision
record. Spun out of
[alembic_implementation_plan](2026-06-24_alembic_implementation_plan.md) §E, per that plan's own flag
("large enough that E may spin to its own plan once C and D land"). Slices A-D have since landed (C's
by-sessions eviction and D's Athanor P1/P2 both shipped 2026-06-30/07-01); that made E ready when this
historical plan was drafted, before the later replay and pointer substrates superseded its implementation.
Architecture: [alembic memory + engrams](../technical_architecture/2026-06-09_alembic_memory_and_engrams.md) §9.

## Goal

One append-only log of graph mutations (`GraphMutation`), with two read-only projections over it: the
**Alembic** current fold (already live — slice C's short/long-term memory model reads the live graph
directly, unaffected) and the **Timeline** historical replay (net-new: undo/redo at single-step
granularity, plus a scrubber to browse and restore past states). Per decision #5/#6 (locked 2026-06-24,
restated here): a kernel `GraphMutation` type building on `NodeAuditEventKind`; an append-only log
mirroring the tessera/cable `LogStore` **shape**, not its trait; checkpoint-interleaved replay; the
view-intent stream promoted to a parallel composed stream (never merged into the structural log); undo/redo
on the same substrate; the Timeline surfaces as an orrery scrubber/time-axis, not a docked pane. This log is
local memory, not an engram (no content-addressing, distillation, or federation) — E5 (Timeline → engram)
is the one place a chosen past state crosses into slice A.

## What already exists (verified against the code, 2026-07-01)

- **`NodeAuditEventKind`** (`graph-kernel/src/persistence.rs:249`) — 9 variants (`TitleChanged`, `Tagged`,
  `Untagged`, `Pinned`, `Unpinned`, `UrlChanged`, `ActionRecorded`, `Tombstoned`, `Restored`), dual
  rkyv+serde derives. Verified **dead code**: zero call sites anywhere outside its own definition. Its own
  doc comment already frames it as "the sequence of audit events in the WAL" — written for a WAL that
  doesn't exist yet. This is slice E's ready-made metadata vocabulary.
- **`GraphMutation` / `GraphIntent` do not exist.** Verified: no `enum`/`struct` of either name anywhere in
  the workspace. **Doc-drift note**: the 2026-06-24 plan's decision table cites "app/intents.rs" as a
  current home for these types — that path does not exist in this repo. The nearest real file,
  `graph-kernel/src/intents.rs`, is a different, already-in-progress module (portable `BrowserCommand`/
  `EdgeCommand`) whose own header comment defers `GraphMutation`/`GraphIntent` promotion to a future
  "Slice 57c", not this one. Flagging per the project's own doc-vs-code verification norm; not fixing the
  older doc here, since this plan supersedes its operative content for slice E.
- **`apply_graph_delta` IS the universal mutation chokepoint — as of the
  [write-path migration](2026-07-01_graph_write_path_migration_plan.md) (2026-07-01, same day this plan
  was drafted).** History of this bullet: the original research found `apply_graph_delta` had exactly 2
  real call sites against 95+ direct kernel-mutator calls from orrery alone (the Phase 6.5 boundary was
  declared but unenforced), so this plan initially redirected the recording hook to each mutator's own
  body. Mark then asked for the migration to be finished instead: every primitive durable mutator is now
  `pub(crate)`, all shell/runtime code routes through `GraphDelta` (extended with 16 new variants to
  cover the real mutation surface), and a `fixtures` feature covers test code. **So E1's recording hook
  instruments exactly one function — `apply_graph_delta` — as the 2026-06-24 plan originally intended.**
  Two recording caveats survive the migration, both documented in the kernel's Phase 6.5 comment: the
  compound kernel ops (`from_snapshot`, `cross_graph::copy_*`) stay outside the delta funnel and need
  their own recording treatment (compound events, or internal decomposition), and the transient-state
  exemptions (positions, session counter, lifecycle) are deliberately not recorded — they are not graph
  truth. The event log will also want per-`Graph`-instance opt-in (record the session graph, not the
  scratch subgraphs platen/engram-thaw legitimately build through the same funnel).
- **The tessera/cable `LogStore`** (`moothold/src/tessera/log_store.rs`, `murmuring/src/cable/log_store.rs`)
  is real, shipped, and proven (two-peer p2panda-net convergence tests pass for both). It implements
  `p2panda_store::LogStore` — a network/author/topic-shaped trait (`VerifyingKey`, topic resolution, async
  redb) — over a redb table keyed `author(32) ++ log_id_be(8) ++ seq_be(8) -> hash`, one write path, index +
  payload committed atomically. Slice E has no networking, no multi-author concern: the pattern worth
  mirroring is the **storage shape** (ordered fixed-prefix keys, one write path, native range-scan), not
  the trait — adopting `p2panda_store::LogStore` itself would pull that dependency into graph-kernel /
  session-runtime for zero sync benefit. Neither store's `prune_entries` is implemented (both stub it as a
  no-op) — there is **no existing precedent for truncate-after** (the operation redo-invalidation needs).
- **`session_graph_store::save()` / `WindowCtx::save_session()`** already does a full `GraphSnapshot` +
  `graph.json` overwrite on ~35 call sites across meerkat (every navigation, node edit, pane change, and on
  close) — verified via grep, not periodic or throttled. This is already, today, close to
  write-after-every-mutation. A "checkpoint" for slice E is not new I/O so much as stamping a log position
  into a save path that already fires constantly.
- **No undo/redo mechanism exists.** `Ctrl+Z`/`Ctrl+Y` are already bound (`register-input/src/defaults.rs`)
  to `workbench:undo`/`workbench:redo` action ids, and `actions.rs` already catalogues
  `PersistUndo`/`PersistRedo`/`PersistSaveSnapshot`/`PersistRestoreSession`/`PersistOpenHistoryManager` —
  verified zero dispatch handler anywhere in meerkat for any of these. Today, pressing Ctrl+Z is a silent
  no-op.
- **No continuous-drag scrubber substrate exists.** The only slider primitive
  (`list_pane::SliderSpec` / `settings_pane_view::slider_view`) is a segmented N-cell discrete picker (a
  hue-picker shape) with no pointer-drag behavior at all. The only real drag precedent in the whole chrome
  is `swatch.rs`'s vertex-drag hull editor, which works via host-side pointer hit-testing because genet has
  no native range input. A continuous-drag scrubber thumb would be first-of-its-kind UI engineering, not a
  reskin.
- **`node-lineage`'s `SharedNavigationMemory`** (`graph/history.rs`) already solves the exact
  append-log + single-cursor + "a new branch after stepping back forks and preserves the old forward path"
  problem — for per-node browse history (one URL sequence per node), not graph-mutation history (the whole
  graph's structure). The *algorithm* (forward-fork on a new entry while not at the tip) is a strong,
  tested precedent for E4's redo-truncation rule; the *generic type*
  (`OwnerScopedMemory<K,V,O,B>`) is not a clean fit to reuse directly, since its "owner" is naturally
  one node, not the whole graph/session.
- **`graph_engram` / `save_graph_engram`** (slice A, already shipped) is the manual, opt-in, content-addressed
  freeze — separate from and unaffected by this slice, and exactly the entrypoint E5 calls once a scrubbed
  past state is chosen for durable promotion.

## Plan / done conditions

Produced via a research → three-competing-design → judge workflow, then hand-verified against the code
(see the chokepoint correction above). The judge's synthesis leaned MVP-first for phasing, adopted
Architecturally-pure's "mandatory self-invertible payloads" discipline (cheap now, expensive to retrofit),
and Reuse-maximizing's storage-shape thrift — corrected here on the one point that didn't hold up (the
chokepoint).

- **E1 (event type).** A new kernel enum, not an extension or wrap of `NodeAuditEventKind`:
  ```
  enum GraphMutation {
      NodeSpawned { id, url, position },
      NodeRemoved { id, snapshot: NodeSnapshot },       // self-invertible: carries what re-spawning needs
      NodeMoved { id, from, to },
      EdgeAsserted { from, to, kind },
      EdgeRetracted { from, to, kind, snapshot },        // self-invertible
      FieldChanged { id, delta: FieldDelta },            // boxed before/after, NOT an opaque blob —
                                                          // splittable into first-class arms later
                                                          // without a log-format break
      MetadataChanged { id, kind: NodeAuditEventKind },  // NodeAuditEventKind's first real call site
  }
  ```
  Every variant that destroys or overwrites data carries what it needs to invert itself inline — spend the
  design time now, while the enum has zero callers and zero logged data; retrofitting inverses after real
  mutations are logged in the old shape means a second replay pass to recover them. `MetadataChanged`'s
  known gap: `NodeAuditEventKind` carries only the new value (old value is a query-time diff over prior log
  entries, per its own doc comment) — accept this for metadata specifically (lower-stakes than structural
  undo) or extend it with an `old` field now, while it's still uncalled. Decide before E1 lands; it is
  free now and not free later. Done: the enum compiles, round-trip serializes, and a unit test applies
  each variant's own stored inverse and asserts the graph returns to its prior state.
- **E1 (recording).** Instrument `apply_graph_delta` — since the
  [write-path migration](2026-07-01_graph_write_path_migration_plan.md) it is the enforced single
  external write path (every raw mutator is `pub(crate)`), so one recording hook in one function covers
  every shell/runtime mutation with no call site needing to remember anything. Recording is a
  per-`Graph`-instance opt-in (e.g. an `Option<recorder>` on `Graph` the host sets on the session graph
  only), so the scratch graphs platen/engram-thaw build through the same funnel don't pollute the log.
  The compound kernel ops (`from_snapshot`, `cross_graph::copy_*`) sit outside the funnel by design and
  get compound-event treatment when E1 lands. Done: every mutating gesture in the live app produces a
  logged `GraphMutation`; scratch-graph construction produces none.
- **E1/E2 (undo/redo — ship first, no storage yet).** Undo/redo as **inverse-mutation application**
  against an in-memory cursor (not replay-from-checkpoint): undo pops the in-memory sequence and applies
  the stored inverse directly via the existing kernel mutators; redo re-applies forward; a new mutation
  while the cursor isn't at the tail truncates the abandoned forward entries (node-lineage's proven
  forward-fork rule, borrowed as an algorithm shape only — not the generic `OwnerScopedMemory` type, whose
  one-node ownership doesn't fit a whole-graph cursor cleanly). O(1) per step; needs no checkpoint-cadence
  decision at all. Wires the already-reserved, already-silent `workbench:undo`/`workbench:redo` action ids
  and Ctrl+Z/Ctrl+Y to real behavior for the first time. **This alone is a complete, demoable, shippable
  slice** — no persistence, no UI beyond the existing keybindings. Explicitly, honestly scoped:
  in-session only (the cursor is not persisted/reloaded across a restart) until a real need for
  cross-session undo is named. Done: Ctrl+Z/Ctrl+Y actually undo/redo the last structural edit in the live
  app; a new edit after undo correctly drops the redo tail.
- **E2 (log storage + checkpoints).** An append-only per-session sidecar (single table keyed `seq_be(8) ->
  GraphMutation`, mirroring cable/tessera's ordered-key *shape* with the author/topic prefix dropped — one
  local author needs no author dimension; same embedded store `session_graph_store` already depends on;
  dual rkyv+serde matching `persistence.rs` convention), written at the existing `save_session()` call
  sites (~35 of them — no new recording chokepoint is invented for durability, only the append call is
  new). No new checkpoint-cadence scheduler: stamp the current log `seq` into the sidecar at each of those
  already-firing full-snapshot saves, and treat that snapshot as a checkpoint for free — this defers the
  plan's own open "checkpoint cadence N" question exactly as it anticipated ("tune by measured replay
  cost"), rather than guessing a number speculatively. Add a replay-equality test once the log exists (fold
  the log from the last checkpoint, diff against the live graph) as the standing correctness backstop.
  Done: the log survives a restart; folding it reconstructs a known sequence exactly.
- **E3 (Timeline replay + scrubber).** A read-only fold-to-nearest-checkpoint-then-replay-forward
  function; ship the **discrete `SliderSpec` cell-picker** (already wired chrome-side, zero new
  pointer-drag plumbing) as the only scrubber for the first release — one cell per checkpoint or coarse
  time-bucket, docked as a slim orrery-coupled time-axis per decision #6 (not a pane). Clicking a cell folds
  read-only onto a scratch/preview graph; it never advances the undo cursor or touches
  `session_graph_store`'s live save path. Do **not** build a continuous-drag thumb control up front — every
  design angle in this plan's own workup costed that as real, first-of-its-kind UI engineering with no
  existing precedent besides `swatch.rs`'s narrow case; defer it until the discrete version ships and is
  found lacking. Use real replay-cost measurement at this point to decide whether the piggybacked
  checkpoint (E2) needs a dedicated cadence mechanism at all. Done: a user can click a past point and see a
  read-only reconstruction of the graph at that point, with the live graph untouched.
- **E4 (cross-session persistence, parallel view-intent stream — explicit fast-follow, not required for
  E1-E3's value).** Reload the undo cursor from the log on session open (closes the in-session-only gap
  named above); promote `ViewIntent` from a single current sidecar to an appended `ViewIntentEvent` stream
  (camera, arrangement, hidden-relation toggles), composed by the Timeline UI keyed by shared
  `at_ms`/`session_id` but **never** merged into the `GraphMutation` log — a structural undo and a camera
  pan must stay independent (the well-known "undo did something I didn't expect" failure mode). Gate this
  phase on E1-E3 actually proving useful in practice, not on a fixed schedule.
- **E5 (Timeline → engram, last).** Scrub to a past incarnation, then call the existing (unchanged)
  `save_graph_engram` entrypoint with the replayed state — the sole, deliberate crossing from this local
  log into eidetic R0. No new mechanism; the only new code is passing a replayed snapshot instead of the
  live one.

## Gotchas

- **The chokepoint correction above is the single most consequential finding in this plan** — verify it
  again if graph-kernel's mutator surface changes shape before E1 lands (e.g. if a future refactor
  consolidates mutations back through `GraphDelta`, the recording hook should move with it).
- **Self-invertible payloads are a one-time design cost, not a one-time engineering cost.** Every future
  `GraphMutation` variant added after E1 inherits the discipline of carrying its own inverse — a real,
  ongoing tax on future kernel work, not just this slice's.
- **`MetadataChanged`'s old-value gap** (inherited from `NodeAuditEventKind`) needs an explicit decision
  before E1, not a silent inheritance — see E1 above.
- **Field/coupling granularity** stays coarse (one `FieldChanged` variant, boxed before/after struct) by
  design; first-class per-field arms are additive later, not a v1 requirement.
- **Do not adopt `p2panda_store::LogStore` or its async/network-shaped API** — confirmed dead weight for a
  local-only log; mirror the storage shape only.
- **Truncate-after (redo invalidation) has no precedent in either existing LogStore impl** (both stub
  `prune_entries` as a no-op) — this is genuinely new logic against a storage layer that has never needed
  to support removal before; budget a real test pass for it, not just "borrow node-lineage's algorithm and
  assume it transfers."
- **Keep it R0**: this log is local memory only, never an eidetic engram, until E5's deliberate,
  single crossing point.
- Slices C and D (the plan's stated precondition for spinning E out) are both now landed enough to proceed:
  C's by-sessions eviction shipped 2026-06-30; D's Athanor P1 (idle cadence) and P2 (consolidation) shipped
  2026-06-30/07-01. D's P3 (off-thread actor) remains blocked on facet extraction and does not gate E.

## Progress

- 2026-07-01: Drafted via a 9-agent research → three-competing-design → judge-synthesis workflow (parallel
  research over the plan's own scoping, `NodeAuditEventKind`, the tessera/cable `LogStore` precedent, the
  orrery UI substrate, and undo/redo precedent; three angled proposals — MVP-first, architecturally-pure,
  reuse-maximizing; a judge pass synthesizing a recommendation). Hand-verified the synthesis against the
  live code before writing this doc (per the project's doc-vs-code norm) and **caught one incorrect claim
  in the process**: the judge's recommended chokepoint, `apply_graph_delta`, has exactly 2 real call sites
  (not the universal funnel it was assumed to be) against 95+ direct kernel-mutator calls from orrery
  alone — corrected to instrumenting each kernel mutator's own body, mirroring B5's proven
  `navigate_node`-stamp pattern. Not started in code.
