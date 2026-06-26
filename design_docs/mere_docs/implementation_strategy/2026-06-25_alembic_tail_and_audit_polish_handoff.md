# Handoff: Alembic tail + chrome/audit polish

**Date**: 2026-06-25
**Status**: Handoff backlog for another agent. Each item is scoped with its files, the change, and any
gotcha or blocker. Grouped by readiness. The Alembic core (slices A, B, C, and D-forgetting) shipped
2026-06-24; this is the deferred tail plus the pane/omnibar audit items.

**Cross-cutting gotcha (read first).** Many chrome items below touch files Mark is editing concurrently
(his shellbar + graph-signals work has `views.rs`, `render.rs`, `menus.rs`, `pane_data.rs`,
`command.rs`, `command_drain.rs`, `lib.rs`, `main.rs`, plus `graph-kernel`, `orrery`, `intel` dirty).
Commit with explicit pathspec or per-hunk `git apply --cached` so a bare commit never sweeps his
uncommitted work in. Verify the tree builds before starting (it has been building on his stable state).
The items marked **[chrome-hot]** should wait until his chrome work commits, or be done with care.

**Related:** [Alembic implementation plan](2026-06-24_alembic_implementation_plan.md) (slices + the full
decision table), [local_models_harness_brief](../research/2026-06-24_local_models_harness_brief.md) (the #4 lane).

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

1. **Athanor's steady-heat actor (slice D's remainder).** Forgetting *logic* shipped
   (`session-runtime/athanor.rs` propose/apply, run from the Alembic "forget stale recent now" row via
   `node_ops::run_forgetting_pass`). What remains is the background **armillary actor** that schedules it
   steady-heat (throttled at idle, yields to foreground), vs today's manual trigger. Model it on the
   fetch/sync actor shape (`crates/armillary` + the constellation). Add **consolidation / facet** passes
   (consolidation is light: graph engrams already dedup by content-addressing; the value is relating
   version chains). Stays inside R0 (proposes; host applies).

2. **Surface the forget result in Steward.** ✅ **DONE 2026-06-25.** `HostObservability` now keeps a
   structured `ForgettingPass { dropped, at }` (the last pass), set by `record_forgetting_pass` which
   `run_forgetting_pass` calls in both branches (dropped 0 when nothing was stale, so a no-op pass still
   shows it ran). `steward_rows` gained a live "Last forgetting" row — `dropped N page(s) · Ns ago`
   (reusing `observability::age`) or `not run yet`. Complements the existing `alembic.forget` diagnostic
   in the Apparatus log rather than replacing it. New test `steward_surfaces_a_recorded_forgetting_pass`
   pins both the not-run and the recorded-count states. (Built on the A2 Steward work.)

3. **In-pane one-click promote/demote (slice C).** Promotion works today via the existing Add-tag
   gesture (tags persist → Saved is durable). A one-click "keep" from a Recent row / "release" from a
   Saved row would be nicer. Primitives exist: `Graph::insert_node_tag` / `remove_node_tag`,
   `get_node_by_url`. **Open decision first:** is "keep" a magic `saved` tag, or a dedicated bookmark
   flag on the node (a kernel schema add)? `is_pinned` is a physics position-pin, not a keep, so it is
   not the answer. Decide before wiring. **[chrome-hot]** (`pane_data` + `input` drain).

4. **Editable eviction policy (slice C).** ✅ **DONE 2026-06-25.** `EvictionPolicy` gained serde +
   `cycled()` (the `7d → 30d → 90d → forever` ladder); `PersonaSettings` carries an `eviction_policy`
   field (serde-defaulted, so older `ui.json` still loads). The host loads it into `Presentation` at boot
   (`main.rs`), the Alembic **Recent header** row is now a clickable button (`↻`) that cycles + persists
   it (`pane_data.rs` row → `input.rs` `alembic:eviction:cycle` drain → `frame_ops::cycle_eviction_policy`),
   and **`run_forgetting_pass` now reads the persisted policy** instead of the hardcoded default, so
   editing actually changes what gets forgotten. Persisted in persona settings (not `settings_store`).
   Tests: `eviction_policy_cycles_through_the_ladder` + the persona round-trip now covers the policy.

5. **By-sessions eviction policy (slice C).** `EvictionPolicy` ships `KeepForever` + `KeepDays(n)`. The
   doc also wants "drop after N sessions", which needs a per-node last-session stamp the `GraphSnapshot`
   does not carry yet. Add the session counter + per-node stamp, then a `KeepSessions(n)` arm in
   `memory_levels`. Tracked, not a no-op stub.

6. **rkyv compaction of graph engrams (slice A optimization).** `save_graph_engram` uses serde_json
   (consistent with the live `graph.json`, and it sidesteps the rkyv-from-store alignment gotcha). The
   architecture doc prefers rkyv for compactness. Override `TypedPayload::serialize_to_bytes` for
   `GraphEngram` with rkyv (handle the read-alignment: copy store bytes into an `AlignedVec` before
   `access`). Measure the size win first; only worth it if engrams get large.

7. **Engram compose / merge (decision #1).** Union two graph engrams retaining per-member provenance,
   as an Athanor per-compose option (default: merge-by-identity, layer the context, since
   `import_provenance` is a `Vec`). Post-A/D; the spine (`graph_engram`) + the schema support exist.

---

## C. Larger spun-out work (own plans, not quick items)

1. **Event log + Timeline (slice E).** Scoped to an implementation spec in the Alembic plan (§E:
   `GraphMutation` type building on `NodeAuditEventKind`, an append-only log mirroring the tessera/cable
   `LogStore`, checkpoint-interleaved replay, undo/redo, the orrery scrubber). Large; likely its own
   plan when picked up.

2. **Local-models inference/training harness (#4).** The [local_models_harness_brief](../research/2026-06-24_local_models_harness_brief.md)
   scopes it: an `InferenceProvider` + `AdapterLoader` seam beside `intel/embed`'s `EmbeddingProvider`,
   Burn-wgpu wasm-reachable, native runtimes behind the seam, the armillary harness, a no-training first
   slice (seam + stub → RAG shim → `ModelAdapterManifest` save/load). Its own build effort.

---

## Progress

- 2026-06-25: Handoff drafted to surface the Alembic tail + the chrome/audit polish for another agent,
  after the A–D core landed. Verified the audit items against the code (recover-deleted is already wired;
  the shellbar gap now includes the new Alembic pane). Flagged the chrome-hot collision set.
