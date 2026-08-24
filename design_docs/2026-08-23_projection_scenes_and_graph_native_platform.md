# Projection Scenes and the Graph-Native Application Platform

**Date:** 2026-08-23  
**Status:** direction, discussed with Mark 2026-08-23; no code task opened here  
**Scope:** sharpen the projection-scene test, record the scene-catalog cuts, and
derive the capabilities Mere, Scenograph, Cambium, and Genet need to extend the
web platform with graph-native application behavior.

**Related:**

- [projection grammar catalog](mere_docs/research/2026-08-15_projection_grammar_catalog.md)
- [Scenograph content catalog](mere_docs/research/2026-08-18_scenograph_content_catalog.md)
- [shelfmark format note](mere_docs/technical_architecture/2026-08-16_shelfmark_format_note.md)
- [family composition thesis](2026-08-12_family_composition_thesis_brief.md)
- [Turnstone suite composition and capability census](2026-08-22_turnstone_suite_composition_and_capability_census.md)
- Genet `components/cambium/ARCHITECTURE.md`
- Genet `docs/2026-08-12_meristem_scope_cut_and_component_contract_brief.md`
- Genet `docs/2026-08-14_web_platform_host_contract_plan.md`

**2026-08-23 follow-on:** the §8 receipts are scheduled in the
[projection receipts plan](mere_docs/implementation_strategy/2026-08-23_projection_receipts_plan.md):
wave 1 is Matrix and coordination with mer3ly as first consumer and the
gazette Ledger as heterogeneous second; the field receipts stay gated there.

## 1. Ruling

A scene is a reusable projection regime, not a renamed dataset, application
surface, arrangement, query, or item representation. It composes a reading,
encodings, an arrangement, relation forms, guides or backdrops, and interaction
policy toward one legible purpose. It must still read true against a second,
heterogeneous dataset.

The scene inventory is therefore expected to be small. Fifty names are easy to
produce by changing product nouns or one projection lever. That does not yield
fifty scenes. The useful work is to find the smaller set of representations in
which source entities and relations become categorically different projected
objects.

Before applying the scene test, identify which projection layer owns the
candidate's novelty: authority, reading, encoding, arrangement, relation form,
guide or backdrop, composition, interaction, realization, or the total scene
recipe. If the novelty is exhausted by one lower layer, the candidate belongs
there even when the resulting application surface is distinctive. This
pre-filter rejects readings and composition capabilities wearing scene names.

For every candidate that survives the layer pre-filter, answer:

1. Which authority elements and values does it read?
2. Which selection, grouping, aggregation, traversal, temporal, spatial, or
   statistical derivation does it perform?
3. What does an entity become in this scene?
4. What does a relation become?
5. What geometric law determines position, size, path, containment, or region?
6. Which guides, scales, fields, spaces, or backdrops are required?
7. How does every projected or derived object map back to source authority?
8. Which intent changes source truth, reading parameters, projection state, or
   view state?
9. What second dataset proves that the recipe is not an ontology skin?

Changing graphlet depth, product vocabulary, styling, backdrop, or one
arrangement lever produces a configuration or variant unless the change also
alters the representation's governing purpose and source-to-scene mapping.

## 2. Current scene cut

This is a catalog judgment, not a portable-contract commitment. Two statuses
must remain independent:

1. **Categorical status:** whether a recipe is a distinct scene family.
2. **Contract evidence:** whether a consumer has forced its portable meaning
   and the required promotion receipts have landed.

This direction note may change categorical status without advancing a contract
gate. In the table, **no promotion asserted** means exactly that: the row makes
no new claim about implementation, consumer proof, or portability.

### Categorically distinct scene families

| Scene | Governing representation | Contract evidence named here |
| --- | --- | --- |
| **Orrery** | Entities become spatial bodies; relations become selectable routes; topology governs the view. | No promotion asserted. |
| **Mosaic** | Entities become adjacent media tiles; adjacency carries kinship and the collection becomes the ground. | No promotion asserted. |
| **Atlas** | Entities become markers in a referenced coordinate system; routes, ranges, regions, and geographic context retain real-world meaning. | No promotion asserted. |
| **Tabletop** | Entities become tangible pieces on an authored, collidable ground; placement and zones carry the composition. | No promotion asserted. |
| **Timeline** | Entities become events, spans, samples, or trails on a temporal axis; order, interval, concurrency, and change govern the reading. | No promotion asserted. |
| **Matrix** | Two readings become axes; relations, values, similarities, or deltas become addressable cells. | Two-reading receipts are proposed below. |
| **Partition** | A hierarchy becomes nested, value-bearing area; an entity is an enclosure or share of a whole. | No promotion asserted. |
| **Delta** | Descent topology governs the scene: stable identities become version, branch, membership, and reconciliation structures across epochs. If a retained base scene merely acquires change marks, that is the Diff operator below. | No promotion asserted. |
| **Calendar** | Temporal facts are grouped by recurring units on a semantic temporal backdrop; entries or aggregates inhabit the cells. | No promotion asserted. |
| **Scatter** | One source entity may produce several marks; quantitative values determine position while other facets determine size, color, form, labels, or uncertainty. | No promotion asserted. |
| **Profile** | Selected facets become comparable axes or panels; an entity becomes a multi-value profile. | Gazette is a candidate consumer; no promotion asserted. |
| **Setscape** | Membership becomes containment and overlap; sets become regions and entities inhabit intersections. | No promotion asserted. |
| **Deck** | One graphlet is repeated across configurable facet or metadata panels, with linked identity and declared shared or independent scales. | No promotion asserted. |
| **Territory** | Positioned sites and a winner rule partition a space; an entity becomes a site with a categorical region. | No promotion asserted. |
| **Contour** | A scalar field becomes bands, isolines, extrema, or raster; entities act as samples, anchors, or sources. | No promotion asserted. |
| **Current** | A vector field becomes arrows, streamlines, sources, sinks, and vortices. | A simulator, radio model, or system emulator is only a candidate consumer. |
| **Cartogram** | Named regions deform by quantitative values while preserving selected adjacency or recognizability constraints. | No forcing consumer is cited here. |
| **Volume** | Values and entities inhabit a three-dimensional space through slices, voxels, isosurfaces, landmarks, or nested volumes. | Portable depth semantics remain unproven here. |
| **Distribution** | Samples become contributor-aware bins, densities, quantiles, outliers, or intervals governed by a statistical distribution. | No named consumer is cited here. |
| **Simplex** | Values constrained to one total place an entity in a simplex; barycentric position carries the composition. | No forcing dataset is cited here. |
| **Provenance** | Sources, transformations, confidence, authorship, and freshness become an inspectable derivation structure. | No promotion asserted. |
| **Document** | Entities become passages, figures, annotations, or embedded surfaces in narrative order; relations become references, quotations, and transclusions. | No promotion asserted. |

### Candidates whose categorical status remains unresolved

| Candidate | Current judgment |
| --- | --- |
| **Topological network** | It may be Orrery over typed network facts rather than a separate total regime. Bearer, capacity, and route constraints must change more than the dataset vocabulary to separate it. |
| **Streams** | Earns separation from Timeline only if its shared axis may be ordinal, quantitative, or procedural; stream membership sets one axis and cross-stream relations remain explicit. |
| **Rosette** | Knot has landed poem and lyric proofs, but the 2026-08-23 review reopens whether the recipe is categorically more than polar placement plus chord encoding. A non-prosodic transfer would sharpen that judgment. |
| **Phase** | It may be Scatter plus a temporal trajectory. It earns scene status only if state-space navigation and dynamical structure govern the entire representation. |
| **Incidence** | An n-ary relation becoming a hub, region, or connector may be a relation form rather than a scene. A complete regime must be shown. |
| **Alignment** | Ordered sequences become parallel rows with correspondence columns, gaps, substitutions, and repeats. It remains unclear whether this is a total scene or a Matrix or Streams configuration. |

### Collapsed or reclassified proposals

- **Chronicle** is Timeline with era bands, causal arcs, and replay controls.
- **Circuit** is Topological Network unless typed ports and orthogonal routing
  are source-significant constraints. Without those facts it is an abstract
  network skin.
- **Loom** becomes Streams only after the non-temporal shared-axis rule is
  demonstrated.
- **Spotlight** is a graphlet-producing N-order search combined with radial or
  focus-and-context arrangement.
- **Fog** and **Grove** are last-visited, unvisited, freshness, and tending
  readings or metadata channels. They do not establish scene identity.
- **Ledger** is Matrix with entities on one axis and selected facets on the
  other.
- **Canopy** merely renames a hierarchy reading plus tree arrangement.
- **Itinerary**, ordered selection, path selection, rank, and lasso are graph
  utilities that produce or order a scope.
- **Sequence** and **Statechart** remain ordinary readings of appropriately
  typed data until a product forces a more general projection law.
- **Pulse** and **Feed** are promising entity representations. They enrich a
  node with recent or live data rather than defining a scene.
- **Comparison** is a Matrix flavor at another scope.
- **Diff** is a comparison operator over a retained base scene. It aligns stable
  identities across two epochs and produces addition, removal, move, and value
  marks without replacing the base scene's governing representation.
- **Tag lattice** is a maximal-shared-tag derivation followed by a layered-DAG
  arrangement. Its derived groups may be projected through Setscape,
  Partition, Matrix, or another scene without becoming authoritative nodes.
- **Matryoshka** names nested scene composition: portals, coordinate spaces,
  and authority scopes. A product may use the name for a complete recipe, but
  the reusable novelty belongs to platform composition until such a recipe
  survives the scene test.
- **Neighborhood** is a semantically inferred graphlet. The embedding that
  produces it is a reading or arrangement, not scene identity.
- Node-level, graphlet-level, mere-level, and moot-level lineage are Delta at
  different scopes.

## 3. Matrix is a family over two readings

Matrix is categorically important because neither axis must be the entire
graph. Each axis is an independently produced reading or graphlet.

Initial flavors:

| Axes | Cell meaning |
| --- | --- |
| graphlet A x graphlet A | relation identity, multiplicity, family, direction, strength, or absence |
| graphlet A x graphlet B | cross-scope relations, matching, coverage, or transfer |
| entities x selected facets | exact values, missingness, validation, or difference; the Ledger form |
| entities x scenes/readings | the same source identities compared across projections |
| entities x epochs | presence, value, relation, or representation delta |
| selected entities x selected entities | an ad hoc comparison produced by selection tools |

A cell is a projected object with its own instance identity. It must cite the
source relation, value, contributor set, or derivation that produced it. Picking
a cell may inspect the projection result or route an authorized intent to the
underlying sources; those targets are distinct.

## 4. One source may have several projected instances

The projection grammar already permits one source object to have multiple
projected instances. This becomes a platform-level rule rather than an edge
case.

One entity may appear simultaneously as a Scatter point, Matrix heading, Deck
card, uncertainty mark, legend specimen, Document fragment, or selected-detail
instance. The instances share source identity while retaining separate scene,
space, geometry, representation, and hit identity.

Required behavior:

- source selection can emphasize every visible instance;
- hover and keyboard focus may remain instance-local;
- an instance identifies the facet, relation, or derivation it represents;
- source actions and projection actions remain distinguishable;
- accessibility groups or cross-references repeated appearances rather than
  presenting unrelated duplicates;
- remote and frozen realizations preserve the same source mapping;
- disappearance of one instance does not imply removal of source authority.

This rule is load-bearing for Matrix, Scatter, Deck, comparison and Diff marks,
and Document scenes.

## 5. Graph utilities and enriched graph elements

Several rejected scenes are useful capabilities at a different layer.

### Scope-producing utilities

Search, N-order traversal, lasso, box selection, ordered selection, path
selection, brushing, tag intersection, maximal-shared-tag derivation, and set
operations should produce a stable graphlet or equivalent derived scope with
provenance. A scene consumes that scope without caring how it was produced.

The useful split is:

- Mere owns membership, ordering, derivation, reconciliation, and source
  identity;
- Cambium owns reusable pointer, keyboard, focus, cancellation, and
  announcement behavior;
- the application owns the command that creates, saves, shares, or mutates a
  graphlet;
- a scene owns only how the scope is projected.

### Entity representations

Feed, pulse, meter, badge, portrait, snapshot, and live surface are entity
representations. A scene may request them according to data and level of
detail. They should not require separate scene classes.

An enriched representation may cite several facets and expose several derived
marks while retaining the entity's source identity. High-frequency rendering
may use a Sprigging leaf; semantic structure, values, and available actions
must remain available to DOM, accessibility, and automation paths.

### Nested scene composition

A scene may contain, link to, or open another scene without nested composition
becoming the outer scene's identity. The reusable capability must carry:

- portal and nested-space identity;
- the inner scene's coordinate, focus, and navigation boundary;
- explicit authority scope and authorization at the crossing;
- source-to-instance mapping across the boundary;
- accessible entry, exit, naming, and alternate realization;
- deterministic remote and frozen behavior.

This capability supports Deck, Document, Matrix drill-through, and a possible
Matryoshka product recipe. It belongs to the platform rather than to one named
scene.

## 6. Field scenes remain distinct

Territory and Contour are related but not interchangeable.

- **Territory** evaluates a categorical winner, such as nearest, strongest, or
  highest-priority site. Every point belongs to one result region; boundaries
  arise where candidates tie.
- **Contour** evaluates a scalar value. Thresholds produce bands or isolines;
  points may be high, low, missing, or outside the supported extent without
  belonging to a winning entity.
- **Current** evaluates a vector. Direction and magnitude produce arrows,
  streamlines, sources, sinks, and vortices.

The three may share field evaluation, rasters, legends, region geometry, and
GPU resources. Their projected objects and questions differ.

## 7. Platform consequences

The web platform remains one realization system. The Merely stack adds durable
graph identity, projection, coordinated views, fields, local-first authority,
and remote composition beside it. Those capabilities should enter at their
own layers rather than being disguised as DOM concepts.

### Mere

Mere should own graph-browser-specific orchestration:

- graphlet and reading algebra for search, traversal, selection, brushing,
  grouping, tags, paths, and epoch scopes;
- source-to-instance and source-to-derived-object bindings;
- contributor and derivation provenance;
- linked selection facts and comparison state across scenes, while preserving
  the producing view for every selection;
- temporal metadata, epoch queries, lineage, and diff scopes;
- authority-scope transitions for nested scenes and portals;
- field providers and mappings from graph facts into field samples;
- saved scene recipes and active `ViewIntent`, separate from graph truth;
- intent routing that distinguishes source, reading, projection, and view-state
  targets.

Mere does not need one universal scene implementation. It needs the graph-aware
bindings and coordination that let several realizations remain views of the
same authority.

The current portable selection noun is `chirograph::Selection`, which carries
only `source` and `targets`. The coordinated-view direction here identifies the
forcing condition for a future resolution strategy, but does not advance A2's
gate by itself. Union, intersection, and crossfilter remain unadopted until two
coordinated views actually require and prove them.

### Scenograph

Scenograph remains product-free. The scene exercise identifies contract
pressure rather than an implementation queue:

- several instances may cite one source;
- derived marks need identity, values, contributor provenance, and semantic
  descriptions;
- scales, axes, legends, annotations, units, thresholds, and missing-value
  behavior need portable meaning when a proof forces them;
- Matrix needs two independent readings and addressable cells;
- Deck needs nested spaces with declared shared or independent scales;
- nested scenes need portable portal, space, source-mapping, and authority-scope
  semantics;
- relation identity must survive routes, cells, adjacency, containment,
  ribbons, order, and alignment;
- backdrops and fields need explicit visibility, hit, collision, extent, and
  provenance policy;
- epochs and diffs must preserve instance identity sufficiently for linked
  interaction.

Each addition still requires a forcing consumer, a heterogeneous second
consumer, deterministic carriage, and an accessible frozen realization.
Scenograph owns the portable structural meaning of a guide or nested-space
declaration as supplied by an adapter; the application still owns domain
meaning and authority. Scenograph does not own widgets, input behavior, focus
policy, or platform accessibility objects.

### Cambium

Cambium should own reusable interaction and semantic composition over Genet
elements and custom leaves:

- coordinated source selection across repeated instances;
- single, multiple, range, lasso, box, ordered, and brush selection behavior;
- a virtualized, keyboard-navigable Matrix with semantic row, column, and cell
  targets;
- Deck or panel composition with shared selection and declared scale sharing;
- interactive and accessible realizations of Scenograph-provided scales, axes,
  legends, thresholds, units, and filters;
- timeline scrubbers and before/after comparison controls;
- scene hosting with unified focus, pointer capture, overlays, and typed events;
- portal entry and exit, focus restoration, navigation, and announcements across
  nested scene boundaries;
- keyboard equivalents and announcements for direct manipulation, pinning,
  region selection, and cell activation;
- accessible table, tree, or long-form alternates for dense or spatial scenes.

The existing Cambium component rule continues to fit: props in, retained local
interaction state, typed events out. Applications lower events into their own
actions and effects. Cambium does not acquire graph authority or product
policy.

### Genet

Genet continues to own DOM, CSS, style, layout, paint, input, accessibility,
browser behavior, and platform realization. The graph-native platform adds
pressure at existing neutral seams:

- custom leaves and DOM content share one focus and pointer-capture model;
- semantic scene targets remain addressable through AccessKit, scenarios,
  genet-probe, and automation;
- DOM, scene paint, fields, video, web content, and three-dimensional surfaces
  share the wgpu device and explicit composition order;
- CSS and theme values can style scene realizations without becoming graph
  facts;
- print, snapshot, and frozen export can request a semantic alternate rather
  than capturing pixels alone;
- capability reporting names support or degradation for picking, fields,
  nested spaces, live surfaces, motion, depth, accessibility, and frozen output.

Backend-specific behavior remains behind the web-platform host contract.
Graph-native semantics do not become pseudo-web standards merely because Genet
realizes them.

### Shelfmarks and composed projections

Shelfmark v1 names one authority and one projection. Matrix introduces a
declared contract gap: its two readings may come from different authorities,
and repeated source instances may require authored delta sections to address a
particular projected instance. This direction note does not alter the v1
envelope. The [shelfmark format note](mere_docs/technical_architecture/2026-08-16_shelfmark_format_note.md)
owns the eventual citation shape after a forcing proof determines it.

Catalog collapse also cannot silently reinterpret an identifier already
written to a citation. If a former scene name was emitted as a reading,
arrangement, or other registry id, a resolver must either preserve its meaning
through a versioned alias or report the incompatibility. A catalog judgment by
itself neither proves wire exposure nor authorizes an alias.

## 8. Acceptance receipts

The following receipts would prove the platform shape without requiring every
scene in the catalog:

1. **Repeated source identity:** one source entity appears as a Scatter point,
   Matrix heading, and Deck card. Selecting any appearance emphasizes the
   others; focus remains instance-local; every appearance exposes facet or
   derivation provenance.
2. **Two-reading Matrix:** two independently produced graphlets form the axes.
   Cells preserve relation identity or contributor provenance, round-trip
   through Graphshell, and admit an accessible table realization.
3. **Derived-mark integrity:** a Distribution or Contour scene emits derived
   marks that remain selectable, name their values and contributors, and never
   masquerade as source nodes.
4. **Mixed realization:** one scene combines DOM controls, Sprigging or GPU
   marks, accessible semantic targets, and automation addressing without two
   focus or action models.
5. **Local, remote, and frozen parity:** one scene runs interactively in a
   local host, crosses a Graphshell session to a viewer without source access,
   and freezes into a navigable semantic document or table.
6. **Field distinction:** the same sample dataset produces Territory and
   Contour scenes whose categorical and scalar meanings remain distinct through
   rendering, picking, legends, and accessible output.
7. **View-state authority:** scope, filters, selected facets, arrangement
   constraints, backdrop, and camera save and restore without entering graph
   truth; an authorized source edit still travels through the application.
8. **Coordinated-view selection:** two views over one authority contribute
   selections whose combination rule is explicit, deterministic, serialized,
   and removable. This is the receipt that may open A2's resolution half; the
   direction note alone does not.
9. **Composed citation and compatibility:** a two-reading Matrix whose axes use
   different authorities round-trips through a shelfmark-compatible citation,
   preserves any instance-scoped authored delta, and retains checkability for
   every required input. A previously emitted registry id for a collapsed name
   either reconstitutes with its original meaning or produces an explicit
   incompatibility report rather than silently selecting a newer recipe.

These receipts would demonstrate a graph-native application platform extending
the web platform while preserving the authority and ownership boundaries of
Mere, Scenograph, Cambium, Genet, and their applications.
