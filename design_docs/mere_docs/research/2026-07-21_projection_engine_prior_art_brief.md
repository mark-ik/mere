# Projection Engine Prior Art: the compiler/runtime destination, and the 3D question

**Date**: 2026-07-21
**Status**: Research brief (with Mark). Began as three chat questions (is mere's direction "a
projection engine alongside genet"; prior art; does it point to 3D). Same-day it absorbed the
hulls-as-fields follow-up (§5), an adversarial review that corrected the architectural
conclusion, and a second critique that named the destination (§1, §5-§8): a **projection
compiler and runtime for interactive surfaces**, portable across the product family. All
critique claims code-verified before adoption (see Progress). No code changed this session.
**Code touched (read-only survey)**: `crates/canvas/arrangements`, `crates/canvas/canvas`
(underlay, cartography_scene, fields, sprite_hull), `crates/canvas/cartography` (strategy,
request, projection), `crates/forme` (topology, SplitPanes), numen `field.rs`/`coupling.rs`,
`merecat/src/shell.rs`, `isometry-core/src/overmap.rs`, `woodshed-views/src/stage.rs`.

**Related**:

- [graph_projections_research](2026-06-22_graph_projections_research.md) pins the surface-level
  vocabulary (arrangement / lens / projection). This brief works below all three: the engine.
- [modular_integration_plan §1](../implementation_strategy/2026-06-02_modular_integration_plan.md):
  the graph-rooted projection model. Within mere that rule stands; §5 generalizes the *engine*
  beneath it to sources beyond mere's graph.
- [data_oriented_doctrine_brief](../../2026-07-02_data_oriented_doctrine_brief.md): the §5 model
  (typed snapshots in, scene values out, deltas between) is the doctrine at the scene boundary.
- [one_node_facets_layer_map](../technical_architecture/2026-07-18_one_node_facets_layer_map.md)
  and [node_dissolution_facets_plan](../implementation_strategy/2026-07-18_node_dissolution_facets_plan.md):
  the facet system §5's source-fact modeling rides. **S2 is complete** (2026-07-19, `631b852`):
  the kernel `Node` carries no geometry; placement persists as `arrangement.position` facets.
- [participant_gate_packs_plan](../implementation_strategy/2026-07-17_participant_gate_packs_plan.md):
  "gestures route back as authorized intents" (§1) is this gate's vocabulary; scene actions are
  typed proposals, not direct mutations.
- [isometric_orrery_camera_plan](../implementation_strategy/2026-06-22_isometric_orrery_camera_plan.md):
  the shipped 2.5D lane §4 builds on.
- [burn_utilization_brief](2026-07-04_burn_utilization_brief.md): §7's signal-producer posture
  consumes the approved burn direction, not a new lane.
- [node_representation_arrangement_plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md):
  representation stays orthogonal to placement; §5 sharpens this into "the representation
  measures, the projection places."

---

## 1. The framing question, and the destination

The sentence that survived the first review:

> Mere's distinctive subsystem is a graph-to-surface **projection engine**. Merecat composes its
> surfaces **beside** genet's document-engine surfaces, over a **shared scene, raster, and
> compositor substrate**.

The second critique extends it into a destination:

> Mere's correct destination is a **projection compiler and runtime for interactive surfaces**.
> Dots joined by strokes are one representation among many.

The core operation:

```text
source data + relationships + signals + projection settings
    → select and derive
    → map data to visual channels
    → solve placement
    → produce an interactive scene
    → route gestures back as authorized intents
```

**Stage vocabulary (ruled with Mark, 2026-07-21)**: the serialized projection settings are a
**score**, the middle stages are **choreography**, the output is an **inhabited scene**.
Compiler/runtime stays as the explanatory metaphor, and each half is load-bearing. Compiler,
because the input is a spec, not a call: scores are data (diffable, versionable, shareable as
engrams/packs, sendable over a constrained radio link), and the stage boundaries give N source
adapters and N placement solvers independence over the projection graph as the intermediate
representation. Runtime, because scenes are inhabited, not printed: facts stream, signals
arrive late stamped with generations, users drag; the runtime owns incremental recompute,
generation-keyed caches, scene diffs out, and gestures resolving through hit shapes to intents
back. The stack already runs this pattern twice (cambium/xilem's declarative views with a
reconciling runtime; genet's incremental layout), so the short form is **"xilem for space"**;
the database vocabulary for the same commitment is incremental view maintenance over a
materialized projection graph. Limits held honestly: there is no language and no VM
("compiler" means staged translation with defaults, and a spec syntax if it ever exists is
TOML in an engram), and the framing pays rent only when a pipeline end has more than one
occupant, which is what proof 4 (isometry) establishes.

Every stage has precedent (§3) and partial implementation (§2), and no stage is currently
general: the present contract requires mere's `kernel::Graph` on the way in
(`ProjectionRequest`), assumes one `NodeKey` becomes one point plus radius on the way out
(`Projection`), and the one host path that exists (`project_canvas_strategy`) discards
everything but positions. The compiler/runtime is a synthesis still to be performed; §6-§8 say
in what order and what proves it.

Within mere the graph-rooted rule stands (the graph is the root; surfaces are contingent
projections). The engine beneath, though, should not require its source to *be* mere's graph:
hocket phrases, isometry places and tiles, woodshed theory material, retinue destinations and
radios are all projectable sources (§7), and mere is the richest authoring host among them, the
place where cross-source references get related, viewed, arranged, and acted on through one
grammar of interactive scenes.

The Navigator rule is untouched: projections are surfaces above the engine; the engine does not
multiply them.

## 2. What exists today (code-verified 2026-07-21)

- **Inner layout contract**: `arrangements::Layout::step()` returns `HashMap<N, Vector2D<f32>>`.
  Position-only, 2D-only, but generic over `N` (already kernel-neutral). `LayoutExtras` carries
  `pinned`, `dragging`, `embedding_by_node`, `axis_value_by_node`, `domain_by_node`.
- **Analytic contract**: `cartography::LayoutStrategy` maps `ProjectionRequest → Projection`
  one-shot. `ProjectionRequest` hard-requires `&kernel::graph::Graph` (the kernel coupling §6
  removes). `ViewIntent` already carries `form_factor`, a **`dimension: ProjectionDimension`**
  (the dimension-parametric seam §4 wants), `focus`, `filter`, `target_size`, `axis_values`.
- **Scene boundary**: `cartography::Projection` = `PositionedNode { node, position, radius }`
  (radius is a proto-scale channel), polyline `PositionedEdge`s, `overlays`, `minimap`,
  `content_bounds`, open `metadata`. One `NodeKey` = one instance; no footprints beyond radius;
  no representation slot; no hit/action model. This is the ceiling the §5 scene contract lifts.
- **Host wiring**: merecat never calls `project_canvas_strategy`; that function itself returns
  `Vec<(NodeKey, PortablePoint)>`, discarding radius/edges/overlays/bounds. The gloss's
  `project_canvas_lens` keeps the overlay channel; the main view keeps positions only.
- **Geometry is sidecar truth** (S2 complete): placement persists as `arrangement.position`
  facets, round-trip tested at the adopt seam.
- **2.5D is shipped**: isometric yaw/tilt camera, billboard cards, z-index depth, synthetic
  height-by-degree on stems, headed-verified 2026-06-22.
- **Fields are live canvas citizens**: numen `Field` (identity + scalar/vector definition +
  `FieldExtent::{Global, Region, AttachedToNode}` + lifecycle), couplings
  (`field → NodeSelector × response × strength`, `ContainmentWall` in the recognized core, open
  IRI tail reserving `selection/member`). Canvas `fields.rs`: Region move/resize drag, hover,
  visibility, delete, coupling rebuild on move.
- **Per-node hulls exist**: `trace_sprite_hull` (RGBA → simplified polygon, decimated for drag
  handles) + the persisted `ARRANGEMENT_SPRITE_HULL` facet.
- **The product family already duplicates the engine piecemeal** (the consumer pull for §6):
  isometry's `Overmap::layout()` (`isometry-core/src/overmap.rs:123`) hand-rolls a
  deterministic Fruchterman-Reingold force layout, authored positions overriding, then
  isometry-views converts to a cambium `GraphCanvasSwatch`; woodshed's `related_swatch`
  (`woodshed-views/src/stage.rs:48`) hand-computes radial placement into the same swatch type.
  Both are re-implementations of layouts mere's catalog already owns.
- **Nothing geographic anywhere**: no lat/lon, no map projection, no GNSS type; sennet does not
  surface position data yet; no map/image underlay lane exists.

## 3. Prior art, six lanes

Each lane is precedent for one slice of §1's pipeline, each with one lesson.

### 3.1 Grammar of graphics (Wilkinson; Vega-Lite / Vega)

Vega-Lite specifications map data fields to visual encoding channels (x, y, color, size,
shape); a compiler fills in scales, axes, legends by rule and emits a full scenegraph. This is
§1's "map data to visual channels" stage, and the compile framing itself.

**Lesson**: name the channel set explicitly and make the mapping a declarative, serializable
value, not code. Serializable mappings are engrams, so arrangements travel as packs. Compiler
posture: defaults derived by rule, so a spec stays small.

### 3.2 Projectional editing (JetBrains MPS)

No parser; the AST is the only truth; projection rules render it as text, tables, math,
diagrams; editing happens through the projection back onto the AST; MPS 3.0 added switchable
per-concept projections.

**Lesson**: projections must be writable, and **writes return to the authority that owns the
fact**. Dragging a gnode writes arrangement state (`arrangement.position`); accepting a
suggested edge writes graph truth (`assert_relation`); a retinue gesture (§7) becomes a command
retinue authorizes and performs. Scene actions are intents, not mutations (the participant
gate's vocabulary).

### 3.3 Moldable development (Glamorous Toolkit)

Views as cheap, per-object, context-activated artifacts; thousands of tiny custom tools per
system.

**Lesson**: per-content-class views must be cheap to define, or nobody defines them. A content
class is a facet bundle plus schema engram; its default representations and preferred channels
belong in that bundle. Apparatus's retargeting-with-selection is GT's inspector pattern.

### 3.4 Zoomable UIs (Pad++, Piccolo)

Zooming as fundamental navigation; semantic zooming (representation changes with scale); the
Pad++ → Jazz → Piccolo lineage worked out infinite-canvas scene and camera machinery.

**Lesson**: mostly confirmation (LOD is semantic zoom; the infinite canvas defaults doc is
Pad++'s posture). Worth stealing: Pad++ lenses, a viewport region imposing a different
projection on what passes under it.

### 3.5 deck.gl (over MapLibre/Mapbox)

A data-driven layer stack (data arrays → accessors → GPU attributes → shaders) composited over
a base map renderer it does not own, with pluggable views and in-shader Mercator. Two engines,
one composite, a thin contract between.

**Lesson**: the geographic lane is an adapter, not an engine. A geographic fact through a fixed
transform is thin; the base map is an underlay layer. GNSS radios are source facts plus pin
intent; unpositioned peers arrange around them.

### 3.6 Spatial hypertext (Aquanet → VIKI → VKB, Shipman & Marshall)

Users on spatial canvases stopped making explicit links and expressed structure through
placement; VIKI/VKB parsed arrangement to recognize implicit structure and offered it back.

**Lesson**: projection runs both directions. "Tiles form a map, edges are adjacency" is a
layout *and* a recognizer. The recognizer is an optional producer whose suggestions write back
through the §3.2 rule.

### On novelty

This survey found no close match for the full combination: a general projection compiler and
runtime beside a full web engine, where web documents are ordinary projectable members among
heterogeneous sources. GT is nearest in spirit, deck.gl in architecture. Each component has
precedent; the integration is the work, and proven components do not remove integration risk.

## 4. The 3D question

2D-first holds, 2.5D is shipped, full 3D is task-and-display-gated.

- Ware & Franck (1996) and successors: 3D node-link viewing **with stereo and motion cues**
  substantially improves path-tracing (roughly 3x graph-size comprehension at equal error),
  including for abstract, layout-generated 3D.
- The caveat, reinforced by Cockburn & McKenzie: monoscopic 3D on a flat screen mostly
  underperforms 2D for reading and recall; occlusion, viewpoint dependence, and pointing costs
  eat the gains.
- Immersive analytics reopened the question for headsets, where immersion restores the cues.

Posture: (1) source truth stays dimension-neutral (S2 done; facts like altitude are data, not
geometry). (2) 2.5D is the shipped rung, and its height-by-degree is layout-derived z used as
presentation emphasis under a fixed tilt, the honest counterexample to any "never
layout-invented z" rule. (3) Free 3D navigation earns its place when the display supplies real
depth cues and the task is structure-tracing, or when a third axis is data (terrain, GNSS
altitude, tabletop height). (4) Keep seams honest and cost honestly: `ViewIntent.dimension` and
`supports_3d` already exist; footprints later admitting volumes is a design decision now and a
migration never; but a real 3D lane is rendering, picking, occlusion, physics-dimensionality,
and camera work (the isometric plan's dimensional-hotswitch notes scope it).

## 5. The model

Three structures, kept distinct:

1. **Source data.** Hocket tracks and phrases, retinue destinations and links, isometry places
   and tiles, woodshed theory material, mere graph objects. Each source owns its native truth
   and operations; the engine holds stable references, never copies of authority.
2. **Projection graph.** The selected items and derived relationships relevant to one view.
   Derivation lives here: grid adjacency, similarity edges, a time ordering can all be derived
   for a view without becoming authored source truth.
3. **Projection scene.** Visual instances, coordinate spaces, footprints, layers, content
   slots, hit regions, routed relationships.

The separation buys the two things the current `Projection` cannot say: one source object may
appear several times in one scene (a hocket phrase in the loop table, the history branch, the
similarity map, and the handoff view, with no duplication of truth), and an edge may act as a
visible stroke, a layout constraint, a shared tile boundary, a route, or a containment
relationship depending on the projection.

A **projected item** needs roughly:

- stable source reference + separate instance id
- parent coordinate space
- transform + footprint (point, rectangle, polygon, path; later volume)
- representation reference (glyph, card, sprite, live pane, snapshot, nested projection)
- intrinsic size constraints + assigned size
- layer, depth, visibility, LOD
- hit shape + available actions

**The representation measures content; the projection places it.** That division is how a
browser pane sits in a phyllotaxis spiral without the engine learning to render a web document,
and it is the general form of the representation-orthogonality rule.

It also corrects this brief's earlier `TreeGeometry` stance: the *solvers* stay separate, but
symmetric splitting (nested rectangles), phyllotaxis (transformed rectangles), and a map
(polygons) are all layouts of surface instances, and all should emit the same projection-scene
contract.

### Frames, groups, regions, fields, pins: five primitives, one gesture

The first draft over-unified here (pin as hull-of-one; hulls as fields). Corrected: keep the
primitives distinct, and let the hull *gesture* compose them.

- **Frame**: a coordinate space and transform.
- **Group**: enumerated membership with a shared transform.
- **Region**: an authored or derived contour.
- **Field**: behavior applied over a region (numen's existing primitive).
- **Pin**: a constraint between an item or group and a frame.

Drawing a hull over nodes creates a group + a region + optionally a field, together, as one UI
gesture; dragging the hull is one group-transform, not N node edits. Carry stays a dial
(rigid → spring → `ContainmentWall` → visual only). Freeform draw is stroke → simplify →
contour (concave needs point-in-polygon + RDP; the sprite-hull decimator is kin). A hull of one
node is a small group with its own region, tunable in an Apparatus hull section. A sprite
silhouette and a group hull **reuse** polygon editing and hit-testing while keeping different
meanings (collider shape vs authored region). Membership is capture-at-draw and enumerated
(new-node joining is an explicit gesture); membership stays engine-side unless explicitly
asserted as graph structure through the §3.2 rule. Geographic projections are then just frames
whose transforms derive from source facts (CRS, lat/lon, altitude, accuracy, observation time,
as facets), and user placement is `arrangement.position` plus pin intent, with layout output as
cache. Hull z-order sharpens the existing field-overlap follow-up.

## 6. Crate direction: the scenograph family

**Named (ruled with Mark, 2026-07-21)**: family repo **`scenograph`** (the eidetic/conatus
family-repo pattern), founded MIT/Apache + edition 2024, all four names verified free on
crates.io the same day. The members share the `sceno-` stem, one function morpheme each:

- **`sceno`** (core, the bare stem): source references, values, scores, channels, coordinate
  spaces, footprints, scene snapshots, action intents. No kernel, no genet, no wgpu, no product
  deps.
- **`scenomise`** (choreography / layout; mise-en-scène survives in the name, and it reads as a
  verb on the economise pattern, "to scenomise" = to arrange into a scene): today's generic
  `arrangements` algorithms plus rectangular subdivision, adjacency tiling, geographic
  transforms.
- **`scenotime`** (runtime; the spacetime echo is earned, time is load-bearing here):
  incremental evaluation, dependency tracking, caching, strategy registry, signal generations,
  scene diffs, the inhabited-scene half.
- **`scenograph`** (facade, bevy-style): a thin re-export of the three, so a consumer gets a
  one-dependency option and the family's best name does work.

Founding posture: sceno + scenomise can be founded up front (their contents are proof-1/2
material); scenotime may be founded as a name-holding placeholder in the same pass, with its
substance arriving as proofs 3-5 make its absence hurt. The naming discipline: crate names and
the score/choreography/inhabited-scene stage vocabulary are family-internal and doc-level; UI
copy stays plain per the plain-vocabulary rule (users see "layout" and "arrangement," never
"choreography").

**graphshell: the family's remote lens (destination ruled with Mark, 2026-07-22).** It tempted
as the engine umbrella, but pelt (its analog) is genet's *host shell*, product-coupled and
wrapped around the engine — the opposite coupling profile from a portable engine family. Read
the right way around, "graph shell" names the layer *above* the engine, and Mark's extension
gives that layer its product form: **a wasm-first thin client for web and mobile, connecting to
your own running apps over p2p — like a cloud service, except the app is the backend, wherever
it runs.** The wire protocol is what scenograph already builds: scores travel out, scene
snapshots + scenotime diffs travel back, gestures return as intents through the participant
gate (persona → grant → typed proposal → attributed apply), so the client holds a revocable
grant and a lens, never truth. The wasm constraints line up: a thin client is receive-diffs /
paint / hit-test / send-intents, so single-threaded (no SAB wall), no client-side script engine
(no-JIT posture costs nothing), and genet's WebGPU-in-browser receipts cover rendering. The
one-state-N-windows doctrine extends unmodified: ortet and ramets, some ramets on other
devices. Availability beyond your devices comes from the mesh/moot voluntary-hosting lanes, not
a datacenter; constrained links get the degenerate form (a LoRa-scale "scene" is a
management-snapshot of facts). **Three consumers with existing roadmap pull**: isometry-web
(players joining a DM's session from a browser IS this client), the radio companion app
(Merely LLC's phone-side management surface — the commercial pull), and the remote merecat
lens. **Sequencing gate**: behind the proof ladder — needs the contract proven on two products
(proof 4) and scenotime's diff lane; the new pieces it demands are a gated projection-server
seam in apps (score subscription → scene stream + intent sink), the diff wire format, and the
client itself. Reviving the name still takes the serval→genet renaming discipline (the name
means the donor everywhere in the corpus today), but this use finally deserves it: a shell over
graphs, thin as a shell.

Migration posture:

- Migrate the kernel-neutral parts of `cartography` into `sceno`; the remaining cartography
  becomes **mere's graph adapter** (source → projection graph).
- Rename/migrate `arrangements` into `scenomise` (its `Layout<N>` is already generic).
- `forme`, `platen`, `canvas`, and mere's fields stay in mere.
- Genet, wgpu, retinue, isometry, and burn stay out of the portable core.
- Cambium's `GraphCanvasSwatch` consumes small projection scenes rather than growing into the
  engine (it is currently the thing isometry and woodshed hand-feed).

One standing ruling to reconcile on the record: the data-oriented doctrine brief rules "no
shared core; the doctrine is the unit of reuse." The scenograph family threads it the way
netrender already does: what is shared is the **output** side (a scene contract), while every
product keeps native truth behind an adapter. The family already accepts a shared scene
contract at the paint level (everyone emits netrender scenes); sceno adds one at the placement
level. A shared scene is not a shared data model.

The extraction risk is freezing the wrong contract, and the mitigation is §9's order: the
contract lands as types with mere as first consumer and isometry as second **before** any
crates.io publication beyond name-holding 0.0.1s.

## 7. What each product forces the engine to learn

- **Isometry, the second implementation.** It already duplicates deterministic force layout in
  `Overmap::layout` and converts to a cambium swatch. It exercises authored coordinates,
  visibility scope, polygons, grids, adjacency, props, and nested tactical maps.
- **Hocket: time as a coordinate space.** Tracks become lanes, phrases media-backed regions,
  layers overlap; the retained history graph becomes another projection. One phrase appears in
  the loop table, history branch, similarity map, and handoff view without duplicating truth.
  **Gate**: hocket's own doctrine names the arrange view as its scope-creep canary, and a
  timeline projection of phrases is exactly arrange-view-shaped. Hocket contributes
  time-as-coordinate-space as a *design requirement* on sceno; building hocket's timeline
  projection is gated by hocket's cap rule, not by this engine's roadmap. ("Score" also
  collides softly with hocket's musical domain; qualify it there when the day comes.)
- **Woodshed: instrument coordinate systems and paths.** A fretboard is a mapping from
  (string, fret) to screen coordinates; notes are projected marks, fingerings paths; the
  related-material swatch currently hand-builds its radial placement.
- **Retinue: streaming, uncertain, capability-controlled facts.** A management snapshot exposes
  destinations, interfaces, routes, links, radios, battery, signal quality, GNSS position,
  accuracy, observation time, trust. Gestures emit commands (connect, ping, transfer,
  configure) that retinue authorizes and performs. Projection results stay local; compact facts
  and projection settings are what travel over constrained links.

## 8. Burn's place, and the signal vocabulary

Burn is a first-class **signal producer**, kept off the rendering path. The common vocabulary
(extending the existing `IntelligenceSignals` contract):

- per-item scalar values; vector embeddings; pairwise affinity; clusters and memberships;
  classifications; suggested relationships or layouts
- each stamped with model id, generation, confidence, provenance, observation time

The runtime (`projection-runtime`) caches and invalidates by generation. Projection settings
map signals into position, size, grouping, opacity, LOD, or relation strength. A learned
suggestion becomes durable truth only through an explicit accepted action (§3.2 rule; no
placebo structure). The whole engine runs with signals absent: ML is structural, not
decorative, and never load-bearing for correctness.

## 9. Sequencing: five proofs

The earlier P0 wiring probe survives as a smoke test, with its decisiveness demoted: the chat
examples already establish that footprints are needed, and `project_canvas_strategy` returns
positions only, so wiring that path alone would reinforce the old boundary.

1. **Wire the existing analytic catalog through merecat** (the smoke test: one strategy exposed
   as a setting/action, projection applied, headed receipt).
2. **Introduce the portable scene contract** with point, rectangle, and polygon footprints
   (instance ids, coordinate spaces, representation slots per §5), consumed by mere.
3. **Browser nodes as pane slots in a configurable phyllotaxis spiral.** Recency controls scale
   and LOD; focused content stays live; small items degrade to snapshots, cards, glyphs. This
   is the "representation measures, projection places" proof.
4. **Isometry consumes the same contract** for its overmap and one tile-board projection,
   deleting its hand-rolled force layout.
5. **A fixture-driven geographic projection** (facts from fixtures; map/image underlay), then
   live retinue/tulle/sennet location facts when they exist.

**Done means**: the same serialized projection settings drive a mere pane spiral and an
isometry map, with neither portable crate depending on either product. At that point "one
projection engine" is an implemented boundary rather than an expanded graph canvas.

Continuous: hull authoring (§5's gesture, on the shipped field-region machinery) can proceed
independently and converge when frames enter the scene contract; burn rides existing seams.

## Open questions

- **Projection-graph representation.** A materialized structure per view, or a query/lens over
  sources evaluated by the runtime? (Determines caching granularity.)
- **Settings schema.** The serialized projection settings of §9's done condition: engram schema,
  versioning, and how much of `ViewIntent` survives into it.
- **Channel set boundary.** Is opacity/emphasis a placement channel or representation concern?
  Lean: representation, but recency-shrink sits near the line.
- **Lens wiring.** `ProjectionLens` is vocabulary without a consumer at `visible_walk`; does it
  bite at the projection-graph (selection/derivation) stage instead?
- **Frame transforms beyond translation.** Does rotating/scaling a group transform member
  offsets, or only translation carries? Likely per-group setting; pick a default.
- **Recognizer standing.** Does the spatial-hypertext recognizer become a signal producer
  (suggested relationships from placement), stamped like any §8 signal?
- **Lens regions.** Pad++-style viewport lenses: design-space slot or distraction until
  N-projection composition works?

## Sources

Prior art (web, verified 2026-07-21):

- Vega-Lite: [A Grammar of Interactive Graphics](https://idl.cs.washington.edu/files/2017-VegaLite-InfoVis.pdf) (Satyanarayan, Moritz, Wongsuphasawat, Heer; InfoVis 2016); [encoding channels](https://vega.github.io/vega-lite/docs/encoding.html)
- MPS projectional editing: [How Does MPS Work](https://www.jetbrains.com/mps/concepts/); [basic notions](https://www.jetbrains.com/help/mps/basic-notions.html); [Supporting Diverse Notations in MPS' Projectional Editor](https://mbeddr.com/files/gemoc2014-MPSNotations.pdf)
- Glamorous Toolkit / moldable development: [gtoolkit.com](https://gtoolkit.com/); [book.gtoolkit.com basics](https://book.gtoolkit.com/learn-the-basics-of-glamorous-toolkit-5hetr3qaqcfv42xap3v4j39o2)
- Pad++ / ZUI: [Pad++: A Zoomable Graphical Interface System](https://www.cs.umd.edu/~bederson/images/pubs_pdfs/p23-bederson.pdf) (Bederson & Hollan); [Zooming user interface](https://en.wikipedia.org/wiki/Zooming_user_interface)
- deck.gl: [deck.gl docs](https://deck.gl/docs); [views](https://deck.gl/docs/developer-guide/views); [Deck.gl: Large-scale Web-based Visual Analytics Made Easy](https://ar5iv.labs.arxiv.org/html/1910.08865)
- Spatial hypertext: [Finding and Using Implicit Structure in Human-Organized Spatial Layouts of Information](https://dl.acm.org/doi/fullHtml/10.1145/223904.223949) (Shipman & Marshall); [Seven Directions for Spatial Hypertext Research](https://people.engr.tamu.edu/shipman/SpatialHypertext/SH1/shipman.pdf)
- 3D effectiveness: [Ware & Franck 1996](https://doi.org/10.1145/234972.234975); [Cockburn & McKenzie on spatial memory in 2D/2.5D/3D](https://ir.canterbury.ac.nz/items/6b4acfe3-7fdb-4d43-83af-639ab1972403); [Beyond the classical monoscopic 3D in graph analytics](https://www.researchgate.net/publication/280852653_Beyond_the_classical_monoscopic_3D_in_graph_analytics_An_experimental_study_of_the_impact_of_stereoscopy); [Immersive Analytics: Time to Reconsider the Value of 3D](https://www.researchgate.net/publication/328283405_Immersive_Analytics_Time_to_Reconsider_the_Value_of_3D_for_Information_Visualisation); [AR graph study](https://www.frontiersin.org/journals/virtual-reality/articles/10.3389/frvir.2023.1155628/full)

Code grounding (read 2026-07-21): `crates/canvas/arrangements/src/{lib,registry}.rs` (Layout
trait, LayoutExtras, LayoutCapability, supports_3d), `crates/canvas/cartography/src/strategy.rs`
(LayoutStrategy), `crates/canvas/cartography/src/request.rs` (ProjectionRequest's kernel::Graph
requirement; ViewIntent.dimension), `crates/canvas/cartography/src/projection.rs`
(PositionedNode point+radius ceiling), `crates/canvas/canvas/src/underlay.rs`
(projection_from_positions, identity_arrangement), `crates/canvas/canvas/src/cartography_scene.rs`
(project_canvas_strategy positions-only; no host caller), `crates/canvas/canvas/src/fields.rs`
(field-region interaction), `crates/canvas/canvas/src/sprite_hull.rs`,
`crates/system/session-runtime/src/arrangement_facets.rs` (arrangement.position +
ARRANGEMENT_SPRITE_HULL), `crates/forme/forme/src/topology.rs` (visible_walk ignoring
ProjectionLens), `crates/forme/forme/src/tree/layout.rs` (SplitPanes), numen
`field.rs`/`coupling.rs`, `merecat/src/shell.rs` (per-surface rasterize + ordered composite),
`isometry-core/src/overmap.rs:123` (hand-rolled deterministic force layout),
`woodshed-views/src/stage.rs:48` (hand-built radial GraphCanvasSwatch).

## Progress

- 2026-07-21: Created from a chat probe (projection engine framing; prior art; 3D). Six-lane
  prior-art sweep; first channel-model sketch; first sequencing.
- 2026-07-21 (same session): folded in Mark's hulls-as-fields follow-up.
- 2026-07-21 (same session): **first adversarial review applied** (all claims code-verified):
  corrected the §1 conclusion to "beside genet over a shared scene/raster/compositor substrate";
  pipeline described as separate lanes, not one live chain; channel work retargeted from
  `Layout::step()` to the `Projection` boundary; provenance split into source facts /
  constraints / derived geometry; stale S-lane sequencing dropped (S2 landed 2026-07-19); 3D
  recast task-and-display-gated; novelty softened; P0 recast as a wiring probe.
- 2026-07-21 (same session): **second critique applied** (claims verified: `ProjectionRequest`
  requires `kernel::Graph`; `project_canvas_strategy` returns positions only;
  `Overmap::layout` at overmap.rs:123 duplicates force layout, critique cited :85;
  woodshed stage.rs hand-builds radial placement; `ViewIntent.dimension` found in the same
  pass). Adopted the compiler/runtime destination and the §1 pipeline; the three-structure
  model (source / projection graph / scene) with source-vs-instance separation and the
  projected-item contract; "representation measures, projection places"; softened the
  `TreeGeometry` separation to shared-contract/separate-solvers; **corrected the frames
  over-unification** into five primitives (frame / group / region / field / pin) composed by
  the hull gesture; the sibling-workspace direction with cartography becoming mere's graph
  adapter; the four product forcing functions; burn as a stamped signal producer off the
  rendering path; resequenced to the five proofs with the serialized-settings done condition.
- 2026-07-21 (same session): **judgment pass + naming ruled.** Identity assessment recorded in
  chat: the destination changes emphasis, not kind (the walk from "browser with a graph view"
  to "graph-native workspace that browses" was already underway via the one-node ruling and
  pane taxonomy); the engine must stay extraction-from-product (pressure-vessel posture), and
  the "projection compiler" vocabulary stays internal, "merely a browser" stays the product
  voice. Added to the brief: the §1 stage vocabulary (**score → choreography → inhabited
  scene**) and condensed why-compiler/runtime rationale ("xilem for space"); the §6 **scenograph
  family naming** (family repo `scenograph`; `sceno` core / `scenomise` choreography /
  `scenotime` runtime / `scenograph` facade; all names verified free on crates.io; graphshell
  parked as the future shell-layer name, the pelt analog, never the engine umbrella); the §6
  no-shared-core reconciliation (shared output contract, netrender-pattern, not a shared data
  model); the §7 hocket arrange-view canary gate. Naming candidates considered and dropped:
  pantograph + alidade (taken), planisphere/periplus/camera-lucida (free but weaker fit),
  scenemise/scenetime (superseded by the o-stem forms), sceno/topos/scenograph tri-name under a
  graphshell umbrella (fragment + topos-theory baggage + donor-name ambiguity).
- 2026-07-21 (same session): **scenograph founded; proof 1 executed.** Family repo pushed
  ([mark-ik/scenograph](https://github.com/mark-ik/scenograph) `5a730e1`; crates.io publication
  = Mark's step). Proof 1 landed in merecat (palette action → canvas strategy seam →
  recompute-gated host loop → headed receipt, `RESULT ok`); the spiral packs 15 fixed-extent
  nodes into overlap, making the footprint channel an **empirical finding, not a prediction**.
  Execution + findings tracked in the
  [projection_proofs_plan](../implementation_strategy/2026-07-21_projection_proofs_plan.md);
  this brief stays the direction record.
