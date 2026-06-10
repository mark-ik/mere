# Orrery Element — Phase 1 Implementation Plan

**Date**: 2026-06-02
**Status**: Plan for review. Phase 1 of the serval-as-host flip
([2026-06-01_serval_host_flip_plan.md](2026-06-01_serval_host_flip_plan.md), Phase 1).
The serval-side perf prerequisites (A/B/C + persistent Stylist) are done and
verified, so the per-frame motion mechanism is ready. This doc plans the orrery
element itself.
**Grounded in**: a read-only recon across mere + serval (platen producer, serval
element/paint extension points, gyre, the current-host orrery), plus a first-hand
read of serval's stacking-paint to settle the camera model.

---

## What the recon settled

- **`platen::orrery::orrery_paint_list` is built and host-neutral** (commit
  `1110a26`). Graph in, one `CanvasPaintList` (a flat `Vec<PaintCmd>` under one
  `PushTransform(camera)`) out: edges as strokes, nodes as rects, resolved
  visual-coupling overlays (halo/tint). It reads the graph's **committed**
  positions, not a live gyre sim. Nodes are rects, not glyphs; no labels yet. No
  host consumes it — wiring it live is a Phase-1 build item.
- **serval has no custom-element / replaced-element / custom-paint hook.** The box
  tree is a closed set (block/flex/grid, `<img>`-only replaced, parley text). So
  "the orrery element" is **not** a new serval box type. It is a **host-side
  composition**: a plain serval `<div position:relative>` holding one abs-pos
  subtree per visible node, plus a composited paint underlay.
- **The layer-2 motion mechanism is verified.** A host mutates the DOM via
  `LayoutDomMut` and drives `IncrementalLayout::apply`; a per-frame inline-`style`
  transform change on an already-transform-bearing node returns `RepaintOnly`
  (layout skipped), now on the cheap persistent-Stylist replacement path.
- **gyre is ready for the node side**: `cull_aabb(viewport) -> Vec<NodeKey>`
  (virtualization), `position_of` / `positions` (per-frame read), `hit_test(point)
  -> NodeKey` (node point-pick), `tick` / `pin` / `unpin` (sim + drag). **Not
  built in gyre**: edge-pick, marquee/rect-select, any edge-geometry accessor.
- **The current host orrery** is a bespoke Masonry widget (`graph_canvas.rs`) that
  paints directly and does **not** use `orrery_paint_list`. Node positions are a
  static ring layout (no physics in the live host). The one clean portable piece
  is `camera.rs` (Camera + world/screen mapping + nearest-node hit-test, kurbo
  math, unit-tested). Inertia, the wheel=pan / ctrl+wheel=zoom mapping, edge-pick,
  and marquee do not exist anywhere and are net-new.

## The architecture: host-side composition, one camera transform (Option A)

The orrery element is a host-side composition of three layers under **one camera
transform**. The element owns: a serval container `<div>`, a gyre `Simulation`, the
`Camera`, and the per-frame loop.

```text
   container <div id=orrery-stage> with CSS transform = camera matrix
              │   (zoom + pan; serval propagates it to the subtree, incl. abs-pos)
   ┌──────────┴───────────────────────────────────────────┐
   │ layer 1: scene-paint underlay (orrery_paint_list)      │  world space,
   │   edges + demoted-node glyphs + coupling overlays,     │  emitted under the
   │   as Vec<PaintCmd>; shares the camera matrix            │  same camera matrix
   ├────────────────────────────────────────────────────────┤
   │ layer 2: physics-positioned DOM children                │  world space; each
   │   one abs-pos serval subtree per visible node at its     │  child sits at its
   │   world position; moved by a per-frame inline transform  │  gyre world pos,
   │   = translate(gyre.position_of(node)); pool-toggled vis  │  the container
   └─────────────────────────────────────────────────────────┘  transform applies
   composition seam: concat layer-1 commands + the document's ServalPaintList
   commands into one stream -> translate -> one netrender::Scene
```

### The camera as one container transform (decided: Option A)

The clean model is one camera: a CSS `transform` (the camera matrix, zoom + pan) on
the orrery container `<div>`, applied by serval to the whole subtree — the abs-pos
node children and the in-flow content alike — and the underlay emitted under the
same matrix. Both layers live in **world space**; the single transform maps world to
screen. This is CSS-correct (a transformed element is a containing block + stacking
context for its abs-pos descendants, which paint within its transformed space) and
keeps the per-frame work minimal: a node only re-emits its inline transform when its
**world** position changes (a physics tick), not on every pan/zoom — pan/zoom is one
mutation on the container, not N on the children.

**This requires fixing serval's paint first.** Today serval's Tier-2 stacking
(`paint_stacking.rs:80-88`) lifts each abs-pos child into a layer painted "on a clean
stack at its own absolute origin" (a point), so an ancestor's transform **matrix**
does not compound onto its abs-pos descendants (translate is partly foldable into the
origin point; zoom/scale is not applied at all). Verified first-hand. So build step
**1A** makes lifted abs-pos layers inherit the ancestor transform chain — the
CSS-correct behavior, with value beyond the orrery (any transformed container with
abs-pos descendants currently mispaints). Everything else builds on 1A.

Hit-testing: a screen point maps `screen_to_world` through the camera, then hits node
content via serval `FragmentQuery` and scene geometry via gyre, both in world space.

---

## Decisions (★ = decided 2026-06-02 with the architect)

- **★ D1 — Camera model: Option A (decided).** Fix serval's Tier-2 stacking so
  lifted abs-pos layers inherit the ancestor transform chain, so one CSS `transform`
  (the camera matrix) on the orrery container moves the whole subtree — abs-pos node
  children and in-flow content — CSS-correctly. Chosen over the per-child
  screen-space workaround (Option B) for correctness and minimal per-frame work
  (pan/zoom is one container mutation, not N child mutations). This is the
  foundational build step 1A and improves serval paint beyond the orrery.
- **★ D2 — Virtualization: pre-materialized pool (decided).** Keep a bounded pool of
  node subtrees alive and toggle visibility with an attribute-only change
  (`display`/off-viewport) so cull enter/leave stays on the `RepaintOnly` path,
  materializing new subtrees only when the pool is exhausted. Avoids the
  structural-relayout churn that `apply_structural` add/remove would incur as nodes
  stream in/out of view.
- **★ D3 — Scene hit-testing: edge-pick + marquee included (decided).** Build the
  full two-hit-test split in Phase 1: add to gyre an edge-geometry accessor,
  `edge_hit_test(point)`, and `rect_select(region)` (nodes + edges), alongside the
  existing `hit_test` (node point-pick) and `cull_aabb`. Node content still routes
  through serval `FragmentQuery`. These are net-new features (no current-host
  equivalent), delivered in one pass for complete scene interaction.
- **D4 — Composition seam.** Concatenate the underlay `CanvasPaintList` command
  stream with the document's `ServalPaintList` stream (remapping the font/image key
  namespaces so the two side-tables do not collide) and feed the combined slice to
  `translate_paint_cmd_stream` -> one `netrender::Scene`. Keep `DrawExternalTexture`
  in reserve for Phase 4 (web/scrying tiles), not the orrery underlay.
- **D5 — Camera type.** Unify the two camera types (`mere/app camera.rs`
  center-based vs `platen scene_paint::Camera` offset-based) into one host-neutral
  `Camera` the element and the producer share, so pan/zoom math matches between the
  underlay transform and the per-child screen mapping. Lift `camera.rs` (the clean,
  tested piece) to a shared home.
- **D6 — Demoted-node glyph.** Off-screen nodes demote to the underlay. Phase 1
  draws them as the producer's existing node rect (cheapest, already there); richer
  glyphs/labels are a later refinement. The producer needs a culled entry point
  (take a pre-filtered projection / node set) so it draws only the demoted set.
- **D7 — Navigation.** Author wheel=pan, ctrl+wheel=zoom, middle-click=pan, and
  inertia fresh on serval's input model (all net-new; the current host is
  plain-wheel=zoom with no inertia). Per the graph-canvas navigation directive:
  treat the canvas as an infinite-canvas document, keep inertia.

---

## Build sequence (done-conditions, not estimates)

Sub-phases are independently checkpointable. 1A is the foundational gate: the
single-camera-transform model (D1=A) rests on it, so it lands and is verified first.

- **1A — serval: ancestor-transform propagation to abs-pos layers.** Fix
  `paint_stacking.rs` so a lifted abs-pos layer is painted under its ancestor
  transform chain (matrix), not on a clean stack at a point origin. The
  `compute_transform_matrix` fold already supplies the per-element matrix; this
  threads the active transform(s) into each `Deferred` layer so `paint_context`
  re-establishes them. **Done**: existing stacking + transform tests still pass; a
  new test proves an abs-pos child under a `transform: scale()+translate()` ancestor
  paints scaled+translated (not at its untransformed origin); verified single-thread
  and parallel.
- **1B — Composition seam.** A `paint_list_render` (or host) helper that
  concatenates two `PaintList` command streams with font/image key namespacing.
  **Done**: a test composites an underlay stream + a serval document stream into one
  Scene, keys non-colliding.
- **1C — gyre: live-position bridge + scene queries.** (i) A culled
  `orrery_paint_list` entry taking a pre-projected/filtered node set, fed from gyre
  `positions()` (the producer reprojects unchanged). (ii) gyre gains an
  edge-geometry accessor (over the synced `(NodeKey, NodeKey)` pairs + body
  positions), `edge_hit_test(point)`, and `rect_select(region) -> nodes + edges`.
  **Done**: underlay + DOM children reproject from live gyre positions; gyre answers
  node-pick, edge-pick, cull, and rect-select, each test-covered.
- **1D — The orrery element (serval-hosted demo).** Compose the three layers in a
  pelt-live-shaped serval host: container div with the camera CSS transform (1A),
  `cull_aabb`-driven abs-pos children at their world positions via a
  **pre-materialized pool** (visibility toggled attribute-only → `RepaintOnly`),
  and the underlay (1B + 1C). **Done**: the orrery renders, pans/zooms via the
  container transform, drags a node, virtualizes through the pool without structural
  churn, and shows force + visual couplings, hosted by serval. (Built where it can
  be proven now; re-homing into mere's serval-host comes with the later flip phases.)
- **1E — Navigation + two-hit-test split.** wheel=pan / ctrl+wheel=zoom /
  middle=pan + inertia, driving the container camera transform. Node content via
  serval `FragmentQuery`; scene geometry (empty-space, node, edge, marquee) via gyre
  (`screen_to_world` first). Preserve the current `GraphAction` contract
  (NodeActivated / NodeMoved / NodeDropped with the `CLICK_SLOP` drag-vs-click
  threshold). **Done**: click opens a node, drag moves+persists it, edge + marquee
  select work, empty-space drag pans with inertia, behaviors match the current host.

## Net-new vs port

- **Port** (with reconciliation): `camera.rs` (Camera + world/screen + the
  drag-vs-click threshold) into a shared home unified with the platen Camera, and
  lowered to the container's CSS transform matrix.
- **Rebuild against serval**: everything else in `graph_canvas.rs` (Masonry Widget
  impls, pointer/scroll handling, direct `Painter` draws, the Xilem View) — the
  direct paint becomes `orrery_paint_list` going live; node discs+labels become
  abs-pos serval DOM children moved by inline transform under the container camera.
- **Net-new** (not ports): the abs-pos transform-propagation fix (1A), inertia, the
  wheel=pan / ctrl+wheel=zoom mapping, edge-pick, and marquee/rect-select.

## Deferred past Phase 1

Richer node glyphs + labels + theming in the producer; per-node radius; a dedicated
`EngineId` for the canvas paint sublist; stylesheet hot-reload on `IncrementalLayout`
(carried over from the persistent-Stylist work).
