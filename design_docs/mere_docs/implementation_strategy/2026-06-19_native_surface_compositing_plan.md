# Native Surface Compositing Plan: overlays above embedded WebView surfaces

**Date**: 2026-06-19
**Status**: Planning (direction set with Mark; no code yet). How meerkat's chrome, and its
modal overlays especially (context menu, palette, find, settings), composites above embedded
native surfaces (scrying System WebViews) and the host-composited content layers, so an overlay
is never occluded by content.
**Code**: `crates/meerkat/` (render.rs compositing + present, scrying_host.rs), `wgpu-scry/scrying`
(the WebView2 producer).

Sibling docs:

- [unified_document_host_plan](2026-06-17_unified_document_host_plan.md): the one-shell-document
  chrome; the context menu lives in that document. This plan owns the layer *below* it, how the
  shell document composites over embedded native content. The in-document z-order (menu over the
  orrery node cards) was fixed there by document order; this plan is the native-surface half.
- [node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md): the
  orrery node/card model. This plan's "snapshot texture vs live visual" split is the compositing
  half of the decision that the orrery card is a snapshot and the live view moves to pelt.
- [scrying_tile_plan](2026-06-10_scrying_tile_plan.md): the WebView2 producer. This plan changes
  how its output is hosted, not the producer itself.

---

## The problem

A context menu (and any modal chrome overlay) is occluded by embedded content in particular,
sticky states: once a scrying System WebView, a pelt tile backed by one, or its content card is
on screen, it draws over the menu and stays that way for the session. The overlay's clicks are
lost in that region.

## Findings (code-verified 2026-06-19)

1. **Two separable layers.** Everything meerkat *rasterizes* (orrery scene, node cards,
   content-card textures, the chrome) is composited in one sequence in `render.rs`, and the chrome
   composites **last and full-window** (render.rs:1724, placement `[0,0,w,h]`), after the orrery
   scene (1244), pelt tiles (1267), content cards (1307), and the scry texture (1398). So for
   rasterized content the order is already correct: the chrome, with the menu, is on top. A plain
   texture content card does not beat the menu.

2. **Scry bypasses that compositor.** The scrying System WebView is not (only) a texture meerkat
   draws. It is a native WebView2 composition visual, HWND-parented, parked on-screen at the card
   rect and re-positioned every frame (scrying_host.rs:437-442). Windows DWM composites that visual
   **above** meerkat's whole swapchain. Since meerkat renders the entire chrome, including the menu,
   *into* that swapchain, the menu is structurally beneath the scry visual regardless of the
   internal compose order. The `texture_view` composite at render.rs:1398 exists, but the live
   visual sits over it.

3. **The on-top visual is a card-format artifact, not a requirement.** The visual is shown directly
   ("the scrying demo's model") for two card reasons: (a) a card floats, pans, and zooms, so the
   visual is chased every frame via `set_offset`; and (b) DWM culls an off-screen visual, which
   kills the capture, so a card that can leave the viewport must keep its visual live and on-screen.
   A floating, possibly-off-screen card cannot be a texture drawn on demand. A **pelt tile** has
   none of these constraints: a tile is a fixed pane that does not pan, zoom, or leave the viewport.

## The direction

Two surfaces, two hosting modes, the chrome above both:

- **Static snapshot (the orrery card):** a captured **texture**, composited under the chrome at the
  existing render.rs:1398 path. A frozen thumbnail has no live visual, so it never beats an overlay.
  This is the compositing half of the node-rep decision (orrery card = snapshot of the last visit;
  the live view moves to pelt).
- **Live view (a pelt tile):** the WebView2 composition visual goes into **meerkat's own
  composition tree, z-ordered below the chrome**, with the chrome swapchain transparent over the
  tile region. The compositor stacks WebView, then chrome, then overlays; the menu wins because it
  is genuinely above, not because content was hidden. The tile is fixed, so the per-frame
  `set_offset` chase goes away and DWM culling stops mattering.

This retires the floating-live-WebView entirely. The only surface that needed the on-top hack was a
live WebView in a floating orrery card, and the model already moves live content to pelt, so the
orrery never hosts a live visual again. It is also cheaper: no per-frame reposition.

## The present-path change

Today meerkat presents a bare wgpu swapchain, and scrying parks its WebView2 visual as a sibling
that DWM draws on top. The change: present meerkat's window as a small **composition tree**:

- WebView2 tile visuals at the bottom (live pelt content), at their tile rects.
- meerkat's chrome swapchain on top, transparent over the tile regions so the WebView shows
  through, opaque for the chrome and overlays.

DWM then composites bottom to top: WebView visuals, then the chrome, then the chrome's own overlays
(menu, palette), which are opaque in the swapchain. An overlay over a tile is over the WebView
because the whole chrome surface is above it.

## Phases

Done-conditions, not dates.

- **P1, snapshot-texture orrery card.** The orrery card composites only the captured texture (no
  live visual in the orrery); retire the per-frame visual hosting for an orrery-pegged scry. Done
  when a menu over an orrery card is over it.
- **P2, composition-tree present.** Restructure meerkat's present from a bare swapchain to a
  composition tree: chrome swapchain on top (transparent over tile regions), WebView2 tile visuals
  below. Done when the chrome and its overlays sit above all embedded native content.
- **P3, pelt live-WebView hosting.** Host the live System WebView as a pelt tile's content through
  the composition tree (visual below the chrome), not a floating card; per-tile, fixed-rect, no
  offset chase. Done when a System WebView lives in a tile and a menu draws over it.

## Open questions

- Whether DWM lets meerkat order a WebView2 visual below a swapchain visual within one composition
  target cleanly (the `DesktopWindowTarget` / `new_attached` path scrying already uses), or whether
  the chrome swapchain itself must become a composition surface in that tree.
- The transparency contract: the chrome swapchain must be transparent exactly over the live-tile
  rects and opaque elsewhere. The host already clears the chrome scene transparent, so the tile
  regions need to stay un-painted by the chrome.
- macOS / Linux: this is Windows/DWM-specific (WebView2). The other targets' embedded-surface story,
  and whether the composition-tree model generalizes, is out of scope here.

## Progress

- 2026-06-19: Plan created from a debugging session. The symptom (a context menu occluded by scry
  System WebViews, pelt tiles, and content cards in sticky states) traced to scry's native-visual
  hosting (scrying_host.rs:437-442) sitting above meerkat's swapchain via DWM, distinct from the
  rasterized compose order (render.rs:1724, chrome-last), which is already correct for textures.
  The in-document menu-over-node-cards z-order was fixed the same session in the shell document
  (document order, chrome last). Direction set with Mark: snapshot-texture orrery + composition-tree
  pelt hosting, retiring the floating live WebView. No code yet.
