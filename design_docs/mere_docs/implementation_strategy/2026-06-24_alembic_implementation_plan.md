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
content-hashed, `verify_integrity` passes), and re-opening it thaws the same graph back.

**Landed 2026-06-24** (commits `af89808`, `4f51175`): `session-runtime/graph_engram` —
`save_graph_engram` / `save_graph_snapshot_engram` / `open_engram_as_session` / `list_graph_engrams`
over the eidetic typed-payload substrate (a `GraphEngram` newtype, orphan rule), `RedactionPolicy`
(conservative default strips thumbnails / favicons / session state). Host: `Command::SaveGraphEngram`
(palette + context menu + `>save_graph_engram`) freezes the focused graph to the private fjall store.
Verified: 6 tests incl. a real fjall save → close → reopen → thaw (survives restart). The thaw lib
primitive (`open_engram_as_session`) is built + tested; surfacing it as a **browse + click-to-open**
gesture lands in slice B (its UI home), so A is the freeze half + the open API, B is the open UX.

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

All seven resolved with Mark 2026-06-24 (his calls in **bold ✓**).

| # | Decision | Resolution |
| --- | --- | --- |
| 7 | Graph-engram **redaction** | **✓ Mark: redact private info, granularly opt fields back in.** Built in slice A: `RedactionPolicy` strips thumbnails / favicons / session state (scroll + form drafts) by default, per-field opt-in via flags; structure + addresses + tags + classifications + provenance always kept. |
| 2 | Promote-in-place vs always-distil | **✓ Mark: tagging adds to long-term memory (retained) — promote-in-place.** A tag (or bookmark) keeps the node in long-term, raw retained; not a capture-to-distillate handoff. Lands in slice C. |
| 1 | **Merge semantics** (compose) | **✓ Mark: agreed — merge-by-identity, layer the context** (`import_provenance` is a `Vec`, so source records coexist); exposed as an Athanor per-compose option. Lands with the compose sub-feature (post-A/D); does not gate save/open. |
| 3 | Settings placement | **Mark asked "Apparatus or Steward? sync or async?" → answered:** Athanor splits by the §8 axis — its **config knobs go in Apparatus** (the at-rest plane: dedupe policy, eviction thresholds, steady-heat schedule), its **live passes surface in Steward** (the in-flight plane), its faults in Apparatus diagnostics. No new pane. It runs **async** — an armillary actor off the UI thread, never synchronous on render. Primary settings home = **Apparatus**. |
| 6 | Timeline surface | **✓ Mark: agreed — an orrery scrubber / time-axis**, not a docked pane (slice E). |
| 5 | Event-log shape | **Mark: scope for implementation here.** Promote from "its own plan" to a scoped slice-E section in this plan (mutation-log granularity, checkpoint cadence, view-intent stream). **Next planning task.** |
| 4 | The lora lane | **Mark: spin off a local-models + harness design doc.** A dedicated doc for the local-models lane (dataset build, LoRA train/apply, the harness / Distillery-as-trainer), referencing the geist models brief. **Spinoff doc to write.** |

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

- 2026-06-24: **All 7 open decisions resolved with Mark** (see §3). Agreed: merge-by-identity (#1),
  promote-in-place — tagging retains into long-term (#2), Timeline = orrery scrubber (#6), redact
  private + granular opt-in (#7, already built in A). Answered #3 (settings): Athanor knobs → Apparatus,
  live passes → Steward, runs async (armillary actor). Two follow-ups Mark requested: **#5** scope the
  event-log for implementation (expand slice E in-plan), and **#4** spin off a dedicated local-models +
  harness design doc (the LoRA / Distillery-trainer lane).
- 2026-06-24: **Slice A landed** (commits `af89808` spine, `4f51175` host gesture + persistence test).
  The freeze/thaw foundation: `session-runtime/graph_engram` over the eidetic typed-payload layer
  (`save_typed` / `load_typed` / `list_typed`), which turned out to be a richer substrate than the plan
  assumed (content-hashed, schema-tagged, integrity-verified, multi-schema — already tested), so the
  binding is a `GraphEngram` newtype + a `RedactionPolicy`, not hand-rolled blob plumbing. Host:
  `Command::SaveGraphEngram` (registry-native — palette, context menu, `>save_graph_engram`). 6 tests,
  incl. a real fjall close+reopen proving "survives restart." Refinement: A delivers the freeze gesture
  together with the `open_engram_as_session` lib primitive; the **thaw-into-the-running-orrery UX**
  (browse, click-to-open) moves to slice B where it has a pane to live in. Chose serde_json over the doc's rkyv (consistent with
  the live `graph.json`, sidesteps the rkyv-from-store alignment gotcha); rkyv compaction stays a noted
  later optimization.
- 2026-06-24: **Plan written; code-verified.** Confirmed the spine is live: `GraphSnapshot` to/from,
  the full `Engram` envelope, and the `eidetic::Store` / `FjallStore` **already opened in meerkat**
  (used for tombstones + the content store), so slice A is a graph-snapshot schema + save/open helpers
  over an existing store, not new infrastructure. Sequenced A (spine) → B (pane) → C (memory levels) →
  D (Athanor) → E (event log / Timeline); resolved the redaction default (#7) that gates A and the
  settings/Timeline-surface decisions (#3, #6), deferred merge / promote-model / event-log / lora to
  their phases. No code yet.
