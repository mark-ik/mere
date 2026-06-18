# Node Representation and Arrangement Plan — pluggable node forms, live arrangements

**Date**: 2026-06-18
**Status**: Planning (with Mark). The orrery-as-element work (Phase 2 of the
[unified document host plan](2026-06-17_unified_document_host_plan.md)) made the focused
orrery's nodes DOM cards. That was right for the *semantic* half (a11y, JSON-LD legibility,
the expand/content state) and wrong for the *visual* half: it flattened nodes from
draggable, content-typed objects into uniform label pills, and hardwired a node's visual to
one form. This plan generalizes the node's presentation into two pluggable, customizable
layers, **representation** (what a node looks like) and **arrangement** (how nodes lay out),
while the node's *truth* (content, identity, edges) stays authoritative in the kernel + DOM.
**Code**: `crates/orrery/` (orrery + gyre), `crates/meerkat/` (render, window_view, input),
`crates/platen/` (cartography dispatch).

Sibling / converging docs:

- [interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md), the
  parent spine: this plan is its **represent** layer. Per the ownership map it owns the
  Representation set (tile / card / textured-body / shape / scripted) and the LOD
  "materialization is state" machine, plus the **scene-wide** arrangement choice.
  Unified-document-host owns the DOM-materialization mechanism its card/tile forms ride;
  field-regions owns localized/scripted arrangement; window-composition owns the
  external-texture-input bridge the textured-body form rides (P2).
- [window_composition_plan](2026-06-11_window_composition_plan.md) — orrery (authority) vs
  panes (views); this plan is the *node-rendering* layer inside the orrery the pool holds.
- [unified_document_host_plan](2026-06-17_unified_document_host_plan.md) — the DOM/document
  half; the DOM representation forms (tile, card) are its node materialization, and its
  "materialization is state" rule (a glyph by default, a DOM subtree on expand) is the hinge
  between the two plans.
- [modular_integration_plan](2026-06-02_modular_integration_plan.md) — orrery/workbench/gloss
  as projections of graph truth; representation + arrangement are how the orrery projection
  draws.
- [scriptable_field_regions_plan](2026-06-13_scriptable_field_regions_plan.md) — a field region
  is a placed spatial rule region whose rhai rules govern forces, edge-visibility, and node
  *layout* (arrangements) inside its extent. It owns the localized + scripted half of
  arrangement (and the "scripted" representation form shares its rhai substrate); this plan owns
  the per-node representation and the scene-wide arrangement delta.
- [cartography_aether_layout_seam](../technical_architecture/2026-05-29_cartography_aether_layout_seam.md)
  — gyre owns the rapier physics (bodies, colliders, the QueryPipeline, drag-pinning);
  arrangement is a constraint the physics respects, not a replacement for it.
- [cartography_layer_brief](../research/2026-05-10_cartography_layer_brief.md) — the layout
  regimes (force-directed, rapier rigid-body, zone-pinned, …); the arrangement design space.
- [two_natured_kernel_brief](../research/2026-05-30_two_natured_kernel_brief.md) §4 —
  content-authoritative / experience-derived; the through-line (truth vs presentation).

---

## The thesis: presentation is two pluggable layers

A node is the object that represents a website (or any addressable thing) in the graph. Its
*truth* (content, identity, edges) is authoritative in the kernel and broadcast as DOM (the
a11y tree, the JSON-LD). Its *presentation* is experience-derived, and should be free:

- **Representation** — what a single node looks like. Today this is hardwired: a binary
  per-pane flag (`render_as_cards`) toggles two fixed forms (a rich gnode tile vs a flat
  card). A node *cannot* be anything else. Generalize it to a per-node, customizable choice
  over an open set of forms.
- **Arrangement** — how nodes position relative to each other. This substrate is mature: the
  `crates/orrery/arrangements` registry + `apply_strategy_positions` apply a scene arrangement
  each frame, and the [scriptable field regions plan](2026-06-13_scriptable_field_regions_plan.md)
  adds placed, scripted, localized arrangement rules. The delta for the vision (a scene picks an
  arrangement, nodes assume and keep assuming it, customizable) is mostly UX + per-scene
  persistence, tracked here; the scripted/localized half is owned by the field-regions plan.

Mark's framing (2026-06-18): a DOM card is a fine *truth* for a node; the unnecessary problem
is being *locked* to it. The node should be representable as a textured rapier body, a bare
shape, or a scripted decoration just as readily as a card, "any sort of object you can make a
texture for, with attendant physics," so the graph feels alive (with scene decoration and
scripting). Both layers user-customizable. The card is one citizen, not the mandated form.

## Representation — a pluggable per-node form

The same node-truth, rendered as any of an open set of forms. Forms that carry semantics emit
serval DOM (so the a11y / JSON-LD legibility holds); forms that carry liveliness emit a
texture or scene primitive. Every form rides the same gyre rapier body, so drag and physics
work regardless of form (the collider is the hit-target; the visual is decoupled from the
body but co-located at its position).

The open set (initial):

- **tile** — the gnode anatomy: a fixed-footprint colored shape (square = document, rounded =
  menu, circle = feed), the favicon textured on the *face*, the caption beside. The "physical
  chip" feel. Still the live form on secondary panes today (the reference, not lost code).
- **card** — the compact label pill (today's focused-pane form). The natural form for the
  *expand / content* state: label, favicon, and eventually a content preview.
- **textured body** — an arbitrary texture (favicon, thumbnail, rendered content, custom
  image) on the rapier body. The most literal "object you can texture."
- **shape** — bare geometry (the tile without favicon/caption); minimal, cheap, dense graphs.
- **scripted** — a custom-drawn / decorated form, the open-ended liveliness and
  scene-decoration hook.

Architecture:

- A per-node `Representation` (enum first, trait-object if forms need to carry their own
  state), defaulted per content-type and per scene, user-overridable per node. The binary
  `render_as_cards` flag is the seed to generalize.
- The orrery renders each node through its representation's renderer. DOM forms diff into the
  shell document (semantics hold); textured/scripted forms emit onto the body. The
  **external-texture element view** (the window-composition P2-companion lynchpin, still
  unbuilt) is the bridge for the textured/scripted forms once they need GPU content in the
  pane DOM.
- The **"materialization is state"** hinge ties this to the unified-document plan: a node's
  representation can be LOD- and state-driven, a texture-glyph (tile / textured-body) by
  default, materializing into a richer DOM subtree (a card with a live content preview) on
  focus or expand, and demoting to an underlay dot when culled.

What the card form lost vs the tile, and must regain (code-verified, see Findings): footprint
/ solidity, shape-variety-by-content-type, favicon-as-the-face, and selection emphasis beyond
a recolor. Depth is *not* a deficit (the card already carries more shadow than the tile ever
had).

## Arrangement — a mature substrate, a narrow delta

The arrangement layer is largely built. This plan does not rebuild it; it names the delta for
the "scene picks an arrangement, nodes follow it" vision and points at the owners.

- **`crates/orrery/arrangements`** is a real crate: an arrangement *registry* with *adapters*
  (grid, static layouts, a shared base) — the layout implementations that arrange node subsets.
- **Application is wired.** The orrery's `layout_strategy()` + `apply_strategy_positions`, driven
  each frame in render via `platen::project_orrery_strategy` (focus-aware for radial), overlay the
  active arrangement's positions. Because it runs every frame, nodes already *follow* the
  arrangement as the graph changes (a no-op under force-directed, a live overlay under a strategy).
- **Localized + scripted arrangement is owned elsewhere.** The
  [scriptable field regions plan](2026-06-13_scriptable_field_regions_plan.md): a field region is
  a placed spatial rule region whose rhai rules select an arrangement (from this same family) for
  the nodes in its extent, beside forces (gyre couplings) and edge-visibility. That plan owns
  placement, the rhai rule surface, and per-region scoping.

So the delta for Mark's vision is narrow and mostly UX + scope, not a substrate build:

- a **scene-level arrangement choice that persists** per scene/orrery (not just the live picker),
  so a graph re-opens into its arrangement;
- confirming the **dynamic-follow** semantics under the per-frame overlay meet the "keep assuming
  it as nodes/edges arrive" bar (versus a one-shot a new node disturbs);
- the **customization surface** (the per-scene picker; the field-region rhai path for localized
  rules);
- **breadth** (grid + static exist, radial via the focus strategy; add others as wanted).

This plan tracks that delta; the field-regions plan owns the scripted/localized half, and gyre
keeps owning the physics the arrangement is a constraint on (collision, drag-pin, inertia).

## Findings (code-verified 2026-06-18)

From an investigation workflow (old gnode rendering, current card, drag/physics path, label
data) plus targeted reads.

- **Drag / play is intact.** Grab/drag/fling was never lost. The path is winit-driven and
  bypasses serval DOM hit-testing, so DOM cards painting on top cannot steal the grab:
  `app_handler` MouseInput → meerkat `input.rs:483 pointer_down` → orrery `input.rs:117-120`
  `hit_test(world)` sets `self.drag`; CursorMoved → `cursor_moved` past CLICK_SLOP sets
  `physics.set_dragging(true)` + `physics.pin(node, world)` (kinematic drag-pin, neighbors
  react through springs); release → `physics.unpin` + `settle_physics` (momentum continues).
  Caveat to preserve: the grab is gated by `!point_over_card`, which tests `content_rects`
  (full content/preview cards), **not** the small node pills, so the grab still wins under a
  pill. Keep node-card rects out of `content_rects`.
- **The rich representation still exists in the tree.** The gnode tile is the live form on
  secondary panes (`render_as_cards=false`). Its object-feel vocabulary: fixed 36px footprint
  (`NODE_HALF=18`, lib.rs:77; build.rs `width/height:36px`), shape-by-content-type via a CSS
  class (`NodeShape` types.rs:45-56; frame.rs:185-190; `border-radius` 0/9px/50% build.rs:51-52),
  saturated opaque state colors (`node_color` lib.rs:775-784, build.rs:36-47), the favicon
  textured on the tile *face* (`DrawImage`, frame.rs:204-242), the caption *beside* it
  (`.gcaption` left:42px, build.rs:57-58), all against a near-black slate (build.rs:225-227).
  No shadow/bevel; the depth illusion is flat color + silhouette + favicon against the dark
  field.
- **The card form is flat and fixed.** A single translated `<div class="node-card">` with one
  inline style for every node (window_view.rs:353-401); `OrreryCard` carries only
  `label/x/y/color/favicon` (window_view.rs:294-303), no shape, size, or selection field, so
  the snapshot cannot drive variety even if the view wanted to. Selection is folded into
  `color` only (the orange `#f7a440`); no ring/lift/scale.
- **The representation toggle is one binary per-pane flag.** `set_render_as_cards` (lib.rs:790-792)
  drops the gnode + favicon layers (frame.rs:154-158) and the host draws cards instead. This
  is the seed to generalize into a per-node representation.
- **The arrangement substrate is mature.** `crates/orrery/arrangements` is a registry + adapters
  (grid, static_layouts, a shared base); the orrery's `layout_strategy` + `apply_strategy_positions`
  apply the active arrangement each frame via `platen::project_orrery_strategy` (focus-aware for
  radial, a no-op under force-directed). The
  [scriptable field regions plan](2026-06-13_scriptable_field_regions_plan.md) extends it: a placed
  field region's rhai rules scope an arrangement (+ forces + edge-visibility) to the nodes in its
  extent. So arrangement is largely built; the delta for the scene-wide vision is UX + per-scene
  persistence, not mechanism. The cartography seam doc models the live orrery as a rapier object
  world (hard collision, drag-pinning, settling, inertia; cartography_aether_layout_seam.md:60-63),
  which the arrangement is a constraint on.
- **Label data model.** `Node.title` is one string seeded to the URL at creation
  (mod.rs:319), set to a real page title only after a load completes (`set_node_title`, fed by
  the scrying engine's TitleChanged) or a JSON-LD ingest. The kernel's `node_display_label`
  (display.rs:42) is the standard for the gnodes + roster, but its fallback prefers the bare
  *host* for an extensionless un-loaded URL, which collapses same-site nodes to one string —
  so the card path uses a title-or-URL-slug rule instead (see Progress).

## Phases

Done-conditions, not dates. The visual wins land inside the card form first (immediate,
low-risk), then the representation generalizes, then arrangement.

### P0 — Restore the lost cues to the card form (immediate)

The card regains the gnode's object cues, driven from the same per-node hints the tile used:

- Expose the hints the snapshot lacks: a `node_shape(key) -> NodeShape` accessor (the map
  exists, lib.rs `node_shapes` / frame.rs:185) and a `node_selected(key) -> bool` accessor
  (selection is folded only into `node_color` today); add `shape` + `selected` to `OrreryCard`.
- Shape variety by content type: square / `border-radius:9px` / `border-radius:50%` (the
  build.rs:51-52 values), as a class per card.
- Footprint / solidity: a fixed minimum square footprint with the favicon as the face and the
  caption beside (the gnode anatomy), versus the text-hugging pill — **aesthetic call, see
  Open questions**.
- Selection emphasis beyond recolor: a ring + slight lift on the selected card (scale about
  center so the grab point stays put).
- Keep placement co-located with gyre (`transform:translate(x,y)` from `node_position`), and
  add `pointer-events:none` to `.node-card` / `.orrery` defensively (winit drives input today,
  but this guarantees the cards can never swallow the grab if a future host routes content-band
  clicks through serval dispatch).
- Moveable + resizable cards (Mark's fields-moveable/resizable ask, the node-card case; field
  *regions* are already moveable + resizable, owned by the field-regions plan). Moveable is
  already intact (the gyre drag-pin); resizable is net-new: add a per-node `size` / footprint to
  `OrreryCard` (it carries only `label/x/y/color/favicon` today, window_view.rs:294-303), keep the
  resize-handle hit-targets out of `content_rects` so they never steal the grab, and offer
  size-by-degree as the opt-in (Open question 5).

Done when the focused orrery's cards show content-typed shapes, a clear selection treatment, a
solid footprint, and a resize handle, with grab/drag/fling unchanged (verified by a headed drag).

### P1 — Representation as a pluggable per-node choice

Generalize the binary `render_as_cards` flag into a per-node `Representation` over the open
set (tile, card, textured-body, shape, scripted), defaulted per content-type / scene,
user-overridable. DOM forms diff into the document; the LOD/state hinge selects glyph-vs-card
by focus / expand / cull.

Done when a node's form is a per-node property (not a pane flag), at least tile + card are
selectable per node, and the default mapping is content-type driven.

### P2 — Textured-body and scripted forms

Add the GPU/texture forms on the rapier body via the external-texture element view (the
window-composition P2-companion bridge). The textured-body carries an arbitrary texture; the
scripted form is the scene-decoration / scripting hook.

Done when a node can be a textured body (e.g. a content thumbnail or custom image) on its
physics body, draggable like any other form.

### P3 — Arrangement: the scene-wide + persistence delta (mostly cross-plan)

Not a substrate build (the `arrangements` crate + `apply_strategy_positions` + the field-regions
plan already cover the mechanism and the localized/scripted surface). Confirm and fill the gaps
for the scene-wide vision: a per-scene *persisted* arrangement choice (re-applies on reopen), the
dynamic-follow semantics under the per-frame overlay, and the per-scene customization surface.
Localized + scripted arrangement defers to the
[scriptable field regions plan](2026-06-13_scriptable_field_regions_plan.md).

Done when a scene's arrangement persists and re-applies on reopen, holds as nodes/edges arrive,
stays draggable, and is user-selectable per scene.

## Open questions / aesthetic calls

These are Mark's design calls; the configurability rule says expose them as settings rather
than bake one default.

1. **Footprint** — the literal 36px tile + nameplate-beside, or a hybrid (square favicon face
   with an inline label) that stays compact in dense graphs? The main aesthetic call for P0.
2. **Selection emphasis** — ring vs glow vs scale-lift vs a combination; if scale, scale about
   center so the visual center stays on the world grab point.
3. **Shape vocabulary** — `NodeShape` is only Square/Rounded/Circle (a first cut); many kinds
   (image, note, feed-item) collapse to Square today. Widen content-type → shape, or ship the
   3-shape set for P0?
4. **Label cap** — 24 chars for the on-canvas card (the roster carries a fuller label, so the
   canvas reads terser by design). Confirm.
5. **Size-by-importance** — map degree / weight to footprint (bigger = more connected)? A
   net-new affordance; per the configurability rule, opt-in.
6. **Representation defaults** — the content-type → form mapping, and the per-scene override
   surface. Where the user picks a node's form.
7. **Arrangement defaults + surface** — the per-scene arrangement picker, and which arrangements
   ship first beyond force-directed/radial.

## Progress

- 2026-06-18: **Plan written (with Mark).** Came out of an orrery-as-element fix-up pass and
  Mark's reframe that the node's *presentation* must be pluggable, not locked to the DOM card.
  Grounded on an investigation workflow (drag confirmed intact; the gap = footprint /
  shape-variety / favicon-as-face / selection; the gnode anatomy is the live reference).
- 2026-06-18: **Fix-up pass landed (the orrery-as-element rough edges Mark flagged).**
  - **Label fix (`1c564ab`).** The card path was the one site still doing a naive
    `node.title.rsplit('/')`; `node_display_label` collapses an un-loaded same-site URL to the
    bare host. Now: real page title when loaded, else the URL's last path segment (the readable
    slug, previews the eventual title and keeps same-site nodes distinct), capped to 24 chars
    with a trailing ellipsis. Verified (Bird/Dog/Cat distinct; "Rust_programming_langua…" capped).
  - **Layering cull (`2c6ddb8`).** Off-screen nodes' cards escaped the orrery element into the
    chrome (serval does not clip transformed overflow); cull cards to the pane box, off-screen
    nodes ride the underlay dots.
  - **Snapshot/frame reorder (`2f5141a`).** The card snapshot ran *before* this frame's orrery
    updates (colors, resize, strategy, `frame()`), so cards were a frame stale in position,
    color, and scope and trailed the scene during motion (the "jumping"). Moved the per-frame
    orrery block above the snapshot + chrome render; cards now read this-frame state and align
    with the scene. Also tightens hit-test alignment.
  - **Favicon PNG data-URI (`745682a`).** Cards carry the favicon as a `data:image/png` `<img>`.
    Note: the chrome's `IncrementalLayout` path passes no `ImagePlane`, so the `<img>` is not
    yet decoded/painted in the shell render — wiring image-decode into that path is the
    follow-up to make the icons appear (the encoding is in place).
