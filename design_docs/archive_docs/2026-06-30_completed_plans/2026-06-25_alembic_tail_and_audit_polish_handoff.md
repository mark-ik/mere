# Handoff: Alembic tail + chrome/audit polish

**Date**: 2026-06-25
**Status**: **Archived 2026-06-30 — every quick item done.** Section A (all 4) and section B (all 7,
counting Athanor P1 and engram-compose P1-P3 as done) are complete. The two items that don't fold into
"done": B6 (rkyv compaction) is parked in
[engram_compose_merge_plan §Deferred follow-on](../../mere_docs/implementation_strategy/2026-06-25_engram_compose_merge_plan.md#deferred-follow-on-not-this-plan-parked-here--touches-the-same-file);
section C (event log/Timeline, local-models harness) was already correctly deferred to its own future
plans and needed no spin-out — both bullets already point at the docs that actually scope them
([alembic_implementation_plan](../../mere_docs/implementation_strategy/2026-06-24_alembic_implementation_plan.md) §E,
[local_models_harness_brief](../../mere_docs/research/2026-06-24_local_models_harness_brief.md)).

**Cross-cutting gotcha (read first).** Many chrome items below touch files Mark is editing concurrently
(his shellbar + graph-signals work has `views.rs`, `render.rs`, `menus.rs`, `pane_data.rs`,
`command.rs`, `command_drain.rs`, `lib.rs`, `main.rs`, plus `graph-kernel`, `orrery`, `intel` dirty).
Commit with explicit pathspec or per-hunk `git apply --cached` so a bare commit never sweeps his
uncommitted work in. Verify the tree builds before starting (it has been building on his stable state).
The items marked **[chrome-hot]** should wait until his chrome work commits, or be done with care.

**Related:** [Alembic implementation plan](../../mere_docs/implementation_strategy/2026-06-24_alembic_implementation_plan.md) (slices + the full
decision table), [local_models_harness_brief](../../mere_docs/research/2026-06-24_local_models_harness_brief.md) (the #4 lane).

---

## A. Chrome / pane audit polish

From the 2026-06-24 audit, verified against the code. One audit item was already done:
**recover-deleted-node is wired** (`pane_data.rs:~211` Trail-Removed `recover:` buttons →
`input.rs:~1258` drain → `node_ops.rs:~67` `recover_deleted_node`) — no work needed.

1. **Shellbar buttons for Alembic + Trail.** ✅ **DONE 2026-06-25.** Added a Trail (`⇝`, `U+21DD`) and an
   Alembic (`⚗`, `U+2697`) button to the shellbar strip (`views.rs::shellbar_view`), grouped after Gloss
   as the content/memory cluster; wired the matching `trail` / `alembic` bools into `ShellbarPaneStates`
   (`lib.rs`) and its per-frame construction (`render.rs`). The `ToggleAlembic` / `ToggleTrail` commands
   already existed. The exact-count toolbar test (`tests.rs`) was bumped 11→13 (now 9 shellbar buttons);
   the `>= 7` shellbar harness assertion still holds. Shellbar now covers all 9 toggle-able panes.

2. **Steward rows clickable.** ✅ **DONE 2026-06-25.** Added a bespoke `pane_data::steward_items() ->
   Vec<PaneItem>` (mirroring `alembic_items`): the live-ops status rows as text, then three real action
   buttons — `↻ retry focused` / `⏹ stop focused` / `⚓ pin focused (background)` — keyed `steward:retry`
   / `steward:stop` / `steward:pin`. `render.rs` now builds Steward from `steward_items()` instead of the
   inert `utility_pane_items` path, and `input.rs::drain_list_pane_activations` gained a `Steward` arm
   routing those keys to the existing `retry_focused_content` / `stop_focused_operation` /
   `pin_focused_operation` verbs. The stale inert "Actions" hint row was dropped (buttons replace it).
   New test `steward_exposes_clickable_action_verbs` pins the keys; the live-graph-count test stays green.

3. **Relation-kind picker (the audit's top pick).** ✅ **DONE 2026-06-25.** A two-node selection's
   context menu now lists a "Relate as <kind>" row per curated semantic relation (Cites / Quotes /
   Summarizes / Elaborates / Example of / Supports / Contradicts / Questions / Same entity as / Duplicate
   of / Hyperlink) — `menus.rs::relate_picker_items`, extended into `build_curated_menu_items` when
   `len == 2`. Each row carries a new `ContextAction::RelateAs(SemanticSubKind)` (Copy-preserving) that
   the menu drain routes to `assert_selected_relation(kind)`, mirroring the plain `Relate` (still
   `UserGrouped`). **Presentation note:** `ContextItem` has no nested-submenu support (the "layout
   submenu" is flat appended rows), so the picker is a flat row group like `layout_picker_items` — a
   true nested submenu would be a disproportionate new UI subsystem. New test pins the rows + actions.

4. **Omnibar suggestions.** ✅ **DONE 2026-06-25.** `suggest.rs` now ranks history rows by a
   `(match-quality tier, frecency)` order instead of raw substring/most-recent: host matches
   (exact/prefix/substring, leading `www.` ignored) outrank incidental path/query matches, and within a
   tier visit-frequency-with-recency-tiebreak leads. Self-contained, no data-model change, 4 new tests
   pin the contracts. **Deferred (data-blocked):** favicon + page-title ranking need `History` to carry
   per-visit metadata (it is a back/forward URL stack today, no titles/favicons/timestamps); threading
   that in touches the hot `visit()` call sites (`lib.rs` / `input.rs`), so it is its own item, not part
   of this pass.

**Out of scope (knowing, not doing):**

- **Browser-bar basics** (Ctrl+L focus, Ctrl+F find, reload/stop/zoom, bookmarks) all trace to one
  decision: pages are baked into a single full-height GPU texture with no retained text model. L-effort
  and coupled; it is one knot, not five small fixes. Not a bar refinement.
- **Tile pane** (`PaneContent::Tile`) is the genuinely-unbuilt-but-intended pane (pinned-tile / split
  view, decision recorded 2026-06-16). `Custom` (extension seam) and `System` (micro-diagnostics) are
  kept-on-purpose stubs. The broader gap is *depth* in existing panes (Inspector devtools, Trail lineage)
  more than missing kinds.

---

## B. Alembic tail (deferred from the shipped slices)

1. **Athanor's steady-heat actor (slice D's remainder).** ✅ **P1 DONE 2026-06-30** (host-side idle
   cadence, no actor thread — forgetting is light enough to ride the existing `about_to_wait` tick).
   P2 (consolidation) and P3 (the off-thread armillary actor for heavier passes) remain, gated as
   documented in the spun-out plan. See
   [athanor_steady_heat_actor_plan](../../mere_docs/implementation_strategy/2026-06-25_athanor_steady_heat_actor_plan.md).

2. **Surface the forget result in Steward.** ✅ **DONE 2026-06-25.** `HostObservability` now keeps a
   structured `ForgettingPass { dropped, at }` (the last pass), set by `record_forgetting_pass` which
   `run_forgetting_pass` calls in both branches (dropped 0 when nothing was stale, so a no-op pass still
   shows it ran). `steward_rows` gained a live "Last forgetting" row — `dropped N page(s) · Ns ago`
   (reusing `observability::age`) or `not run yet`. Complements the existing `alembic.forget` diagnostic
   in the Apparatus log rather than replacing it. New test `steward_surfaces_a_recorded_forgetting_pass`
   pins both the not-run and the recorded-count states. (Built on the A2 Steward work.)

3. **In-pane one-click promote/demote (slice C).** ✅ **DONE 2026-06-25.** **Decision: magic `saved`
   tag** (Mark, 2026-06-25 — "a bookmark is just a special case of tag anyway"; consistent with decision
   #2, tagging is the promotion act, and `is_pinned` is a physics pin not a keep). Wired: the orrery
   gained `tag_node_by_url` / `untag_node_by_url` (by-url variants of `tag_selected`); meerkat
   `keep_node` / `release_node` (`node_ops.rs`) add/remove the reserved `saved` tag + `save_session`.
   Alembic **Recent** rows are now `☆` keep buttons (`alembic:keep:<url>`) and **Saved** rows `★` release
   buttons (`alembic:release:<url>`), routed in the Alembic drain (`input.rs`). Release only touches the
   `saved` tag, so a node kept via a user tag stays Saved. New test `keep_and_release_toggle_the_saved_tag`.

4. **Editable eviction policy (slice C).** ✅ **DONE 2026-06-25.** `EvictionPolicy` gained serde +
   `cycled()` (the `7d → 30d → 90d → forever` ladder); `PersonaSettings` carries an `eviction_policy`
   field (serde-defaulted, so older `ui.json` still loads). The host loads it into `Presentation` at boot
   (`main.rs`), the Alembic **Recent header** row is now a clickable button (`↻`) that cycles + persists
   it (`pane_data.rs` row → `input.rs` `alembic:eviction:cycle` drain → `frame_ops::cycle_eviction_policy`),
   and **`run_forgetting_pass` now reads the persisted policy** instead of the hardcoded default, so
   editing actually changes what gets forgotten. Persisted in persona settings (not `settings_store`).
   Tests: `eviction_policy_cycles_through_the_ladder` + the persona round-trip now covers the policy.

5. **By-sessions eviction policy (slice C).** ✅ **DONE 2026-06-30.** `PersonaSettings.session_count`
   increments once at boot; `Graph::navigate_node` stamps each in-place-navigated node's new
   `last_session_visited` field from it (`0` = never stamped, never evicted). `EvictionPolicy::KeepSessions(n)`
   added, with `is_stale` branching by axis (time vs session) instead of a single cutoff dispatch. Not
   wired into the Alembic Recent header's cycle button yet (a chrome decision left for whoever picks up
   that surface). New tests in `memory_levels.rs` / `athanor.rs` / `persona_settings_store.rs`.

6. **rkyv compaction of graph engrams (slice A optimization).** Still open; tracked in
   [engram_compose_merge_plan §Deferred follow-on](../../mere_docs/implementation_strategy/2026-06-25_engram_compose_merge_plan.md#deferred-follow-on-not-this-plan-parked-here--touches-the-same-file)
   now that this handoff is archived.

7. **Engram compose / merge (decision #1).** ✅ **P1+P2+P3 DONE 2026-06-30.** Audited green and built
   snapshot-level (no kernel edit — `PersistedEdge` is `node_id`-keyed): `snapshot_merge::merge_snapshots`
   unions two `GraphSnapshot`s by URL identity (A canonical: same-url nodes layer tags/properties, B's id
   remapped; edges remapped + deduped by endpoints+kind; `import_records` unioned; A's
   fields/couplings/navigation kept, B's dropped + reported). `graph_engram::compose_graph_engrams` thaws +
   folds the sources and saves the union as a **`Derived`** engram whose `ProvenanceRecord.upstream` records
   the source ids — the previously-always-empty lineage finally populated (which unblocks Athanor
   consolidation, B1-P2). P3 shipped as a direct host action (no `ShellCommand`, no Athanor propose/apply —
   compose only touches the store, and the user already names both ids, so neither indirection earns its
   keep): the `>compose_engrams("<id-a>", "<id-b>")` omnibar verb and an Alembic Engrams two-select gesture.
   See [engram_compose_merge_plan](../../mere_docs/implementation_strategy/2026-06-25_engram_compose_merge_plan.md).

---

## C. Larger spun-out work (own plans, not quick items)

1. **Event log + Timeline (slice E).** Scoped to an implementation spec in the Alembic plan (§E:
   `GraphMutation` type building on `NodeAuditEventKind`, an append-only log mirroring the tessera/cable
   `LogStore`, checkpoint-interleaved replay, undo/redo, the orrery scrubber). Large; likely its own
   plan when picked up.

2. **Local-models inference/training harness (#4).** The [local_models_harness_brief](../../mere_docs/research/2026-06-24_local_models_harness_brief.md)
   scopes it: an `InferenceProvider` + `AdapterLoader` seam beside `intel/embed`'s `EmbeddingProvider`,
   Burn-wgpu wasm-reachable, native runtimes behind the seam, the armillary harness, a no-training first
   slice (seam + stub → RAG shim → `ModelAdapterManifest` save/load). Its own build effort.

---

## Progress

- 2026-06-25: Handoff drafted to surface the Alembic tail + the chrome/audit polish for another agent,
  after the A–D core landed. Verified the audit items against the code (recover-deleted is already wired;
  the shellbar gap now includes the new Alembic pane). Flagged the chrome-hot collision set.
- 2026-06-30: **Closed out and archived.** B1 (Athanor idle cadence, P1), B5 (by-sessions eviction), and
  B7 (engram compose, P3) landed — each in its own worktree, tested, merged to main with a clean rebuild
  verification after each merge. B6 parked in engram_compose_merge_plan rather than left orphaned here.
  Moved to `archive_docs/` per `DOC_POLICY.md` §4/§8 (every item is either done or already anchored in
  its own scoping doc).
