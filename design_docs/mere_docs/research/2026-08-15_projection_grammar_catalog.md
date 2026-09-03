# Mere projection grammar catalog

Status: governing research catalog and boundary map
Founded: 2026-08-15
Last reconciled: 2026-08-29
Scope: portable graph readings, visual encodings, arrangements, realization, and interaction  
Governs: the primitive vocabulary and promotion rules used to construct scenes  
Consumed by: [`2026-08-18_scenograph_content_catalog.md`](2026-08-18_scenograph_content_catalog.md), the collection of complete scene recipes  
Incorporates: the projection-engine, graph-projections, Scenograph expansion, node-representation, and field-region research listed under [Related Mere research](#related-mere-research)

**2026-08-23 follow-on:** [Projection Scenes and the Graph-Native Application Platform](../../2026-08-23_projection_scenes_and_graph_native_platform.md)
adds a layer-ownership pre-filter before the scene test, separates categorical
status from contract evidence, records the Matrix and repeated-instance
rulings, and derives consequences for Mere, Scenograph, Cambium, and Genet.
This catalog remains the grammar and boundary map; the follow-on owns the
revised scene judgments.

## Ruling

Mere should not define a graph as a node-link picture.

A graph is an addressable set of entities, relations, fields, and nested spaces. A projection chooses what to read from that authority and how to make it legible. A node can become a card, bar, matrix cell, map symbol, text row, image, nested canvas, or invisible grouping boundary. A relation can become a stroke, ribbon, matrix cell, adjacency, containment, alignment, order, or shared visual channel. Derived marks such as axes and aggregate bars can exist in the projection without pretending to be authoritative graph entities.

This is why an Excel-style chart, a Mermaid-style diagram, and Mere's current spatial graph can belong to the same system. They do not need a common appearance. They need a common account of:

1. what source facts they read;
2. what projected objects they derive;
3. which encodings and arrangements they apply;
4. how a projected object maps back to source authority;
5. which interactions are permitted;
6. how the result is realized, including a static realization.

The catalog is therefore a grammar of projection capabilities. Scenograph scenes are compositions of this grammar; the content catalog collects and names those complete recipes. A scene that needs a missing primitive must first supply the forcing proof for that primitive. The dependency runs one way:

```text
projection grammar -> scene recipe -> product adapter and authority
```

`sceno` carries the grammar and scene result. It does not import the scene catalog or know product scene names.

## The projection stack

```text
authority -> reading -> encoding -> arrangement -> scene -> realization
                |                                      |
                +---------- intents and provenance ----+
```

| Layer | Owns | Does not own |
| --- | --- | --- |
| Authority | Stable source identity, entities, relations, fields, values, mutations | Screen geometry |
| Reading | Selection, traversal, grouping, aggregation, derivation, ordering, faceting | Persistent source meaning |
| Encoding | Marks, visual channels, guides, relation forms, representation choices | Layout solving |
| Arrangement | Positions, sizes, paths, routing constraints, nesting geometry | Product commands |
| Scene | Portable projected instances, spaces, transforms, footprints, routes, regions, provenance | Renderer resources or source mutation |
| Realization | DOM, SVG, canvas, GPU, terminal, image, print, accessibility tree | Graph authority |
| Intent | Select, navigate, filter, edit, annotate, reproject, direct manipulation | Silent scene mutation presented as source truth |

Interaction is orthogonal to realization. The same scene may be interactive in Mere or rendered as a static SVG, PNG, PDF, or accessible table. In Graphshell, `FrozenScene` specifically names a noninteractive realization which can be replaced by an interactive realization from the same scene. It does not describe protocol stability or transport.

For movable spatial projections, the useful placement policies are:

- **Free:** the solver supplies a stable initial arrangement; direct manipulation may establish a new position.
- **Anchored:** the solver supplies a home position; displacement is temporary and the item returns toward home.
- **Pinned:** an explicit constraint fixes an item until the pin is removed. Pinning is independent of free versus anchored behavior.

## Portable graph vocabulary

The following concepts are broad enough to support spatial graphs, charts, diagrams, maps, and documents without importing product meaning into Scenograph.

### Authority elements

- **Entity:** an addressable source object. Mere commonly presents it as a node.
- **Relation:** an addressable or derived association among source objects. Multiple reasons remain multiple relations even when a renderer visually bundles them.
- **Field:** a value or influence defined globally, over a region, on a polygon, or relative to an entity. Numen already establishes fields as a graph element beside nodes and edges.
- **Space:** a nested coordinate or containment domain.
- **Value:** quantitative, ordinal, nominal, temporal, geographic, textual, media, structural, uncertain, or missing data read from authority.

### Projected elements

- **Instance:** a source-backed appearance. One source object may have multiple projected instances.
- **Derived mark:** a projected object computed from one or more source facts, such as a histogram bin, aggregate bar, regression line, hull, axis, or legend entry.
- **Guide:** an explanation of an encoding, including axes, ticks, grids, legends, labels, titles, reference lines, and annotations.
- **Region:** a projected area used for grouping, navigation, hit testing, or explanation.
- **Backdrop:** non-authoritative scene context such as a basemap, page, board, image, grid, zone, or field raster.
- **Intent target:** the source identity, reading parameter, or projection control affected by interaction.

The distinction between instances and derived marks matters. A bar representing the count of twenty nodes is real and selectable in the scene, but it is not automatically a twenty-first source node.

## Capability grammar

### 1. Readings and operators

A reading determines which graph facts are exposed to a projection and which values are derived.

| Family | Operators | Typical proofs |
| --- | --- | --- |
| Selection | select, filter, search, sample, top-k | Spotlight, query results, focus plus context |
| Ordering | sort, rank, stable order, domain order | Table, bar chart, lanes, timeline |
| Grouping | group, partition, nest, cluster, facet | Small multiples, swimlanes, treemap |
| Quantitative derivation | aggregate, count, sum, average, extent, normalize, stack | Bar, histogram, stacked area, Sankey |
| Binning and windows | bin, rolling window, cumulative value, lag, lead | Histogram, sparkline, activity trail |
| Relational traversal | neighborhood, path, component, reachability, degree | Orrery, ego graph, route view |
| Hierarchy | parent, descendants, depth, leaves, aggregate subtree | Tree, treemap, sunburst, pack |
| Temporal | snapshot, diff, interval, duration, trail, freshness | Chronicle, Gantt, provenance trail |
| Geographic | coordinates, projection, bounds, distance, region membership | Atlas, route map, choropleth |
| Set and membership | intersection, overlap, inclusion, exclusion | Venn-like views, UpSet-style matrix |
| Statistical | distribution, quantile, correlation, regression, uncertainty | Box plot, scatterplot, error bands |
| Provenance | source, derivation, confidence, freshness, author | Audit view, evidence trail |
| Repetition | facet, repeat, layer, concatenate | Dashboard, comparison grid, small multiples |

These are reading operations, not necessarily persistent graph mutations. A host may materialize a result when product semantics require it.

### 2. Marks

Marks are the drawable forms used by an encoding. Mere's current `sceno::Representation` covers glyphs, cards, sprites, snapshots, live panes, and an open representation. A general chart and diagram grammar also needs derived marks that are not forced into node representations.

| Mark | Common uses | Source mapping |
| --- | --- | --- |
| Symbol or glyph | Network node, map point, scatter point | One entity or derived sample |
| Text | Label, row, annotation, document fragment | Entity, value, or derived explanation |
| Rectangle or cell | Bar, heatmap cell, matrix cell, treemap tile | Entity, relation, bin, or aggregate |
| Line, rule, or path | Trend, reference, route, connector | Relation, series, guide, or derived path |
| Area | Distribution, interval, stacked quantity, region | Series, field, uncertainty, or group |
| Arc or sector | Polar chart, sunburst, gauge, chord endpoint | Entity, group, or quantitative value |
| Ribbon | Flow, bundled relation, chord | Relation set or aggregate flow |
| Image or media | Thumbnail, sprite, geographic raster | Entity, backdrop, or field |
| Snapshot | Frozen web surface or document preview | Source-backed instance |
| Live pane | Interactive web surface or nested application | Source-backed instance |
| Group or nested canvas | Compound node, facet, diagram frame | Space, group, or derived partition |
| Custom/open mark | Domain-specific renderer contribution | Explicit source or derivation provenance |

### 3. Visual channels

An encoding maps values to channels. Effectiveness depends on the task and data type, so channels should remain explicit rather than buried inside named scene code.

- Position: x, y, z, polar angle, radius, geographic coordinate.
- Size: width, height, area, length, radius, stroke width.
- Color: hue, luminance, saturation, opacity.
- Form: shape, icon, texture, line style, corner treatment.
- Order: rank, sequence, layer, depth, draw order.
- Motion: velocity, direction, pulse, transition, trail.
- Repetition: facet, panel, small multiple, repeated glyph.
- Spatial relation: containment, adjacency, overlap, alignment, distance.
- Media state: still image, live surface, waveform, playback state.

Channels need declared domains, ranges, missing-value behavior, and legends or descriptions when their meaning is not self-evident.

### 4. Guides and backdrops

Charts expose a gap in the older catalog: scales and guides are first-class projection material.

- **Scale:** maps a data domain to a visual range. Scale types include continuous, logarithmic, temporal, ordinal, band, threshold, geographic, and custom mappings.
- **Axis:** explains a positional scale through ticks, values, labels, and title.
- **Legend:** explains color, size, shape, texture, or line encodings.
- **Grid or reference:** supports comparison against values, thresholds, baselines, or regions.
- **Annotation:** relates explanatory text or geometry to projected objects.
- **Basemap or raster:** supplies geographic or field context without becoming graph authority.
- **Page, board, lane, or zone:** supplies a tangible frame and may participate in collision or placement constraints.

A backdrop may be visible, collidable, both, or neither. Those properties belong to the scene or realization contract. Product significance remains in the host.

### 5. Relation forms

A relation is not synonymous with a line.

| Form | Best fit |
| --- | --- |
| Straight, curved, stepped, or polyline stroke | Ordinary node-link relations |
| Directed or labeled connector | Dependency, transition, causality |
| Port-anchored and orthogonal route | Circuit, architecture, flowchart |
| Parallel or fanned routes | Multiple distinct relations between endpoints |
| Bundled route | Dense overview where individual provenance remains recoverable |
| Arc | Ordered or genomic relationships |
| Ribbon | Aggregate flow or chord relation |
| Matrix cell | Dense pairwise relations |
| Adjacency | Tiling, sequence, topology |
| Containment | Hierarchy, membership, compound nodes |
| Alignment or shared channel | Correlation, common category, synchronized event |
| Order | Sequence, priority, dependency without an explicit stroke |

The portable contract should preserve relation identity and endpoints even when the realization replaces a stroke with another form.

### 6. Arrangement families

An arrangement computes geometry under constraints. A reading may support several arrangements, and an arrangement may support several marks.

| Family | Variants | Current Mere position |
| --- | --- | --- |
| Authored spatial | Free placement, pinned ground, tabletop | Expressible through positions and constraints; product recipes remain host-owned |
| Force and constraint | Spring, collision, gravity, clusters | Landed in Seiche; portable score promotion requires a forcing consumer |
| Grid and stack | Rows, columns, masonry, ordered stack | Grid and Stack are portable in Score v4; Grid has a Cartography adapter, while Stack still lacks a current adapter consumer |
| Axial and lanes | Timeline, Gantt, swimlane, categorical columns | Timeline and Kanban are portable in Score v4; generalized axes and guides remain a gap |
| Radial and spiral | Radial, phyllotaxis, polar, concentric | Radial and Spiral are portable in Score v4 and have Cartography adapters |
| Tree and DAG | Tidy tree, cluster, layered DAG, radial tree | Contract gap pending hierarchy and schematic proofs |
| Space-filling hierarchy | Treemap, partition, sunburst, pack | Contract gap pending a hierarchy proof |
| Matrix | Adjacency matrix, heatmap, table matrix | Expressible as a reading; general cell marks and guides remain a gap |
| Flow | Sankey, alluvial, river | Contract gap pending aggregate-flow proof |
| Circular relation | Chord, arc diagram, Rosette | Rosette is landed through Knot over two text datasets |
| Geographic | Map projection, region, route, hull | Geographic and Hulls are portable; the minimum backdrop contract is landed, while raster and scalar field context remains open |
| Cartesian statistical | Bar, line, area, scatter, distribution | Contract gap in derived marks, scales, and guides |
| Tiling and packing | Mosaic, bin pack, Voronoi, Penrose | Penrose is portable in Score v4; generic tiling remains recipe-level |
| Generative | L-system, procedural field, semantic embedding | L-system and Embedded are portable in Score v4; procedural field semantics remain product-owned |
| Nested and faceted | Small multiples, compound nodes, nested canvases | Nested spaces are portable; facet composition needs a forcing proof |
| Three-dimensional | Spatial volume, layered depth, immersive graph | Deferred until a real second consumer forces portable depth semantics |

Portable `score::Arrangement` version 4 names Spiral, Grid, Geographic, Hulls,
Stack, Penrose, LSystem, Timeline, Kanban, Embedded, and Radial, plus the
explicitly untyped `Custom` escape for a registered external solver. `Grid`
means regular cells; `Kanban` is the current categorical-column form;
`Embedded` carries supplied coordinates. `Plotted` remains the ratified future
name if a forcing proof converges the coordinate families on a clearer common
contract. `Tabletop` remains a complete authored-ground scene, not an
arrangement synonym. Cartography currently exposes adapters for Grid,
Phyllotaxis/Spiral, Penrose, L-system, Timeline, Kanban, Semantic Embedding,
Spectral, and Radial. A portable family may exist before Cartography has a
consumer for it, and a local recipe may remain useful without becoming a new
portable variant.

### 7. Interaction and change over time

Interaction acts on source authority, reading parameters, projection constraints, or view state. Each command should say which.

| Intent | Likely target |
| --- | --- |
| Select, inspect, compare | View state plus source identity |
| Navigate, open, focus | Host command against source identity |
| Filter, sort, group, facet | Reading parameters |
| Brush, zoom, pan | View or reading parameters |
| Drill down, roll up | Reading plus nested space |
| Annotate, connect, edit value | Source authority through host policy |
| Drag free item | Placement constraint or product-owned position |
| Displace anchored item | Temporary scene state, then return to solved home |
| Pin or unpin | Explicit placement constraint |
| Change mark, encoding, or arrangement | Projection specification |
| Play, scrub, compare epochs | Temporal reading and scene epoch |

Motion has several meanings and should not collapse into one physics switch:

- solver motion toward an arrangement;
- direct manipulation;
- transition between projections or epochs;
- data-encoded motion;
- ambient product behavior.

Scenotime carries portable epochs, diffs, picking state, and deterministic
transition schedules between revisions. A transition spec declares class
windows, duration ratios, and easing; pure evaluation accepts elapsed time from
the host. The host still owns the clock and product meaning, while its renderer
realizes the sampled values. Solver motion, direct manipulation, data-encoded
motion, and ambient behavior remain separate systems.

## Projection family catalog

These families are capability tests. They are not mutually exclusive chart types and they do not require one adapter per family.

| Family | Makes legible | Reading and encoding | Mere status | Forcing proof |
| --- | --- | --- | --- | --- |
| Orrery or node-link | Local topology, paths, clusters | Entities as glyphs/cards; relations as routes | Landed core case | Keep as baseline, not universal form |
| Dense relation matrix | Pairwise presence, strength, symmetry | Ordered endpoints on two axes; relations as cells | Expressible, contract gap in marks/guides | Same relation set as node-link, with selectable cells and source provenance |
| Table or facet matrix | Exact values across heterogeneous entities | Rows or panels by entity; fields as columns/channels | Expressible | Mixed node types projected without flattening authority |
| Cartesian chart | Comparison, trend, correlation | Quantitative/temporal scales; bars, points, lines, areas | Contract gap | One heterogeneous fixture as bar, scatter, and line readings |
| Distribution | Shape, spread, outliers, uncertainty | Binning, quantiles, density; bars, boxes, areas | Contract gap | Derived marks retain input provenance and accessible values |
| Timeline or schedule | Order, interval, concurrency, change | Time axis; points, intervals, lanes, trails | Partly landed | Entities and relations survive scrubbing across epochs |
| Hierarchy node-link | Parentage, depth, branching | Hierarchy reading; tree or layered arrangement | Contract gap | Same hierarchy as tree and radial tree |
| Hierarchy space-filling | Part-to-whole and subtree weight | Aggregate hierarchy; rectangles, sectors, circles | Contract gap | Same hierarchy as treemap and sunburst without source rewrite |
| Flow | Quantity moving through stages | Aggregate paths; widths and ribbons encode quantity | Contract gap | Sankey-style projection with traceable constituent relations |
| Circular or chord | Pairwise exchange among ordered groups | Polar positions; arcs or ribbons | Expressible recipe | Chord and arc views from the same grouped relation reading |
| Set membership | Overlap and combinations | Set derivation; regions or membership matrix | Contract gap | Venn-like and matrix realization from one membership reading |
| Geographic | Location, route, region, spatial field | Geographic transform; symbols, regions, paths, raster | Minimum scene and backdrop contract landed; raster/scalar field semantics open | Basemap plus selectable graph entities and source-safe routes; field proof remains consumer-gated |
| Schematic or flowchart | Ports, direction, stages, constraints | Typed entities; port-anchored orthogonal connectors | Contract gap | Flowchart and architecture diagram with stable ports and labels |
| Sequence or state | Ordered interaction and transitions | Participants/states plus messages or transitions | Contract gap | Sequence and state views preserve transition identity |
| Authored board or tabletop | Deliberate composition and tangible grouping | Authored positions, zones, props, collision | Expressible product recipe | Cleromancy-style layout stays authored without a generic spread DSL |
| Document or text | Narrative order, quotation, reference | Text blocks, media, links, annotations | Expressible | Source-backed document fragments coexist with graph navigation |
| Faceted small multiples | Comparison across groups or readings | Repeated subscenes with shared or independent scales | Contract gap | One score carries nested spaces and stable shared scales |
| Semantic embedding | Similarity and neighborhoods | Vector reading; positions or contours | Landed locally | Second dataset proves the arrangement is not domain-bound |
| Provenance or trail | Origin, derivation, confidence, freshness | Temporal/provenance reading; paths, layers, annotations | Expressible | Derived scene objects expose their sources and transformations |
| Three-dimensional or immersive | Occlusion, volume, spatial depth | 3D transform, depth, spatial interaction | Defer | A real non-demo consumer and accessible alternate realization |

## Prior named scenes, reclassified

The earlier Scenograph catalog's names remain useful for product design. They should not become peer-level portable primitives.

| Prior scene | Grammar composition | Classification |
| --- | --- | --- |
| Mosaic | Tiling arrangement + image/snapshot marks + optional groups | Product recipe |
| Atlas | Geographic reading + map projection + basemap + symbols/routes/regions | Product recipe; minimum backdrop contract landed, richer raster/field context open |
| Tabletop | Authored ground + zones/props + collision + free/pinned items | Product recipe |
| Chronicle | Temporal reading + axis/lanes + epochs/trails | Product recipe; guide gap |
| Circuit | Layered or authored arrangement + ports + orthogonal routes | Product recipe; port/routing gap |
| Loom | Ordered lanes + flow quantities + ribbons/routes | Product recipe; flow gap |
| Spotlight | Neighborhood reading + radial arrangement + focus/context channels | Product recipe |
| Rosette | Polar arrangement + arcs/ribbons + grouped relation reading | Product recipe |
| Fog | Exploration state + field/backdrop + progressive disclosure | Product-owned semantics |
| Grove | Temporal/freshness reading + spiral or organic arrangement + media marks | Product-owned semantics |

This reclassification keeps the evocative scene names available to Turnstone and the Merely application family while Scenograph stays usable by unrelated hosts.

## Current boundary map

### Already present

- `sceno::Scene` carries separate source and instance identity, nested spaces, projected items, transforms, footprints, representation requests, layers, visibility, hit policy, scalar channels, routed relations, and regions.
- `sceno::Score` version 4 supplies the expanded portable arrangement vocabulary, authored holds, and per-item axis, embedding, and weight disclosures needed for deterministic solving. `Scene` reports both honored and unmet pins.
- Scenomise owns placement; Scenotime owns portable epochs, diffs, and picking.
- Mere's Cartography adapter surface includes Grid, Phyllotaxis/Spiral, Penrose, L-system, Timeline, Kanban, Semantic Embedding, Spectral, and Radial. Score v4 additionally carries portable families such as Stack, Geographic, and Hulls without pretending Cartography has a current consumer for each. Force physics lives in Seiche.
- Cartography provides a representation registry and host-selectable profiles rather than hardwiring one face for every graph class.
- Canvas separates face, body, collider, placement, and behavior concerns.
- Numen defines fields with global, regional, node-attached, and polygon extents plus lifecycle.
- Platen is the projection compiler in Mere's composition spine. It is not graph authority.
- Cambium's `graph_canvas` can derive Sprigging paint and aligned retained
  semantic targets from one swatch. Retinue Signalman `8cea8f9` proves those
  realizations share one focus order, pointer-capture and action route,
  AccessKit tree, and genet-probe address space. This is realization evidence,
  not a new portable grammar primitive.

### Contract gaps exposed by the catalog

1. **Derived marks, scales, and guides.** Bar, line, scatter, histogram, and accessible chart proofs need scene objects that can cite their derivation without masquerading as graph nodes.
2. **Relation endpoints and routing constraints.** Schematics need ports, endpoint anchors, labels, and orthogonal routing while preserving relation identity.
3. **Richer underlay and field data.** C3 closed the minimum portable backdrop
   contract: source-backed identity, kind, transform and footprint, visibility,
   collision, structural hit transparency, and remote carriage. Atlas basemap
   resources, field rasters, scalar legends, and derivation policy beyond that
   minimum still wait on a field consumer.
4. **Facet and shared-scale composition.** Small multiples need nested scenes with declared shared or independent domains.
5. **Headed accessibility receipt — closed.** Graphshell produces semantic structure, HTML, an AccessKit tree, a long-form table, and honored/unmet placement reporting. Turnstone `648bf19` supplies the definitive headed OS screen-reader traversal and routed-interaction receipt.

The open items are research findings, not an instruction to enlarge Scenograph immediately. Each addition needs a projection proof that fails cleanly without it.

## Promotion rules

A capability enters a portable contract only when a named consumer forces it.

The promotion suite is a shared evidence harness, not a forcing consumer. It
begins when a named product consumer opens one proof. The other projections in
the suite test reuse and expose missing contract material; they do not authorize
portable additions that the first consumer did not ask for. Every promoted
addition still requires the second heterogeneous consumer named below.

For every proposed addition, record:

- task or inference it serves;
- required source facts;
- reading and derivation rules;
- marks, channels, guides, and arrangement;
- source and derivation provenance;
- interaction and writeback policy;
- static and accessible realization;
- first forcing consumer;
- second heterogeneous consumer;
- receipt proving deterministic portable behavior.

When that first consumer opens a proof, found the promotion suite with one
heterogeneous fixture and project it as:

1. a node-link orrery;
2. a dense relation matrix;
3. a Cartesian chart with aggregate derived marks;
4. a hierarchy view such as a tree or treemap;
5. a schematic with ports and orthogonal routes.

The source authority must remain unchanged across these projections. Each proof is complete when:

- projected instances and derived marks retain source or derivation provenance;
- a Graphshell carrier round-trips score and scene data deterministically;
- a second dataset uses the same recipe without product-specific fields;
- interactive intents route through host authority rather than mutating portable scene facts silently;
- the same scene can produce an interactive and a static realization;
- the static result has a navigable semantic structure and sufficient text or tabular description;
- no product term or renderer resource leaks into portable types;
- the old duplicated implementation is removed when a portable primitive is promoted.

Do not add a primitive until one of these proofs fails without it. A catalog is a boundary map, not a cleanup queue or implementation schedule.

## What the external systems teach us

External prior art divides into two shelves, and the division is the point. Renderers and toolkits are prior art for drawing a scene. Specification languages and design solvers are prior art for the compiler: what a projection spec *means*, how it is checked, what is left underdetermined, and who is allowed to decide the rest. Mere's realization layer has no shortage of the first kind to learn from. The unresolved questions in the projection stack are almost all on the second shelf.

### Renderers, toolkits, and foundations

- [Vega](https://vega.github.io/vega/docs/specification/) demonstrates a declarative grammar spanning data, transforms, scales, projections, axes, legends, marks, and signals. Mere needs the same separation of concerns, while retaining graph identity and host authority.
- [Observable Plot](https://observablehq.com/plot/) shows the value of composing layered marks rather than treating every visual form as a separate chart type.
- [Graphviz DOT](https://graphviz.org/doc/info/lang.html) and the [Graphviz layout catalog](https://graphviz.org/docs/layouts/) show that graph structure, subgraphs, clusters, constraints, and layout algorithms can remain distinct.
- [Eclipse Layout Kernel](https://eclipse.dev/elk/reference.html) shows the depth required by serious diagram layout: hierarchy, ports, labels, edge routing, fixed positions, and algorithm-specific options.
- [Mermaid](https://mermaid.js.org/intro/) is evidence that one textual authoring surface can cover flowcharts, sequence, class, state, entity-relationship, schedules, Sankey, architecture, radar, treemap, and other families. The transferable lesson is a shared semantic pipeline, not a universal diagram syntax for Mere.
- [D3 hierarchy](https://d3js.org/d3-hierarchy), [D3 shape](https://d3js.org/d3-shape), and [D3 chord](https://d3js.org/d3-chord) show that hierarchy transforms, shape generators, and arrangements remain reusable when marks are not fused to data authority.
- [deck.gl](https://deck.gl/docs) demonstrates the utility of a data-to-layer-to-view pipeline for maps and large GPU scenes.
- [Cytoscape.js](https://js.cytoscape.org/) demonstrates graph styling, selector-driven presentation, compound nodes, and replaceable layout extensions.
- [Munzner's nested model](https://www.cs.ubc.ca/labs/imager/tr/2009/NestedModel/) separates domain problems, abstract tasks and data, visual encoding and interaction, and algorithms. That separation closely matches the projection stack in this document.
- [Brehmer and Munzner's task typology](https://www.cs.ubc.ca/labs/imager/tr/2013/MultiLevelTaskTypology/) supplies a useful why, how, and what account for catalog entries.
- [Mackinlay's APT paper](https://courses.ischool.berkeley.edu/i247/f05/readings/Mackinlay_APT_TOG86.pdf) distinguishes expressive from effective graphical presentations. Mere should be able to express a projection without claiming every encoding is equally effective.
- [Cleveland and McGill](https://www.tandfonline.com/doi/abs/10.1080/01621459.1984.10478080) ground channel choice in graphical perception rather than novelty.
- [WAI guidance for complex images](https://www.w3.org/WAI/tutorials/images/complex/) and [Graphics ARIA](https://www.w3.org/TR/graphics-aria-1.0/) show that charts, maps, and diagrams require structured descriptions and navigable semantics, including nested graphics. [SVG 2 structure](https://www.w3.org/TR/SVG/struct.html) supplies grouping, titles, and descriptions for one realization target.

### Specification languages and design solvers

Eleven systems reviewed against this catalog in the projection grammar report (2026-08-15). Each line states what transfers; the [projection grammar adoption plan](../implementation_strategy/2026-08-15_projection_grammar_adoption_plan.md) is the in-repo record of which gated target carries it and what forcing consumer it waits on. The report's own conclusion is that these are prior art for the compiler, not for the scene: none of them replaces a Mere realization, and none of them has the authority layer this catalog is built around.

- [Vega-Lite](https://vega.github.io/vega-lite/docs/) makes selections first-class spec citizens, fills in scales and guides by rule rather than by hand, and gives `resolve` for per-channel shared or independent scales across composed views. Transfers: the scales-and-guides gap, the facet gap, and the vocabulary for declaring how coordinated views share a scale.
- [Draco](https://github.com/uwdata/draco) encodes visualization effectiveness as weighted constraints held separately from the grammar, and searches for completions of an underspecified design. [Draco 2](https://github.com/cmudig/draco2) replaced the knowledge base without changing what a spec means. Transfers: effectiveness and default-choosing knowledge is versioned *beside* the score, never inside it; a solver proposes, the score records what was chosen.
- [SetCoLa](https://uwdata.github.io/setcola/) scopes layout constraints to predicate-defined sets of nodes and defers instance generation to the runtime, so one authored layout reapplies to a second graph. Transfers: the schematic gap's constraint form, and the catalog's second-dataset receipt.
- [Gemini](https://github.com/uwdata/gemini) treats the transition between two chart states as an authored specification rather than an implicit tween. Transfers: change-over-time gets a spec, not just a pair of epochs, under the expansion brief's animation lane.
- [GoTree](https://dl.acm.org/doi/10.1145/3313831.3376297) factors tree visualizations into visual elements, layout, and coordinate system, making node-link and space-filling settings of one family rather than separate types. Transfers: factor `score::Arrangement` at the hierarchy proof instead of adding monolithic variants. The same paper is the caution: a grammar that factors too eagerly gets a complexity cliff.
- [ATOM](https://www.microsoft.com/en-us/research/publication/atom-a-grammar-for-unit-visualizations/) builds unit visualizations by recursively partitioning data through layout operators until every item has a size and position, and distinguishes unit marks from aggregate marks. Transfers: the recursive-partition operator shape for arrangement factoring, and the unit/aggregate distinction the derived-marks gap needs.
- [Mosaic](https://idl.uw.edu/mosaic/) makes coordination itself data: a selection is a set of clauses carrying source, clients, predicate, and value, with a declared resolution strategy (single, union, intersect, or crossfilter, where crossfilter means a view is filtered by every brush but its own). Transfers: brush, filter, and focus become named serializable citizens instead of host-only state; the resolution declaration is the part usually left implicit.
- [Gosling](https://gosling-lang.org/) declares level-of-detail as visibility conditions with an explicit target, measure, operation, threshold, and hysteresis padding. Transfers: the conditions that select a representation rung become part of the spec, so a remote client can re-select on its own zoom and a static realization can state why a rung was chosen.
- [Penrose](https://penrose.cs.cmu.edu/) separates `ensure` from `encourage`: a hard constraint that must hold and reports when it cannot, against a soft one that is best-effort by design. Transfers: the vocabulary for placement satisfaction, where a pin is ensure-class and an anchored home is encourage-class, and the answer to WebCoLa's silent-soft failure.
- [Bluefish](https://bluefishjs.org/) composes diagrams from declarative relations over a scenegraph that carries hierarchy and adjacency together rather than forcing a single tree. Transfers: confirmation that this catalog's compound scene shape is right, and a reason to hold it deliberately. Nothing to add today.
- [GoFish](https://vis.csail.mit.edu/pubs/gofish/) formalizes Gestalt relations such as uniform spacing, containment, and connection inside one grammar that covers charts and diagrams alike. Transfers: the chart-side evidence for this catalog's central bet, that a chart, a diagram, and a spatial graph can be projections of one system.

**Name collisions.** Two systems on this shelf share a name with a variant in [Arrangement families](#6-arrangement-families) and mean something unrelated. CMU's Penrose is a diagram specification language; Mere's is `graph_layout:penrose`, the aperiodic tiling arrangement (P2 kite-dart or P3 rhombus via Robinson subdivision) registered in `crates/canvas/arrangements`, reached through `PenroseAdapter` under the projection id `penrose.default`. UW IDL's Mosaic is a view-coordination architecture; "Mosaic" in the tiling row is a packing variant with no implementation in the tree. Both names stay, because both are established in their own domain. Cite the shelf when the specification language is meant.

### Material systems and dynamic documents (Ink & Switch, read 2026-08-28)

A third shelf, added 2026-08-28 from an Ink & Switch harvest. These systems are
prior art for neither the renderer nor the compiler but for the **intent
column**: what it looks like when readings, bindings, and overrides are
tangible, manipulable objects in the scene rather than host-only state. Mosaic
(specification shelf) made selections serializable data; these make them
*material*. The collaboration and merge half of the same harvest lands in the
[knot lane brief](2026-08-19_knot_lane_brief.md) rather than here.

- **PlayBook** ([project 032](https://www.inkandswitch.com/project/playbook/),
  plus the internal-draft *User's Guide Volume I* — the system was designed by
  writing its manual first, a use-case-first instrument worth copying as a doc
  genre). Five materials (ink, paper, pin, beam, flux) with physics: paper
  carries ink and composes by size; pin/beam are couplers and linear/radial
  actuators; ink drawn on a beam follows the beam. Transfers, four:
  **flux** is a drawn material whose `contents` property is a live, spreading
  selection — a reading reified as a scene object with properties, the spatial
  cousin of Mosaic's selection-as-clauses, and the first concrete answer this
  shelf has to "what does a first-class reading look like on screen".
  **Slot/card/whisker** is binding vocabulary: a slot is the binding site that
  establishes a property's meaning, a card is a value object that *copies* when
  dragged out, and copying a slot leaves a whisker — a visible live-binding
  edge along which changes propagate, i.e. provenance drawn in the scene.
  **Overriding** is reversible intent: dropping a compatible slot atop another
  substitutes its value temporarily and pulling it away restores the original —
  a "change encoding parameter" intent whose reversal is spatial, never a
  silent scene mutation. And **selections are non-exclusive**: the same
  material may belong to several selections at once, which is this catalog's
  instance/source split stated as a UX principle. Also self-hosting as a test:
  every PlayBook panel is made of the same materials it manipulates.
- **[Drawdeck](https://www.inkandswitch.com/ink/notes/drawdeck/)** (lab note):
  piles (proximity grouping), runestones (behavior tiles), and *curses* —
  temporary modifiers that revert naturally, an independent rediscovery of
  override-and-revert. Side finding: visual-model spatial queries answer
  spatial predicates directly, where indirect metrics struggle.
- **[Portemine](https://www.inkandswitch.com/ink/notes/portemine/)** (lab
  note): propagator networks as PlayBook's candidate compute model —
  bidirectional constraints, explicit monotonic time against cycles, constraint
  solving in userspace. Transfers: prior art for any future reactive or
  constraint lane, sitting beside Penrose's ensure/encourage on the solver
  question rather than replacing it.
- **[Potluck](https://www.inkandswitch.com/potluck/)** (essay): gradual
  enrichment of plain text — named, composable live searches (`{number}`
  referenced by later patterns) act as readings over a text authority,
  spreadsheet-style computations derive values, and dynamic annotations
  overlay the document without ever mutating it. Transfers: the reading shape
  for the document/text projection family — a Potluck search is a selection
  reading over spans, and its annotations are derived marks whose provenance
  is a span. Their stated limitations (fuzzy parsing, maintenance of heavy
  enrichments) are the argument for coverage reporting, which Knot's lens
  doctrine already requires.

## Related Mere research

- [`2026-08-18_scenograph_content_catalog.md`](2026-08-18_scenograph_content_catalog.md): the dependent catalog of complete scenes composed from this grammar.
- [`2026-08-10_scenograph_expansion_brief.md`](2026-08-10_scenograph_expansion_brief.md): expansion candidates and ownership questions.
- [`2026-07-21_projection_engine_prior_art_brief.md`](2026-07-21_projection_engine_prior_art_brief.md): prior-art comparison for the projection engine.
- [`2026-06-22_graph_projections_research.md`](2026-06-22_graph_projections_research.md): graph projection families and early taxonomy.
- [`2026-06-18_node_representation_arrangement_plan.md`](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md): representation and arrangement implementation seams.
- [`2026-06-13_scriptable_field_regions_plan.md`](../implementation_strategy/2026-06-13_scriptable_field_regions_plan.md): field and region model.
- [`2026-07-21_projection_proofs_plan.md`](../implementation_strategy/2026-07-21_projection_proofs_plan.md): proof sequence and portable projection discipline.
- [`2026-08-15_projection_grammar_adoption_plan.md`](../implementation_strategy/2026-08-15_projection_grammar_adoption_plan.md): gated targets carrying the projection grammar report's transfers into mere, genet, and cambium.
- [`design_docs/scenograph_docs/technical_architecture/2026-07-22_scene_contract_note.md`](../../scenograph_docs/technical_architecture/2026-07-22_scene_contract_note.md): scene ownership contract.
- [`2026-07-24_scenograph_0_0_3_release_plan.md`](../implementation_strategy/2026-07-24_scenograph_0_0_3_release_plan.md): historical 0.0.3 release boundary.

## Closing thesis

Mere does not need to predict every future diagram. It needs a small projection grammar that preserves identity, derivation, relation, field, space, interaction authority, and semantic accessibility while allowing radically different visual realizations.

That makes charts and diagrams ordinary projections of graph authority. It also leaves room for Merely's scenes to stay particular, vivid, and product-shaped.
