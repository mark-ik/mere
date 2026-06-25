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

1. **Shellbar buttons for Alembic + Trail.** **[chrome-hot]** The toolbar/shellbar (`views.rs:~556-562`,
   `btn(glyph, panes.<x>, Command::Toggle<X>)`) covers 7 panes (Workbench/Roster/Gloss/Apparatus/
   Inspector/Steward/Comms) but not Trail or the new **Alembic** pane (B1 left it palette/`>alembic`-only).
   Add a `btn(...)` for each + the matching `panes.alembic` / `panes.trail` bool (the per-pane flags are
   built in `render.rs:~324` as `panes.steward: self.pane_of_content(&PaneContent::Steward).is_some()`).
   The `ToggleAlembic` / `ToggleTrail` commands already exist. Small + high-value discoverability.

2. **Steward rows clickable.** Steward's retry/stop/pin are verbs that exist in `node_ops.rs`
   (`retry_focused` / `stop` / `pin_focused_operation`) but are only reachable by typing
   `retry.focused` etc. Today `pane_data.rs::steward_rows` returns display-only `(label, value)` pairs
   rendered via `utility_panes::utility_pane_items` (inert text). Make the action rows clickable
   `PaneItem::button(class, text, key)` and route the keys in the list-pane activation drain
   (`input.rs::drain_list_pane_activations`, the `ShellListPane::Steward` arm — currently Steward queues
   nothing) to the existing verbs. This is the exact pattern the Alembic engram-open + forget rows use.
   The no-placebo / real-feedback rule applies. **[chrome-hot]** (touches `pane_data` + the render path).

3. **Relation-kind picker (the audit's top pick).** **[chrome-hot]** The 17-variant edge vocabulary
   (`pane_data.rs::relation_kind_label`, the `RelationKind` map) is only reachable by typing
   `relate("cites")`; every click-drawn edge is undifferentiated `UserGrouped`. Add a submenu over the
   existing map in the context menu (`menus.rs`) so `AssertEdge` can pick a kind. The kind→`SemanticSubKind`
   mapping already exists in `command_drain.rs::relation_kind_from_str`. "Makes a graph browser worth
   using" per the audit.

4. **Omnibar suggestions.** Live `>`-verb command shell already works; the weak spot is suggestions
   (`suggest.rs`): substring-only, no frecency / favicon / page-title ranking. A ranking pass over the
   existing `OmnibarMatch` generation. Self-contained.

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

2. **Surface the forget result in Steward.** `run_forgetting_pass` records the count via
   `record_diagnostic`, which lands in the **Apparatus** diagnostics buffer; the doc (§8) wants live
   passes in **Steward**'s live-ops view. Add a Steward row (a "last forgetting pass" line, or a tracked
   op). **[chrome-hot]** (Steward rows are `pane_data::steward_rows`; pairs with item A2).

3. **In-pane one-click promote/demote (slice C).** Promotion works today via the existing Add-tag
   gesture (tags persist → Saved is durable). A one-click "keep" from a Recent row / "release" from a
   Saved row would be nicer. Primitives exist: `Graph::insert_node_tag` / `remove_node_tag`,
   `get_node_by_url`. **Open decision first:** is "keep" a magic `saved` tag, or a dedicated bookmark
   flag on the node (a kernel schema add)? `is_pinned` is a physics position-pin, not a keep, so it is
   not the answer. Decide before wiring. **[chrome-hot]** (`pane_data` + `input` drain).

4. **Editable eviction policy (slice C).** The policy is *visible* (`memory_levels::EvictionPolicy`,
   default `KeepDays(30)`, shown in the Recent header) but not yet *editable*. Persist + edit it in
   **persona settings** (`session-runtime/persona_settings_store`, **not** `settings_store` which has
   been Mark's hot file). A cycle/control in the Recent header or the settings lane.

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
