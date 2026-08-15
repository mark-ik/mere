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
**Revised 2026-08-14 (Mark)**: deck 2 rebuilt from consumer archetypes to
scenes proper, reusable total projection regimes; the archetypes survive
inside the entries as founding and transfer datasets.

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
- The decks divide labor: decks 1, 3, 4, and 5 name single levers; deck 2
  names scenes, regimes that fix every lever toward one intent. All of it is
  the swatch pipeline, so anything named here lands as a template, not a
  hardcode.

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

Definition (Mark, 2026-08-14): a scene is a cohesive projection of a dataset
built from graph primitive representations, tailored to a situation yet
reusable, so it applies beyond its founding dataset. A scene flexes every
lever toward one intent; the other decks name the levers, this deck names the
regimes. Changing one lever yields a variant of the same scene, never a new
one. The brand mark's law is the bar for entry: **the projection decides what
a node is**. A recipe that leaves a node being what the orrery already showed
is a variant. And the receipt that a recipe is a scene: it re-applies to a
second dataset and still reads true.

The orrery is scene zero, the reference regime, and every scene obeys its
laws: curation over truth (a scene never writes truth), node identity carried
everywhere, per-instance visibility, the shared element-model hit contract,
one pipeline. Miscibility follows: because every scene is a point in the same
lever space, levers pour between them (Atlas ground under Chronicle's axis
reads as a travel chronicle; Mosaic tiles inside Fog read as an exploration
gallery). Mechanically a scene lands as the template mechanism at graph
scope, parametric, persona-persisted, carrying product intent. Naming: scenes
are a third register beside surfaces and arrangements; where a scene grew
from a brand projection (Mosaic, Atlas) the scene carries the brand name and
its placement lever keeps the same plain word in the arrangement register.
Graphshell remote stays the standing discipline: every scene must survive the
wire to a viewer with no source access.

1. **Mosaic**: the collection, browsed whole. Adjacency-tiled placement where
   nearness carries kinship; nodes are edge-to-edge tiles with snapshot or
   sprite faces; edges withdraw into adjacency itself and fan only on
   selection; the tiles are the ground, so there is nothing behind them.
   Grammar: sort, filter, regroup. Founding: the compendium gallery.
   Transfer: bookmarks, a sample library, the whole graph at census zoom.
   Here a node is a tile.
2. **Atlas**: where things are. Geographic placement over a basemap; nodes as
   badge glyphs at true positions; edges as routed ways; regions as
   territories; the camera speaks map grammar and the world holds still. The
   host supplies placement facts; nodes never persist coordinates. Founding:
   sited radios. Transfer: contacts by locale, photos by place, isometry's
   overmap rendering through it. Here a node is a marker.
3. **Tabletop**: tangible play on a ground that means something. Board
   placement (position is user truth) over the one backdrop that picks;
   nodes as sprites with authored hulls and material character; regions as
   zones; edges withdrawn except measurement ghosts. Grammar: grab, drop,
   measure. Founding: isometry encounters. Transfer: seating charts, floor
   plans, play diagramming. Here a node is a piece.
4. **Chronicle**: when, and what followed. A timeline spine on an era-banded
   ribbon; events as cards, spans as meters; edges as arcs over the axis for
   cause and sequence; scenotime is native, the scrub transport replays
   diffs as motion. Founding: practice and journal history. Transfer: git
   history, the browsing trace, radio traffic; the rehearsal filmstrip is
   its strip form. Here a node is an event.
5. **Circuit**: how it is wired. Grid-snapped blocks with ports; edges as
   orthogonally routed traces, direction always drawn, weight as trace
   width, bundling at density; blueprint backdrop. Grammar: follow the wire,
   select a node and light its closure. Founding: the workspace dependency
   graph. Transfer: audio signal chains, radio chains, build pipelines. Here
   a node is a component.
6. **Loom**: parallel streams and their crossings. Lanes on a shared axis;
   nodes as cards in lane order; in-lane edges withdraw because order
   carries them, and cross-lane edges become the star of the view; flow
   pulses along the axis. Founding: the set beside practice state in
   woodshed. Transfer: channel traffic, multi-track loops, kanban with
   dependencies. Here a node is a beat in a stream.
7. **Spotlight**: one thing, and everything that touches it. The focus
   centered, neighbors ringed by rank; the full fan of cells at the focus so
   every relation is individually legible; context dimmed to glyphs; the
   rest withdrawn, as curation only. Grammar: re-focus to walk the graph.
   Founding: the multi-node connections swatch. Transfer: a contact's
   dossier, an entity audit, impact tracing. Here a node is a witness.
8. **Rosette**: cyclic and angular relations. Members on a wheel bound to a
   cycle; angular position carries the data; edges as chords across the disc
   (aspects); a sky chart or plain wheel behind; time rotates the wheel.
   Founding: cleromancy's ephemeris chart. Transfer: the circle of fifths,
   the year wheel, and text (Mark, 2026-08-14): lines and stanzas as
   stations on the meter's own cycle, chords as recurrence, rhyme,
   assonance, repeated lemmas, semantic kinship, with chord span carrying
   meaning (an end-rhyme spans a stanza; internal rhyme and alliteration
   are the short chords). Candidate forcing consumer: knot, as a
   creative-writing analysis scene over a document's interior; derived
   analysis edges arrive as revealed cells and crystallize only when the
   writer keeps them. The sound half's engine is named (Mark, 2026-08-15):
   **mora** (meter, weight, timing; the moraic unit generalizes across
   stress-, syllable-, and mora-timed languages) with **sonance** inside it
   (rhyme, assonance, alliteration); esp keeps the semantic chords.
   Unfounded; names verified free on crates.io 2026-08-15. Here a node is a
   station on the cycle.
9. **Fog**: the known against the unknown. Visited members lit and placed;
   unvisited ghosts at the fog line; the traveled path is the one privileged
   polyline; the backdrop darkens away from the visited. Grammar: step,
   peek, claim. Founding: the browsing trace corpus. Transfer: VTT fog of
   war, a research reading front, codebase exploration. Here a node is
   territory.
10. **Grove**: tending a living collection. Spiral growth by age; freshness
    as a field, bloom to wilt on the face; tended members brighten, stale
    ones fade; edges as root lines shown only while tending. Grammar: prune,
    touch, graft. Founding: the notes and memory garden. Transfer: feeds,
    dependency pins due for bumps, task rot. Here a node is a planting.

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

Placement against the tiers happens per the brief's method. Scenes need no
contract surface of their own: they land as graph-scope templates over the
shared pipeline, and it is their levers that force contract work. The likely
first pulls: Tabletop and Atlas force L2's backdrop prototypes, Loom rides
L1's set graph, Chronicle's scrub transport is the natural first L5 consumer
and deck 5's Flow its cheapest receipt. Everything else waits for its
consumer.
