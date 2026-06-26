# Host scroll + layout-rect engine adoption

**Date**: 2026-06-25
**Status**: Planned. From the [serval capability-misuse sweep](2026-06-25_context_submenus_plan.md) (4-agent
sweep, 2026-06-25): the single largest "host reimplements an engine feature" cluster.
**Owners**: serval-layout (expose accessors) + meerkat (adopt them).

## Problem

The chrome panes are statically `overflow: scroll`, but meerkat reimplements all the nested-scroll
bookkeeping the engine now owns, and re-derives engine geometry in several places:

1. **Nested scroll is hand-rolled.** Per-pane `f32` offset fields on `WindowView`
   ([window_view.rs:230](../../../crates/meerkat/src/window_view.rs)), wheel routed by raw rect
   comparison ([app_handler.rs:683](../../../crates/meerkat/src/app_handler.rs)), lower-clamp by hand,
   offsets mirrored into both render (`chrome_scroll`) and every hit-test
   ([input.rs](../../../crates/meerkat/src/input.rs) `chrome_click` / `swatch_geometry_at`). The engine
   ships `IncrementalLayout::scroll_at(dom, x, y, dx, dy)` (`serval-layout/incremental.rs:512`) which
   hit-tests the point, walks to the scroll container, clamps to `scroll_extent`, writes the retained
   `element_scroll` map, and merges into the next paint + hit-test. The 2026-06-14 engine capability
   audit states the host wiring should be "one line: wheel -> scroll_at."
2. **`scroll_extent` re-derived 3x.** `roster_view::max_scroll`, `list_pane::max_scroll`, and render's
   scroll-into-view all compute `content_size - inner_box`. The engine's `scroll_extent`
   (`incremental.rs:570`) is identical but private.
3. **Absolute-origin chain-summation re-rolled 3x.** `accumulate_origins`
   ([serval_render.rs:231](../../../crates/meerkat/src/serval_render.rs)), the submenu `abs_origin`
   closure ([render.rs:579](../../../crates/meerkat/src/render.rs)), and the swatch loop in
   `swatch_geometry_at`. The engine computes this as `ServalLaneView::box_model` (getBoundingClientRect-
   shaped, `serval_lane.rs:204`) but exposes it only keyed by `SourceNodeId`, not as a direct
   `absolute_rect(node)` on the `IncrementalLayout` the host holds.
4. **Popup-list scroll-into-view recomputed per frame** (palette / context menu / submenu). The engine's
   `scroll_element_into_view` (`incremental.rs:351`) scrolls only the document viewport and explicitly
   defers nested containers. A genuine engine gap, not host misuse.

## Plan

**P1 — `absolute_rect` accessor + dedup the origin walks (smallest, decoupled).**
- serval: surface `absolute_rect(node) -> Rect` (or `box_model(node)`) directly on `IncrementalLayout`
  (it already has `fragments()`; add the ancestor-sum the host re-rolls). Reuse `serval_lane::absolute_origin`.
- meerkat: replace `accumulate_origins`, the submenu `abs_origin` closure, and the swatch loop with the
  accessor. (The submenu closure is the newest copy, from the just-landed submenu work.)
- Done: one host call site each; no parent-chain summation left in meerkat.

**P2 — wheel -> `scroll_at`; delete host scroll state.**
- meerkat: route the wheel handler through `scroll_at`; remove the per-pane `WindowView` offset fields,
  the rect-routing, the manual lower-clamp, and the `chrome_scroll`/offset-mirroring plumbing into render
  + hit-test (the engine's `merged_scroll` covers caller offsets, per the audit).
- meerkat: delete `roster_view::max_scroll` / `list_pane::max_scroll` (absorbed by `scroll_at`'s clamp),
  or, if any caller still needs the extent, consume a newly-public `scroll_extent` accessor.
- Done: no per-pane scroll offset on `WindowView`; wheel is a single `scroll_at` call; hit-test reads the
  engine's merged scroll.

**P3 — nested scroll-into-view (engine gap).**
- serval: extend `scroll_element_into_view` to nearest-scroll-container alignment (it currently does only
  the viewport). Then meerkat drops the per-frame center-the-active-row recompute in render for the
  palette / context-menu / submenu lists.
- Done: render no longer recomputes a scroll target for the popup lists; the engine centers the active row.

## Findings

- The `chrome_scroll: ScrollOffsets` seam is the host's current way to feed offsets to the renderer; the
  engine's `element_scroll` retained map is the engine-side equivalent. P2 collapses the two.
- Risk surfaced by the submenu review (deferred there, addressed here): the submenu anchor and the menu
  hit-test both ignore the root menu's paint-time scroll offset. P1+P2 fix the root cause (the host stops
  carrying a parallel scroll model).

## Progress

- 2026-06-25: Drafted from the capability sweep. Not started.
