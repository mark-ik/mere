# Mere as the Unifying Graph

**Date:** 2026-07-08
**Status:** vision (Mark, 2026-07-08). The north star that reframes the graph
substrate program (G0-G5). Records what mere is becoming and how the pieces already
built serve it. Companion to `2026-07-08_generic_graph_substrate_plan.md` and
`2026-07-08_g5_mere_rebase_progress.md`.

## 1. The shift

Two moves, one idea.

- **meerkat extracts out as merecat.** The browser application peels off into its
  own thing. mere keeps the graph; merecat is one consumer of it.
- **mere becomes the graph.** Not "a browser," but the graph library that every
  xilem-serval app with graph-shaped data uses. mere is the unifying orrery for
  strophe, merecat, woodshed, and isometry.

"The graph is the mere." An open naming call follows: the term *orrery* (mere's
current word for the graph-canvas tier) may retire, or narrow to the *view* of the
graph, once mere itself names the graph. Flagged, not decided.

## 2. Three layers

- **Portable primitives** (the family, one-way deps, publishable): muniment (vault
  bytes), codicil (edit log), chartulary (the container graph), stemma (lineage),
  scholia (RDF / semantic interop), and personae (identity). Each is a clean crate
  an outside consumer could take alone.
- **mere, the orrery.** Composes the primitives into the rich thing: the container
  graph with mere's web-and-beyond node model, the RDF projection, the lineage, and
  the vault-and-persona indexing. This is the app-facing graph lib. mere *consumes*
  chartulary (the G5 re-base) and *is consumed by* the apps.
- **Consumers.** merecat (the web graph), strophe (the loop / session graph),
  woodshed (the theory graph, already proven in woodshed-graph), isometry (the
  entity / world graph). Each brings graph-shaped data; mere unifies it.

The load-bearing claim: chartulary stays the portable generic core, and mere is the
composing orrery on top. Two layers, not one. mere does not absorb chartulary; it
depends on it and adds the richness apps want.

## 3. Vaults, personae, and cross-vault intelligibility

This is the payoff, and it is why the substrate was built the way it was.

- **A vault is a muniment-backed content-addressed store**: a datalake. An app, a
  persona, a project each hold vaults.
- **personae indexes the vaults.** Identity is the key into the pool: a bunch of
  ways in to your pool of vaults. Many datalakes, one pool.
- **Data is semantically intelligible across vaults.** Because content is
  content-addressed (shareable by hash) and semantic relations are IRIs (scholia's
  ring), data from different vaults and different apps interoperates as real linked
  data. A woodshed practice note can cite a merecat page; an isometry campaign can
  reference a strophe loop; a persona's notes span all their vaults. The private,
  app-specific relations stay private (the two-ring split); only the shared semantic
  ring crosses vault boundaries.

mere is the orrery that makes the pool one legible whole while each vault stays its
own store.

## 4. How the built pieces already serve this

Nothing here is speculative plumbing; it is what G0-G5 produced:

- **muniment** is the vault: content-addressed blobs plus named slots.
- **codicil** is each vault's edit history; **its fork primitive** is how vaults
  branch and how derivations cross them.
- **chartulary** is the graph inside and across vaults; its **two-ring taxonomy** is
  exactly the private-vs-shared distinction that lets vaults interoperate without
  leaking their app-internal relations.
- **scholia** is the interlingua: the semantic ring projected to RDF, the thing that
  makes cross-vault data intelligible.
- **stemma** is the lineage of what came from where, across forks and copies.
- **personae** is the index into the pool.
- **mere's G5 re-base** is what turns mere's own graph into a chartulary graph, so
  mere is a first-class citizen of its own substrate and can host the orrery.

## 5. Open questions

> **Amended 2026-07-18** by [one node, atomic facets, and the layer map](2026-07-18_one_node_facets_layer_map.md):
> 5.1 is retargeted (the kernel `Node` *dissolves* into `chartulary::Container`
> facet-by-facet; "web page" becomes a merecat-defined content class) and 5.4 is
> **resolved-redundant** (Container already is the generalized container node,
> including subgraphs via `GraphBearing`; there is no "above" to build).

1. **merecat extraction scope.** What leaves mere with the browser (genet host,
   rendering, page runtime, the web-runtime node facets: favicon, viewer routing,
   session restore, lifecycle) versus what stays as the orrery (the container graph,
   RDF, lineage, vault and persona indexing). The capability-trait work already drew
   this line once: the runtime facets sat on `Node` and did not participate in the
   generic capabilities. That line is the extraction seam.
2. **The orrery term.** Retire it, or narrow it to the visualization of the graph
   (the canvas) while mere names the graph itself. Mark's call; naming matters here.
3. **Vault / persona index surface.** How personae keys vaults, and the cross-vault
   query surface (a SPARQL-over-the-pool, a semantic search across vaults via
   sibylla, or both).
4. **Where the container model lives.** mere's generalized container node (the true
   content-addressed container: files, media, gemini-to-https pages, mere docs,
   subgraphs) is the richness above chartulary's generic `Container`. Does it live in
   mere, or become a shared mere-adjacent crate the apps also use directly.

## 6. Sequence

1. Finish G5: graph adoption (`chartulary::Graph<Node, EdgePayload>` behind a seam),
   history over the spine, analytics retarget. mere's graph becomes a chartulary
   graph.
2. merecat extraction: peel the browser off along the seam step 5.1 identifies.
3. The vault / persona layer: personae indexes muniment vaults; cross-vault semantic
   queries via scholia (and semantic search via sibylla).
4. Onboard the apps as vaults in the pool: woodshed (proven), then strophe, isometry,
   merecat.

The through-line: the substrate program was never just "extract mere's graph." It
was "build the orrery that unifies every app's graph-shaped data into one
semantically-intelligible pool of vaults." mere is that orrery.
