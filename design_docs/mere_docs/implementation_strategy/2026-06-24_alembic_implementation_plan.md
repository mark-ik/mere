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

**Spine landed 2026-06-24** (commit `9cacd41`): `session-runtime/memory_levels` is the pure read-model
(built spine-first, before the pane/settings wiring, because the chrome files are mid-churn under Mark's
shellbar work). `MemoryLevel` + `level_of`/`is_promoted` (a tag or pin promotes to long-term, per
decision #2); `EvictionPolicy` (KeepForever | KeepDays(n), default 30, with a visible `describe()`);
`evictable_short_term` (the ids a policy drops now, given host-supplied last-visit timing, only
short-term + dated-and-stale, promoted exempt, undated never dropped); `census` for real counts. 5
tests. Remaining (the **wiring**, deferred until the chrome churn settles): render Recent/Saved over
this model in `pane_data::alembic_items` (replacing B1's grounded proxies), surface + edit the policy
(persona settings, not the shellbar-dirty `settings_store`), a promote (keep) action from Recent rows
and demote from Saved, and the eviction pass itself (drop short-term content via `content_store`, the
forgetting Athanor/slice D will later run continuously). The **by-sessions** policy is deferred behind
a per-node last-session stamp the snapshot does not yet carry.

### D — Athanor, the distillation daemon

An armillary actor (steady-heat: throttled at idle, yields to foreground): forgetting (evict expired
short-term), consolidation (dedup by `content_hash`, relate version chains, maintain indices), facet
extraction (later), and **proposal emission** (it proposes engrams / GC / facets for the host to
apply; it never mutates graph truth or edits existing engrams — eidetic R0). Live passes surface in
Steward; its knobs in Apparatus config.

Done when Athanor runs in the background, evicts eligible short-term, emits consolidation proposals,
and stays inside R0 (no direct graph-truth mutation).

### E — Event log + Timeline (scoped for implementation, decision #5)

The substrate undo/redo and the Timeline both ride. One append-only log of graph mutations, two
projections (eidetic R0): the **Alembic** current fold (slice C's live nodes / facets) and the
**Timeline** historical replay. This log is local short/long-term memory, not engrams (no
content-addressing, distillation, or federation); E5 ("Timeline to engram") is the one place a chosen
past state crosses into slice A.

**Code reality (verified 2026-06-24).** Exists: `GraphSnapshot` to/from (the checkpoint primitive);
`kernel::persistence::NodeAuditEventKind` (a per-node event taxonomy: TitleChanged / Tagged / Pinned /
Tombstoned / Restored / ..., serde + rkyv) as a partial mutation vocabulary; the **proven
event-sourcing substrate** in `tessera` ([`moothold/src/tessera/log_store.rs`](../../../crates/moot/moothold/src/tessera/log_store.rs),
a `LogStore` trait impl) and `cable` (`murmuring/src/cable/{log_store,persistent_store}.rs`) to mirror;
the kernel graph mutators (`Graph::add_node` / `add_node_with_id` / `remove_node` + edge / field
methods) as the capture points; the `ViewIntent` sidecar (`session-runtime/view_intent_store.rs`:
`CameraSnapshot` + `HiddenRelationRecord`) as the seed for the view-intent stream;
`SharedNavigationMemory` (`graph/history.rs`), which is per-node **browse** history (where each node
went) and already snapshot-persisted, distinct from this **structural** log (how the graph changed).
Net-new: the `GraphMutation` event type, the persisted append-only log, the recording hook, the
view-intent stream (vs the single current sidecar), undo/redo, and the Timeline replay + scrubber.

**Sub-phases:**

- **E1 (the event type + log store).** A kernel `GraphMutation` enum (NodeSpawned {id, url, pos} /
  NodeRemoved / NodeMoved / EdgeAsserted {from, to, kind} / EdgeRetracted / FieldChanged, plus a
  `MetadataChanged(NodeAuditEventKind)` arm reusing the existing taxonomy), serde + rkyv like the rest
  of `persistence.rs`. A local append-only `GraphMutationLog` mirroring the tessera/cable `log_store`
  shape (append, iterate-from, truncate-after for redo invalidation), persisted as a per-session
  sidecar beside `graph.json`. Local memory, never an eidetic engram (R0).
- **E2 (recording + checkpoints).** Capture each mutation at one chokepoint (wrap the kernel mutators,
  or record at the host apply point so contributions and user gestures both flow through it; pick the
  single seam so nothing is missed). Interleave periodic `GraphSnapshot` checkpoints (the existing
  `session_graph_store::save`): cadence = every N mutations or on idle/close, whichever comes first, so
  replay is "load nearest checkpoint, replay events to T."
- **E3 (replay + the Timeline scrubber).** Fold to the nearest checkpoint, replay forward to time T.
  Surface = an **orrery scrubber / time-axis** (decision #6), not a docked pane; it replays over the
  live orrery (spawns, moves, projections playing out). Read-only: scrubbing never mutates truth.
- **E4 (undo/redo).** The same substrate at single-step granularity: undo reverses the last
  `GraphMutation` (each variant carries its inverse; NodeRemoved keeps the removed node's snapshot for
  re-spawn), redo re-applies; a new mutation after undo truncates the redo tail. Build the log once
  (E1/E2), get both undo/redo and the Timeline.
- **E5 (Timeline to engram).** Scrub to a past incarnation, then `save_graph_engram` that state (slice
  A) to pin it durably and shareably. The one crossing from the local log into the engram store.

**View-intent stream (decision #5 tail, resolved).** Promote the `ViewIntent` sidecar from a single
current state to an append stream of projection changes (camera, arrangement, hidden-relation
toggles), so the Timeline replays *arrangement* too, not only structure. Run it as a **parallel
stream the Timeline composes** (keyed by shared timestamps), not interleaved into the structural log,
so a structural undo and a view change stay independent.

**Open sub-decisions:** checkpoint cadence N (tune by measured replay cost in E3); whether field-layer
and coupling changes are first-class `GraphMutation` arms or a coarse `FieldChanged` (lean coarse
first); how far back replay stays cheap before a checkpoint is mandatory. Large enough that E may spin
to its own plan once C and D land; this section is its implementation spec.

## 3. Open decisions, resolved or deferred

All seven resolved with Mark 2026-06-24 (his calls in **bold ✓**).

| # | Decision | Resolution |
| --- | --- | --- |
| 7 | Graph-engram **redaction** | **✓ Mark: redact private info, granularly opt fields back in.** Built in slice A: `RedactionPolicy` strips thumbnails / favicons / session state (scroll + form drafts) by default, per-field opt-in via flags; structure + addresses + tags + classifications + provenance always kept. |
| 2 | Promote-in-place vs always-distil | **✓ Mark: tagging adds to long-term memory (retained) — promote-in-place.** A tag (or bookmark) keeps the node in long-term, raw retained; not a capture-to-distillate handoff. Lands in slice C. |
| 1 | **Merge semantics** (compose) | **✓ Mark: agreed — merge-by-identity, layer the context** (`import_provenance` is a `Vec`, so source records coexist); exposed as an Athanor per-compose option. Lands with the compose sub-feature (post-A/D); does not gate save/open. |
| 3 | Settings placement | **Mark asked "Apparatus or Steward? sync or async?" → answered:** Athanor splits by the §8 axis — its **config knobs go in Apparatus** (the at-rest plane: dedupe policy, eviction thresholds, steady-heat schedule), its **live passes surface in Steward** (the in-flight plane), its faults in Apparatus diagnostics. No new pane. It runs **async** — an armillary actor off the UI thread, never synchronous on render. Primary settings home = **Apparatus**. |
| 6 | Timeline surface | **✓ Mark: agreed — an orrery scrubber / time-axis**, not a docked pane (slice E). |
| 5 | Event-log shape | **Scoped for implementation (slice E expanded above, 2026-06-24).** A kernel `GraphMutation` event type building on `NodeAuditEventKind`; an append-only log mirroring the proven tessera/cable `LogStore`; checkpoint-interleaved replay (cadence every N mutations or idle/close); the view-intent promoted to a **parallel composed stream**; undo/redo on the same substrate; Timeline = orrery scrubber (#6). E1 type+store → E2 record+checkpoint → E3 replay+scrubber → E4 undo/redo → E5 Timeline-to-engram. |
| 4 | The lora lane | **Spun off 2026-06-24:** [local_models_harness_brief](../research/2026-06-24_local_models_harness_brief.md). The runtime + harness layer the geist brief and the tier-1 research defer: binds the LLM/adapter runtime seam (extend `intel/embed`'s `EmbeddingProvider` to `InferenceProvider` + `AdapterLoader`; Burn-wgpu wasm-reachable, native runtimes behind the seam), the armillary actor harness, the wasm/native split, and a no-training first slice. Architecture stays in the geist brief; marketplace + governance owned elsewhere. |

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

- 2026-06-24: **Slice C pane wiring landed** (commit `44a0dc8`, on top of Mark's stable-uncommitted
  tree, hunk-staged). The Alembic Recent/Saved now render the real model (Recent = recently-visited
  *untagged* short-term, Saved = *tagged* long-term with a count) and the eviction policy is visible
  ("evicting recent memory after 30 day(s)"). Correctness fix carried into the spine: `is_promoted` is
  tags-only — `is_pinned` is a physics position-pin, not a memory-keep (B1 conflated them). Headed-
  verified. Promotion works today via the existing Add-tag gesture (tags persist, so Saved is durable
  across sessions). Tail: in-pane one-click promote/demote, the eviction pass (forget stale short-term
  via `content_store`), and an editable (not just visible) policy in persona settings.
- 2026-06-24: **Slice C spine landed** (commit `9cacd41`): `session-runtime/memory_levels`, the pure
  read-model (level classification + eviction policy + evictable computation + census), 5 tests, built
  spine-first because the chrome/pane files (`pane_data`, `render`, `settings_store`, `menus`) are all
  dirty under Mark's concurrent shellbar work. The wiring (Recent/Saved over the model, the policy
  setting in persona settings, a promote/demote action, the eviction pass over `content_store`) is
  deferred until that churn settles. By-sessions policy deferred behind per-node session stamps.
- 2026-06-24: **Decision #4 spun off** to [local_models_harness_brief](../research/2026-06-24_local_models_harness_brief.md)
  (the local-models runtime + harness lane), DOC_README indexed. Code-grounded: `intel/embed`'s
  `EmbeddingProvider` is the live seam precedent the deferred LLM/adapter runtime extends; eidetic
  `ModelManifest` storage + Burn 0.21 (`aether`/`intel`) + the armillary actor framework are the
  substrate; the gap is the runtime binding + harness + wasm/native split, which the brief scopes
  without re-deriving the geist architecture or the compute marketplace.
- 2026-06-24: **Slice B done + event-log (#5) scoped for implementation.** Slice B landed in two
  commits: B1 (`b029d83`) the Alembic pane (Recent / Saved / Engrams, headed-verified listing a real
  engram), B2 (`7c6e5f4`) a clickable engram row thaws into an Orrery pane beside (headed-verified via
  a ShellCommand routing to `open_engram_beside`; ephemeral + read-only). Then expanded slice E into an
  implementation spec (decision #5): code-verified that `GraphMutation` is net-new but `NodeAuditEventKind`
  is a partial vocab, the **tessera/cable `LogStore`** is the event-sourcing substrate to mirror, the
  kernel `Graph` mutators are the capture seam, `ViewIntent` is the sidecar to grow into a parallel
  composed stream, and `SharedNavigationMemory` is browse-history (distinct from the structural log).
  Sub-phases E1–E5; resolved the #5 tail (view-intent = parallel stream) + the cadence/granularity leans.
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
