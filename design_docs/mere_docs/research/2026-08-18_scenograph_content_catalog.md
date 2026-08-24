# Scenograph Content Catalog

**Date**: 2026-08-18
**Status**: brainstorm catalog, commissioned by Mark. This is the content pass
the [expansion brief](2026-08-10_scenograph_expansion_brief.md) prepared for:
ten candidates in each of five decks (arrangements, scenes, backdrops, node
representations, edge representations), spanning utility and novelty. Entries
are tracked design space, not commitments. The family standard holds: a
candidate ships only when a consumer forces it. Each entry names its likeliest
forcing consumer where one is visible today; consumers named here are
candidates for that role, not work already ordered.
**Revised 2026-08-18 (Mark)**: deck 2 rebuilt from consumer archetypes to
scenes proper, reusable total projection regimes; the archetypes survive
inside the entries as founding and transfer datasets.

**Reviewed 2026-08-23 (Mark):** [Projection Scenes and the Graph-Native Application Platform](../../2026-08-23_projection_scenes_and_graph_native_platform.md)
re-evaluates deck 2 after testing fifty candidate scenes against the grammar.
This catalog remains the implementation and design history, including
Rosette's landed Knot evidence. The follow-on owns the current categorization,
lower-layer reclassifications, separate evidence status, Matrix ruling, and
platform consequences.

**Related**:
[projection_grammar_catalog](2026-08-15_projection_grammar_catalog.md)
(the governing primitive grammar; every scene below is composed from it),
[scenograph_expansion_brief](2026-08-10_scenograph_expansion_brief.md)
(the stagecraft tiers and lanes this catalog fills), the scene contract note
(`design_docs/scenograph_docs/technical_architecture/2026-07-22_scene_contract_note.md`) (the
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
  **core** means it is in the portable contract's recognized vocabulary;
  unmarked entries are open-tail candidates.
- The decks divide labor: decks 1, 3, 4, and 5 name single levers; deck 2
  names scenes, regimes that fix every lever toward one intent. All of it is
  the swatch pipeline, so anything named here lands as a template, not a
  hardcode.

## 1. Arrangements

The portable tier is `score::Arrangement` (Spiral, Grid, Geographic, and Hulls);
the Mere-side registry (Grid, Radial, Stack, Spiral, Timeline, Columns,
Penrose, Fractal, Semantic Embedding, and Spectral) is the richer local end.
This deck names candidates for the portable tier.
`scenomise::relax` is a post-pass over any of them, not a rival entry.

1. **Spiral** (shipped): growth placement from a seed, footprint-aware
   spacing. The default for arriving members.
2. **Grid** (shipped, portable): regular cells, including explicit authored
   cells and deterministic ordinal fallback.
3. **Geographic** (shipped kind): placement anchored to ground coordinates.
   Forcing consumer: Atlas, the sited-radio map.
4. **Columns** (shipped locally): categorical columns, currently reached by
   the `graph_layout:kanban` id. Forcing: site and community grouping.
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
10. **Plotted** (ratified direction, not shipped): supplied coordinates with
    explicit provenance. It is the likely convergence point for direct
    coordinate placement once a proof shows that Geographic and other plotted
    readings can share it. Authored fans and stacks stay scene-local until a
    second consumer forces reusable named slots.

## 2. Scenes

Definition (Mark, 2026-08-18): a scene is a cohesive projection of a dataset
built from graph primitive representations, tailored to a situation yet
reusable, so it applies beyond its founding dataset. A scene flexes every
lever toward one intent; the other decks name the levers, this deck names the
regimes. Changing one lever yields a variant of the same scene, never a new
one. The brand mark's law is the bar for entry: **the projection decides what
a node is**. A recipe that leaves a node being what the orrery already showed
is a variant. And the receipt that a recipe is a scene: it re-applies to a
second dataset and still reads true.

The dependency is deliberate: projection grammar, then scene recipe, then
product adapter and authority. A recipe may only use grammar vocabulary that
already exists. If it needs a missing primitive, that primitive gets a
promotion proof first. A future Mere-side scene register may depend on the
grammar and compiler types; `sceno` must never import the register or know
these names.

The orrery is scene zero, the reference regime, and every scene obeys its
laws: curation over truth (a scene never writes truth), node identity carried
everywhere, per-instance visibility, the shared element-model hit contract,
one pipeline. Miscibility follows: because every scene is a point in the same
lever space, levers pour between them (Atlas ground under Chronicle's axis
reads as a travel chronicle; Mosaic tiles inside Fog read as an exploration
gallery). Mechanically a scene lands as the template mechanism at graph
scope, parametric, persona-persisted, carrying product intent. Graphshell
remote stays the standing discipline: every scene must survive the wire to a
viewer with no source access.

**Levers co-vary; that is why the scene is the unit (Mark, 2026-08-18).**
The catalog originally reserved *Mosaic* and *Atlas* as arrangement names, on
the assumption that a scene is its placement lever plus decoration. It is not.
There is no Atlas without a map: geographic placement with nothing beneath it
is a scatter of dots, because the backdrop is what makes a position mean a
place. And Mosaic's adjacency-as-edge does not arrive from putting nodes next
to each other; the tiles must also size themselves to close the gaps, so the
representation lever is doing half the work of the edge lever. In both cases
the intent is carried by a *combination* no single lever holds, which is
exactly why a scene is the reusable unit and a lever is not.

Two consequences. **Naming**: the brand words belong to the scene register,
not the arrangement register, so the reservations recorded in the
[projection proofs plan](../implementation_strategy/2026-07-21_projection_proofs_plan.md)
are released to it and the placement levers keep plain mechanism names.
Mark's own reading of the two levers: Mosaic's placement is **Grid** (already
in the arrangement register, in its packing variant, which needs no new name),
and Atlas's is *relative persistent spacing* — positions given by the data and
preserved against each other, never force-settled. That mechanic has no
arrangement entry yet; **Plotted** is proposed for it, and it generalizes the
existing Semantic entry, since an embedding and a latitude are the same lever
with different coordinate sources. **Composition**: a scene template must
carry every lever it depends on, not a placement id plus defaults, or it
arrives at a second dataset as a scatter of dots.

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
3. **Tabletop**: tangible play on a ground that means something. Authored
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
   the year wheel, and text (Mark, 2026-08-18): lines and stanzas as
   stations on the meter's own cycle, chords as recurrence, rhyme,
   assonance, repeated lemmas, semantic kinship, with chord span carrying
   meaning (an end-rhyme spans a stanza; internal rhyme and alliteration
   are the short chords). Candidate forcing consumer: knot, as a
   creative-writing analysis scene over a document's interior; derived
   analysis edges arrive as revealed cells and crystallize only when the
    writer keeps them. The sound half's engine is decided (Mark, 2026-08-18):
   **mora** (meter, weight, timing; the moraic unit generalizes across
   stress-, syllable-, and mora-timed typology, and only grapheme-to-phoneme
   is per-language work, a lexicon plus rules, with weight, stress, meter,
   and sonance all general above it) carrying **sonance** as its
   sound-kinship module (rhyme, assonance, alliteration); esp keeps the
    semantic chords. English v1 rides CMUdict. Landed through Knot with poem
    and lyric datasets, using `mora-cmudict`, `mora.perfect-rhyme`, a
    `ProjectionSession`, and a local carrier. Here a node is a station on the
    cycle.
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
4. **Graph paper**: grid, guides, and snap hints; the Grid arrangement's
   natural underlay.
5. **Imported underlay**: a user image or SVG to arrange over, floor plan,
   sketch, schematic. Cheap and broadly useful.
6. **Field heatmap**: a continuous signal layer rendered beneath content, the
   graph signals work given a home in the contract.
7. **Sky chart**: a product-derived backdrop recipe driven by ephemeris facts.
    It becomes portable data only when a remote scene must receive those facts.
8. **Page surface**: a document page with margins and fold lines behind
   content, for print-shaped and knot-embedded scenes.
9. **Sited-region surface**: a surface carrying named drop zones as regions,
   the felt table, the battle zone, the sorting tray.
10. **Depth ambient**: a renderer or realization recipe for parallax layers
     coupled to the camera. It is not portable scene data until a scene must
     carry authored depth facts.

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
7. **Portrait**: identity-first emblem or avatar as the Face. It can sit on
   any Body; contact nodes and personae surfaces are likely consumers.
8. **Meter**: a sparkline or gauge as the face, the node as its own recent
   history. Forcing: telemetry, practice stats.
9. **Shape**: the authored hull with material character, the Body axis made
   visible; heavy, bouncy, slippery nodes that read as such.
10. **Label**: typographic only, the caption is the body; outlines and
    knot-embedded words where chrome would be noise.

## 5. Edge representations

`RoutedRelation` is a polyline with an open kind and a weight; cells-as-edges
rules that each relation between a pair is its own addressable cell. This deck
contains structural relation forms only. Motion overlays and interaction
previews are separate lever classes.

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
8. **Ribbon**: a routed corridor whose width is capacity or volume, the
   Sankey reading of a relation.
9. **Port-anchored route**: an endpoint-specific path for circuits,
   architecture, and flowcharts.
10. **Orthogonal or stepped route**: a schematic connector whose geometry is
    constrained without changing relation identity.

**Motion overlay, separate class**: marching pulses along a relation can carry
traffic or replay state. This is Flow motion over a structural edge form, not
an edge representation itself.

**Interaction preview, separate class**: an elastic connector while relating,
or a revealed-edge suggestion before crystallization. This is a Ghost preview,
not graph truth and not a structural edge form.

## Where this feeds

Placement against the tiers happens per the brief's method. Scenes need no
parallel primitive contract: they land as graph-scope recipes over the shared
grammar, and it is their missing levers that force grammar work. The likely
first pulls: Tabletop and Atlas force L2's backdrop prototypes, Loom rides
L1's set graph, Chronicle's scrub transport is the natural first L5 consumer
and deck 5's Flow its cheapest receipt. Everything else waits for its
consumer.
