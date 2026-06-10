# Multi-Window Tear-Out Plan

**Date**: 2026-06-10
**Status**: Planning. Greenfield on the meerkat host. The tear-out *payload* types
exist (`session-runtime/tearout.rs`); execution is unbuilt. This plan carves the
window seam and stages leaf → branch → fork.
**Related**: [tear-out operations brief](../research/2026-05-11_tearout_operations_brief.md) (the leaf/branch/fork model this implements), [multi-graph activation plan](2026-06-09_multi_graph_activation_plan.md) (MG6 lists this; far-B and multi-window share the per-window-view-over-shared-graph split), [peripheral panes architecture](../technical_architecture/2026-06-06_peripheral_panes_architecture.md) (panes are per-window). Code: `crates/meerkat/`, `crates/system/session-runtime/`, `crates/shell/frame/`.

Drag a pane or tile out of its window into a new OS window that shares the backing
graph + session state. The destination is the tear-out brief's trichotomy — **leaf**
(UI-only, drag), **branch** (new graphlet, Shift+drag), **fork** (new graph,
Ctrl+Shift+drag) — with a toast offering escalation on an unmodified drag. Leaf is
the first milestone; it needs the least new architecture.

This plan supersedes the never-written `2026-06-04_multi_window_plan.md` the
multi-graph plan referenced.

---

## Findings

**The host is a single-window god-struct.** `App` *is* the winit
`ApplicationHandler` ([app_handler.rs](../../../crates/meerkat/src/app_handler.rs)):
it holds one `window: Option<Arc<Window>>`, one `host: SurfaceHost`, and
`window_event` early-returns unless the event's `WindowId` matches that one window.
The `App` struct ([main.rs](../../../crates/meerkat/src/main.rs)) carries ~80 fields
that mix two concerns:

- **Shared session state** (one per app): the graph (today inside `orrery`), the
  `constellation` (content-tile actors), the `fetch`/`sync`/`comms` actor handles +
  the `inbox`, the durable content `store`, `manifests` + the session registry, the
  `theme` registry + `engine_registry`, `observability`, `diagnostics_rx`, `_kernel`.
- **Per-window view state**: `window` + `host` (surface), the chrome `runner` + `dom`
  (each window has its own toolbar / omnibar), `frame_layout`, the camera (inside
  `orrery`), the ~15 `*_rects` / `*_textures` hit-and-paint caches, `cursor`,
  `modifiers`, the drag states (`tab_drag`, `divider_drag`, `frame_divider_drag`,
  `resize_drag`, `titlebar_press`), `width`/`height`, `toolbar_h`, `cursor_icon`,
  `maximized_pane`, `active_content`, `renaming`, the switcher caches
  (`session_thumbnails` / `session_labels` / `session_*_rects`), `shellbar_edge`.

**The orrery is the crux.** `Orrery` bundles the graph + offloaded physics + camera.
Multi-window-over-shared-graph wants the *graph + physics* shared and the *camera +
input + render* per-window. That split is exactly what far-B needs too (the
multi-graph plan: "multi-window compounds with far-B"). It is the single hardest
piece, so the staging defers it.

**Leaf side-steps the orrery split.** Per the tear-out brief §4.1, a **leaf** window
is *workbench-only, no orrery*: it shows a tile facet of a node that still lives in
the donor's graph. Rendering a tile needs the node's URL + its content actor, both of
which live in the **shared** `constellation` + graph — not a second orrery. So leaf
tear-out is reachable without extracting the graph from the orrery, as long as the
constellation and a read view of the graph are shared.

**The substrate is payload-only.** `tearout.rs` ships `TileDragPayload { pane_id,
tile_index }` + `PaneDragPayload { pane_id }` and nothing else; its doc still names
the gpui-era `host::tearout` execution home. winit 0.30 supports N windows on one
event loop (the actors are `!Send` and UI-thread-bound, so shared state over `Rc` is
fine — no cross-thread concern).

---

## Architecture

Split `App` into a `Shell` (the `ApplicationHandler`) over shared state + a map of
window views:

```
struct Shell {
    shared: SharedState,                      // graph(s), actors, caches, manifests, theme, inbox
    windows: HashMap<WindowId, WindowView>,   // one per OS window
    primary: WindowId,                        // the window that owns the orrery (until MW6)
}

struct WindowView {
    window: Arc<Window>,
    host: SurfaceHost,
    runner: ServalAppRunner<...>, dom: ...,   // this window's chrome (toolbar/omnibar)
    frame_layout: FrameLayout,
    kind: WindowKind,                         // Primary (orrery + workbench) | Leaf (workbench-only)
    // per-window view + render caches: cursor, modifiers, drags, *_rects, *_textures,
    // width/height, toolbar_h, cursor_icon, maximized_pane, active_content, renaming,
    // switcher caches, shellbar_edge
}
```

`SharedState` owns the actor handles + `inbox`; `user_event` drains the inbox once and
fans results to the affected window(s) (a fetch outcome → redraw every window showing
that node). Chrome is **per-window** (each window has its own toolbar / omnibar /
shellbar). The orrery (graph + physics + camera) stays on the primary window until
MW6; leaf windows render shared tiles with no orrery.

The one deferred decision: **how the shared graph is held.** Two options, decided at
MW6, not before:

- **A — graph registry (the brief's model).** Extract `Graph` + physics out of
  `Orrery` into `SharedState` as a `GraphId`-keyed registry; `Orrery` becomes a view
  (camera + input + render) over a registry graph. Cleanest; unlocks far-B + a second
  orrery window. Larger refactor.
- **B — Shell-mediated access.** Keep the graph in the primary orrery; the `Shell`
  lends a read view (and mediates mutations) to leaf windows. Lighter, but does not
  scale to branch / fork / a second live orrery.

Lean **A** as the destination; allow **B**'s lightness through MW4 (leaf) since a leaf
only needs a node's URL + the shared constellation.

---

## Phases (done-conditions, not dates)

### MW1 — carve `WindowView`

- Extract the per-window view + render-cache fields out of `App` into a `WindowView`
  struct; `App` keeps the shared state and holds one `WindowView`. Render and input
  operate on `&mut WindowView` + `&mut SharedState`. No behavior change; still one
  window.

Done when meerkat runs exactly as today with the per-window state living in a
`WindowView` the methods take explicitly.

### MW2 — the window registry

- Rename/reshape `App` into `Shell { shared, windows: HashMap<WindowId, WindowView>,
  primary }`; `ApplicationHandler` routes each `window_event` to `windows[&id]` and
  `user_event` fans inbox results to the affected windows. Still one window created.

Done when the single window is driven through the registry (events resolved by id),
with the shared/​per-window seam enforced by the types.

### MW3 — a second window, shared state

- A "new window" command (Cmd/Ctrl+Shift+N) opens a second OS window over the same
  `SharedState`, with its own `WindowView` (own chrome, frame_layout, size). v0: the
  second window is **workbench-only** (`WindowKind::Leaf`), rendering shared nodes'
  tiles through the shared `constellation`. Both windows redraw on a shared-node
  content change.

Done when two windows coexist, the second shows a shared node's live tile, and
closing it leaves the graph + the first window intact.

### MW4 — leaf tear-out gesture

- Drag a workbench tab (or a pane header) past the slop → spawn a leaf window holding
  that tile, carrying the donor's `GraphId`; the donor releases its binding of the
  tile. Within-tile navigation in the leaf updates the shared node (edits propagate to
  the donor). Uses `TileDragPayload` / `PaneDragPayload`.

Done when dragging a tile out opens a leaf window of it, edits propagate both ways,
and closing the leaf does not delete the node (brief §4.1).

### MW5 — branch + fork + the escalation toast

- **Branch** (Shift+drag): mint a `GraphletRef` (`GraphletBinding::Forked`) in the
  donor's forme; the leaf window carries donor `GraphId` + the new `GraphletId`.
- **Fork** (Ctrl/Cmd+Shift+drag): mint a new `SessionId` + `GraphId` via
  `ManifestStore` + a cross-graph rekey of the reachable connected component; weak
  `parent_session` on the new manifest; the new window gets a thin orrery (needs MW6).
- **Toast** on an unmodified drop: `[ Branch ] [ Fork ] [ Keep as leaf ]`, auto-
  dismiss = keep as leaf (brief §3.2).

Done when all three operations run from the gesture model and the toast escalates a
leaf in place, per the brief's identity semantics.

### MW6 — the orrery split (second orrery window; meets far-B)

- Decision A: extract `Graph` + physics into a `SharedState` graph registry; `Orrery`
  becomes a per-window view (camera + input + render) over a registry graph. A
  fork/torn orrery window then runs its own camera over its own (or a shared) graph.
- This is the far-B substrate too: graph-bound leaves resolve their `graph_id` against
  the registry, so leaves of different graphs can coexist in one window.

Done when a window can host a live orrery over a registry graph independent of the
primary window's camera, and far-B leaf coexistence falls out of the same registry.

---

## Open questions

1. **Shared-graph holding (A vs B).** Lean A (registry) as the destination; B's
   lightness is allowed through MW4 since leaf needs only a URL + the shared
   constellation. Forced at MW6.
2. **Chrome per window.** Each window gets its own chrome runner (own toolbar / omnibar
   / shellbar). Confirm the chrome view-model is cheap to instantiate N times (it is a
   `ServalAppRunner` over a fresh `ScriptedDom`).
3. **Switcher per window.** The shellbar session switcher is per-window. Do leaf
   windows (workbench-only) show a switcher at all, or just the donor's? Lean: leaf
   windows hide the switcher (they are a single torn tile, not a session surface).
4. **a11y bridge per window.** Each window needs its own AccessKit adapter +
   projection. Confirm `AccessKitBridge` supports N instances; the projection is
   already a pure function of state.
5. **Inbox fan-out.** One `user_event` drains the shared inbox; results route to the
   windows showing the affected node. v0 may broadcast a redraw to all windows and
   refine later.

---

## Progress

- 2026-06-10: Plan written. Grounded in the live single-window `App`/​
  `ApplicationHandler` (one window, ~80-field god-struct mixing shared session state
  and per-window view state) and the [tear-out brief](../research/2026-05-11_tearout_operations_brief.md)'s
  leaf/branch/fork model. Key finding: **leaf** is workbench-only, so the first useful
  multi-window milestone (MW3–MW4) needs the shared `constellation` + a read view of
  the graph, not a second orrery — the orrery/graph split (the far-B-grade refactor) is
  deferred to MW6. Staged MW1 (carve `WindowView`) → MW2 (window registry) → MW3
  (second shared window) → MW4 (leaf tear-out) → MW5 (branch/fork/toast) → MW6 (orrery
  split, meets far-B). Supersedes the never-written `2026-06-04_multi_window_plan.md`.
