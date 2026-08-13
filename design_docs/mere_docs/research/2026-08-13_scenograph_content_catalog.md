# Scenograph Content Catalog

**Date**: 2026-08-13
**Status**: brainstorm catalog, commissioned by Mark. This is the content pass
the [expansion brief](2026-08-10_scenograph_expansion_brief.md) prepared for:
ten candidates in each of five decks (arrangements, scenes, backdrops, node
representations, edge representations), spanning utility and novelty. Entries
are tracked design space, not commitments. The family standard holds: a
candidate ships only when a consumer forces it. Each entry names its likeliest
forcing consumer where one is visible today; consumers named here are
candidates for that role, not work already ordered.

**Related**:
[scenograph_expansion_brief](2026-08-10_scenograph_expansion_brief.md)
(the stagecraft tiers and lanes this catalog fills), the scene contract note
(`crates/scenograph/design_docs/2026-07-22_scene_contract_note.md`) (the
representation slot's recognized core, `RoutedRelation`, `Region`, `Space`,
hit overrides, `score::Arrangement`),
[swatch_primitive_design](../design/2026-06-27_swatch_primitive_design.md)
(the pipeline these decks parameterize; the cells-as-edges ruling),
[node_body_face_model_plan](../implementation_strategy/2026-06-23_node_body_face_model_plan.md)
(Body and Face as orthogonal node axes),
[sited_device_identity_brief](2026-08-10_sited_device_identity_brief.md)
(Atlas as the geographic arrangement's consumer).

Method notes:

- A representation earns its place by the inference it carries, not by what it
  resembles.
- Every node representation carries node identity: node color and selection
  state read the same in all of them.
- Decks run utility first, novelty last. **shipped** means it exists today;
  **core** means it is in the frozen contract's recognized vocabulary;
  unmarked entries are open-tail candidates.
- The decks compose: a deck-2 scene archetype is roughly one arrangement plus
  one backdrop plus representation choices. That composition is the swatch
  pipeline, so anything named here lands as a template, not a hardcode.

## 1. Arrangements

The portable tier is `score::Arrangement` (Spiral, Board, Geographic today);
the mere-side layout enum (minimap, body, radial, timeline, grid, force) is
the richer local end. This deck names candidates for the portable tier.
`scenomise::relax` is a post-pass over any of them, not a rival entry.

1. **Spiral** (shipped): growth placement from a seed, footprint-aware
   spacing. The default for arriving members.
2. **Board** (shipped): freeform pinned placement, position is user truth.
   The VTT and pinboard workhorse.
3. **Geographic** (shipped kind): placement anchored to ground coordinates.
   Forcing consumer: Atlas, the sited-radio map.
4. **Grid**: sort-keyed uniform cells; a masonry variant packs mixed
   footprints. Forcing: compendium tables, asset pickers.
5. **Ring**: concentric orbits by rank from a focus member. Forcing: the
   connections swatch, focus-and-context views.
6. **Tree**: tidy layered hierarchy; the workbench projection already walks
   one, this makes the drawing portable.
7. **Timeline**: placement by timestamp along a scrollable axis. Forcing:
   practice history, journal entries, scenotime epochs themselves.
8. **Lanes**: parallel tracks sharing one axis. Forcing: set order beside
   practice state in woodshed; channel traffic in signalman.
9. **Path**: members ordered along a routed polyline. Forcing: replay and
   itinerary views, a set walked in order.
10. **Spread**: stacks and fans with named positions, the deck-of-cards
    family. Forcing: cleromancy's spread layouts; a VTT hand of tokens.

## 2. Scenes

Archetypes a consumer would assemble from the tiers. Graphshell remote is the
standing discipline for all ten: each must survive the wire to a viewer with
no source access.

1. **Set graph** (L1, the release gate): staged cards as numbered nodes, Next
   edges for order, one fanned relation per reason. Woodshed.
2. **Whole-graph canvas** (shipped): the orrery, the root swatch over shared
   truth. Turnstone.
3. **Encounter map**: pixel backdrop that picks, token sprites, regions as
   zones, props hit-transparent. Isometry.
4. **Compendium table**: grid arrangement of card representations over a page
   surface. Isometry's data grid.
5. **Station chart**: geographic backdrop, radio nodes with liveness badges,
   link edges weighted by traffic. Signalman.
6. **Fretboard diagram**: lattice space, finger positions as glyphs,
   voice-leading edges. Woodshed.
7. **Rehearsal filmstrip**: a lane of scenotime snapshots as mini-scenes;
   the L4 and L5 meeting point.
8. **Spread reading**: spread arrangement, drawn cards as sprites, annotation
   edges to journal nodes. Cleromancy.
9. **Replay stage**: a scene re-projected from scenotime diffs over host
   time; the games wing's session record and the motion lane's proof.
10. **Embedded slice**: a small-scope swatch living inside a document or a
    second host; the anti-shell receipt as a scene archetype.

## 3. Backdrops

The missing tier (L2). The two waiting consumers lead; the deck widens from
there. Default posture: a backdrop never picks, with `hit` overrides for the
ones that must.

1. **Pixel map**: the VTT ground truth; the one backdrop that picks. Isometry.
2. **Stage floor**: the practice context behind staged content, fretboard or
   stage. Woodshed.
3. **Geographic basemap**: tiles or vector ground under Atlas placement. The
   host supplies placement facts; nodes still never persist their own
   coordinates.
4. **Graph paper**: grid, guides, and snap hints; the board arrangement's
   natural underlay.
5. **Imported underlay**: a user image or SVG to arrange over, floor plan,
   sketch, schematic. Cheap and broadly useful.
6. **Field heatmap**: a continuous signal layer rendered beneath content, the
   graph signals work given a home in the contract.
7. **Sky chart**: an ephemeris-driven backdrop that knows the time; the
   analytic ephemeris work already computes it. Cleromancy.
8. **Page surface**: a document page with margins and fold lines behind
   content, for print-shaped and knot-embedded scenes.
9. **Sited-region surface**: a surface carrying named drop zones as regions,
   the felt table, the battle zone, the sorting tray.
10. **Depth ambient**: parallax layers behind the canvas, coupled to the
    camera. Pure atmosphere until the games wing forces it.

## 4. Node representations

The contract's representation slot. Recognized core first, open tail after.
Body and Face stay orthogonal beneath all of these: any body can wear any
face, and a sprite seeds a body without owning it.

1. **Glyph** (core): dot or icon, the dense-LOD workhorse; the whole graph at
   a glance.
2. **Card** (core): title plus a facet summary; the readable middle LOD.
3. **Sprite** (core): an authored image face; tokens, album art, portraits by
   hand.
4. **Snapshot** (core): a rendered thumbnail of the node's content; the
   browser's memory of a page.
5. **LivePane** (core): the content itself, embedded and live; the top of the
   LOD ladder.
6. **Badge glyph**: a glyph carrying one live scalar, unread count, signal
   strength, due state. Forcing: signalman liveness, inbox-shaped graphs.
7. **Portrait**: identity-first, the emblem or avatar as the body. Forcing:
   contact nodes, personae surfaces.
8. **Meter**: a sparkline or gauge as the face, the node as its own recent
   history. Forcing: telemetry, practice stats.
9. **Shape**: the authored hull with material character, the Body axis made
   visible; heavy, bouncy, slippery nodes that read as such.
10. **Label**: typographic only, the caption is the body; outlines and
    knot-embedded words where chrome would be noise.

## 5. Edge representations

`RoutedRelation` is a polyline with an open kind and a weight; cells-as-edges
rules that each relation between a pair is its own addressable cell. The deck
runs the LOD ladder from collapsed to expressive.

1. **Line**: one collapsed stroke per pair; the LOD floor and the dense-view
   default.
2. **Fanned cells** (the ruling): one family-colored line per relation,
   individually selectable; multi-edge as truth.
3. **Weighted stroke**: thickness from the cell's own metric, never pair
   density.
4. **Directional stroke**: arrowhead, taper, or gradient carrying the in/out
   asymmetry the doctrine already stores.
5. **Styled kind**: dash and texture by relation family; tentative dashed,
   inferred dotted, asserted solid.
6. **Labeled edge**: the kind name riding the polyline at readable zoom.
7. **Bundle**: cells gathered into shared trunks above fan density; the
   collapse rung above fanning.
8. **Flow**: marching pulses along the polyline; traffic, replay, and the
   motion lane's cheapest visible win. Forcing: signalman links, replay
   stage.
9. **Ribbon**: a routed corridor whose width is capacity or volume, the
   Sankey reading of a relation.
10. **Ghost**: the interaction states, elastic preview while relating,
    revealed-edge suggestions before crystallizing; drawn like truth, styled
    as proposal.

## Where this feeds

Placement against the tiers happens per the brief's method: each entry is
scene, arrangement, or backdrop surface before it is code. The likely first
pulls: deck 1's Lanes and Spread ride L1 and the cleromancy consumer, deck 3's
entries 1 and 2 are L2's entrance prototypes, deck 5's Flow is the first L5
receipt. Everything else waits for its consumer.
