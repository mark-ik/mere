# Platen Taffy Retarget Plan

**Date**: 2026-06-04
**Status**: **Complete.** All phases shipped (P1–P4 + mere/app retirement); morphorm
dropped; the tiled workbench renders through serval/taffy. Ready to archive once the
DOC_README index line lands.
**Decision provenance**: the [serval-as-host evaluation §7](../technical_architecture/2026-05-29_serval_as_host_evaluation.md)
owns the call ("Morphorm is obviated; platen survives, reshaped"); the
[serval host flip plan, Phase 2](2026-06-01_serval_host_flip_plan.md) is the
execution frame; [modular integration plan, S4](2026-06-02_modular_integration_plan.md)
schedules it. Mark's 2026-06-04 calls: **full Phase-2 in platen** (platen takes the
serval dep and emits the tile-tree DOM), **move the tiling model into platen now**,
and **replace the legacy frame model now** (not coexist) per DOC_POLICY §3.

> Note: this doc's `DOC_README.md` index line is pending. Another agent is mid-edit
> on `DOC_README.md`; the line lands once that file is free, to avoid sweeping their
> uncommitted work (per the no-sweep standing constraint). Flagged per DOC_POLICY §9.

---

## Goal

Drop morphorm from platen. The workbench tile geometry comes from serval's taffy
(the engine that already lays out the chrome), and platen owns the tiling model +
the DOM emission. One layout engine in the stack, not two.

## Findings (verified against the code, 2026-06-04)

- **The morphorm seam is small.** [`platen::layout`](../../../crates/platen/platen/src/layout.rs)
  (`layout_plan` → morphorm Row/Column tree → rects) and
  [`platen::layout_node`](../../../crates/platen/platen/src/layout_node.rs) (the
  morphorm `Node` glue) are the whole of it. `project_tree` (forme `Arrangement` →
  geometry-free `WorkbenchPlan`) is **not** morphorm and stays.
- **The authoring layer is `xilem_serval`** (`el`, `on_click`, `lens`, `AnyView`,
  `ServalAppRunner`) + `serval_scripted_dom`. meerkat already runs it for the chrome
  and already reads laid-out rects back from a `ScriptedDom` (`measure_class_bottom`,
  `fragments_from_scripted_dom`, `hit_test_node`). So "tile-tree as taffy-laid-out
  DOM, read content rects back" reuses paths that already exist.
- **serval can composite external textures** (`PaintCmd::DrawExternalTexture` +
  `install_external_texture`, `components/paint/netrender_painter.rs`), but
  **`xilem_serval` exposes no element that emits one** (only the generic `el`), and
  serval is a clear field for documentation only right now. So tile **content**
  cannot live fully inside the serval DOM this pass without serval work.
- **platen-core is GUI-free** and deliberately keeps the renderer host-side (its
  Cargo.toml: "the heavy renderer stays host-side and this dep does not propagate
  through the contract crates"). Its dependents are `meerkat`, `orrery-host`, the
  legacy `mere/app`, and `platen/domain/workbench`. Putting `xilem_serval` in
  platen-core would push serval into all of them.
- **`platen/domain/workbench` is not the tiling state** — it is the a11y/uxtree
  projection of platen's `WorkbenchProjection`. The tiling model belongs in
  platen-core.
- **platen-core already has a `workbench` module, and it is a *different*, legacy
  model**: `FrameState` / `PaneBinding` / `WorkbenchProjection` keyed by `NodeKey` +
  `PaneId` over verso-core surfaces (the pre-spine frame/pane model). meerkat's
  `Workbench` is the forme-`Arrangement` slots/stacks model. Mark's call: **replace**
  the legacy one with the forme model.
- **The legacy frame model is nearly dead — replacing it is contained** (recon
  2026-06-04):
  - The live host is insulated: `session-runtime` (meerkat's chain) **does not depend
    on platen at all**; it uses the modern `frame` crate only for `GraphId` /
    `SessionId` / `PaneId`.
  - The modern `frame` crate doesn't use the legacy types (it names
    `platen::FrameState` only in a "legacy" comment).
  - Type-level consumers of the legacy model: **only** `platen/domain/workbench`
    (a11y) — which has **no reverse-deps** (a leaf). `signal_router` references a
    signal *name* (`WorkbenchProjectionRefreshRequested`), not the types. `mere/app`
    uses **zero** of the frame-model types.
  - So replacing the model touches: the a11y leaf crate (retarget or stub) + the
    platen exports. Nothing in the live host chain.
- **morphorm-drop wrinkle**: the legacy `mere/app` *does* call `platen::layout`
  (`panes.rs`: `layout_plan`, `LaidOutSlot`, `LayoutConfig`, `TilePlan`). Fully
  deleting `platen::layout` needs `mere/app` off it first — a trivial inline
  equal-weight split in `panes.rs` (it is the retiring host; no need to keep morphorm
  alive or retire the app early).

## Architecture (target)

- **platen-core (`crates/platen/platen`), stays GUI-free.** Gains the tiling
  **model**: slots, tab-stacks, active tab, per-slot weights, serialization, over
  forme's `Arrangement` + the existing `project_tree` / `WorkbenchPlan`. The morphorm
  layout is **deleted**; platen-core carries no layout engine (serval lays the DOM
  out downstream).
- **New crate `platen-view` (serval-coupled), only meerkat depends on it.** A
  `workbench_view(&model) -> View` that emits the tile tree as flex DOM: a row of
  slot columns, each a tab strip + a content-slot placeholder tagged with its
  member (and texture key). Depends on platen-core + `xilem_serval` +
  `serval_scripted_dom`. This keeps the serval dependency off orrery-host and the
  legacy app, matching platen-core's existing host-side-renderer discipline. (A
  feature gate on platen-core was rejected: workspace feature unification would pull
  serval into the other dependents anyway, and a dead cfg gate is the pattern Mark
  dislikes.)
- **meerkat** drives the platen model, owns a second `ServalAppRunner` over
  `platen-view`'s view (a "workbench root", composited in the content band in Tree
  mode the way chrome + orrery are), reads each content slot's rect back from that
  DOM, and composites the constellation actor's texture there. This is today's
  mechanism; only the rect source changes (taffy, not morphorm).

### Deferred (flagged, not in this pass)

- **In-DOM content-roots.** Tile content as a real serval external-texture element
  (so serval paints it), gated on a serval/`xilem_serval` canvas-or-external-texture
  element. Until then, content is host-composited at the DOM-derived rect.
- **Draggable dividers.** flex-basis updated on an `on_pointer` drag (serval has the
  pointer-drag foundation). First cut is equal-weight, matching current behavior.
- **Floaters / sticky-notes.** The hybrid absolute-positioning modes (eval §7's
  sub-choice). Docked flex tree first.

## Phases (done-conditions, not dates)

- **P1 — Replace the legacy frame model with the forme tiling model.** Port
  `meerkat::workbench::Workbench` (slots/stacks/active, open/close/group/split/
  activate, `laid_out_slots`) into platen-core's `workbench` module, **replacing** the
  legacy `FrameState`/`PaneBinding`/`WorkbenchProjection`. Update platen's exports
  (drop the frame-model re-exports, add the model). Retarget or stub the a11y leaf
  crate `platen/domain/workbench` (its `WorkbenchProjection`→uxtree projection is now
  obsolete; an a11y projection of the forme model is a flagged follow-on). Switch
  meerkat to `platen`'s model and delete `meerkat::workbench`; the workbench unit tests
  move to platen. morphorm `layout_plan` stays for now (render path unchanged).
  *Done*: platen owns the one workbench model, meerkat drives it, workspace green.
- **P2 — `platen-view` emits the tile-tree DOM.** New crate; `workbench_view` emits
  the flex DOM; headless unit tests assert the emitted node/class structure (a row of
  N slot columns, each strip + content placeholder, tagged per member).
  *Done*: the view crate builds + tests the DOM shape.
- **P3 — meerkat runs the workbench root (parallel to morphorm).** A second
  `ServalAppRunner` lays out `platen-view`'s DOM; meerkat composites the band and
  reads content rects back, compositing actor textures there. Gated behind the
  existing morphorm path so both can be compared on screen.
  *Done*: the tiled view renders through serval, at parity with the morphorm path.
- **P4 — Flip + delete morphorm.** meerkat's Tree branch uses the serval path; give
  the legacy `mere/app` `panes.rs` a trivial inline equal-weight split so it no longer
  calls `platen::layout`; delete `platen::layout` + `platen::layout_node`; drop the
  `morphorm` workspace dep. *Done*: morphorm is gone, the tiled view renders through
  serval/taffy, the whole workspace builds + tests green.

## Risks

- **serval dep propagation** — mitigated by the separate `platen-view` crate
  (platen-core stays serval-free).
- **DOM-rect readback timing** — the workbench root is laid out after its scene is
  built, so content rects are one frame behind, the same one-frame-lag pattern the
  current tile strips already use (`set_tile_strips` requests a redraw on change).
  Acceptable; note it rather than fight it.
- **Cartography coexistence** — Tree mode composites the workbench root; Cartography
  composites the orrery. They are mutually exclusive bands, as today.

## Progress

- **2026-06-04** — Approach decided with Mark (full Phase-2 in platen; model moves
  into platen now; replace the legacy frame model). Plan grounded in the code
  (morphorm seam, xilem_serval authoring layer, serval external-texture gap, platen
  dependents). Recon confirmed the legacy frame model is contained (live host chain
  insulated; only the a11y leaf + a signal name consume it).
- **2026-06-04 — `mere/app` retired** (`0066070`). Mark's call: only meerkat. The
  legacy Xilem+Masonry host was a leaf binary (no reverse-deps); removed from the
  workspace members + deleted. Clears the last morphorm consumer outside meerkat.
- **2026-06-04 — P1 done** (`c8a2ed7`). `platen::workbench` is now the forme tiling
  model (moved from meerkat), replacing `FrameState`/`PaneBinding`/
  `WorkbenchProjection`. The a11y leaf crate retargets onto it (structure-only
  `Workbench`→uxtree). meerkat consumes `platen::Workbench`; its workbench module +
  tests moved to platen. morphorm `layout_plan` still in place. Whole workspace
  builds; platen 44, meerkat 22+48, workbench(a11y) 4 green.
  - *Follow-on logged*: richer a11y labels (resolved titles/URLs from the graph) for
    the workbench projection — deferred from P1's structure-only projection.
- **2026-06-04 — P2 done** (`2f5bfa2`). New `platen-view` crate: `workbench_view`
  emits the tile tree as `xilem_serval` flex DOM (a row of `wb-slot` columns, each a
  `wb-strip` tab row over a `wb-content` placeholder tagged `data-member`).
  `WorkbenchScene` is the geometry-free runner state, built from the model via
  `from_workbench(&Workbench, &Graph)`; clicks record a `WorkbenchAction` to drain.
  `WORKBENCH_SHEET` carries the flex CSS. 5 headless DOM-shape tests green. platen-core
  stays serval-free; only platen-view couples. Confirms the xilem_serval API
  understanding (the crate compiles + diffs against the real authoring layer).
- **2026-06-04 — P3 done** (`cf8fe30`). meerkat flips Tree mode to the serval
  workbench root: a second runner over the `WorkbenchScene`, synced from the model +
  graph + pin state each frame; the workbench DOM is rasterized as the content band
  (taffy lays the tiles out), and each `wb-content` placeholder's rect + `data-member`
  is read straight back (`fragments_from_scripted_dom` / `rect_of`) to composite that
  tile's actor texture there. Tree clicks route to the workbench root; a tab switch /
  close / pin is drained onto the model. platen-view gained pin parity (no regression).
  Decided to flip directly (not run parallel to morphorm) since mere/app was retired.
- **2026-06-04 — P3 cleanup** (`d9ea25f`). Removed the now-dead chrome-composited tile
  strips (TileStrip/TileTab/TileAction + methods + view + styles + test) — the serval
  workbench root owns the strips now.
- **2026-06-04 — P4 done** (`7fce7b9`). **morphorm dropped.** Deleted the morphorm
  files (`platen::layout`, `layout_node`) and the laid_out_slots/PlacedSlot/PlacedTab
  geometry from the model; removed the workspace pin + platen's dep (gone from
  Cargo.lock). The workbench is geometry-free; layout is serval/taffy via platen-view.
  `tree_projection` kept (the spine projection, not morphorm). Whole workspace builds;
  all crate tests green.
  - *Follow-on logged*: `slot_views` (P1) and `project_tree`/`WorkbenchPlan` are now
    parallel geometry-free reads of the model; `project_tree` has no caller after the
    flip. Consider unifying platen-view onto `project_tree` or retiring it — separate
    cleanup, not load-bearing.
- **2026-06-04 — first-run fixes** (`ca68dd2`). From Mark's first GUI run:
  - *Fill the band*: the workbench root had no definite size (taffy hands the root the
    viewport as *available* space, not a definite size), so the flex row shrank to
    content — narrow slots, zero-height content, empty tiles. `WorkbenchScene` now
    carries the band `viewport` and the root is sized to it. (This is why the screenshot
    tiles were dark; the rect readback was reading ~0-height content rects.)
  - *Double-click to open*: a double-click on an orrery node opens the tiled workbench
    from it (the gesture Mark wanted; Ctrl+T + the toolbar button still toggle).
  - *Stacks*: `ContextAction::TileGroup` → `Stack`, menu row "Open in a stack",
    `Workbench::group_all` → `stack_all`.
  - *Still open*: drag is unwired in the tiled view (tab reorder/move + divider resize);
    a deferred follow-on, flagged with Mark.
- **2026-06-04 — second-run fix + the drag trio.** From Mark's runs of the tiled view:
  - *Absolute tile rects* (`cfacec5`): only the rightmost tile presented, in the wrong
    slot — taffy's `final_layout` (what `rect_of` returns) is parent-relative, so every
    slot's content reported the same slot-local origin and the tiles stacked. Fixed by
    summing the workbench > slot > content offset chain.
  - *Drag D1* (`e05cc70`): drag a tab to move it between slots / reorder within a slot
    (`Workbench::move_to_slot_of`); the tab activates on press, the drop slot is read
    from the content placeholder's `data-member`.
  - *Drag D2* (`6eea2fb`): drop-target highlight (the slot's strip lights up,
    `WorkbenchScene.drag_target`) + zone drop — outer quarter splits the tab out
    (`split_beside`), center moves / stacks. The host records each tile's window rect
    per frame to resolve target + zone.
  - *Drag D3* (`b096e6f`): per-slot width weights (`Slot.weight`, `weights` /
    `set_weights`); platen-view emits `flex-grow: weight` + `wb-divider` gutters; a
    divider drag reweights the two neighbours.
  - *Open follow-ons*: a drag ghost/preview (only the target highlights today); richer
    a11y labels; unify `slot_views` with `project_tree`.
