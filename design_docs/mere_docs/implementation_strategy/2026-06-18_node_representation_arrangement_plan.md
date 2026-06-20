# Node Representation and Arrangement Plan — pluggable node forms, live arrangements

**Date**: 2026-06-18
**Status**: Planning (with Mark). The orrery-as-element work (Phase 2 of the
[unified document host plan](2026-06-17_unified_document_host_plan.md)) put the focused
orrery's nodes into **host-positioned DOM** (cond 1, the custom-layout `<orrery>` element,
is deferred; these are not a full element yet). That was right for the *semantic* half — but
the semantic win belongs to the **node-as-container** (it rides a11y / tab / focus / automation,
broadcasts JSON-LD, carries the expand/content state) **independent of which sprite is shown**;
it is not card-coupled. The early framing was wrong for the *look* half: it flattened nodes from
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
- [tearout_composability_plan](2026-06-19_tearout_composability_plan.md) (continuing the archived window-composition plan) — orrery (authority) vs
  panes (views); this plan is the *node-rendering* layer inside the orrery the pool holds.
- [unified_document_host_plan](2026-06-17_unified_document_host_plan.md) — the DOM/document
  half; it owns the **node-as-container-in-the-document** (the a11y / tab / focus / automation
  spine), *not* a node's look. The DOM representation forms (tile, card) ride its materialization
  mechanism, and its "materialization is state" rule (a glyph by default, a DOM subtree on expand)
  is the hinge between the two plans. This plan owns the **sprite / representation layer** (the
  card is *one* sprite, never the node's truth-form).
- [native_surface_compositing_plan](2026-06-19_native_surface_compositing_plan.md) — owns the
  **snapshot-texture-vs-live-visual** split for the orrery card and the host-surface **layering**
  (the four-way by-nature split: serval DOM subtree / external WebView2 native visual below chrome /
  dormant snapshot / orrery scene texture). The orrery card = snapshot is its compositing half.
- [cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
  — owns the **dormancy/snapshot + incremental-layout** performance strategy that the LOD
  "materialization is state" machine and the textured/scripted forms depend on (bake = GPU
  rasterize, *not* a CPU-layout skip; snapshot justified by dormancy/memory, not per-frame perf).
  Cross-ref only; do not restate.
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

A node is a first-class **graph-object container** that lives *in* the document, so it rides the
host spine (accessibility, tab order, focus, automation). Its *truth* (content, identity, edges)
is authoritative in the kernel and broadcast as DOM (the a11y tree, the JSON-LD) **on the node
container itself, independent of which sprite is shown**. It carries a *free sprite* plus physics
and behavior; its *presentation* is experience-derived, and should be free:

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

Mark's framing (2026-06-18, **sharpened 2026-06-19**): the **node-as-container carries the
truth / identity** (a11y, tab, automation, JSON-LD); the **card is one ephemeral sprite / content
preview, never the node's truth-form** and never the drag handle. (The earlier "a DOM card is a
fine *truth* for a node" framing was retracted — see the 2026-06-19 Progress entries; the node is
grabbed through gyre directly, and the card is a static snapshot.) The unnecessary problem is being
*locked* to one sprite. The node should be representable as a textured rapier body, a bare shape,
or a scripted decoration just as readily as a card, "any sort of object you can make a texture for,
with attendant physics," so the graph feels alive (with scene decoration and scripting). Both
layers user-customizable. The card is one sprite, not the mandated form.

## Representation — a pluggable per-node form

The same node-truth, rendered as any of an open set of forms. Forms that carry semantics emit
serval DOM (so the a11y / JSON-LD legibility holds); forms that carry liveliness emit a
texture or scene primitive. Every form rides the same gyre rapier body, and the **face is the
collider**: a node's hit-target is its face geometry (a square today; an arbitrary polygon or
custom shape later, via parry's convex / compound shapes), not a generic box around it.
`NODE_HALF`'s fixed half-extent generalizes to a per-node parry shape that both size and shape
drive. The label is co-located at the body's position but sits outside the collider, so it
never enters the hit-test or the physics; gyre's `QueryPipeline` already picks by collider, so
face-as-hitbox follows once the collider tracks the face.

The open set (initial):

- **tile** — the gnode anatomy: a fixed-footprint colored shape (square = document, rounded =
  menu, circle = feed), the favicon textured on the *face*, the caption beside. The "physical
  chip" feel. Still the live form on secondary panes today (the reference, not lost code).
- **card** — an **ephemeral content-preview anchored to a node, not a representation form** (and
  never the drag handle; resolved 2026-06-19, see Progress). It snapshots the last visit (favicon
  + shape are only the un-visited fallback) and rides the *expand / content* LOD state. It is *not*
  a sprite the node "is"; the node's sprite is one of the forms below, with the card layered as the
  preview.
- **textured body** — an arbitrary texture (favicon, thumbnail, rendered content, custom
  image) on the rapier body. The most literal "object you can texture."
- **shape** — bare geometry (the tile without favicon/caption); minimal, cheap, dense graphs.
- **scripted** — a custom-drawn / decorated form, the open-ended liveliness and
  scene-decoration hook.

Architecture:

- A per-node `Representation` (enum first, trait-object if forms need to carry their own
  state), defaulted per content-type and per scene, user-overridable per node. The binary
  `render_as_cards` flag is the seed to generalize.
- The orrery renders each node through its representation's renderer, **split by nature** (the
  compositing layering is owned by
  [native_surface_compositing_plan](2026-06-19_native_surface_compositing_plan.md); the
  external-texture *input* bridge by [tearout_composability_plan](2026-06-19_tearout_composability_plan.md)
  per the spine, line 139 — name both, they are distinct):
  - **serval-rendered content** diffs into the shell document as a **real DOM subtree** (a11y /
    find-in-page / selection / true scroll hold).
  - **genuinely-external content** (a live page / scry WebView2) rides a **native composition
    visual z-ordered below the chrome** (native-surface-compositing), **not** a document
    external-texture element.
  - the **orrery scene** (the gyre field) is correctly a **texture**.
  So there is no universal "external-texture element = the bridge for the textured/scripted forms."
  The textured-body **input** bridge (forward a hit, texture-local, to a producer behind it) is
  tearout-composability's; the **compositing layering** is native-surface-compositing's.
- The **"materialization is state"** hinge ties this to the unified-document plan: a node's
  representation can be LOD- and state-driven, a texture-glyph (tile / textured-body) by
  default, materializing into a richer DOM subtree (a card with a live content preview) on
  focus or expand, and demoting to an underlay dot when culled. **Demoting to a texture is not a
  per-frame-perf win**: baking is the GPU rasterize step, not a CPU-layout skip (serval already
  has `IncrementalLayout`), and the snapshot is justified by **dormancy / memory** — you cannot
  hold N live layout sessions in RAM for N previews — **not** per-frame performance. The
  active / focused sprite stays **live** the engine way (cold layout once, incremental thereafter).
  Owner of the bake-vs-live + dormancy strategy:
  [cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
  §4(b); cross-ref only, do not restate.

What the card form lost vs the tile, and must regain (code-verified, see Findings): footprint
/ solidity, shape-variety-by-content-type, favicon-as-the-face, and selection emphasis beyond
a recolor. Depth is *not* a deficit (the card already carries more shadow than the tile ever
had).

## Arrangement — a mature substrate, a narrow delta

The arrangement layer is largely built. This plan does not rebuild it; it names the delta for
the "scene picks an arrangement, nodes follow it" vision and points at the owners.

- **`crates/orrery/arrangements`** is a real crate: an arrangement *registry* with *adapters*
  (grid, radial, penrose, phyllotaxis, lsystem written and dispatched; kanban, timeline,
  semantic_edge_weight, semantic_embedding written but not yet dispatched), the layout
  implementations that arrange node subsets.
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
- **The arrangement substrate is mature, and more built than first credited.**
  `crates/orrery/arrangements` is a registry + adapters: grid, radial, penrose, phyllotaxis, and
  lsystem, plus **kanban, timeline, semantic_edge_weight, and semantic_embedding** already
  written. The orrery's `layout_strategy` + `apply_strategy_positions` apply the active arrangement
  each frame via `platen::project_orrery_strategy` (focus-aware for radial, a no-op under
  force-directed). But only four are *dispatched* into the pickable set (`ORRERY_LAYOUT_STRATEGIES`:
  phyllotaxis / grid / penrose / lsystem, `cartography_scene.rs:150`); semantic / timeline / kanban
  exist as adapters but are unwired (their input signals are not yet plumbed). So Decision 7's
  "wire semantic next, then timeline / kanban" is exposing built layouts, not authoring them. The
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

### P0 — Restore the lost cues to the card form (mostly landed)

**Status (2026-06-19 audit):** the cue set is in and wired end to end (content-type silhouette,
favicon-as-face, state-only color with a selection ring, node-on-collider, menu-over-cards, and
favicons painting). `render.rs:510` sources the card from the new `node_state_color` /
`node_selected` / `node_shape` accessors (`lib.rs:790-808`). Open: the selection *scale-lift*
(deferred at `window_view.rs:471-472`, pending a confirmed translate+scale transform), a *hover*
channel (Decision 2's faint brighten, no handler yet), and *resizable* cards (no `size` /
footprint field on `OrreryCard` yet).

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

Split in two (code-verified 2026-06-19): only one half needs the external-texture **input**
bridge, and the DOM route for the other half needs the chrome image-decode gap closed first.

- **P2-static — a non-interactive textured face (the achievable half).** A node whose face
  shows an arbitrary texture (thumbnail, favicon, custom image), draggable and selectable like
  any other form. It does **not** need the input bridge: winit drag handles the grab (it
  bypasses the DOM, so it wins as long as the face rects stay out of `content_rects`), and
  select rides the document hit-test (a click resolves to the face element; mapping that node
  back to its URL is the host step the card path already uses). You select and drag the *node*,
  not click *into* the texture. **Now gated on P1 alone:** the second prerequisite, the **chrome
  image-decode gap**, was closed 2026-06-19 (serval-layout's `IncrementalLayout` caches a decoded
  `ImagePlane`, refreshed at full layout, `incremental.rs:97/158/722/969`), so `<img>` data-URIs
  paint. What remains is (1) P1's per-node `Representation` hook to slot the form (today it is a
  binary per-pane flag). (The `<external-texture>` route avoids the decode path but asks the host
  to register a wgpu texture per node.) Done when a node can be a textured body on its physics
  body, draggable and selectable, the face driving the collider (P0/P5), and the texture painting.
- **P2-interactive — live texture content (gated on the input bridge).** A node whose texture is
  itself interactive: a live page as a body, the compat WebView node, a canvas you draw on. **By
  nature this is genuinely-external content** (C2): it wants a **native composition visual** below
  the chrome (owned by
  [native_surface_compositing_plan](2026-06-19_native_surface_compositing_plan.md)) **PLUS** the
  `<external-texture>` element to **bear input** (forward a hit, in texture-local coords, to the
  producer behind it), the external-texture-input bridge owned by the
  [tearout_composability_plan](2026-06-19_tearout_composability_plan.md) (the window-composition
  continuation). The compat-WebView-node case specifically rides native-surface-compositing's
  API-forwarding path; the DOM-textured-body case rides tearout's serval-hit-relay bridge — they
  are distinct routes, not duplicates. Blocked until the input bridge lands.

The scripted form (scene-decoration / scripting hook) shares the field-regions rhai substrate and
sequences after P2-static.

### P3 — Arrangement: the scene-wide + persistence delta (mostly cross-plan)

Not a substrate build (the `arrangements` crate + `apply_strategy_positions` + the field-regions
plan already cover the mechanism and the localized/scripted surface). Confirm and fill the gaps
for the scene-wide vision: a per-scene *persisted* arrangement choice (re-applies on reopen), the
dynamic-follow semantics under the per-frame overlay, and the per-scene customization surface.
Localized + scripted arrangement defers to the
[scriptable field regions plan](2026-06-13_scriptable_field_regions_plan.md).

Done when a scene's arrangement persists and re-applies on reopen, holds as nodes/edges arrive,
stays draggable, and is user-selectable per scene.

### P4 — Retire the Stage-2 live-preview card (node-open routes to pelt)

The card model's **Stage 2** (the floating live-actor card in the orrery, promoted by
double-click, carrying the X / tile buttons) is **dropped**: a node opens in a **pelt tile**,
and the orrery keeps only the static **Stage-1 snapshot** (the peek without the X). This
implements the 2026-06-19 node-vs-card decision (the live view moves to pelt; the orrery card
stays a snapshot); the compositing half (live content hosted in a tile, not a floating surface)
is [native_surface_compositing](2026-06-19_native_surface_compositing_plan.md) P3. The Stage-2
surface was built by the archived
[card_system_and_staging_plan](../../archive_docs/2026-06-09_completed_plans/2026-06-07_card_system_and_staging_plan.md);
this retires it.

Code-anchored scope (verified 2026-06-20):

- **Re-point the promotion gesture.** `toggle_live_preview` (node_ops.rs:242, the double-click on
  the focused node) opens the node in a pelt tile (`open_workbench` + `workbench.open_tile` +
  `focused_tile`, the path menus.rs / nav_sync already use) instead of `live_previews.insert`. The
  same double-click from the snapshot / unvisited card (card.rs:574, render.rs:1007) routes there.
- **Remove the live card.** Drop the `live_previews` set (window_view.rs:227) and its render branch
  (render.rs:884-934, `live_card`), the X close button (render.rs:1418-1422, input.rs:244), and the
  `live_previews.insert` sites (nav_sync.rs:79/104, node_ops.rs:131/208). `needed_members`
  (node_ops.rs:222) reduces to the workbench tiles, since the orrery no longer keeps a live actor.
- **Keep the snapshot (Stage 1).** The static "last visit" peek (the render.rs `else` branch, no
  `content_rects`) stays untouched. Its scene source moves from the live-preview demote ("its last
  scene is kept as the node's snapshot") to the **pelt tile's close / blur** depositing its last
  scene as the node's snapshot. This is the one non-mechanical part: today only the live-preview
  demote populates a fresh snapshot, so the tile lifecycle has to take over that deposit.

Done when double-clicking a node opens it in a pelt tile (no floating live card or X in the
orrery), a visited node in the orrery shows only the static snapshot, and closing the tile leaves
the node's snapshot current.

Overlap: this touches `card.rs` (the live-card view + the X + the double-click-promote hint),
Mark's concurrently-edited file, so coordinate before editing it; the render / input / state parts
are meerkat-side and independent.

## Decisions (2026-06-19, with Mark)

The seven open calls below, resolved. The configurability rule holds: where a default is named,
it is the default of a setting, not a baked constant.

1. **Footprint, face = body.** A fixed-footprint object, not a text-hugging pill, and the face
   *is* the collider (see Representation). Square today; arbitrary polygon / custom shape later.
   The label is variable and LOD-driven (beside when sparse, hidden when dense / zoomed out, gone
   at the underlay-dot LOD), co-located but outside the body.
2. **Selection, three channels.** Hover, selection, and focus read at once because each owns a
   channel: hover a faint brighten, selection a ring + slight scale-lift (scaled about center so
   the world grab point stays put), focus its own ring. Selection leaves *color* free for
   activation state, resolving the recolor collision.
3. **Shape, data not hardcoded.** Ship the 3-shape set (Square / Rounded / Circle) for P0, but
   route content-type to shape through a lens, not a match arm. The same rule extends to **edge
   styles and field styling**: one styling lens (the NODE_SHEET pattern widened to edges + fields),
   not three hardcoded paths.
4. **Label, terse and width-driven.** Keep the canvas terser than the roster; ellipsize to the
   face width at the current zoom rather than a fixed char count; expose label density (off /
   terse / full) as a setting.
5. **Size-by-importance, opt-in, drives the collider.** Map a metric (degree first, pluggable to
   centrality / recency) to footprint, log-scaled and clamped, opt-in. Size drives the parry
   collider, not just the visual, so physics and picture stay in sync.
6. **Representation surface, the scene pane.** Scene-wide defaults (content-type to form, plus
   arrangement and edge/field styling) live in a **scene pane**: a folded pane in the **consolidated
   shell document** that [unified_document_host_plan](2026-06-17_unified_document_host_plan.md)
   Phase 1 produced (now done, 4/4 conditions; cross-ref, do not restate its status), alongside
   roster / apparatus. Per-node overrides stay on the node context menu; the scene pane owns the
   defaults.
7. **Arrangement surface, the scene pane; semantic next.** Force-directed (default) + radial +
   grid cover the everyday; wire **semantic** (group-by-relation) next, then timeline / kanban.
   The adapters already exist (arrangements `semantic_edge_weight` / `semantic_embedding` /
   `timeline` / `kanban`), but each is **gated on an input that is not plumbed**, so wiring is
   deriving that input + threading it, not a bare dispatch line (code-checked 2026-06-20):
   `kanban` and `timeline` read `ViewIntent::axis_values` (a per-node categorical / numeric axis);
   `semantic_edge_weight` is a *streaming* strategy (per-frame `step_with` state, not the one-shot
   `project_orrery_strategy`) and is only meaningful with `IntelligenceSignals::affinity` (without
   it, it is plain force-directed, redundant with gyre); `semantic_embedding` needs host
   embeddings. The cheapest real entry is a **graph-derived axis** (kanban by content-type /
   classification, timeline by a node timestamp), which still wants the axis plumbed host →
   platen. The per-scene picker lives in the scene pane and persists per scene (the P3 delta).

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
- 2026-06-19: **Open questions resolved (with Mark); P2 split + first implementation step set.**
  All seven design calls decided (see Decisions). Refinements folded in: the face is the collider
  (parry geometry, `NODE_HALF` to a per-node shape, label outside the body); hover / selection /
  focus are three channels; styling is data across node shape + edge style + field style (one
  lens); the scene pane is the home for scene-wide representation + arrangement + styling defaults,
  per-node overrides on the context menu. P2 split into P2-static (textured face, the achievable
  half) and P2-interactive (needs the external-texture input bridge). Scoping the implementation
  surfaced that P2-static as a DOM citizen is gated not on the input bridge but on the **chrome
  image-decode gap** (the shell `IncrementalLayout` emits an empty `ImagePlane`) and on P1's
  per-node `Representation` hook. Closing the image-decode gap is the first step: it makes card
  favicons paint *and* unblocks the static textured face. It is a serval-side change (the
  session's emit path), consumed by meerkat across the git dep.
- 2026-06-19: **Node-vs-card model sharpened (with Mark); collider + menu fixes landed, headed-verified.**
  The node and the card are strictly separate. The **node** is the physical object rendered AT its
  gyre collider (draggable; what physics pins and edges connect to). The **card** is an ephemeral
  content preview anchored to it (a snapshot of the last visit as primary content; favicon + shape
  are only the un-visited fallback). The card is not a node-representation form and never the drag
  handle, so the earlier option (a) framing (make the card act as the node) was wrong; the node is
  grabbed through gyre directly. Stage 2 (in-orrery live view) is dropped: clicking a node opens it
  in pelt, the orrery card stays a static snapshot.
  Landed this session (uncommitted, mixed with concurrent work; verified by a headed drive):
  - **Node on its collider.** The body had been rendering at an absolute box's static-flow position,
    beside the collider (a "void" I had been decorating around); `left:0;top:0` anchors it to the
    orrery element origin so the transform lands it on the collider. A headed drag confirms the body
    now grabs and moves the node (edges connect to the body).
  - **Press routes to gyre (cond-3 removed).** A left orrery press reaches gyre directly; its
    `CLICK_SLOP` splits a click (select) from a drag (move). No card-as-handle, no `chrome_click`
    interception of the node press.
  - **Menu over node cards.** The shell document reordered to `(orrery, panes, chrome)` so the
    chrome and its overlays paint + hit-test over the node cards (the nodes had occluded the menu
    and stolen its clicks).
  - **Card favicons paint.** Closed the chrome image-decode gap serval-side (serval-layout
    `IncrementalLayout` caches a decoded `ImagePlane`, refreshed at full layout); the favicon-as-face
    cue renders now.
  The menu-vs-host-composited-surfaces precedence (scry System WebViews drawn over the menu) is a
  separate native-compositing issue, split out into
  [native_surface_compositing_plan](2026-06-19_native_surface_compositing_plan.md).
- 2026-06-19: **Cross-plan consolidation pass (orrery/host cluster reconciliation).** Folded the
  node-vs-card sharpening (filed only in the Progress log until now) **up** into the thesis,
  Status, and the Representation open set, and aligned this plan with its two siblings and the
  parallelism research doc. Specifically:
  - **Node-as-container / card-as-one-sprite reframe (C1, this plan owns it).** The thesis now
    states the node-as-container carries the truth (a11y / tab / focus / automation, JSON-LD)
    *independent of which sprite is shown*; the **card is one ephemeral content-preview, not a
    representation form and never the truth-form**. Retracted the earlier "a DOM card is a fine
    *truth* for a node" line. The Status line no longer over-claims the orrery-element (Phase 2's
    cards are host-positioned DOM; cond 1, the custom-layout `<orrery>` element, is deferred) and
    decouples the semantic win from the card.
  - **By-nature compositing layering (C2) cross-ref.** Corrected the universal "external-texture
    element = the bridge for textured/scripted forms" claim: serval content → real DOM subtree;
    genuinely-external (scry) → native composition visual below chrome (native-surface-compositing),
    *not* a document element; orrery scene → texture. P2-interactive now names both routes (the
    compat-WebView case on native-surface-compositing's API-forwarding path; the DOM-textured-body
    case on tearout's serval-hit-relay input bridge).
  - **Bake = dormancy-not-perf correction (C3) on the LOD hinge.** Stated that demoting to a
    texture is the GPU rasterize step, not a CPU-layout skip; the snapshot is a dormancy/memory
    decision, the active sprite stays live via `IncrementalLayout`. Cross-ref the research doc
    §4(b), no restatement.
  - **Two added sibling cross-refs.** native_surface_compositing_plan (the snapshot-vs-live split
    and host-surface layering) and the cross_platform_parallelism_strategy research doc (the
    dormancy/snapshot + incremental-layout strategy owner) added to the sibling block; previously
    only referenced inside a progress entry.
  No DOC_README change is triggered (no doc add / move; relative links only).
- 2026-06-19: **Plan audited against the tree; four staleness points reconciled.** Code-verified
  every load-bearing claim. (a) P0 marked mostly-landed: the cue set + collider + menu + favicons
  are in and wired (`render.rs:510` sources from `node_state_color` / `node_selected` /
  `node_shape`, `lib.rs:790-808`); open are scale-lift, hover, and resize. (b) The arrangements
  substrate corrected (Findings + the Arrangement section + Decision 7): radial / penrose /
  phyllotaxis / lsystem are dispatched, while kanban / timeline / semantic_edge_weight /
  semantic_embedding are written but unwired, so Decision 7 is wiring built layouts, not authoring
  them. (c) P2-static's image-decode prerequisite retired (closed serval-side, `incremental.rs`),
  leaving it gated on P1 alone. (d) Noted dead `point_over_orrery_card` (`input.rs:735`, defined
  but uncalled after the cond-3 removal) for cleanup. Five most pressing slices ranked for Mark:
  P1 representation hook, wire the dormant arrangements (semantic_edge_weight first), finish P0's
  open channels, P2-static textured face, scene pane + per-scene arrangement persistence. Starting
  P1. No DOC_README change (edit in place).
- 2026-06-19: **P1 implemented (per-node Representation); code-complete, build-blocked on a dep
  diamond.** Generalized the binary `render_as_cards` flag into a per-node form choice, true to the
  node-vs-card sharpening (the card is the preview layer, not a form): a `Representation { Tile,
  Shape }` enum (orrery `types.rs`; TexturedBody / Scripted join at P2 when their texture / script
  paths land, held out now to stay warning-clean), a per-node override map + `node_representation`
  accessor (the override, else the content-type default keyed off `node_shape`) +
  `set_node_representation` / `clear_node_representation` (orrery `lib.rs`). The focused orrery's
  snapshot stamps the form (`render.rs`) and `node_card_view` renders Tile (favicon + caption) vs
  Shape (the bare content-typed face) (`window_view.rs`). The user-facing surface is the node
  context menu's "Show as tile / shape" picker (✓ current), a `ContextAction::SetRepresentation`
  applied to the selection (`menus.rs` / `lib.rs`), per Decision 6. `render_as_cards` stays the
  separate compositing axis (DOM-vs-raster), unchanged. The default lens is uniform Tile, so the
  look is unchanged; the per-node override is the new capability. An orrery unit test locks the
  override-or-default contract. **Not yet built**: a fresh Cargo.lock resolve hit a `windows`
  0.61/0.62 diamond (`ipc-channel` 0.22 anchors 0.61 via serval; `gpu-allocator` 0.28 unified onto
  it; `wgpu-hal` 29.0.3 needs 0.62, so its dx12 backend fails). Pre-existing, independent of P1, a
  workspace-pin decision; the P1 code is reviewed and complete pending the dep fix. Persisting the
  per-node override across a reload is a follow-up (it joins the cartography sidecar / scene-pane
  defaults).
- 2026-06-20: **P0 selection lift landed (Decision 2); the remaining forward slices found
  build-needed.** The selected card now lifts: its face grows slightly (4px) and recenters on the
  gyre collider so the world grab point stays put, via width/height growth + a recentered translate
  (not a CSS scale, so no transform-origin dependency; the collider stays `NODE_HALF`, the lift is
  visual only). Completes Decision 2's selection channel (ring + lift), leaving color free for
  activation state. Code-checked the rest of the live scope; each remaining slice needs a build to
  land safely, so they wait on the dep fix: (a) the dormant arrangements are input-gated, not bare
  dispatch adds (kanban / timeline need `axis_values`; `semantic_edge_weight` is streaming +
  affinity-gated; `semantic_embedding` needs embeddings — corrected Decision 7); the cheapest entry
  is a graph-derived axis (kanban by content-type) plumbed host → platen. (b) Size-by-importance
  (Decision 5) needs a per-node gyre collider radius (`NODE_BODY_RADIUS` is fixed) and must ship the
  collider + visual together for physics / picture sync. (c) The hover channel needs new
  hovered-node state + a cursor hit-test (only fields track hover today). (d) The resize handle
  rides (b)'s per-node size.
- 2026-06-20: **Scoped P4 (retire the Stage-2 live-preview card).** Headed verification surfaced
  the card occluding nodes; clarified with Mark that it is the **live preview** (the actor card
  with the X / tile buttons, promoted by double-click, `live_previews`), not the static snapshot
  (the X-less peek, which stays). Mapped the Stage-2 surface against the tree (state
  window_view.rs:227, promote node_ops.rs:242, X render.rs:1418 / input.rs:244, render branch
  render.rs:884-934, needed_members node_ops.rs:222) and wrote P4: re-point the double-click to open
  the node in a pelt tile, remove the live card, keep the snapshot, with the snapshot's scene
  deposit moving to the pelt tile's close. Touches Mark's `card.rs`, so implementation is
  coordination-gated.
