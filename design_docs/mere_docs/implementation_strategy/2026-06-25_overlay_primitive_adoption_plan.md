# Overlay primitive adoption

**Date**: 2026-06-25
**Status**: Done 2026-06-25 (P1 + serval `overlay_rect` + P2 card + P3 helper, committed). From the
[serval capability-misuse sweep](2026-06-25_context_submenus_plan.md).
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
- 2026-06-25: **P1 done.** (a) the stale stacking docs in `overlay.rs` + `select.rs` already describe
  the z-index / Appendix-E model and call the "must be last sibling" rule obsolete (landed `fa5f32a`);
  (b) the submenu builds via `overlay_at(0,0,..)` and (c) the context menu via `overlay_at(menu.x,
  menu.y,..)` (also `fa5f32a`); the tear-out ghost now builds via `overlay_at(0,0,..)` too (`79dc637`),
  so render only re-stamps the live cursor left/top. No hand-written `position:absolute;left/top` left
  at the menu / submenu / tear-ghost **build** sites.
- 2026-06-25: **P2/P3 finding (sequencing).** P2 wants `focus_card_view` to feed `overlay_at`, but the
  focus card needs width/height + shadow and `overlay_at` emits only left/top — adding a `.attr("style",
  ..)` would clobber the position. That is exactly P3's size-carrying variant. So the natural order is
  **P3 first** (add serval `overlay_rect(rect, content)`), then P2 adopts it in the card (with
  `anchor_point` + `Placement` for the right/flip-left math, keeping the vertical-center + band clamp),
  then P3 also lands comms-pane + shellbar. `anchored_card_rect` is host-side (called from render), and
  the shell view already rebuilds each frame, so there is no per-frame-rebuild regression in moving the
  card's position into the view.
- 2026-06-25: **P3 serval + P2 done (committed).** serval `overlay_rect(x, y, w, h, content)` added +
  tested (`edafb400`); the focus card adopts it (`2bd6336`) — `anchored_card_rect` routes its right/
  flip-left through `anchor_point` + `Placement` (node as a gap-wide keep-out box), and the three card
  kinds build via `overlay_rect` (geometry) with visuals on an inner fill div. 160 bin tests green.
- 2026-06-25: **P3 comms/shellbar finding (needs a call).** Adopting `overlay_rect` at the *build* site
  for the comms pane + shellbar is not a clean win: their geometry is a **layout output** (`comms_rect`
  from the frame-tree leaves `render.rs:401`; the shellbar rect from window `w`/`h` + toolbar height,
  post-layout), which is exactly why render patches it via `set_attribute` after layout. `overlay_rect`
  is build-time, so owning that geometry would need the host to feed post-layout rects into `Chrome`
  state and rebuild the chrome view — a layout → feed → rebuild cycle heavier than the current targeted
  patch, risking two-pass layout per frame. The card was the right `overlay_rect` consumer (its rect is
  build-time state); for comms/shellbar the render-time `set_attribute` reads as the *correct* pattern
  for layout-dependent geometry, not a misuse. Options: (a) leave comms/shellbar render-patched and treat
  P3 as served by `overlay_rect` + the card; (b) a small shared helper formatting the geometry string
  render hand-writes (cosmetic dedup, no behaviour change); (c) the full state-threading refactor
  (heaviest, questionable value). Recommend (a), optionally (b).
- 2026-06-25: **P3 closed via (b); overlay plan complete.** Chose the geometry-string helper: render's
  two hand-written comms / shellbar geometry strings now route through one `overlay_geometry_style(x, y,
  w, h, flex)` (`6e7a142`), mirroring `overlay_rect`'s geometry (the shellbar's optional flex edge
  included). No behaviour change; the surfaces stay render-patched because their rect is a layout output.
  **Overlay plan done:** P1 (`fa5f32a`/`79dc637`), serval `overlay_rect` (`edafb400`), P2 card
  (`2bd6336`), P3 helper (`6e7a142`).
