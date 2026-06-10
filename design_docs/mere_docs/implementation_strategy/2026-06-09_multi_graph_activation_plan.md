# Multi-Graph Activation Plan

**Date**: 2026-06-09
**Status**: Planning. The substrate (session manifests, per-session storage,
switcher thumbnails) is built; this plan wires meerkat to use it.
**Related**: [shellbar plan F2.3](2026-06-09_shellbar_plan.md), [graph session manifest plan](2026-05-11_graph_session_manifest_plan.md), [switcher thumbnails plan](2026-05-14_switcher_thumbnails_plan.md), [multi-window plan](2026-06-04_multi_window_plan.md), [peripheral panes architecture](../technical_architecture/2026-06-06_peripheral_panes_architecture.md) (panes are per-window), [composition spine](../technical_architecture/2026-05-21_mere_composition_spine.md). Code: `crates/system/session-runtime/`, `crates/meerkat/`, `crates/shell/frame/`.

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

- Bottom-anchored strip in the shellbar: one row per session (display name + `build_switcher_thumbnail`), active marker, "+" to create, click to switch, right-click to rename / close.
- Closes F2.3's graph-switcher half; persona chip remains a reserved slot.

Done when the shellbar lists sessions with live thumbnails and drives create / switch / close without keybinds.

### MG5 — transition to Model B (the goal)

- Lift `frame_layout` to window scope; stop swapping it on switch.
- Give leaves a real `graph_id`; mark graph-bound leaf kinds (orrery, roster, gloss, Inspector-node-face) as following the active graph, and window-chrome kinds (Apparatus, Steward, Comms) as graph-independent.
- `switch_session` becomes "set active graph + re-source the graph-bound leaves"; window-chrome leaves persist untouched.

Done when switching graphs keeps the window's pane arrangement and re-sources only the graph-bound panes, with no full content-band swap.

### MG6 — later

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
  No code yet.
