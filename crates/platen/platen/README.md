# platen

`platen` is the graph-aware composition surface for the
[mere](https://crates.io/crates/mere) browser. It turns reducer-owned
workspace state into renderable layout: the canvas scene that the workspace
graph view draws against, the frame/workbench arrangement, and the
surface-placement plan that
[`verso-core`](https://crates.io/crates/verso-core) receives.

In the printing-press metaphor: the platen is the press itself: the layer
that knows graph semantics (where a node goes, how panes bind to nodes, how
the workbench is arranged) and presses that knowledge into renderable form.

## What's in the crate

- **`canvas_scene`** — graph-to-canvas scene derivation.
  - `CanvasSceneOptions` — input options (view ID, scene mode, 2D/3D
    dimension, visible-node mask, default node radius).
  - `build_canvas_scene_input()` — turns a `kernel::graph::Graph`
    into a `CanvasSceneInput` packet for the canvas layer.
  - `graph_view_id_to_canvas()` — utility to map `GraphViewId` to the
    canvas's `ViewId`.

- **`document_scene`** — engine-document-to-canvas wrapper. Pairs an
  `inker::EngineDocument` with a viewport + `StyleConfig` and runs
  `document_canvas::layout_document` to produce a `DocumentRenderPacket`.
  The two scene helpers (canvas + document) are siblings in the
  canvas-swatches taxonomy: the workbench composer can place a graph
  swatch in one pane and a document swatch in another.

- **`workbench`** — frame / pane model and selectors over reducer-owned
  state.
  - **Frame model**: `FrameId`, `FrameState`, `PaneBinding`, `ProjectedPane`,
    `WorkbenchProjection`.
  - **Arrangement bridge** (plain data): `ArrangementSnapshot`,
    `ArrangementContainer`, `ArrangementMember`. `TileSlot` is re-exported
    from verso-core for convenience.
  - **Mutators**: `upsert_pane_binding`, `remove_pane_binding`,
    `assign_frame_pane`, `clear_frame_pane`, `set_frame_root_view`,
    `set_binding_surface_host`, `assign_view_and_frame_pane`,
    `remove_view_and_frame_pane`, `set_view_and_frame_surface_host`.
  - **Selectors and projections**: `select_active_frame`,
    `select_active_root_view`, `project_frame`, `project_active_workbench`,
    `project_surface_placements`, `project_active_surface_placements`,
    `snapshot_frame_arrangement`, `snapshot_active_arrangement`,
    `slot_for_index`.

## How it relates to other workspace crates

platen sits between [`graphshell`](https://crates.io/crates/graphshell)'s
reducer-owned workspace state and the renderable outputs (canvas scene +
surface placements) that downstream layers consume.

```text
       graphshell::app_state
              │ workspace state, frames, pane bindings
              ▼
            platen
       ┌──────┴──────┐
       ▼             ▼
  canvas_scene    workbench
       │             │
       ▼             ▼
 CanvasSceneInput    WorkbenchProjection / ArrangementSnapshot
 (drawn by the       SurfacePlacementPlan ───► verso-core
  canvas layer)
```

- [`graphshell`](https://crates.io/crates/graphshell) —
  `graphshell::app_state::composition` wraps platen's selectors over
  reducer-owned `GraphWorkspace` state; reducer mutators
  (`upsert_pane_binding`, `assign_view_and_frame_pane`, …) operate on that
  state.
- [`verso-core`](https://crates.io/crates/verso-core) — platen produces
  `SurfacePlacementPlan`s composed of `SurfaceSlotPlacement`s with
  `verso_core::surface::TileSlot`s; verso-core owns the surface lifecycle
  that consumes them.
- **`graph-canvas`** (workspace-internal) —
  `canvas_scene::build_canvas_scene_input()` returns
  `graph_canvas::scene::CanvasSceneInput<NodeKey>`; the canvas crate handles
  drawing.
- [`document-canvas`](https://crates.io/crates/document-canvas) —
  `document_scene` wraps `document_canvas::layout_document` so platen can
  hand a `DocumentRenderPacket` to a netrender / gpui consumer with the
  same shape as a graph-canvas scene.
- **`kernel`** (workspace-internal) — platen consumes the portable
  graph (`Graph`, `GraphViewId`, `NodeKey`) and pane (`PaneId`) types.

## Status

Pre-1.0. Active-frame and root-view selectors, frame arrangement snapshots,
surface-placement projection, and the workbench mutator surface are all in
place. Reducer-safe arrangement reconciliation (full apply semantics for
arbitrary frame edits) lands as concrete consumers need it.

## License

MPL-2.0.
