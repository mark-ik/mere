# Projection Engine Prior Art: mere's framing beside genet, and the 3D question

**Date**: 2026-07-21
**Status**: Research brief (with Mark). Answers three questions from a chat probe: is mere's
direction "a projection engine alongside genet's web rendering engine"; what is the prior art;
does the framing point to 3D after initial composition. Ends with a proposed sequencing; no code
changed this session.
**Code touched (read-only survey)**: `crates/canvas/arrangements` (Layout trait, registry,
catalog), `crates/canvas/canvas` (underlay, identity arrangement), `crates/canvas/cartography`
(Projection), `crates/forme` (SplitPanes tree layout).

**Related**:

- [graph_projections_research](2026-06-22_graph_projections_research.md) pins the vocabulary this
  brief reuses: an **arrangement** positions current nodes, a **lens** picks the load-bearing
  topology, a **projection** is a whole surface reading the graph a particular way. This brief
  works one level below all three: the engine that realizes them.
- [modular_integration_plan §1](../implementation_strategy/2026-06-02_modular_integration_plan.md)
  is the graph-rooted projection model (graph is the sole root; every surface is a contingent
  projection). This brief names that model's engine and checks it against outside precedent.
- [data_oriented_doctrine_brief](../../2026-07-02_data_oriented_doctrine_brief.md): the
  stack-wide representational discipline. The channel model in §5 is that doctrine applied to the
  scene boundary.
- [one_node_facets_layer_map](../technical_architecture/2026-07-18_one_node_facets_layer_map.md)
  and [node_dissolution_facets_plan](../implementation_strategy/2026-07-18_node_dissolution_facets_plan.md):
  the position-provenance facet proposed in §5 rides the S lane (spatial completion, geometry
  sidecar) rather than adding a new store.
- [burn_utilization_brief](2026-07-04_burn_utilization_brief.md): the learned-mapping lane in §5
  is a consumer of the approved burn direction, not a new lane.
- [node_representation_arrangement_plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md):
  representation (a node's look) stays orthogonal; channels decide where, how large, and in what
  form slot, never node identity styling (which stays NODE_SHEET-derived).

---

## 1. The framing question

"A projection engine alongside genet's web rendering engine" is right about the pairing and wrong
about the preposition. Mere does not render beside genet; genet renders everything (host chrome
and web content alike, per the xilem_serval host direction). Mere's distinctive engine sits
**above** genet: it decides what portion of graph truth becomes a scene, where each member lands,
at what size, in which form, and genet realizes the result. The pipeline, in current crate terms:

```
graph truth (chartulary containers + facets, kernel graph)
  → forme::Arrangement            (which members; identity or curated)
  → forme::ProjectionLens         (which topology is load-bearing)
  → arrangements::Layout          (where members go; today: 2D position deltas only)
  → cartography::Projection       (the positioned scene snapshot)
  → canvas underlay → paint list  (host-neutral PaintCmds)
  → genet                         (realizes pixels; web content is one lane inside members)
```

Every stage of that pipeline exists and is host-neutral. So "mere is becoming a projection
engine" is not a proposal; it is a description of what is already built, minus one narrowing:
the Layout stage emits exactly one channel (2D position), which is why everything still reads as
"dots with different edges" even though the catalog behind it is much richer (Phyllotaxis with
radius curves, Penrose tilings, L-systems, Timeline/Kanban over host axes, SemanticEmbedding,
a URN-keyed registry with a WASM mod lane).

The Navigator rule is untouched by this framing: projections are surfaces above the engine; the
engine does not multiply them.

## 2. What exists today (code-verified 2026-07-21)

- **Layout contract**: `Layout<N>::step()` returns `HashMap<N, Vector2D<f32>>`
  (`crates/canvas/arrangements/src/lib.rs`). Position-only and 2D-only are baked in.
  `LayoutExtras` already carries the seams a richer engine wants: `pinned` (persistent
  do-not-move intent), `dragging`, `embedding_by_node` (host-supplied 2D coordinates from any ML
  pipeline), `axis_value_by_node` (Numeric or Categorical per-node axis inputs), and
  `domain_by_node`.
- **Registry**: URN layout ids, `LayoutCapability` metadata including `is_topology_sensitive` and
  a reserved `supports_3d` flag (all built-ins 2D today), `LayoutProvenance::{Builtin, NativeMod,
  WasmMod}`.
- **Positions are not graph truth** (the S2 decision): the underlay projects through a caller
  position lookup with committed-position fallback; seiche feeds live motion. The dissolution
  plan's S lane finishes this (geometry sidecar; `Node.position/velocity` retire).
- **One arrangement, two realizations**: the canvas and the tiled workbench are two projections
  of one forme `Arrangement` (`canvas/src/underlay.rs`, `identity_arrangement`). Forme's
  `SplitPanes` mode already lays visible members out as taffy tiles.
- **Nothing geographic anywhere**: no lat/lon, no map projection, no GNSS type in mere; sennet
  does not surface position data yet.

## 3. Prior art, six lanes

Each lane is real precedent for one slice of the framing, and each carries one lesson mere
should adopt (or has already adopted without naming it).

### 3.1 Grammar of graphics (Wilkinson; Vega-Lite / Vega)

The channel vocabulary itself. Vega-Lite specifications are mappings from data fields to **visual
encoding channels** (x, y, color, size, shape); a compiler fills in scales, axes, and legends by
rule, and emits a full scenegraph. Fifty years of statistical graphics collapse into "data →
channels → marks."

**Lesson**: name the channel set explicitly and make the mapping a declarative, serializable
value, not code. A serializable mapping is exactly an engram, which makes arrangements shareable
through the pack lane for free. Also the compiler posture: sensible defaults derived by rule, so
a mapping spec stays small.

### 3.2 Projectional editing (JetBrains MPS)

The strongest precedent for "many notations of one truth." MPS has no parser; the AST is the only
truth, projection rules render it as text, tables, math, or diagrams, and **editing happens
through the projection back onto the AST**. MPS 3.0 added switchable alternative projections per
concept.

**Lesson**: projections must be writable, not just readable. Dragging a gnode, renaming a
cluster, accepting a suggested edge: each is an edit through the projection that lands as a typed
kernel assertion (the 2026-06-22 doc's write-back contract, independently confirmed as the thing
that makes projectional systems livable).

### 3.3 Moldable development (Glamorous Toolkit)

GT treats views as cheap, per-object, context-activated artifacts: any object defines its own
inspector views, and a working system accumulates thousands of tiny custom tools. Built on
Pharo with a significant Rust part; the whole IDE is projections over live objects.

**Lesson**: per-content-class views must be cheap to define, or nobody defines them. This lands
directly on the facet ruling: a content class is a facet bundle plus schema engram, and its
default representations and preferred channels belong in that same bundle. The Apparatus
retargeting-with-selection behavior is GT's inspector pattern under another name.

### 3.4 Zoomable UIs (Pad++, Piccolo)

Pad++ made zooming the fundamental navigation and named **semantic zooming**: objects change
representation with scale. The lineage (Pad++ → Jazz → Piccolo → Piccolo2D) worked out the
scene-graph and camera machinery for infinite canvases three decades ago.

**Lesson**: mostly confirmation. LOD levels are semantic zoom; the infinite canvas defaults doc
is Pad++'s navigation posture. The one idea worth stealing: Pad++ lenses let a *region of the
viewport* impose a different projection on whatever passes under it, which is a cheaper
per-region variant of "switch the whole surface's arrangement."

### 3.5 deck.gl (over MapLibre/Mapbox)

The closest industrial analog of "projection engine alongside rendering engine." deck.gl is a
data-driven layer stack (data arrays → per-layer accessors → GPU attributes → shaders) composited
**over a base map renderer it does not own**, with pluggable views (map, orthographic,
first-person) and Mercator projection evaluated in-shader. It proves the pairing scales to
millions of features, and that the projection side and the base-content side can be separate
engines with a thin compositing contract.

**Lesson**: the geographic lane is an adapter, not an engine. Lat/lon through a fixed projection
function into world coordinates is a thin transform (deck.gl does it in a shader); the base map
is an underlay layer beneath the graph scene, exactly where the canvas underlay already sits.
GNSS-positioned radios are data-carried positions plus `pinned`; unpositioned peers arrange
around them.

### 3.6 Spatial hypertext (Aquanet → VIKI → VKB, Shipman & Marshall)

The inverse lane, and the deepest cut. Given a spatial canvas, users **stopped making explicit
links** and expressed structure through placement: piles, lists, adjacency. VIKI/VKB then parsed
the arrangement to recognize implicit structure and offered it back as suggested explicit
structure. VKB also kept a navigable history scrubber over the space's evolution.

**Lesson**: projection runs both directions. "Tiles form a map and edges are adjacency" is not
only a layout (edges → tile borders); it is also a recognizer (user placement → suggested
edges). The write-back contract already covers the assertion path; the recognizer is a distinct,
optional producer that makes hand-arrangement first-class instead of decorative. This is the
prior art that says the manual-arrangement lane deserves equal standing with computed layouts.

### What has no clean precedent

No shipped system pairs a *general* projection engine with a *full web engine* where web
documents are ordinary graph members among other content classes. GT is the nearest in spirit
(own renderer, everything is a projection) but has no web lane; deck.gl is the nearest in
architecture but is domain-fixed to geospatial layers. The combination is the novel claim, and
the six lanes above cover every component of it individually, which is about as de-risked as
novel gets.

## 4. The 3D question

Short answer: the framing points at 3D-capable, not 3D-first, and the literature says to keep it
that way.

What the research record supports:

- Ware & Franck (1996) and successors: 3D node-link viewing **with stereo and motion depth
  cues** substantially improves low-level path-tracing tasks (up to ~3x graph-size comprehension
  at equal error). The gain is real and repeatable.
- The same literature's caveat: **monoscopic 3D on a flat screen** (the case that actually ships
  on a laptop) mostly underperforms 2D for reading tasks. Occlusion, viewpoint dependence, and
  harder pointing eat the projection gains, and 3D does not improve spatial memory for
  arrangement recall, which is the thing mere's spatial canvas trades on.
- Immersive analytics (Marriott et al., and the AR/VR study wave since) reopened the question
  for headsets: immersion restores the depth cues that made 3D win in the lab. Active research
  frontier, not settled shipping practice.

The ruling this suggests for mere:

1. **Graph truth stays dimension-neutral.** Truth carries no dimensionality today (positions are
   sidecar, not graph truth) and that should be preserved through the dissolution S lane.
2. **The channel design should be dimension-parametric on paper, 2D in implementation.** The
   Layout trait bakes `Vector2D`; when the channel redesign (§5) touches that signature anyway,
   choosing a representation that admits a z component later costs a design decision now and a
   migration never. `supports_3d` is already reserved in `LayoutCapability` for exactly this.
3. **3D earns its place only where the third axis is data**: terrain under a geographic
   projection, elevation of a radio fix, tabletop height, an eventual headset lane. Layout-invented
   z (force-directed 3D scatter) is the case the literature warns about; nothing should build it
   speculatively. This is the position-provenance distinction doing double duty: data-carried 3D
   positions justify a 3D scene target, invented z does not.

So: 3D post initial composition is plausible and cheap to keep open, and the stack beneath is
already capable (wgpu throughout; rapier has a 3D half; genet's WebGPU lane). It is not a reason
to complicate the first channel redesign beyond keeping the signature honest.

## 5. The channel model (the design this points at)

One design move makes every chat example (adjacency tile maps, pane spirals that shrink old
content, symmetric split growth, geographic and tabletop scenes) a catalog entry instead of a
special surface:

**Promote the layout output from a position delta to a small set of scene channels.**

- `position` (today's only channel)
- `extent` (a region: rect now, polygon later; what tile maps and pane layouts emit)
- `scale` (what recency-shrink and importance-grow emit)
- `z` / depth (reserved; §4 posture)

With two supporting decisions:

- **Position provenance as a facet** on the geometry sidecar: `layout-computed` |
  `data-carried` (GPS fix, map coordinate, timestamp) | `user-placed` (pin). Pinning is the
  mechanism; provenance is the truth. Data-carried and user-placed positions are inputs the
  layout must respect; layout-computed are its outputs. The spatial-hypertext recognizer lane
  (§3.6) reads user-placed regions; the geographic adapter (§3.5) writes data-carried ones.
- **The mapping is a value** (§3.1): which channels a projection uses, from which facets and
  signals, serialized as an engram so arrangements travel as packs.

This is deliberately a brief-level sketch. The trait surgery (does `step()` return a channel
struct; do extents flow through `CanvasSceneInput`; what do the eight existing layouts emit for
extent) is a plan's job, and per DOC_POLICY §8 that plan gets written when code work starts.

## 6. Proposed sequencing

Probe before design; the catalog's breadth is unexercised and the redesign should be shaped by
observed limits, not predicted ones.

- **P0, composition probe (next session-sized step)**: drive what exists, unmodified, in
  merecat: Phyllotaxis arrangement + card representations + a static image underlay standing in
  for a map/tabletop. Record which of the chat examples fail on missing channels versus missing
  wiring. This is the implementation-feedback-loop rule applied before the design instead of
  after.
- **P1, channel redesign plan**: the §5 trait surgery as a dated implementation_strategy plan,
  informed by P0. Includes the dimension-parametric signature decision (§4.2).
- **P2, position provenance**: the facet + sidecar slot, sequenced with (not beside) the
  dissolution plan's S lane so spatial completion lands once.
- **P3, geographic adapter**: lat/lon projection function + map underlay + pinned data-carried
  fixes. Gated on a real position producer (sennet/tucket GNSS surfacing), which does not exist
  yet; build the adapter when the first producer lands, deck.gl-style thin.
- **P4, adjacency tiling**: the rectangular-dual / adjacency-preserving treemap layout, first
  consumer of the `extent` channel. Hardest layout math in the set; last for that reason.
- **Continuous**: the burn lane needs no new seam. `embedding_by_node` already accepts any
  learned 2D mapping; learned channel mappings and pin-correction training signal ride the
  burn_utilization_brief's approved lanes.

## Open questions

- **Channel set boundary.** Is `opacity`/emphasis a layout channel or purely a representation
  concern? Lean: representation (NODE_SHEET identity stays orthogonal), but the recency-shrink
  example sits near the line.
- **Extent authority.** When a layout emits extents and the workbench's taffy split also computes
  rects, which wins where? (Likely: layouts own canvas-space extents, taffy owns docked-pane
  space, and the two never meet, but P0 should confirm.)
- **Recognizer standing.** Does the spatial-hypertext recognizer (§3.6) become a signals-layer
  producer (suggested edges from placement), and if so what confidence/provenance does it write?
- **Lens regions.** Are Pad++-style viewport lenses (a region imposing a different projection)
  worth a slot in the design space table, or a distraction until N-projection composition works?

## Sources

Prior art (web, verified 2026-07-21):

- Vega-Lite: [A Grammar of Interactive Graphics](https://idl.cs.washington.edu/files/2017-VegaLite-InfoVis.pdf) (Satyanarayan, Moritz, Wongsuphasawat, Heer; InfoVis 2016); [vega.github.io/vega-lite](https://vega.github.io/vega-lite/)
- MPS projectional editing: [How Does MPS Work](https://www.jetbrains.com/mps/concepts/); [Supporting Diverse Notations in MPS' Projectional Editor](https://mbeddr.com/files/gemoc2014-MPSNotations.pdf)
- Glamorous Toolkit / moldable development: [gtoolkit.com](https://gtoolkit.com/); [book.gtoolkit.com basics](https://book.gtoolkit.com/learn-the-basics-of-glamorous-toolkit-5hetr3qaqcfv42xap3v4j39o2)
- Pad++ / ZUI: [Pad++: A Zoomable Graphical Interface System](https://www.cs.umd.edu/~bederson/images/pubs_pdfs/p23-bederson.pdf) (Bederson & Hollan); [Zooming user interface](https://en.wikipedia.org/wiki/Zooming_user_interface)
- deck.gl: [deck.gl docs](https://deck.gl/docs); [Deck.gl: Large-scale Web-based Visual Analytics Made Easy](https://ar5iv.labs.arxiv.org/html/1910.08865)
- Spatial hypertext: [Finding and Using Implicit Structure in Human-Organized Spatial Layouts of Information](https://dl.acm.org/doi/fullHtml/10.1145/223904.223949) (Shipman & Marshall); [Seven Directions for Spatial Hypertext Research](https://people.engr.tamu.edu/shipman/SpatialHypertext/SH1/shipman.pdf)
- 3D effectiveness: Ware & Franck 1996 via [Beyond the classical monoscopic 3D in graph analytics](https://www.researchgate.net/publication/280852653_Beyond_the_classical_monoscopic_3D_in_graph_analytics_An_experimental_study_of_the_impact_of_stereoscopy); [Immersive Analytics: Time to Reconsider the Value of 3D](https://www.researchgate.net/publication/328283405_Immersive_Analytics_Time_to_Reconsider_the_Value_of_3D_for_Information_Visualisation); [Transforming graph data visualisations from 2D displays into AR 3D space](https://www.frontiersin.org/journals/virtual-reality/articles/10.3389/frvir.2023.1155628/full)

Code grounding (read 2026-07-21): `crates/canvas/arrangements/src/lib.rs` (Layout trait,
LayoutExtras, catalog exports), `crates/canvas/arrangements/src/registry.rs` (LayoutCapability,
supports_3d, provenance), `crates/canvas/canvas/src/underlay.rs` (projection_from_positions,
identity_arrangement), `crates/forme/forme/src/tree/layout.rs` (SplitPanes).

## Progress

- 2026-07-21: Created from a chat probe (Mark: "is the direction a projection engine alongside
  genet; prior art; does it point to 3D?"). Surveyed the arrangements/canvas/cartography/forme
  code, ran the six-lane prior-art sweep, took the 3D literature position, sketched the channel
  model, and proposed P0-P4 sequencing with the composition probe first. No code changed.
