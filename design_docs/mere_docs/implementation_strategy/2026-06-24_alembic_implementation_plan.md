# Alembic implementation plan: graph engrams, the memory pane, and Athanor

The build plan that realizes the [Alembic memory + engrams architecture](../technical_architecture/2026-06-09_alembic_memory_and_engrams.md)
(the design seed). That doc stays the canonical model; this sequences the build into shippable slices,
resolves or defers its 7 open decisions, and grounds each phase in the code that already exists.

The Alembic is a multi-part subsystem (a 3-level memory model, graph engrams over eidetic, the docked
memory pane, the Athanor distillation daemon, and an event-log / Timeline). It is doc-only today, so
this is from-scratch, but the **spine already exists** and most of the early work is wiring.

## 1. Code reality (verified 2026-06-24)

**Exists:**

- **`GraphSnapshot`** — `Graph::to_snapshot` / `from_snapshot`
  ([graph-kernel](../../../crates/graph/graph-kernel/src/graph/snapshot/)), rkyv, already what
  session-runtime persists. The freeze/thaw primitive.
- **The `Engram` envelope** — [eidetic-core](../../../crates/eidetic/eidetic-core/src/engram.rs):
  `schema` / `payload` / `content_hash` / `privacy` / `provenance` / `trust` / `bounds` /
  `envelope_version`, with `Engram::new`, `id() -> ManifestId`, `verify_integrity()`. Fully built.
- **The eidetic store, live in the host.** `eidetic::Store` (trait) + `eidetic_fjall::FjallStore`
  (`open(path)`) is **already opened in meerkat** (`main.rs`) and used today for deleted-node
  tombstones (`eidetic::record_deleted` / `list_deleted`, `node_ops.rs`) and the content store
  (`session-runtime/content_store.rs`). So engrams have a live home; the pattern to mirror is the
  `record_deleted` / `list_deleted` helper pair.
- **`armillary`** — the off-UI-thread actor framework (actor / message / pool), the shape Athanor is.
- **The peripheral-pane infra** — `frame::PaneContent` (Steward / Apparatus / Roster / Inspector /
  Gloss …), the list-pane render + scroll + a11y + the `Toggle*` registry commands. The template a
  new Alembic pane follows (Steward is the closest precedent: a docked list pane over a live store).

**Net-new:**

- The `mere.graph-snapshot/v1` schema def + typed `save_graph_engram` / `open_engram_as_session`
  helpers (the §3 freeze/thaw boundary).
- The Alembic pane (`PaneContent::Alembic` is aspirational, not yet in the enum).
- The 3-level memory model: short→long promotion (tag/bookmark), the configurable eviction policy,
  the Recent / Saved data.
- The Athanor daemon (an armillary actor) + its proposal-emission seam.
- The append-only `GraphMutation` event log + the Timeline (mostly net-new; the mutation *vocabulary*
  exists in `app/intents.rs`, the persisted log does not).

## 2. Phases (done-conditions, not dates)

### A — Graph-engram spine (the foundation)

Define `mere.graph-snapshot/v1`; add `save_graph_engram(store, graph, redaction)` (snapshot + redact +
serialize + `Engram::new` + write through the store) and `open_engram_as_session(store, id) -> Graph`
(read + verify + deserialize + `from_snapshot`), mirroring `record_deleted` / `list_deleted`. Wire a
host gesture: "Save graph as engram" (a registry command) and "Open engram" (thaw into a graph /
window, reusing the orrery at engram scope). Browsing is read-only; editing forks a thaw.

Done when a user freezes the focused graph to an engram, the engram persists (survives restart,
content-hashed, `verify_integrity` passes), and re-opening it thaws the same graph into the orrery.

### B — The Alembic pane

Add `PaneContent::Alembic` + a docked list pane (Steward-shaped) with the three sections: **Recent**
(short-term), **Saved** (long-term), **Engrams** (the list from A, click → open in the orrery). A
`ToggleAlembic` registry command + the dock entry. Recent/Saved render from whatever C surfaces (until
C, they show a grounded subset: Recent = the nav/recent set, Saved = bookmarked/tagged nodes). The
context-binding toggle (global vs selection-filtered) per the dock contract.

Done when the Alembic pane opens from the palette / dock, lists engrams, and clicking one opens it.

### C — The three memory levels

Promotion short→long as an affirmative act (tag / bookmark a node promotes it + its group); the
configurable eviction policy (by sessions or by days, user-overridable, never silent); free
demotion/deletion. The Recent (working set, eviction policy visible/editable) and Saved (durable kept
set) sections become real over this model.

Done when bookmarking/tagging promotes a node to Saved durably, the eviction policy is visible +
editable, and Recent evicts per policy.

### D — Athanor, the distillation daemon

An armillary actor (steady-heat: throttled at idle, yields to foreground): forgetting (evict expired
short-term), consolidation (dedup by `content_hash`, relate version chains, maintain indices), facet
extraction (later), and **proposal emission** (it proposes engrams / GC / facets for the host to
apply; it never mutates graph truth or edits existing engrams — eidetic R0). Live passes surface in
Steward; its knobs in Apparatus config.

Done when Athanor runs in the background, evicts eligible short-term, emits consolidation proposals,
and stays inside R0 (no direct graph-truth mutation).

### E — Event log + Timeline (its own scoping)

The append-only `GraphMutation` log + periodic `GraphSnapshot` checkpoints (+ a view-intent stream),
giving undo/redo (single-step) and the Timeline (scrub to T, replay). Largest + most net-new; spun to
its own plan when reached. "Timeline → engram" is the handoff to A (distil a past state).

## 3. Open decisions, resolved or deferred

| # | Decision | Resolution |
| --- | --- | --- |
| 7 | Graph-engram **redaction** | **Resolve now (gates A):** `save_graph_engram` strips private / heavy `GraphSnapshot` fields by default (favicon bytes, session scroll, form drafts, thumbnails, raw metadata), keeping structure + addresses + tags + classifications + provenance; an explicit policy arg opts fields back in. |
| 2 | Promote-in-place vs always-distil | **Defer to C; default lean = promote-in-place** (a retain-flagged pool; tag/bookmark keeps the node long-term, raw retained), matching "tagging promotes." Revisit when facet extraction lands. |
| 1 | **Merge semantics** (compose) | **Defer to the compose sub-feature (post-A/D); default = merge-by-identity, layer the context** (`import_provenance` is a `Vec`, so multiple source records coexist); exposed as an Athanor per-compose option. Does not gate basic save/open. |
| 3 | Settings placement | **Resolved:** Athanor knobs live in **Apparatus config** (the §8 lean), not a new pane. |
| 6 | Timeline surface | **Resolved:** an **orrery scrubber / time-axis**, not a docked pane (slice E). |
| 5 | Event-log shape | **Defer to E** (granularity + checkpoint cadence + whether view-intent shares the log). |
| 4 | The lora lane | **Out of scope** here; its own future plan (the geist / Distillery training lane). |

## 4. Slice A detail (the first build)

1. **Schema** — register `mere.graph-snapshot/v1` via eidetic's `schema_def`; payload = `rkyv(GraphSnapshot)`.
2. **`save_graph_engram(store, graph, redaction) -> Result<ManifestId>`** — `to_snapshot`, apply the
   redaction default (§3 #7), serialize, `Engram::new` with the schema + privacy (default local) +
   provenance + bounds, write through the store (mirror `record_deleted`). Lives where the eidetic
   helpers do, consumed by the host.
3. **`open_engram_as_session(store, id) -> Result<Graph>`** — load by id, `verify_integrity`,
   deserialize, `from_snapshot`. The host opens it as a new graph in the orrery at engram scope.
4. **Host gestures** — a `SaveGraphEngram` + `OpenEngram` registry command (palette / `>` verb), and
   the `>scene`-style wiring. (No pane yet; B adds the browse surface. A is verifiable headless + via
   the command echo: save, restart, open, the graph round-trips.)

Verification: a unit/integration test that `save → (drop) → open` round-trips a `Graph` through a temp
`FjallStore` with `verify_integrity` passing, plus a headed save/reopen once the command lands.

## Cross-references

- [Alembic memory + engrams](../technical_architecture/2026-06-09_alembic_memory_and_engrams.md) — the architecture seed this realizes.
- [peripheral panes architecture](../technical_architecture/2026-06-06_peripheral_panes_architecture.md) — the dock the Alembic pane (B) joins; the Apparatus/Steward axis (D's home).
- [memory tiers brief](../research/2026-05-11_memory_tiers_brief.md) — the prior two-tier model the seed refined to three levels (C).
- [statement kernel brief](../technical_architecture/2026-06-19_statement_kernel_brief.md) — short/long-term vs engram framing.
- eidetic ([eidetic-core](../../../crates/eidetic/eidetic-core/) + [eidetic-fjall](../../../crates/eidetic/eidetic-fjall/)), [armillary](../../../crates/armillary/), [graph-kernel snapshot](../../../crates/graph/graph-kernel/src/graph/snapshot/).

## Progress

- 2026-06-24: **Plan written; code-verified.** Confirmed the spine is live: `GraphSnapshot` to/from,
  the full `Engram` envelope, and the `eidetic::Store` / `FjallStore` **already opened in meerkat**
  (used for tombstones + the content store), so slice A is a graph-snapshot schema + save/open helpers
  over an existing store, not new infrastructure. Sequenced A (spine) → B (pane) → C (memory levels) →
  D (Athanor) → E (event log / Timeline); resolved the redaction default (#7) that gates A and the
  settings/Timeline-surface decisions (#3, #6), deferred merge / promote-model / event-log / lora to
  their phases. No code yet.
