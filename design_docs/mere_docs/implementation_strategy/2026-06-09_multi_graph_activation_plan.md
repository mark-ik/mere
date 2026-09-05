# Multi-Graph Activation Plan

**Date**: 2026-06-09
**Status**: In progress. MG1–MG5 done plus the host text path — meerkat runs
multi-graph with a window-scoped pane layout (near-Model-B): the shellbar
switcher creates / switches / closes / **renames** graphs (labelled tiles), and
switching keeps the panes while re-sourcing the graph-bound ones. MG6's **far-B
(different-graph leaves coexisting) and multi-window tear-out are now delivered /
owned by the [tearout_composability_plan](../../archive_docs/2026-07-04_completed_plans/2026-06-19_tearout_composability_plan.md)** (its
P1 explicitly converges far-B / MG6, and `OpenGraphBeside` summons a second Orrery
pane, `session_ops.rs:470`). What remains uniquely here: the **persona chip** (gated
on multi-persona) and **per-session engine-profile escalation** (manifest field
present, unwired).
**Related**: [shellbar plan F2.3](2026-06-09_shellbar_plan.md), [graph session manifest plan](2026-05-11_graph_session_manifest_plan.md), [switcher thumbnails plan](2026-05-14_switcher_thumbnails_plan.md), [multi-window plan](2026-06-10_multi_window_plan.md), [peripheral panes architecture](../technical_architecture/2026-06-06_peripheral_panes_architecture.md) (panes are per-window), [composition spine](../technical_architecture/2026-05-21_mere_composition_spine.md). Code: `crates/system/session-runtime/`, `crates/meerkat/`, `crates/shell/frame/`.

> **Historical/supersession note (2026-09-05):** This is the Meerkat activation
> record. Its MG evidence remains useful context; the current multi-window
> direction is the [one-state, n-windows design](../design/2026-07-05_one_state_n_windows_design.md).

Give one window many graphs, switchable from the shellbar. The destination is
**Model B** (the window holds the panes, the graph is the content that flows
through the graph-bound ones); **Model A** (a session owns its whole content band)
is the working checkpoint reached first. F2.3's shellbar switcher is the surface
over this; the persona chip is out of scope (gated on multi-persona).

---

## Findings

**Almost the entire substrate is built and unconsumed by meerkat.**

- `GraphSessionManifest` + `ManifestStore` ([manifest.rs](../../../crates/system/session-runtime/src/manifest.rs), [manifest_store.rs](../../../crates/system/session-runtime/src/manifest_store.rs)) are complete: per-session `<sessions_dir>/<session_id>/manifest.json`, `load_from_disk` / `insert` / `update` / `flush_dirty` (atomic temp-then-rename) / `move_to_trash` (kill into `.trash`). The module doc lists the exact host lifecycle (load at start, insert on create, mark_dirty on mutation, flush debounced + on exit, trash on kill). The manifest already carries `root_graph_id`, `sub_graph_refs`, `display_name`, `persona_id`, `parent_session` (fork), `consolidated_engrams`, `engine_profile`, `policy`, `storage_path`.
- `switcher_thumbnail::build_switcher_thumbnail` (the F2.3 row renderer) is built and unconsumed.
- `tearout.rs` exists for the later multi-window arc; `frame` exports `SessionId` + `GraphId`; `PaneNode` leaves already carry a `graph_id` field (the Model-B hook).

**What meerkat does today** is a flat single session: `<data_dir>/mere/{graph.json, frame.json, content/, views/}`, one `orrery`, every leaf `GraphId::default()`. No `ManifestStore`, no `sessions/` layout, no registry, no switch, no create. The "registry + Cmd-N = new graph" from the older panel-architecture memory predates the meerkat host flip and is **not** in current meerkat; only the `leaf.graph_id` scaffold survived.

So the gap is purely the runtime layer: a session registry, an active session, switch/create/close, and per-session storage. The data types and persistence are done.

---

## The model: A is a checkpoint, B is the goal

A "session" is a graph-shaped unit of work. The question is how much of the window
it owns.

**Model A (checkpoint): a session owns its whole content band.**
Session state = {graph, camera, frame_layout, view-intents}. The window owns
{toolbar, shellbar, comms, the active-session pointer}. Switching swaps the entire
content band wholesale and rebuilds the one active orrery from the target graph.
`leaf.graph_id` is just a tag. Inactive sessions are cold on disk. This is the
point at which multi-graph *works* (create / switch / persist), with the simplest
possible switch semantics.

**Model B (goal): the window owns the panes, the graph is the content.**
The window's `frame_layout` persists across switches. Leaves carry a real
`graph_id`. Graph-bound leaves (orrery, roster, gloss, the Inspector's node face)
re-source to the active graph on switch; window-chrome leaves (Apparatus, Steward,
Comms) stay put. Near-B re-sources *all* graph-bound leaves to one active graph;
far-B lets leaves of different `graph_id` coexist in one window (an orrery of graph
X beside a roster of graph Y), which is also what multi-window tear-out composes
with. This is the panel-graph-architecture intent ("panels carry graph_id,
summonable from shellbar, tearable to new windows").

**A → B** is a focused move: lift `frame_layout` from session-scoped to
window-scoped, give leaves a real `graph_id` instead of `default()`, and replace
"swap the whole band" with "re-source the graph-bound leaves to the active graph."
Everything else (registry, storage, switcher, create/close) is shared between A and
B, so building A first is not throwaway.

---

## Phases (done-conditions, not dates)

### MG1 — per-session storage

- Move meerkat's flat dir to `<data_dir>/mere/sessions/<session_id>/{manifest.json, graph.json, frame.json, views/}`. The `ManifestStore` root is `<data_dir>/mere/sessions/`.
- Adopt the host lifecycle: `load_from_disk` at startup; if empty, seed one default session; `flush_dirty` debounced + on exit; `mark_dirty` on graph/frame/camera mutation.
- One-time migration: if a flat `<data_dir>/mere/graph.json` exists with no `sessions/`, mint a `session_id`, move the flat graph/frame/views into `sessions/<id>/`, write its manifest. No graph is lost.
- Content cache stays shared (persona-scoped, `<data_dir>/mere/content/`), matching the manifest's default `EngineProfileBinding::PersonaScoped`; per-session content is a later escalation the manifest already models.

Done when meerkat reads and writes the active graph under `sessions/<id>/` with a manifest, the migration carries an existing graph over, and a fresh install seeds one default session.

### MG2 — registry + active session + switch (reaches Model A)

- `App` holds the `ManifestStore` + `active_session_id` + a `SessionState` (graph, orrery, frame_layout, camera, view-intents) for the active session.
- `switch_session(target)`: persist the current session's state, load the target's graph + frame + camera, rebuild the orrery from the loaded graph, redraw. Inactive sessions stay cold (on disk).

Done when two sessions can be created on disk and switched between, each restoring its own graph + camera + pane layout, persisting across restart.

### MG3 — create / close / rename

- New session: mint `session_id` + `root_graph_id`, empty graph, default single-orrery frame, insert manifest, make active. Bind Cmd-N.
- Close: `move_to_trash` (recoverable); refuse to close the last session (or seed a fresh default).
- Rename: set `display_name`; absent name derives from graph contents per the manifest's rule.

Done when create / close / rename work end to end and survive restart, with the trash recoverable on disk.

### MG4 — the F2.3 shellbar switcher

- Bottom-anchored strip in the shellbar: one tile per session (`build_switcher_thumbnail` swatch under a short label), active marker, "+" to create, click to switch, "×" to close. Left/Right edges only for now (the Top/Bottom strip is too thin for the vertical stack).
- Rename: a host text-into-scene path (`text::HostText`, parley → `netrender_text`) labels the tiles (display name, else derived from the graph) and powers an inline rename (F2 / right-click a tile → type → Enter). **Done** (host text path).
- Closes F2.3's graph-switcher half; persona chip remains a reserved slot.

Done when the shellbar lists sessions with live thumbnails and drives create / switch / close without keybinds.

### MG5 — transition to Model B (the goal) — **done (near-B)**

- Lift `frame_layout` to window scope; stop swapping it on switch.
- Give leaves a real `graph_id`; mark graph-bound leaf kinds (orrery, roster, gloss, Inspector-node-face) as following the active graph, and window-chrome kinds (Apparatus, Steward, Comms) as graph-independent.
- `switch_session` becomes "set active graph + re-source the graph-bound leaves"; window-chrome leaves persist untouched.

Done when switching graphs keeps the window's pane arrangement and re-sources only the graph-bound panes, with no full content-band swap.

Near-B is reached: all graph-bound leaves follow one active graph (they read the live orrery, so the re-source is automatic; the leaf `graph_id` tags are kept consistent via `retag_graph_bound` for far-B to resolve per-leaf). Far-B (leaves of *different* `graph_id` coexisting) is MG6.

### MG6 — later

> **Reconciled 2026-06-15:** the first two items moved to the
> [window composition plan (archived)](../../archive_docs/2026-06-19_completed_plans/2026-06-11_window_composition_plan.md) (which supersedes
> MW4–MW6 and delivers far-B in P1/P2 plus `OpenGraphBeside`). Only the persona chip
> and the engine-profile escalation are still owned here.

- Far-B: leaves of different `graph_id` coexisting in one window.
- Multi-window tear-out over `tearout.rs` (each window an independent active-graph view; compounds with far-B).
- Persona chip in the shellbar, gated on multi-persona (persona-model brief v1).
- Per-session engine-profile escalation (manifest field already present).

---

## Open questions

1. **Inactive sessions cold vs warm.** Cold (on disk, rebuilt on switch) for v1; warm (in memory, physics paused) is a later perf tweak if switch latency bites.
2. **Window-scoped vs session-scoped split.** Lean: window-scoped = toolbar, shellbar edge, comms, the active-session pointer; session-scoped = graph, camera, view-intents, and (until MG5) frame_layout.
3. **Content cache scope.** Shared/persona-scoped for v1 (avoids re-fetch across sessions); per-session escalation later via `EngineProfileBinding::SessionScoped`.
4. **Default-session identity on migration.** The migrated flat session gets a fresh `session_id`; its `display_name` derives from graph contents unless the user renames.

---

## Progress

- 2026-06-09: Plan written. Grounded in the live session-runtime substrate
  (`ManifestStore` / `GraphSessionManifest` complete and unconsumed; `switcher_thumbnail`
  built; `tearout` present) and current single-session meerkat (`GraphId::default()`
  everywhere). Model B set as the goal, Model A as the checkpoint reached at MG2.
- 2026-06-09: **MG1 done.** meerkat now splits storage into a **shared** root
  (`<data_dir>/mere/`: settings, content cache, comms) and **per-session** dirs
  (`<data_dir>/mere/sessions/<id>/`: graph.json, frame.json, views/). `App` holds
  a `ManifestStore` + `active_session_id`; `bootstrap_sessions()` scans `sessions/`,
  one-time-migrates a pre-MG1 flat `graph.json` (+ frame + views) into a minted
  session, or seeds one default session on a fresh install, opening the
  most-recently-updated as active. `save_session` advances the active manifest's
  `updated_at` and flushes the registry. 3 tests (seed / migrate / reuse); full
  meerkat suite green (44 lib + 55 bin).
- 2026-06-10: **MG2 done (Model A reached).** Added `Orrery::set_graph` (a wholesale
  in-place graph swap that reconciles the derived views + reseeds the *existing*
  physics actor, mirroring `ingest_graph`, so no actor churn) and
  `Constellation::clear`. meerkat gained `create_session` / `switch_session` /
  `load_active_session` / `cycle_session`: switching persists the current session,
  loads the target's graph + camera + frame, re-points the orrery in place, and
  clears the prior session's runtime caches (content actors, cards, tiles,
  workbench). Interim keybinds (until the F2.3 switcher): **Ctrl+N** new graph,
  **Ctrl+PageDown/Up** cycle sessions. 3 tests (create / switch-restores-each-graph /
  cycle); meerkat suite green (44 lib + 58 bin). Remaining of MG3: close + rename.
- 2026-06-10: **MG3 done (close), MG4 done (switcher).** `close_session` trashes a
  session (refusing the last, switching to the most-recently-updated survivor first,
  `move_to_trash` for recovery). The **F2.3 shellbar switcher** (`switcher.rs`) is a
  host-drawn strip of per-graph thumbnail tiles anchored at the bottom of the
  Left/Right shellbar (Top/Bottom strips are too thin for the vertical stack, so the
  switcher is suppressed there): each tile draws its `build_switcher_thumbnail` swatch,
  the active tile is highlighted, a "×" trashes (omitted on the last session), and a
  trailing "+" mints a graph. `render.rs` rasterizes the scene + composites it over the
  shellbar chrome (the gloss minimap pattern) and offsets the local hit-rects into
  `session_{row,close,add}_rect`; `input.rs` routes a left press in the shellbar region
  through close → add → row. Thumbnails refresh on every session/graph change
  (`refresh_session_thumbnails`: active from the live orrery, inactive from cold
  `graph.json`) plus a cheap active-only update on each `save_session`. **Rename is
  deferred** — there is no host text-into-scene path yet (the switcher is textless),
  so naming waits on that or a text-entry affordance. 2 switcher tests
  (row+add-but-no-close for one session / close-box-per-session + downward stacking);
  meerkat suite green (44 lib + 60 bin).
- 2026-06-10: **MG5 done (near-Model-B reached).** The content `frame_layout` is now
  **window-scoped**: it persists at the shared `mere/` root (not per-session) and
  stays put across switches, so a graph swap re-sources the panes instead of
  rearranging them. `frame` gained `PaneContent::follows_active_graph()` (the
  graph-bound vs window-chrome policy: orrery / workbench / gloss / roster / inspector
  / tile follow the graph; steward / comms / apparatus / system are window-chrome) and
  `FrameLayout::retag_graph_bound(graph)` (re-points only the graph-bound leaves),
  plus `GraphId::nil()` for the window-chrome "unbound" tag. meerkat: `save_session`
  writes the frame to `mere_root`; startup loads it from there (one-time carry-up of a
  pre-MG5 per-session frame); `load_active_session` keeps `self.frame_layout` and
  `retag_graph_bound`s it to the target session's `root_graph_id`; leaves are minted
  with the active graph id (graph-bound) or nil (chrome) instead of a random
  per-leaf id; the MG1 migration leaves the flat `frame.json` at the root. Tests:
  `frame` +2 (classification / retag-only-graph-bound), meerkat +1 (switch keeps the
  roster pane + re-sources the orrery leaf), migration test updated (frame stays at
  root). Whole workspace check clean; meerkat suite green (44 lib + 61 bin), frame 12.
- 2026-06-10: **Host text path done (MG4 textless deferral + rename closed).** A new
  `text::HostText` (parley `FontContext`/`LayoutContext<[f32;4]>` held once) shapes a
  line and pushes it into a host-drawn `netrender::Scene` via `netrender_text` — the
  same parley 0.10 stack document-canvas uses, without its document packet pipeline.
  Deps added: `parley`, `netrender_text`. The **switcher tiles are now labelled**
  (swatch over a name row, truncated-to-fit on the 40px strip): the display name,
  else one derived from the graph (`derive_session_label`: first non-welcome node's
  host/title, else "New"). Labels cache in `session_labels` in lockstep with the
  thumbnails. **Inline rename** works: F2 renames the active session, right-clicking a
  tile renames that one; typing edits a buffer the switcher draws with a caret (tail-
  fitted), Enter commits (sets `manifest.display_name`, empty clears it), Escape
  cancels, click-away commits. Tests: switcher +1 (`fit_label`), meerkat +1 (rename
  set/clear/cancel). meerkat suite green (44 lib + 63 bin); workspace check clean.
  Follow-ups: IME / paste in the rename buffer; caching the switcher scene to avoid
  per-frame reshaping; wider labels (hover tooltip) given the 40px constraint.
