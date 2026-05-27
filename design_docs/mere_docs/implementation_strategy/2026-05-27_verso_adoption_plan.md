# Verso Adoption Plan

**Date**: 2026-05-27
**Status**: Plan. Sequences verso adoption, which absorbs route 2 (the custom
child-hosting tiling widget) per the
[between-tiles layout seam](../technical_architecture/2026-05-26_between_tiles_layout_seam.md).
Companion to the [composition spine](../technical_architecture/2026-05-21_mere_composition_spine.md)
(§7 verso = realization) and the [component fit-map](../technical_architecture/2026-05-26_component_fit_map.md)
(verso = "the clearest gap").

---

## The finding that shapes the sequence

Reading the two verso crates against the live host changes where this should
start:

- **`verso-core` (1,031 LOC)** is framework-free surface-*lifecycle* bookkeeping:
  `SurfaceCommand` (Present / Retire / Focus), `SurfacePlacementPlan`,
  `SurfaceLifecycleState` with a deferred-command backlog and retries, the
  `SurfaceCommandSink` trait. Keyed on `PaneId` / `NodeKey` / `GraphViewId`.
- **`tile-state` (985 LOC)** is the within-tile mutable state: navigation
  history, per-URL document cache, `SurfaceProducer`-backed long-lived tiles.
  Keyed on `NodeKey`.

Two consequences:

1. **Both are keyed on substrate-era ids** (`NodeKey` / `PaneId`), which the
   clean `mere-app` host does not use — it has graph-node UUIDs, forme tile-ids,
   and a `MainView`, no per-pane `NodeKey`. Adopting either as-is would drag the
   substrate identity model back in. Spine §14.3 already committed the fix:
   *"TileManager survives as Verso runtime keyed by a forme-assigned tile /
   surface id — no longer NodeKey."* That reshape is the prerequisite.

2. **Verso's machinery pays rent for *live* and *external* surfaces, not static
   in-scene tiles.** Present/Retire/Focus with deferred retries is for surfaces
   that can fail to appear and need re-driving — system WebViews
   (scrying), producer-backed textures. Within-tile history + document cache is
   for tiles the user *navigates*. The current workbench tiles are static,
   member-bound, single-document. Route 1 (the Xilem `zstack` + `transformed`
   composition) renders them fine. So **verso's first real consumer is the live /
   external-surface path, which clusters with scrying** — not the in-scene
   workbench layout.

The honest read: "fold route 2 into verso adoption" does not mean "build a verso
tiling widget for the workbench now." It means route 2's realization widget is
**one phase of a verso milestone whose centre of gravity is live + external
tiles**. Route 1 holds the static workbench until a phase actually needs to
replace it (clipping, within-tile navigation, or an external texture).

## Phases

### Phase 0 — Reshape verso keys to the forme/tile-id model (prerequisite) ✅

**Done 2026-05-27.** `verso-core` and `tile-state` moved off `NodeKey` / `PaneId`
/ `GraphViewId` onto a single forme-assigned `TileId(Uuid)` (new type in
`verso-core::surface`; the host maps `forme::ArrangementNodeId` → `TileId`). A
surface is now addressed by `(SurfaceHostId, TileId)` — the `(view, pane, node)`
triple collapsed to one tile key, since a tile *is* both the location and the
content. Tests: verso-core 19/19, tile-state 14/14; `mere-app`, `platen`,
`kernel`, `session-runtime` all build; no substrate key remains in either
crate's API.

Two things surfaced during execution, beyond the original text:

- **`ViewerSurfaceHost` + `ViewerSurfaceError` moved kernel → verso-core**
  (`verso-core::host`), keyed on `TileId`. A surface-allocation seam is
  realization (verso's domain), not graph truth (kernel's) — it was used only by
  verso's apply, so the move also de-clutters kernel.
- **`platen::project_surface_placements` / `project_active_surface_placements`
  retired** (deleted, with their re-exports). They were unused substrate-era
  functions building placements from `ArrangementSnapshot` members'
  `NodeKey`/`PaneId` — the sole non-verso consumer forcing the old shape, and
  superseded by `tree_projection` + `layout`. The rest of `workbench.rs` (the
  `WorkbenchProjection` / a11y types `platen/domain/workbench` consumes) stays.

Open tidy-up (deferred, needs the dependency-drop go-ahead): `verso-core` no
longer references `kernel`, so its `kernel` dep is now dead weight.

### Phase 1 — `WorkbenchTiling` realization widget (route 2 proper)

A host-side Masonry widget (lives in `mere-app` beside `graph_canvas.rs`, the
established home for custom widgets; extract to a `verso-host` crate when a
second consumer appears):

- Holds child tile `WidgetPod`s + the `WorkbenchPlan` + `LayoutConfig`.
- `layout()` runs `platen::layout_plan` at the **real** widget size (no
  `resize_observer` round-trip, no one-frame lag) and `place_child`es each tile
  at its rect; clips each child to its rect (the gap route 1 has).
- A Xilem `View` + `Element` + `Splice` managing the child `ViewSequence`
  (mirrors `zstack`'s ~400 LOC plumbing; the children are positional, slot
  order, so no per-child view params).
- Swaps the workbench pane off route 1.

**Tests (masonry `TestHarness`, headless):** `create_with_size((w,h), widget)`,
then assert each tile widget's window rect — `to_window(Point::ZERO)` +
`border_box_size()` (`window_origin()` was removed upstream 2026-05-25 as a
transform footgun; use `to_window` / `window_transform`) — equals its
`layout_plan` rect; `mouse_click_on` / a pointer at a tile rect's centre routes
to the right child (a `Recorder` tile confirms). Placement and hit-routing
correctness is automated; only subjective visual quality needs eyes.

This phase delivers route 2's concrete wins for in-scene tiles (clipping, no
lag). It is optional-now: do it when the static workbench's route-1 limits
(no clipping) actually bite, or as the proving ground for the widget before
Phase 3 needs it for external surfaces.

### Phase 2 — Live document tiles (within-tile state)

Wire `tile-state` (post-reshape) for the navigated document tile: within-tile
history + document cache replace `AppState.current`'s single `RenderedTile`.
This is where back/forward *within* a tile and instant cached navigation start
paying. `verso-core` lifecycle still mostly idle (in-scene, no deferral).

### Phase 3 — External-surface tiles (scrying) — verso pays full rent

The cluster where verso, the Xilem driver edit, and the scrying brief converge:

- `verso-core` lifecycle (Present/Retire/Focus + deferred retries) drives
  external-texture surfaces that can fail to appear.
- A `SurfaceTile` Masonry widget opts into `PaintLayerMode::External`; the
  `WorkbenchTiling` widget (Phase 1) hosts it.
- The Xilem `MasonryDriver` gains an app-facing hook forwarding
  `composite_external_layers` (the vendored-lib edit, now with a real consumer —
  see the [scrying DOM-bridge brief](../../../../serval/docs/2026-05-26_scrying_dom_bridge.md)
  for the orthogonal observable channel).
- `scrying-engine`'s producer becomes the `SurfaceProducer` behind the tile.

**Gate + stub landed 2026-05-27 (builds green; runtime GPU verification pending).**
The external-surface *pipe* is wired end to end with a solid-color stub:

- **Xilem `with_external_compositor`** (vendored fork, commit `813179ae`) —
  `MasonryDriver` forwards `composite_external_layers` to an app closure
  (mirrors `start_callback`); the closure gets the shared device/queue, target,
  and the external layers (`widget_id` + bounds).
- **mere-app `surface_tile.rs`** (commit `76b9889`) — `SurfaceRegistry`
  (`WidgetId → content`, the decoupler), `SurfaceTileWidget` (reserves an
  `External` layer + registers its color, paints nothing), `composite_color`
  (staging texture + `copy_texture_to_texture` into the target), and the `main()`
  wiring (one registry shared between views and the closure; `MainView::Surface`
  + a `surf` button show one tile).

Remaining for the real lane: swap the registry value (`[u8;4]` → producer frame
texture handle) and `composite_color`'s copy source (`scrying-engine`'s
`SurfaceProducer`). The `SurfaceTile` currently mounts directly in a pane; the
Phase 1 `WorkbenchTiling` widget will host it once that lands. The DOM-bridge
observable channel (the brief) is orthogonal and rides the same pixel path.

## Sequencing decision (for the user)

Two orders, both valid:

- **A — realization-first:** Phase 0 → Phase 1 now (get the clipping/no-lag
  tiling widget + its harness test landed against the static workbench), then 2,
  then 3. Pro: route 2 ships and is proven before external surfaces depend on it.
  Con: builds ~400 LOC of widget plumbing for tiles that work today under route 1.
- **B — consumer-first:** Phase 0, then jump to Phase 3 (scrying), building the
  tiling widget there where verso's lifecycle + external compositing actually pay
  rent, and backfill Phase 2. Pro: every phase has a live consumer, no premature
  realization machinery. Con: the workbench keeps route 1 longer (no clipping
  until Phase 1 lands within the scrying work).

Recommendation: **B.** It keeps the "don't build ahead of a consumer" discipline
the fit-map argues for, and it means the first verso code (Phase 0 reshape) is
the one piece needed under either order. Phase 1's widget then lands inside the
scrying milestone, where its external-surface hosting is load-bearing rather than
a nicety over a working route 1.

## What this plan does not change

- Route 1 stays the live workbench path until a phase replaces it.
- `platen::layout` (the between-tiles geometry authority) is unchanged; verso
  consumes its `LaidOutPlan`, it does not duplicate it.
- The orrery (cartography projection, `graph_canvas`) is untouched; verso is the
  tree-projection's tile realization.
