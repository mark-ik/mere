# Node Editor + Customization: feasibility probe

**Date**: 2026-06-21
**Status**: Probe / unsettled. A feasibility sketch + design-space map for a per-node
**editor applet** (sprite editing / bitmap import) whose output is the node's texture/shape
**and** its physics hitbox. Raised by Mark off the P0-resize thread: *size* is a simple
settled knob (shipped); the *sprite / editability / customizability* of a node is "totally
unsettled." This doc frames that space; it does not settle it.
**Related**: [node_representation_arrangement_plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md)
(the Representation forms + the size knob), the gyre collider (crates/orrery/gyre).

---

## The ask

A node editor applet that pops up on right-click → **"Edit node"** (and, later, lives in a
**node facets menu** we have not built yet). It offers basic, MS-Paint-level sprite editing
**or** import of a bitmap (or whatever format) that becomes the node's texture/shape — and
that sprite **shapes the collider's hitbox**, so the node's grab + physics match its drawn
silhouette, not a square or a ball.

## Is it possible? Yes — the pieces already half-exist

- **Sprite as the node face.** A node face is already a chrome DOM card, and we already paint
  an arbitrary image on it: the favicon is a PNG **data-URI `<img>`**, and the snapshot card
  is a host-rasterized page preview encoded the same way (2026-06-21). A user-authored sprite
  is the *same mechanism* — a per-node data-URI `<img>` (or `<external-texture>` for a live
  one). This is a new **Representation** form (e.g. `Sprite` / `Custom`) slotting into the P1
  hook beside `Tile` / `Shape`, not a new rendering path.
- **Bitmap import** is the easy on-ramp: a file pick → decode (the `image` crate is already a
  dep, used by `favicon_data_uri`) → store the RGBA → data-URI it onto the face. Ships before
  any in-app editor.
- **In-app editing** (MS-Paint level) is a real but bounded UI: a fixed-grid pixel canvas
  (e.g. 32×32 / 64×64) with pencil + fill + a small palette is the minimal version; a free
  bitmap canvas is the richer one. It is a serval document like the other overlays (palette /
  settings), so it composites correctly now that the **shell z-stack** is principled
  (2026-06-21) — an editor pane is just another chrome layer over the orrery.
- **Sprite → collider (the hard, interesting part).** Today gyre gives every node a fixed
  `ColliderBuilder::ball(NODE_BODY_RADIUS)` (gyre lib.rs:441). Deriving the collider from the
  sprite is a spectrum:
  1. **Bounding box** sized to the sprite (the `size` case, generalized to a rect).
  2. **Convex hull** of the sprite's opaque pixels — one rapier convex polygon. Cheap, robust,
     good enough for most silhouettes.
  3. **Concave outline** (marching-squares on the alpha → a polyline → a rapier compound /
     decomposed collider). Faithful to arbitrary shapes; the most work + the per-node cost to
     watch at graph scale.
  rapier supports all three. The choice is a perf-vs-fidelity call (see open questions).

## The through-line: the hitbox axis

(2) "grow the collider" and (1) "sprite shapes the hitbox" are the **same axis** — the node's
collider derives from its appearance — at two points on it:

| step | appearance source | collider |
|------|-------------------|----------|
| today | fixed | `ball(NODE_BODY_RADIUS)` |
| **(2), approved** | the `size` knob | a **ball/box grown to `node_size`** — grab matches the visual square |
| **(1), unsettled** | the sprite | a **hull/outline from the sprite alpha** — grab matches the silhouette |

So growing the collider with `size` is not a throwaway tweak: it is step 1 of this axis, and
it forces the same gyre work the sprite case needs (a **per-node collider radius/shape**, not
the global `NODE_BODY_RADIUS` — which today also feeds the forces (forces.rs) and the
`LayoutView` edge-attach (gyre lib.rs:325), so per-node-ness threads through all three). Doing
(2) lands that plumbing; (1) later swaps the ball for a sprite-derived shape on top of it.

## The node facets menu (unbuilt)

"Edit node" wants a richer home than the right-click context menu: a **node facets** panel —
a per-node inspector exposing the node's facets for editing (representation, size, sprite,
color/state, tags, engine pin, relations). The context menu stays the quick path; the facets
panel is the deep one (and the natural host for the editor applet). This is net-new UI; it
overlaps the `apparatus` system-inspector pattern (a side panel) but is node-scoped. It is
also where the scattered per-node toggles we keep adding to the context menu
("Show as tile/shape", "Size by degree", engine pins) would consolidate.

## Settled vs unsettled (Mark's framing)

- **Settled** (shipped): the **size** knob — a scalar per node + size-by-degree. Simple.
- **Unsettled**: everything about the **sprite** — the editor's scope, the sprite format, the
  collider-from-sprite fidelity, the facets-menu shape, persistence of per-node image data,
  and how "custom sprite" relates to the other Representation forms and to P2's scripted form.

## Open design questions

1. **Editor scope.** Minimal (fixed pixel grid + pencil/fill/import) or richer (free canvas,
   brushes, layers)? Start minimal; import-first.
2. **Sprite format + resolution.** A fixed small grid (pixel-art, cheap, on-brand for nodes)
   or arbitrary bitmaps (flexible, heavier)? Affects storage + the collider extraction.
3. **Collider fidelity.** Box / convex hull / concave outline — and at what graph scale does
   the per-node shape cost bite? Likely default to convex hull, opt into concave.
4. **Facets menu.** Panel / pane / modal? What facets, in what order? Does it subsume the
   per-node context-menu toggles?
5. **Persistence + identity.** A sprite is real per-node data (KB, not a scalar). Where does it
   live — the node's content/kernel, a sidecar, a blob store? Does it travel with the node
   across sync?
6. **Representation relationship.** Is "custom sprite" a new `Representation` variant, or is a
   sprite orthogonal (any form can carry one)? Likely a variant for the face-replacing case.
7. **Scripted / live forms.** Does the editor connect to P2's scripted form (the field-regions
   rhai substrate) and to live-texture nodes (P2-interactive, owned by
   [native_surface_compositing_plan](../implementation_strategy/2026-06-19_native_surface_compositing_plan.md))?
   A static sprite is the floor; scripted/live is the ceiling.

## Suggested first steps (smallest real progress, in order)

1. **Grow the collider with `size`** (approved). Lands the per-node collider radius in gyre
   (collider + forces + LayoutView) — the hitbox-axis plumbing both (2) and (1) need.
2. **Bitmap import → custom face.** A `Representation::Sprite` carrying a per-node data-URI
   image, set from a file pick. No editor yet; proves the sprite-as-face + persistence path.
3. **Convex-hull collider from the sprite.** Swap the grown ball for a hull of the sprite's
   opaque pixels — the first real sprite→hitbox step.
4. **The facets panel + the in-app editor** — the big unsettled UI, once 1–3 prove the spine.
