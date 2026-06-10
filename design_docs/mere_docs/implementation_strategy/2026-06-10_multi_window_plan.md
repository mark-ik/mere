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

**Reshape entry point (found while finishing MW1):** the remaining per-window fields
do *not* yield to the mechanical bulk-move that carried MW1's 32 fields, so MW2 starts
with the structural reshape itself:

- Replace `WindowView`'s `#[derive(Default)]` with a real `WindowView::new(...)`: the
  surface (`window`, `host`), the chrome (`dom`, `runner`, `workbench` + its dom /
  runner), the frame (`frame_layout`, `next_pane_id`, `maximized_pane`,
  `active_content`), and the input bits (`cursor`, `modifiers`, `toolbar_h`, `width`,
  `height`) are not `Default`-able / are computed at construction, so they thread
  through the constructor rather than slotting into a derive.
- These also can't be bulk-rewritten (`self.host` is a substring of `self.host_text`;
  `self.toolbar_h` of `self.toolbar_height()`), so the access-site rewrite is the
  careful per-site pass that the `App`→`Shell` split does anyway.
- Order: `WindowView::new` + move structural fields → group the rest into `SharedState`
  → `Shell { shared, windows, primary }` + route by `WindowId` → `ShellCommand` seam.

**MW2 scope (2026-06-10, grounded against the post-X1 code):**

*Field destinations.* The ~30 fields still on `App` split three ways:

- **→ `WindowView` (this phase's move, with measured access sites):**
  `runner` + `dom` (89 + 7 — the bulk is mechanical `self.runner.update(...)` →
  `view.runner.update(...)`), `workbench` + `workbench_dom` + `workbench_runner`
  (26 + 4 + 6), `frame_layout` (27) + `next_pane_id` (6) + `maximized_pane` (13) +
  `active_content` (10), `cursor` (27) + `modifiers` (24) + `toolbar_h` (7) +
  `width` (18) + `height` (14), `window` (11) + `host` (5), and — resolving open
  question 3 here — `a11y_bridge` + `a11y_action_routes` (5 + routes): the
  bridge installs against one window, so it is per-window by construction.
  Roughly 360 sites total, versus MW1's ~140; the known substring traps
  (`host`/`host_text`, `toolbar_h`/`toolbar_height()`, `cursor`/`cursor_icon`)
  rule out bulk rewrite for exactly the fields with the most sites.
- **→ `SharedState` subsystems:** `content` { constellation, store,
  content map, fetch_handle, engine_registry }, `session` { manifests,
  active_session_id, session_dir, mere_root, session_thumbnails,
  session_labels, host_text }, `presentation` { theme, chrome_theme,
  chrome_sheet, active_theme_id, saved_tab_cap, shellbar_edge — the *setting*
  is shared; which windows render a shellbar is the `WindowKind` template's
  call }, `comms` { comms_handle }, `sync` { sync_handle }, `inbox`
  { KernelInbox, diagnostics_rx }, plus `observability`.
- **→ stays on `Shell`:** `orrery` (58 sites; the MW6 IOU). Moving it into
  `WindowKind::Primary`'s payload now would rewrite all 58 into
  primary-window lookups for zero pre-MW3 benefit — MW2 seats the
  `WindowKind` *marker* and MW6 moves the payload. Also `clipboard`
  (system-global, 2 sites) and `_kernel`.

*The Option dividend.* `window` and `host` are `Option` only because
`resumed()` arrives after construction. In the registry a `WindowView` is
constructed *inside* `resumed` (window create → `SurfaceHost` boot → a11y
install → insert into `windows`), so both become non-Option fields and the
`self.host.as_ref().unwrap()` scatter dies with the move.

*X1 changed one thing under this plan.* `App` now carries
`scrying: ScryingHost` (compatibility-view WebViews). Its producers are
**HWND-parented**, so the pool cannot be naively shared: the compat *pins*
are session state (→ `SharedState.content`) and the producer pool is
per-window (→ `WindowView`); the type already separates the two internally.
Consequence for MW4: a torn-out compat tile spawns a fresh WebView parented
to the new window — consistent with spawn-on-drop, no WebView migration.

*Event-loop rewiring (the three real seams in `app_handler.rs`):*

1. `resumed` — today early-returns if the window exists; becomes "create the
   primary if `windows.is_empty()`" and otherwise a no-op (winit calls it on
   every resume).
2. `window_event` — today early-returns on id mismatch (the registry's
   degenerate form); becomes `windows.get_mut(&id)` dispatch.
   `CloseRequested` forks by kind: primary saves + exits (today's behavior),
   a non-primary window just drops its view (MW3's consumer).
3. `user_event` — the multicast inventory, measured: fetch pages /
   subresources / contributions mutate **shared** state (then redraw the
   windows showing the affected members); the **sync indicator** and every
   **comms update** write *chrome state* via `runner.update`, which becomes a
   write per window whose template carries that chrome (sync chip: all;
   comms pane: the windows with it open). MW2 wraps these writes in the
   fan-out loop while N=1 so MW3 inherits a real seam, per the
   no-load-bearing-broadcast-redraw note above.

*The command-seam v0 boundary* (tempering "mutations applied as commands" so
MW2 does not balloon): pure view mutations (drags, scroll, cursor icon,
hover) stay direct on `(&mut WindowView, &mut SharedState-subsystem)` narrow
borrows — they never alias two windows. `ShellCommand` v0 covers what needs
full `&mut Shell` or a second window: `Exit { save }`, `SwitchSession(id)`
(clears constellation + scrying, reloads orrery + frame), `SetTheme(id)`
(rebuilds every window's sheet), `SpawnWindow(kind)` / `CloseWindow(id)`
(seated now, consumed by MW3/MW4), and the existing `drain_pending_command`
dispatch re-homed as command application. Handler shape:
`fn on_*(view: &mut WindowView, shared: &mut ..., ...) -> Vec<ShellCommand>`,
applied by `Shell::apply` after the borrow ends.

*Step order, each step compiling + 44/63 tests green (the MW1 discipline):*
(a) `WindowView::new` + the structural move, window/host first (kills the
Options), then chrome runners, frame cluster, input bits — methods stay on
`App`; (b) group `SharedState` subsystems (pure field moves); (c) the
receiver shift, one module at a time (render → input → frame_ops →
app_handler), converting `&mut self` methods to `(view, shared)` functions;
(d) rename `App` → `Shell`, add `windows: HashMap<WindowId, WindowView>` +
`primary`, route by id; (e) `ShellCommand` + drains through it. Touch-point
not to forget: `agent_harness` and `save_session` read fields that move
(`frame_layout` → view), so their signatures shift in (c).

**Step (a) is done (2026-06-10, commits `1afe35d` + `6397b26`).** All
per-window state lives in `WindowView` behind `App.view`, minted by
`WindowView::new`; methods still hang off `App`. The focused reshape session
resumes at **(b)** and runs (b)→(e). Remaining churn is ~285 shared-field
access sites (measured), dominated by `orrery` 58, `observability` 49,
`constellation` 33 — note `orrery` *stays on `Shell`* (the MW6 IOU), so its
58 sites become `self.orrery` → `self.shared`-less `Shell`-level access, not a
subsystem path; only the `SharedState`-bound fields take the deeper
`self.shared.<subsystem>.<field>` path. Decision still open for that session
(deferred deliberately, not yet chosen): **flat `SharedState` first then
subdivide** vs **subdivide into subsystems in one pass** — the subsystem map
above (`content` / `session` / `presentation` / `comms` / `sync` / `inbox` +
`observability`) is the target either way; flat-first just stages the path
churn. The substring traps that shaped (a) no longer bite here — the shared
fields (`orrery`, `constellation`, `manifests`, ...) have no method/sibling
collisions — so (b) is closer to a clean bulk pass than the carve was.

Done when the single window is driven through the registry (events resolved by id,
mutations applied as commands), with the shared/​per-window seam enforced by the types
— and the `window`/`host` unwrap scatter is gone (both non-Option inside `WindowView`).

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
- 2026-06-10: **MW1 continued — texture + interaction clusters carved (27 fields
  total).** `WindowView` now holds the 9 hit-rect caches, the 6 paint-texture caches,
  and the 12 interaction fields (scroll / drags / cursor_icon / pending_exit /
  context_set / renaming). `CachedTile` + `ResizeDrag` made `pub(crate)`. ~72 access
  sites updated; behavior-preserving (44 lib + 63 bin green). **Deliberate MW1/MW2
  boundary:** the surface / chrome / input-device fields (`window`, `host`, `runner` +
  `dom`, `workbench*`, `frame_layout`, `cursor`, `modifiers`, `toolbar_h`, size) move
  *with* MW2's registry, because that is where the method signatures shift to
  `(&mut WindowView, &mut SharedState)` + the `ShellCommand` seam — moving them now
  would only churn them twice. The remaining clean view-session fields
  (`focused_tile`, `live_previews`, `content_location`, `shown_location`,
  `active_content`, `maximized_pane`, `next_pane_id`, switcher caches, `host_text`)
  are the last MW1 cluster.
- 2026-06-10: **MW2 scoped against the post-X1 code** (the scope block under the
  MW2 phase). Headlines: ~360 access sites move (vs MW1's ~140), dominated by
  `runner` (89, mostly mechanical) and `orrery` (58 — which is the argument for
  keeping `orrery` on `Shell` as the explicit MW6 IOU rather than seating it in
  `WindowKind::Primary`'s payload now). `window`/`host` become **non-Option**
  inside `WindowView` (constructed in `resumed`), killing the unwrap scatter.
  New fact the plan predated: X1's `ScryingHost` splits across the seam —
  compat pins are shared, the HWND-parented producer pool is per-window, and a
  torn-out compat tile spawns a fresh WebView in the recipient (no migration).
  `a11y_bridge` + action routes resolve per-window (open question 3). The
  command seam is bounded for v0: view mutations stay direct narrow borrows;
  commands cover Exit / SwitchSession / SetTheme / Spawn-CloseWindow + the
  re-homed `drain_pending_command` dispatch. `user_event`'s multicast inventory
  is measured: sync-chip + comms writes are the per-window chrome-state writes
  to wrap in the fan-out loop while N=1. Step order (a)-(e) in the scope block,
  each step green under the MW1 discipline.
- 2026-06-10: **MW1 clean-carve complete — view-session cluster moved (32 fields
  total).** `WindowView` now also holds `centered`, `content_location`,
  `shown_location`, `focused_tile`, `live_previews` (what this window looks at within
  the shared graph). Careful exclusions: `content_location` is also a `Chrome` field in
  the meerkat *library* — only the bin's `App` accesses were rewritten; the switcher
  caches (`session_thumbnails` / `session_labels`) + `host_text` are **shared-derived**,
  so they stay on `App` headed for `SharedState`, not `WindowView`. The frame cluster
  (`frame_layout`, `next_pane_id`, `maximized_pane`, `active_content`) + the structural
  fields (`window`, `host`, `runner`/`dom`, `workbench*`, `cursor`, `modifiers`,
  `toolbar_h`, size) move in **MW2** with the `App`→`Shell` reshape, the `ShellCommand`
  seam, and the method-receiver shift to `(&mut WindowView, &mut SharedState)`. 36
  bin-file access sites updated; behavior-preserving (44 lib + 63 bin green).
- 2026-06-10: **MW2 carve complete — every per-window field now lives in `WindowView`.**
  Four clusters moved off `App` into `App.view` across this and the prior session: the
  **frame** cluster (`frame_layout`, `next_pane_id`, `maximized_pane`, `active_content`;
  unblocked by a hand-written `Default for FrameLayout` + a `Default` `ContentPane`), the
  **scrying** cluster (`scrying`/`ScryingHost`, `scrying_rect`, `scrying_input_focus`,
  per-window because each WebView is HWND-bound, folding in the X1/X2 work), the
  **window/size/input** cluster (`window`, `host`, `toolbar_h`, `width`, `height`,
  `cursor`, `modifiers`), and the **chrome-runner** cluster (`dom`, `runner`, `workbench`,
  `workbench_dom`, `workbench_runner`). The runners are `!Default` (a serval document
  authority can't be conjured), so this **ends the derive-`Default` era**: `WindowView`
  now has an explicit [`WindowView::new(dom, runner, workbench, workbench_dom,
  workbench_runner)`](../../../crates/meerkat/src/window_view.rs) over a fresh runner pair,
  which is exactly how a second window will be minted over the same shared session.
  Collision-aware edits throughout: `self.host`/`self.host_text`,
  `self.toolbar_h`/`self.toolbar_height()`, `self.window`/`self.window_control()`, and
  `self.workbench`/`self.workbench_open()` all share a literal prefix, so those went
  per-site or by trailing-char form, never bulk. ~140 access sites updated; behavior-
  preserving (44 lib + 63 bin green). **`App` is now cleanly bisected:** `view:
  WindowView` is all per-window state; the remaining ~25 fields (orrery, the actor
  handles, constellation, the session registry + switcher caches, theming, observability,
  a11y, settings, inbox) are the shared set headed for `SharedState`. The reshape
  (group `SharedState`, then `Shell { shared, windows: HashMap<WindowId, WindowView>,
  primary }` + route by `WindowId` + the `ShellCommand` seam) is the only MW2 work left.
