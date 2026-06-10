# Short-term memory substrate — implementation plan

**Date**: 2026-05-14
**Status**: Implementation plan — picks substrates per short-term consumer; pins what stays in-memory vs. what gets a JSON sidecar vs. what gets a heavier key-value store
**Scope**: The [memory-tiers brief](../research/2026-05-11_memory_tiers_brief.md) §3 names six short-term consumers (branch state, fork state, view intent, transient diffs, in-progress edits, ephemeral diagnostics) and defers the substrate-per-consumer pick. This plan makes that pick.

**Related**:

- [`../research/2026-05-11_memory_tiers_brief.md`](../research/2026-05-11_memory_tiers_brief.md) — the partition story this plan implements.
- [`2026-05-14_view_intent_sidecar_plan.md`](2026-05-14_view_intent_sidecar_plan.md) — view intent is already on JSON sidecars (v0a landed). One consumer down.
- [`2026-05-11_tearout_operations_brief.md`](../research/2026-05-11_tearout_operations_brief.md) — branch and fork operations produce the per-consumer state this plan persists.
- [`crates/eidetic/src/engram.rs`](../../../crates/eidetic/src/engram.rs) — long-term substrate. This plan defines the *not-engram* shapes.

---

## 1. The principle restated

> Short-term state is durable enough to survive an app restart, cheap enough that creating lots of it doesn't matter, and discardable without ceremony.

Eidetic engrams are expensive; ephemeral working state shouldn't pay that cost. The substrate per consumer should be *as small as possible* for what that consumer needs.

## 2. Picks per consumer

| Consumer                  | Substrate                            | Why                                                                                 |
| ------------------------- | ------------------------------------ | ----------------------------------------------------------------------------------- |
| `ViewIntent`              | JSON sidecar (landed §5.3)           | Tiny payload per pane, hand-inspectable, rare writes. Already shipped.              |
| **Branch state**          | JSON sidecar per branch              | One file per branch; the graph-tree's lineage entries serialise compactly.          |
| **Fork state**            | Session directory (same as long-term) | A fork *is* a session; `manifest.durable = false` flags it as throwaway-eligible.  |
| **Transient diffs**       | In-memory only                       | Diffs are derived on demand from two graph states. Persisting wastes I/O.           |
| **In-progress edits**     | In-memory until commit               | Edits coalesce into the next consolidation or the on-disk `graph.json`.             |
| **Ephemeral diagnostics** | In-memory (existing apparatus buffer) | Already the right shape. Engram form happens on `permission.denied`-severity consolidation only. |

Three of six are already-decided (ViewIntent shipped, transient diffs in-memory, ephemeral diagnostics in-memory). The actual decisions this plan makes are **branch state** and **fork state**.

## 3. Why JSON sidecars (not fjall) for branch state

The memory-tiers brief sketched "fjall or simple JSON sidecar." Pick: **JSON sidecar**. Reasons:

- **Branches are coarse-grained.** One branch ≈ one logical chunk (a graphlet + its anchor + accumulated members). The cardinality is low (tens to low hundreds per session, not millions).
- **Writes are infrequent.** A branch's state changes when the user adds an anchor or accepts a member; not 60 times per second. fjall's strength is high-write-rate workloads; JSON sidecars match this access pattern.
- **Hand-inspectability matters during dev.** Per the same memory the session_graph_store comment header cites, JSON wins for "is this thing on disk what I expect?" debug loops.
- **One persistence story to maintain.** session_graph_store + manifest_store + view_intent_store all use atomic JSON. Branch state being the fourth keeps the cognitive load flat.

When does fjall pay? **It doesn't, until short-term writes get bursty enough to matter.** That trigger doesn't exist in Mere today; defer the substrate change to when a real consumer pushes through it.

## 4. Branch state file layout

```text
<session_dir>/
├── manifest.json
├── graph.json
├── views/
└── branches/                       (NEW; this plan)
    ├── <branch_id>.json            ← BranchState sidecar
    └── ...
```

`<branch_id>` is the graphlet's UUID (from `graph-tree`'s existing identity model). One file per branch keeps blast radius small — corrupting one branch doesn't take down the others.

`BranchState` v0 shape:

```rust
pub struct BranchState {
    pub schema_version: u32,
    pub branch_id: BranchId,
    pub donor_session_id: SessionId,
    pub donor_anchor: NodeKey,
    pub created_at: SystemTime,
    pub members: Vec<BranchMember>,
    pub last_activity_at: SystemTime,
}
```

The shape mirrors `graph-tree`'s in-memory `GraphTree<NodeKey>` but flattens for disk. Round-trip fidelity is "enough to reopen the branch in the same shape the user closed it" — strictly less than an engram, more than the apparatus buffer.

Atomic writes via `.tmp` + rename (the established pattern); save-on-mutation in v0 (no debouncing until profiling shows it matters).

## 5. Fork state — riding on the session directory

A fork is a new session. The session directory layout already covers it:

```text
<sessions_dir>/<fork_session_id>/
├── manifest.json   ← `durable: false` flags the fork as throwaway-eligible
├── graph.json      ← the fork's session graph
└── views/          ← per-pane ViewIntents (eventual)
```

No new substrate. The "short-term-ness" of a fork is a flag, not a separate storage path.

Consolidating a fork (the user gesture in the memory-tiers brief §4) sets `durable: true` and mints an engram from `graph.json`. The session directory stays; it's no longer a candidate for throwaway sweep.

## 6. Throwaway sweep

Short-term consumers that hold "throwaway" state need a sweep policy. v0a defines the contract (when does sweep run, what does it touch); v0b lands the implementation.

**Sweep policy v0:**

- Run on app exit (graceful shutdown).
- Targets: branches whose `last_activity_at` is older than `branch_ttl_days` AND not referenced by an open session; forks with `durable: false` AND `last_activity_at` older than `fork_ttl_days`.
- Defaults: `branch_ttl_days = 30`, `fork_ttl_days = 7`. Both per-persona configurable.
- Honors the [consolidate-on-idle policies](../research/2026-05-11_memory_tiers_brief.md) §4.1 — auto-consolidation runs *before* sweep, so anything the user opted into preserving lands as an engram first.

Sweep doesn't run automatically without opt-in; v0a stores the policy + provides the API. The "always-on auto-cleanup" decision is a persona-settings choice, not a hardcoded default.

## 7. v0a → v0b → v1 sequence

**v0a (this turn):**

- Plan doc captures decisions per consumer.
- **No code lands.** Branch state needs `graph-tree` integration that hasn't materialised in the host yet (branch operations are still tear-out-conceptual, not implemented). Premature primitives would just be three files in `system/session-runtime` with no consumers.

**v0b (when branch operations land):**

- `branch_store` module in `system/session-runtime` (mirroring `session_graph_store` / `view_intent_store`): `save_branch_state`, `load_branch_state`, `branch_state_exists`, plus a list/walk helper for sweep.
- `BranchState` Serde shape with schema_version + the v0 fields above.
- Host wiring: branch-create stamps a new sidecar; branch-mutate updates it; consolidate-branch produces an engram + deletes the sidecar; sweep-on-exit removes stale ones.

**v1 (with the persona model):**

- Per-persona TTL config.
- Persona-scoped sweep on persona-switch.
- Throwaway-persona sweep on app exit (whole persona directory cleared).

## 8. Open questions

1. **When does branch state become engram-shaped?** The memory-tiers brief §4 says consolidation produces an engram via eidetic. That pipeline exists at the engram level but isn't wired into branch ops. v0b adds the bridge.
2. **Sweep concurrency.** Sweep on exit runs while the manifest store flushes dirty manifests. Order matters: flush dirty manifests *first* (so a manifest pointing at a branch doesn't get sweep-orphaned), then sweep eligible branches. Implementation detail, not a substrate decision.
3. **Cross-session branch references.** A branch in session A could reference a node in session B (graph-tree's federation hooks). v0 brands branches as session-scoped; cross-session is a Phase-4 federation concern. If it lands sooner, branches go in a `<data_root>/branches/` directory instead of `<session_dir>/branches/`.
4. **fjall as a future substitute.** If write bursts ever appear (e.g. a "live shared editing" mode where many branches mutate per second), revisit JSON sidecars vs. fjall on benchmark data. Until then JSON wins on simplicity.

## 9. What this plan commits to

- **JSON sidecars are the v0 short-term-persistent substrate.** No fjall, no rkyv binary, no engram-shaped indirection.
- **Branch state gets its own sidecar layout** (`<session_dir>/branches/<branch_id>.json`).
- **Fork state rides on the existing session directory** with `manifest.durable = false` as the throwaway flag.
- **Three other consumers (diffs, edits, diagnostics) stay in-memory.** ViewIntent is already on disk.
- **Sweep is opt-in policy, not hardcoded default.** TTL configurable per persona.
- **v0a is plan-only; v0b implements branch_store when branch ops actually land.** No premature primitives.
