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
    shared: SharedState,                      // grouped subsystems (below)
    windows: HashMap<WindowId, WindowView>,   // one per OS window
    primary: WindowId,                        // the window that owns the orrery (until MW6)
}

struct WindowView {
    surface: WindowSurface,                   // per-window surface + config (shares one device)
    runner: ServalAppRunner<...>, dom: ...,   // this window's chrome (toolbar/omnibar)
    frame_layout: FrameLayout,
    kind: WindowKind,                         // payload enum (below)
    // per-window view + render caches: cursor, modifiers, drags, *_rects, *_textures,
    // width/height, toolbar_h, cursor_icon, maximized_pane, active_content, renaming,
    // switcher caches, shellbar_edge
}

enum WindowKind { Primary(OrreryView), Leaf, Forked(OrreryView) }  // payload = who owns a camera
```

**Event handlers emit commands; the `Shell` applies them.** `window_event` borrows
exactly one `WindowView` + the `SharedState` subsystem it needs and returns a
`Vec<ShellCommand>` (`SpawnLeaf { payload, donor }`, `ReleaseTile`, `RouteRedraw`,
`PromoteLeafToBranch`, …); the `Shell` then applies them with full `&mut self`. This is
the same record-intent-then-drain pattern the host already runs everywhere
(`drain_pending_command` / `drain_pending_context` / `drain_comms_intent`), so it is
in-idiom — and it sidesteps every multi-window aliasing question (tear-out is a
two-window op: donor releases a binding, recipient acquires it; you cannot casually
take two `&mut` from one `HashMap`). It also gives a window-system-free test seam for
tear-out logic, and makes MW5's toast escalation just another command. Decide this
before **MW2** (the method-signature split), not after — MW1's field carve commits to
no signatures, so nothing is foreclosed yet.

**`SharedState` is subdivided, not a second god-struct.** Group it into subsystems so
window code takes the narrow borrow it needs: `content` (constellation + store +
fetch), `identity` (manifests + sessions), `presentation` (theme + engine_registry),
`comms`, `inbox`. `user_event` drains the inbox once and **multicasts** results to the
affected windows from day one — chrome-relevant shared state (sync status, inbox
badges) flows from `SharedState` through the fan-out, since per-window `ScriptedDom`
means per-window chrome *state* (a v0 broadcast-redraw would otherwise become quietly
load-bearing).

**Chrome is per-window, which is leverage, not just cost.** Each `WindowKind` is a
different DOM *template*: a leaf window gets a slim chrome (no shellbar, no switcher)
with no layout code, the MW5 toast is a DOM overlay, the drag ghost is a positioned DOM
element. Serval's DOM layout makes these variations free.

The one deferred decision: **how the shared graph is held** — and it is deferred behind
a trait, not left open. Leaf rendering depends on a read-only `NodeView` seam (resolve
`member → url` + node metadata; content comes from the shared constellation). Both
options implement `NodeView`, so the choice does not leak into leaf code:

- **A — graph registry (the brief's model).** Extract `Graph` + physics out of
  `Orrery` into `SharedState` as a `GraphId`-keyed registry; `Orrery` becomes a view
  over a registry graph. Cleanest; unlocks far-B + a second orrery window. Larger
  refactor. Implements `NodeView`.
- **B — primary-orrery access.** The primary orrery's graph implements `NodeView`
  directly; leaf windows render against it. Lighter; carries MW3–MW4.

Lean **A** as the destination; **B** carries through MW4 behind the same `NodeView`
trait, so MW6 swaps the impl without chasing ad-hoc accessors.

---

## Phases (done-conditions, not dates)

### MW1 — carve `WindowView`

- Extract the per-window view + render-cache fields out of `App` into a `WindowView`
  struct; `App` keeps the shared state and holds one `WindowView`. Render and input
  operate on `&mut WindowView` + `&mut SharedState`. No behavior change; still one
  window.

Done when meerkat runs exactly as today with the per-window state living in a
`WindowView` the methods take explicitly.

### MW2 — the window registry + the command seam

- Rename/reshape `App` into `Shell { shared, windows: HashMap<WindowId, WindowView>,
  primary }`; `ApplicationHandler` routes each `window_event` to `windows[&id]` and
  `user_event` multicasts inbox results to the affected windows.
- Establish the command seam now (before the method signatures harden): per-window
  handlers borrow one `WindowView` + the needed `SharedState` subsystem and return
  `Vec<ShellCommand>`; the `Shell` applies them with full `&mut self`. Subdivide
  `SharedState` into its subsystems.

Done when the single window is driven through the registry (events resolved by id,
mutations applied as commands), with the shared/​per-window seam enforced by the types.

### MW3 — a second window, shared state (one device, N surfaces)

- **Split `SurfaceHost`** (in `serval-winit-host`) into a shared `RenderCore` (the one
  wgpu device + netrender `Renderer`) and a per-window `WindowSurface` (surface +
  config). Today `SurfaceHost::boot` mints its own device per call, so two windows
  would be two devices and the shared constellation texture could not be sampled into a
  second swapchain. One device + N surfaces lets a leaf **blit the donor's node
  texture** into its own backbuffer — the cheapest possible multi-window, and the
  payoff of this whole architecture.
- A "new window" command (Cmd/Ctrl+Shift+N) opens a second OS window over the same
  `SharedState`, with its own `WindowView` (own chrome, frame_layout, size). v0: the
  second window is **workbench-only** (`WindowKind::Leaf`), rendering shared nodes'
  tiles through the shared `constellation` behind the `NodeView` seam.
- Present must not block across windows: keep acquire non-blocking (skip-on-outdated is
  already there) so a slow window does not stall another's input on the shared loop.

Done when two windows coexist on one device, the second shows a shared node's live
tile (texture shared, not re-rendered), and closing it leaves the graph + the first
window intact.

### MW4 — leaf tear-out gesture

- Drag a workbench tab (or a pane header) past the slop. The drag **ghost is rendered
  as chrome inside the donor surface** (a positioned DOM element); the leaf window is
  spawned **on release (drop)**, not mid-drag. This is the only portable shape: a
  live window-follows-cursor would need a mid-gesture pointer-grab transfer Wayland
  forbids, and winit's `drag_window` is a window-*move* request, not this. The spawn is
  a `SpawnLeaf` command carrying `TileDragPayload` / `PaneDragPayload`.
- The leaf window carries the donor's `GraphId`; the donor releases its binding of the
  tile. Within-tile navigation in the leaf updates the shared node (edits propagate to
  the donor) via the `NodeView` seam.
- **Re-docking** (dragging a leaf back into a window) is **out of scope for MW4** — it
  is the same cross-window-drop-detection problem, which on Wayland routes through the
  OS data-device protocol (no global cursor position to hit-test against). Scoped to a
  later phase so the gesture model is not invalidated by deciding it late.

Done when dragging a tile out drops a leaf window of it on release, edits propagate
both ways, and closing the leaf does not delete the node (brief §4.1). The closed-leaf
rule is deliberate: the node stays graph-reachable but tile-less, rather than
returning the tile to the donor (the command seam makes either trivial; this is the
chosen one).

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

1. **Shared-graph holding (A vs B).** *Resolved into the architecture:* both impls sit
   behind the `NodeView` seam, so A (registry) is the destination and B (primary-orrery)
   carries MW3–MW4 with no leakage. MW6 swaps the impl. Not a fork any more.
2. **Switcher per window.** *Leaning resolved:* per-`WindowKind` chrome is a different
   DOM template, so a leaf window simply uses a slim template (no shellbar / switcher).
   A leaf is a single torn tile, not a session surface.
3. **a11y bridge per window.** Each window needs its own AccessKit adapter + projection.
   Confirm `AccessKitBridge` supports N instances; the projection is already a pure
   function of state.
4. **Re-docking a leaf.** Out of scope through MW5; it is the same cross-window-drop
   problem (Wayland routes it through the OS data-device protocol). Decide its phase
   before locking the gesture model so it is not invalidated late.
5. **Frame pacing on the shared loop.** N surfaces present serially on one thread. Keep
   acquire non-blocking; revisit only if two windows on different-refresh monitors
   actually contend (Fifo + frame-latency 2 should absorb the common case).

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
- 2026-06-10: **MW1 begun — first cluster carved.** New
  [window_view.rs](../../../crates/meerkat/src/window_view.rs) holds `WindowView`; the
  9 per-frame **hit-rect caches** (switcher rows / close / add, roster rows, apparatus
  buttons, gloss nodes, tile rects, content rects, close-button rects) moved off the
  `App` god-struct into `App.view`. These are pure view geometry (rebuilt each render,
  read by input to route a press), so they carve cleanly first. 32 access sites
  updated across render / input / frame_ops / app_handler; behavior-preserving (meerkat
  44 lib + 63 bin green). `WindowView` grows cluster by cluster next: paint-texture
  caches → interaction / drag state → frame + layout → window / surface → chrome
  runners, until `App` splits into `Shell { shared, windows }` at MW2.
- 2026-06-10: **Architecture revised after an external review** (verified against the
  code, not taken on faith). Adopted: (1) a **command seam** — per-window handlers emit
  `Vec<ShellCommand>` the `Shell` applies with full `&mut self`, matching the host's
  existing drain-intent pattern; settles every two-window-aliasing question before MW2
  hardens signatures. (2) A **`NodeView` trait** for leaf rendering, turning the A/B
  graph-holding fork into an impl detail behind a stable seam. (3) **`SharedState`
  subdivided** into subsystems (content / identity / presentation / comms / inbox);
  inbox **multicasts** so shared chrome state (sync / inbox badges) flows through
  fan-out, not a load-bearing broadcast-redraw. (4) **One wgpu device, N surfaces** —
  *verified*: `SurfaceHost::boot` mints a device per call today, so MW3 must split it
  (in `serval-winit-host`) into a shared `RenderCore` + per-window `WindowSurface`,
  which is what lets a leaf blit the donor's node texture rather than re-render it. (5)
  Tear-out is **spawn-on-drop** with an in-donor drag ghost (live window-follows-cursor
  needs a mid-gesture pointer-grab transfer Wayland forbids; winit `drag_window` is a
  move request); **re-docking** scoped out. (6) `WindowKind` is a payload enum
  (`Primary(OrreryView)` / `Leaf` / `Forked(OrreryView)`) so camera ownership is typed.
  Per-`WindowKind` chrome = a different DOM template (slim leaf chrome answers the
  switcher-per-window question). Tempered the reviewer's vsync-stall claim to a
  non-blocking-acquire note. No staging change.
