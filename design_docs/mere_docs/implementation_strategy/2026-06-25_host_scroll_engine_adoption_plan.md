# Host scroll + layout-rect engine adoption

**Date**: 2026-06-25
**Status**: Planned. From the [genet capability-misuse sweep](../../archive_docs/2026-07-03_completed_plans/2026-06-25_context_submenus_plan.md) (4-agent
sweep, 2026-06-25): the single largest "host reimplements an engine feature" cluster.
**Owners**: genet-layout (expose accessors) + meerkat (adopt them).

## Problem

The chrome panes are statically `overflow: scroll`, but meerkat reimplements all the nested-scroll
bookkeeping the engine now owns, and re-derives engine geometry in several places:

1. **Nested scroll is hand-rolled.** Per-pane `f32` offset fields on `WindowView`
   ([window_view.rs:230](../../../crates/meerkat/src/window_view.rs)), wheel routed by raw rect
   comparison ([app_handler.rs:683](../../../crates/meerkat/src/app_handler.rs)), lower-clamp by hand,
   offsets mirrored into both render (`chrome_scroll`) and every hit-test
   ([input.rs](../../../crates/meerkat/src/input.rs) `chrome_click` / `swatch_geometry_at`). The engine
   ships `IncrementalLayout::scroll_at(dom, x, y, dx, dy)` (`genet-layout/incremental.rs:512`) which
   hit-tests the point, walks to the scroll container, clamps to `scroll_extent`, writes the retained
   `element_scroll` map, and merges into the next paint + hit-test. The 2026-06-14 engine capability
   audit states the host wiring should be "one line: wheel -> scroll_at."
2. **`scroll_extent` re-derived 3x.** `roster_view::max_scroll`, `list_pane::max_scroll`, and render's
   scroll-into-view all compute `content_size - inner_box`. The engine's `scroll_extent`
   (`incremental.rs:570`) is identical but private.
3. **Absolute-origin chain-summation re-rolled 3x.** `accumulate_origins`
   ([genet_render.rs:231](../../../crates/meerkat/src/genet_render.rs)), the submenu `abs_origin`
   closure ([render.rs:579](../../../crates/meerkat/src/render.rs)), and the swatch loop in
   `swatch_geometry_at`. The engine computes this as `GenetLaneView::box_model` (getBoundingClientRect-
   shaped, `genet_lane.rs:204`) but exposes it only keyed by `SourceNodeId`, not as a direct
   `absolute_rect(node)` on the `IncrementalLayout` the host holds.
4. **Popup-list scroll-into-view recomputed per frame** (palette / context menu / submenu). The engine's
   `scroll_element_into_view` (`incremental.rs:351`) scrolls only the document viewport and explicitly
   defers nested containers. A genuine engine gap, not host misuse.

## Plan

**P1 — `absolute_rect` accessor + dedup the origin walks (smallest, decoupled).**
- genet: surface `absolute_rect(node) -> Rect` (or `box_model(node)`) directly on `IncrementalLayout`
  (it already has `fragments()`; add the ancestor-sum the host re-rolls). Reuse `genet_lane::absolute_origin`.
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
- genet: extend `scroll_element_into_view` to nearest-scroll-container alignment (it currently does only
  the viewport). Then meerkat drops the per-frame center-the-active-row recompute in render for the
  palette / context-menu / submenu lists.
- Done: render no longer recomputes a scroll target for the popup lists; the engine centers the active row.

**P4 — consolidate the scrollbar thumb on genet's painter (keep the gray rectangle).**
- The scrollbar thumb (the "gray rectangle") is genet's *own* design: genet-render's `push_scrollbars`
  ([genet/.../genet-render/src/render.rs:188](../../../../genet/components/genet-render/src/render.rs))
  paints a translucent-grey, 8px-wide right-edge thumb (`SCROLLBAR_COLOR` / `SCROLLBAR_WIDTH`). meerkat
  keeps a **copy** in its render glue ([genet_render.rs:258](../../../crates/meerkat/src/genet_render.rs)),
  pulled from pelt-live by the glue-extraction plan. meerkat's copy is the *better* one: it places the
  thumb at the **absolute** origin via `accumulate_origins`, fixing the nested-in-positioned-ancestor case
  genet's own docstring explicitly defers ("nested scrollers would need origin accumulation").
- genet: upstream meerkat's origin-accumulation into genet-render's `push_scrollbars` (reusing the P1
  `absolute_rect` accessor), so genet's scrollbar is correct for nested scrollers.
- meerkat: drop the host copy; genet's painter draws the thumb (meerkat already renders through that
  path). The gray rectangle is preserved, now single-sourced and engine-owned.
- Later: grow toward CSS scrollbar styling (`scrollbar-color` / `scrollbar-width`) and a horizontal
  scrollbar, now that the thumb lives in the engine.
- Done: one `push_scrollbars` (genet's, nested-correct); meerkat no longer paints scrollbars; the look is
  unchanged.

## Findings

- The "gray rectangle" scrollbar is **not** a host invention: genet-render already paints it
  (`SCROLLBAR_COLOR` translucent grey, 8px, right edge). meerkat duplicated it in its glue and *improved*
  it (absolute origin for nested scrollers). So adopting the engine's scroll **keeps** the look — it
  consolidates the two copies onto genet's engine-owned painter (upstreaming meerkat's fix), rather than
  deleting any visual. The host's version is not "worse than genet's"; it is genet's design with the fix.
- The `chrome_scroll: ScrollOffsets` seam is the host's current way to feed offsets to the renderer; the
  engine's `element_scroll` retained map is the engine-side equivalent. P2 collapses the two.
- Risk surfaced by the submenu review (deferred there, addressed here): the submenu anchor and the menu
  hit-test both ignore the root menu's paint-time scroll offset. P1+P2 fix the root cause (the host stops
  carrying a parallel scroll model).

## Progress

- 2026-06-25: Drafted from the capability sweep. Not started.
