# One Node, Atomic Facets, and the Layer Map

**Date:** 2026-07-18
**Status:** ruled (Mark, 2026-07-18, in design conversation). Amends the
[unifying-graph north star](2026-07-08_mere_as_the_unifying_graph.md): resolves
its open question 5.4, retargets 5.1, and answers "is mere still too
browser-shaped?" with a concrete dissolution program. Companion to the
[participant gate + packs plan](../implementation_strategy/2026-07-17_participant_gate_packs_plan.md)
(whose facet/pack/gate machinery this doc leans on) and the
[boundary pass plan](../implementation_strategy/2026-07-09_mere_turnstone_boundary_pass_plan.md)
(whose slice C invented the sidecar pattern this doc generalizes).

**2026-07-22 boundary amendment:** Conatus has since landed as the family repo
for numen, quint, and seiche. The former plan to promote Mere's entire canvas
family as one unit is superseded by the
[Graphshell remote projection host plan](../implementation_strategy/2026-07-22_graphshell_remote_projection_host_plan.md):
generic arrangements move to `scenomise`, kernel-neutral scene contracts move
to `sceno`, the shared interactive view grows through Cambium/Sprigging under a
real Woodshed consumer, and the kernel-aware `mere-canvas` remains in Mere.

## 1. The redundancy, named

Three container-ish abstractions were stacking:

1. `chartulary::Container`: identity, addresses, content-addressed body,
   media type, title, tags, and (since B0, 2026-07-17) a `nested` graph.
2. North-star OQ 5.4's proposed "generalized container node, the richness
   above chartulary's `Container`" (files, media, pages, docs, subgraphs).
3. mere's kernel `Node`: the web-page anchor (rkyv-archived; favicon,
   thumbnail, mime, viewer-adjacent fields, position/velocity).

Layer 2 is redundant with layer 1: Container already spans OQ 5.4's list,
including subgraphs (`GraphBearing`). **OQ 5.4 is resolved-redundant; there is
no "above" to build.**

Layer 3 decomposes into: a graph-fact core that *is* Container (id, addresses,
title, tags, body, mime), web-runtime facets that are OQ 5.1's extraction
targets, and spatial fields (`position`, `velocity`) that belong to the
arrangement layer, not graph truth. Remove the two non-Container parts and
what remains is Container.

## 2. The rulings

1. **One node.** `chartulary::Container` is the neutral node. mere's kernel
   `Node` dissolves into it facet-by-facet; "web page" becomes a content class,
   never the node's essence. OQ 5.1 is retargeted from "thin the Node" to
   "dissolve the Node."
2. **Atomic facets, for all node metadata.** Not one content sidecar: every
   optional metadatum is a **facet**, a typed record keyed by node id + facet
   id, so metadata is content-specific in principle. The shared minimum is
   Container's own capability surface; everything else attaches.
3. **Content classes are data, modder-extensible.** A content class is a facet
   bundle plus a schema. Schemas are eidetic schema engrams (the existing
   `SchemaDefinition` machinery, same as the pack schema); custom classes ship
   as packs and are granted through the gate. Two facet tiers:
   compile-time capabilities (chartulary's trait table) for structural
   features, runtime facets (schema-validated records) for everything else.
4. **The browser dogfoods the facet system.** The web-page content class
   (favicon, thumbnail, viewer metadata, restore fidelity) is turnstone's facet
   bundle, defined with the same machinery a modder would use. If the facet
   system cannot carry the browser, it cannot carry modders.
5. **The lake is the name.** mere *means* a small lake (seiche, its
   oscillation). The product thesis "the lake your data pools in, which all
   your apps drink from" is already encoded; product vocabulary stays
   pool / vault / orrery, and "it's literally a data lake" is the one-line
   industry gloss, not a name.

## 3. The layer map (homes)

- **Portable primitive families and standalone repos:** Eidetic (muniment,
  codicil, chartulary, scholia), Conatus (numen, quint, seiche), personae,
  armillary, and servitor. Promotion gate stays consumer-pull with the sanity
  check.
- **Spatial/arrangement:** the extraction landed in `repos/conatus` on
  2026-07-21: numen defines fields, quint evaluates them, and seiche integrates
  them into dynamic layout. `Node.position/velocity` still retire onto the
  cartography-geometry sidecar and seiche state. Position is an arrangement
  facet, not node essence.
- **The projection and canvas split** (amended 2026-07-22): generic scene
  contracts live in Scenograph; generic analytic arrangements migrate from
  Mere `arrangements` to `scenomise`; `mere-cartography` remains Mere's graph
  adapter; kernel-aware `mere-canvas` remains Mere's graph surface. Shared
  interactive graph-view behavior promotes through Cambium/Sprigging only when
  Woodshed consumes it.
- **mere, the composing library (the pool):** vault + persona indexing,
  cross-vault semantics (scholia), search (sibylla), the kernel-aware graph
  canvas and source adapters, private-memory integration, and optional peer
  projections. Multi-crate and legitimately so; not a bucket once the queued
  subtractions land (Node dissolution, Murm/Moot promotion, and the
  Scenograph/canvas split).
- **Consumers:** turnstone, isometry, hocket, woodshed. Turnstone's end-state
  dependencies: mere (graph/pool/canvas), genet (engine), servitor (gate),
  session-runtime sidecars, and its own **web content class**. The browser
  becomes the platform's first and best mod.

## 4. What this session already proved on-seam

- The denizen binding (mere `953bf09`) is a facet in all but name: host
  knowledge keyed by node id, sidecar beside the graph, kernel Node untouched.
- Denizens are Container residents (servitor `1af0c91` over chartulary
  nesting), which is the end-state shape, no migration awaiting them.
- The pack schema round already chose schema engrams as the extension
  mechanism; ruling 3 reuses that decision rather than adding a second one.

## 5. Sequencing (targets, not durations)

1. **Record** (this doc) + amend the north star's open questions. DONE with
   this doc's landing.
2. **Facet-store design round:** where the runtime-facet mechanism lives.
   Lean: chartulary-generic (facet = typed sidecar keyed by `Identified::Id`)
   with pluggable validation, eidetic supplying the schema validator
   mere-side; decided with code in hand, B0-style spike.
3. **Spatial completion:** Conatus extraction is done. Move positions fully
   onto the geometry sidecar and retire `Node.position/velocity`.
4. **Node dissolution ladder:** facet-by-facet along OQ 5.1 (favicon →
   thumbnail → viewer metadata → restore fidelity → ...), each to the
   turnstone-side web content class; each move has the sidecar pattern as its
   proven home. The ladder ends when `Node`'s remainder is Container.
5. **Projection/canvas split:** execute the Scenograph migration and land the
   shared Cambium/Sprigging interaction surface through the Woodshed consumer.
6. **The pool surface** (north star step 3): personae-indexed vaults,
   cross-vault queries, p2p share/cooperate.

The participant-gate lane continues in parallel on the current seam (turnstone
palette + install next); nothing above blocks it, and denizens already sit on
the end-state side of every line drawn here.

## 6. Open questions

1. **Facet-store home** (sequencing step 2 decides with code in hand):
   chartulary-generic vs mere-side; how compile-time capabilities and runtime
   facets present as one surface to consumers.
2. **The rkyv performance question.** Does Container + facets carry web-page
   workloads (snapshot load, hot graph ops) as well as the specialized rkyv
   `Node`? If a measurement says no, the web node persists as a *physical
   representation* while being conceptually a Container profile. A
   measurement, not a principle; it is the one thing that could keep a
   distinct type alive.
3. **Migration mechanics** for existing graphs (rkyv snapshots + journal
   history through the dissolution ladder); the prod-journal window and the
   B0.5 envelope migration are prior art.
4. **CLOSED 2026-08-18: facet grants.** Which facets a denizen may read or write is a gate
   scope question (the structural cap's path vocabulary may want a facet
   dimension); connects to the participant plan's ability-axis note.
   **Shaped 2026-08-16** by the
   [facet signaling round](2026-08-16_facet_signaling_and_control_loops.md):
   the missing dimension is a facet-namespace `Cap` kind with prefix coverage,
   sitting beside Power (equality) and Scope (segment prefix). The gate already
   scope-checks `SetFacet` and `RemoveFacet` by node id, so it governs which
   nodes may be touched but not which facet namespaces may be written. The
   `mere-capability` leaf and `Cap::Facet` now implement that shape, and the
   gate checks it for both `SetFacet` and `RemoveFacet`.

## Progress

- **2026-08-18 (OQ 4 CLOSED):** the capability algebra moved into the shared
  `mere-capability` leaf before gaining `Cap::Facet`; a `web.` grant now covers
  `web.viewer` by dot segment while refusing `denizen.binding`, and the gate
  requires that facet grant independently of node scope.
- **2026-08-16 (facets as a medium):** a design round extended the facet system
  from metadata to signaling, recorded in
  [facet signaling and control loops](2026-08-16_facet_signaling_and_control_loops.md).
  Facets are participant-to-participant communication through nodes rather than
  node-to-node awareness; interior signaling inside a nested graph is separated from
  exterior marks across graphs by the single-serialization-point argument; a
  decaying signal is ruled onto facets rather than tags; and open question 4
  gains a definite shape. One verified gap named against the servitor reactive
  substrate: cascade depth is bounded, cascade frequency is not.
- **2026-07-27 (OQ 5.1 CLOSED):** the dissolution ladder completed. Kernel
  `Node` is now `{ container, images }`: `container` is
  `chartulary::Container<Uuid, Address>` and `images` is the explicitly retained
  D0 map of small content-addressed experience handles. Arrangement,
  presentation, visit, provenance, classification, literal-property, and
  derivation metadata all live in the graph's one `FacetStore`; Turnstone's
  web-page and note classes use the same data/schema seam as packs. Legacy
  snapshot columns and v1 graph engrams migrate on read. The north-star
  extraction boundary is now settled in code, so its 5.1 question is closed.
- **2026-07-18 (later):** Implementation plan founded:
  [node dissolution + facets plan](../../archive_docs/2026-08-06_completed_plans/2026-07-18_node_dissolution_facets_plan.md)
  (lanes F/S/D, the D-gate rkyv measurement, the full `Node` field-by-field
  destination table; D0 identified as the image externalization plan's
  already-designed phase 2).
- **2026-07-18:** Ruled and recorded (Mark + design conversation). North-star
  OQ 5.4 resolved-redundant, OQ 5.1 retargeted at dissolution. Grounding
  verified same-day: quint/seiche still mere-side, `Node` still carries
  position/velocity, geometry sidecar exists, schema machinery per the pack
  round. No code changed by this doc.
