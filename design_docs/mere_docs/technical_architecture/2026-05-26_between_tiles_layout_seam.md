# Between-Tiles Layout Seam — Morphorm in platen

**Date**: 2026-05-26
**Status**: Canonical for the layout seam. Refines the
[composition spine](2026-05-21_mere_composition_spine.md) §9 ("Projection +
realization") and the [re-scaffold](2026-05-21_app_architecture_rescaffold.md)
§4 ("Engines & tiles"). Crate-side authority **built + tested**
(`platen::layout`); host adoption is a named follow-on (§4).

---

## The seam

> **Platen owns the arrangement *between* tiles. Masonry owns the content
> *within* each tile.**

The composition spine already split *what a workbench is* (`forme`) from *how
it is laid out* (`platen` projections). The tree projection
(`platen::project_tree`) produces a **geometry-free** `WorkbenchPlan` — slots
laid side by side, members joined by `StackedWith` collapsed into tab-stacks.
What it did *not* answer: where each slot actually sits in a viewport. The
re-scaffold left that to the host, which laid slots out with Xilem `flex_row` /
`flex_col`. That worked for v1 but put the between-tiles geometry in the view
layer — the wrong owner under the spine, and a dead end for resizable split
ratios, nested splits, and responsive re-flow.

This seam moves between-tiles geometry into platen, where it belongs, using
[morphorm](https://crates.io/crates/morphorm) as the engine. Morphorm is a
standalone, GUI-free layout solver (four units — Pixels / Percentage / Stretch /
Auto — over Row / Column / Overlay / Grid). Platen implements morphorm's
`Node` / `Cache` traits over its own tree and reads back a rect per slot. The
host then *places* tile content into those rects; Masonry lays out only what is
*inside* a tile (text blocks, the graph canvas, engine output).

## Why morphorm (not hand-rolled, not Masonry flex)

- **It is the right owner.** Between-tiles arrangement is a projection concern,
  not a view concern. Putting it in platen keeps the spine honest: `forme` →
  `platen` (structure *and now* geometry) → host (realization).
- **It is GUI-free.** Morphorm provides no widgets, no tree, no containers — you
  implement `Node`/`Cache` for your own types. So platen takes a layout solver
  without taking a UI framework, and the result is unit-testable without a
  window (four tests in `platen::layout` assert exact rects).
- **It nests.** A forme arrangement maps node-for-node onto a morphorm tree, so
  nested splits and grouped sub-arrangements are the same recursion, not a
  special case — the capability the spine's non-tree benches (corridor, lattice,
  compare-fan) will need.
- **Masonry flex can't express it.** Flex re-derives weights every frame inside
  the view layer; it has no place to hold persisted split ratios keyed
  `(FormeRef, ProjectionKind)`, and double-computes a layout platen should own.

## The mapping (node-for-node)

`platen::layout::layout_plan(plan, viewport, config)` builds a morphorm tree
from a `WorkbenchPlan` and returns a `LaidOutPlan` (a `PortableRect` per slot):

| `WorkbenchPlan`       | morphorm node                                             |
|-----------------------|-----------------------------------------------------------|
| root (slots in a row) | `Row`, `Pixels(viewport)`, gap = splitter gutter          |
| `PlanSlot::Tile`      | a `Stretch` leaf                                          |
| `PlanSlot::Tabs`      | a `Column`: `Pixels(strip_height)` strip leaf + `Stretch` content leaf |

`LaidOutSlot::Tabs` carries the whole slot rect, the strip rect (across the
top), and the content rect (below it, where the active tab — the first —
renders). Inactive tabs are within-tile chrome, not separate rects.

**Coordinates.** Morphorm writes parent-relative positions into the cache. The
root sits at the origin, so its direct children (the slots) come out
viewport-absolute; a tab-stack's strip/content are offset by their slot's
origin during readback. Output rects are `kernel::geometry::PortableRect`
(consistent with `FrameViewModel.active_pane_rects`).

**Split ratios are projection-state, not structure.** v1 lays every slot out
with equal `Stretch(1.0)` weight. Per-slot weights — the resizable splitter
ratios persisted `(FormeRef, ProjectionKind)` per the spine §9 — feed in as
`Stretch(weight)` when the resize gesture lands. The structure here does not
change to add them.

## What this does *not* move

- **Within-tile layout stays Masonry's.** Text block stacking, the orrery's
  node positions, engine document flow — all unchanged, all inside a tile rect.
- **The orrery (cartography projection) is untouched.** This seam is the *tree*
  projection's geometry. Cartography has its own world-position geometry; the
  `GraphCanvas` widget keeps owning its camera and node placement.
- **FrameTree (OS-window pane splits) is untouched.** It is a deliberately
  tree-shaped chrome authority above the workbench, not between tiles.

## 4. Host adoption (the named follow-on)

The authority is built and tested; the live `mere-app` workbench pane still uses
`flex_row` / `flex_col`. Consuming `LaidOutPlan` means *absolute* placement —
the host positions each tile at a platen-computed rect rather than letting flex
re-derive it. Xilem offers two routes, both needing runtime verification (GUI
layout the headless toolchain can't check):

1. **Composition** — `zstack` (top-left aligned) of `transformed(sized_box(tile)
   .fixed_width(w).fixed_height(h)).translate((x, y))` per slot. Cheap (no new
   widget) but leans on `transformed` for interactive children; verify pointer
   routing into `text_input` / the canvas survives the affine.
2. **Custom tiling widget** — a child-hosting Masonry widget that lays its
   children out at the platen rects (the shape `graph_canvas.rs` already follows
   for paint, extended to host child `Pod`s). More code, but the robust,
   clip-correct, verso-shaped answer — and the natural home when verso's
   realization layer lands.

**The double-layout caveat.** Whichever route, the host must *not* also run flex
over the same slots — that re-computes the geometry platen owns. Absolute
placement (fixed-size boxes at fixed origins) is the discipline that keeps the
seam clean.

Recommended sequencing: do the composition spike (route 1) first to close the
loop and feel the interaction, then graduate to the custom widget (route 2) when
verso lands or when route 1's affine handling proves fragile.

**Route 1 landed (2026-05-26).** The workbench pane (`mere-app/panes.rs`) now
consumes `LaidOutPlan`: each slot is placed via `transformed(sized_box(view)
.fixed_width/height(..)).translate((x, y))` inside a top-left `zstack` stretched
to fill (`Dimensions::STRETCH`). `transformed` sets a real `WidgetPod` transform
(border-box coordinate space), so pointer hit-testing follows the placement —
the tiles are interactive, not just painted in place. The tiling region's size
round-trips through `AppState.content_size` via a `resize_observer` (one-frame
lag on resize, then stable; the loop is safe because the stretched `zstack`'s
measured size is independent of where tiles are placed inside it). Flex no longer
lays the slots out, so the double-layout caveat holds. **Runtime GUI
verification pending** (the headless toolchain can't check visual layout): watch
for first-frame placement at the default `content_size`, and that pointer events
land through the affine. Route 2 (custom child-hosting widget) remains the
robust, clip-correct graduation, especially once verso lands.

## Status

- `platen::layout` — **built + tested** (`layout_plan`, `LaidOutPlan`,
  `LaidOutSlot`, `LayoutConfig`; morphorm glue in `platen::layout_node`). Four
  unit tests assert the gap split, the tab strip/content split, and full-height
  stretch.
- `morphorm = "0.8"` — new external workspace pin.
- Host adoption — **pending** (§4).
