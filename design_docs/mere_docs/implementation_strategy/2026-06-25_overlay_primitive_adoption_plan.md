# Overlay primitive adoption

**Date**: 2026-06-25
**Status**: P1 executing 2026-06-25 (a/b/c below). From the [serval capability-misuse sweep](2026-06-25_context_submenus_plan.md).
**Owners**: serval (overlay docs + a size-carrying variant) + meerkat (adopt `overlay_at`/`anchor_point`).

## Problem

xilem-serval provides `overlay_at(x, y, content)` (point-anchored popup), `anchor_point(trigger, popup,
Placement)` (element-anchored, with flip), and `Placement` (`overlay.rs`). meerkat predates them and
hand-writes `position: absolute; left/top` strings, re-stamped each frame in render, for most floating
surfaces. The nested submenu ([context_submenus_plan](2026-06-25_context_submenus_plan.md)) is the one
correct `anchor_point` consumer, but even it left a hand-rolled placeholder.

Surfaces and the right primitive:
- **Context menu** ([views.rs:288](../../../crates/meerkat/src/views.rs) build + [render.rs:519](../../../crates/meerkat/src/render.rs) re-stamp) — point-anchored, `overlay_at`. The two writes race (view sets, render overwrites same frame).
- **Submenu panel build** ([views.rs:322](../../../crates/meerkat/src/views.rs)) — a `menu.x + 200.0` placeholder render.rs immediately overwrites via `anchor_point`. Build with `overlay_at(0,0,...)`.
- **Tear-out ghost** ([render.rs:506](../../../crates/meerkat/src/render.rs)) — point-anchored to `cursor+12`, `overlay_at`.
- **Object/focus card** ([card.rs:596](../../../crates/meerkat/src/card.rs) `anchored_card_rect`) — hand-rolls the exact RightOf/LeftOf-with-flip math `anchor_point` + `Placement` provide; the same pattern the submenu now uses correctly.
- **Comms pane + shellbar** ([render.rs:690](../../../crates/meerkat/src/render.rs), `659`) — re-stamp position **and** width/height/flex each frame. `overlay_at` only emits left/top, so these motivate a size/edge-carrying variant rather than per-surface re-stamps.

Also stale: the overlay's own docs describe the **pre-z-index** stacking model (must be last sibling), which
directly misled the submenu work (it caused the `.context-menu-layer` wrapper bug). serval-layout now has
full CSS 2.1 Appendix E stacking + z-index (`paint_stacking.rs`).

## Plan

**P1 — the cheap, high-value slice (a/b/c, executing 2026-06-25).**
- **(a)** Fix the stale stacking docs in `xilem-serval/overlay.rs:6-30` (+ the `overlay_at` doc) and
  `xilem-serval/select.rs:16-18`: position:absolute auto-lifts above in-flow content, z-index orders within
  a stacking context, the "must be last sibling / no z-index" rule is obsolete.
- **(b)** Replace the submenu `menu.x + 200.0` placeholder with `overlay_at` (let the render `anchor_point`
  pass own the real position). Removes the magic guess + the duplicate position string.
- **(c)** Build the context-menu panel and the tear-ghost via `overlay_at` instead of hand-written
  position strings. The render-side viewport clamp (max-height/overflow) stays a separate concern.
- Done: no hand-written `position: absolute; left/top` for the menu / submenu / tear-ghost build sites;
  the overlay docs match the engine.

**P2 — `anchored_card_rect` -> `anchor_point` (dedup the flip math).**
- Replace the bespoke RightOf/LeftOf+flip in `card.rs::anchored_card_rect` with `anchor_point` +
  `Placement`, feeding `overlay_at` in `focus_card_view`. Keep the card-specific vertical-center + band
  clamp. Removes a second copy of the side/flip logic the submenu now shares.

**P3 — a size/edge-carrying overlay variant (serval).**
- The comms pane and shellbar need width/height (and the shellbar a flex edge), beyond `overlay_at`'s
  left/top. Add an overlay variant that carries size (e.g. `overlay_rect(rect, content)`), then adopt it
  in render for those two surfaces so they stop re-stamping a full geometry string each frame.

## Non-goals

- The command palette + find bar are intentionally in-flow flex-centered/flex-end surfaces, not
  point-anchored; `overlay_at` does not apply. The suggestions dropdown is plain in-flow document order.

## Progress

- 2026-06-25: Drafted from the capability sweep; P1 (a/b/c) in flight.
