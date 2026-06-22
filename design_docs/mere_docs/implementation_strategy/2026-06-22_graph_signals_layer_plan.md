# Graph Signals Layer Plan: one analysis layer feeding arrangements, encodings, and the gloss lens

**Date**: 2026-06-22
**Status**: Planning (with Mark). Phase A (kanban/timeline arrangements) landed; this plan is
the backbone for the rest.
**Code**: `crates/orrery/cartography` (the signals types), `crates/orrery/arrangements`
(the adapters), `crates/orrery/gyre` (the coupling force), `crates/orrery/orrery` (recompute
hook, encodings), `crates/meerkat/` (the gloss lens, the settings exposure), petgraph.

The orrery already *imports* petgraph's `astar` / `dijkstra` / `has_path_connecting` /
`kosaraju_scc` and never uses them (the build warns), and `cartography::IntelligenceSignals`
(clusters, affinity, bridges, importance, embeddings) exists as a type and is **never
computed** (every adapter gets `::default()`). So we are not missing the machinery, we are
missing the layer that *runs* it. Build that one layer and three features get richer at once:
arrangements, visual encodings, and the gloss swatch.

Sibling / converging docs:

- [node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md):
  the arrangement half of node-rep. This plan is the signal substrate its semantic arrangements
  consume; Phase A (kanban/timeline) lives there, enriched here.
- [object_card_plan](2026-06-21_object_card_plan.md): the per-object card surfaces per-node
  config; the encodings here (size, color) are scene-wide defaults the card overrides per node.
- [scriptable_field_regions_plan](2026-06-13_scriptable_field_regions_plan.md): gyre's
  `CouplingForce` (the field-well mechanism) is the same seam the affinity coupling rides.

---

## The thesis: a signals layer, recomputed on graph mutation, with three consumers

A **graph signals layer** computes analyses over the kernel graph and caches them, recomputing
only when the graph changes (a new node / edge), never per frame. The analyses are cheap,
graph-local, and host-side:

- **Centrality / importance** — degree (have it), then betweenness / PageRank (petgraph or a
  small impl). → `IntelligenceSignals::importance`.
- **Communities** — Leiden / Louvain over the undirected graph. → `IntelligenceSignals::clusters`.
- **Bridges / articulation** — structural cut nodes/edges (petgraph). → `IntelligenceSignals::bridges`.
- **Affinity** — per-pair semantic weight (edge weight + shared tags + co-citation, first pass;
  content similarity later). → `IntelligenceSignals::affinity`.
- **Structural embedding** — a 2D layout from topology alone (spectral / Laplacian eigenmaps,
  or node2vec), recomputed on change. → `IntelligenceSignals::embeddings`.
- **Paths** — on demand (a selected pair) via `dijkstra` / `astar`. → a transient highlight.

This layer feeds **three consumers**:

1. **Arrangements** (the layout dimension) — community → kanban-by-cluster columns; affinity →
   the gyre coupling (below); structural embedding → `semantic_embedding`.
2. **Encodings** (the visual dimension) — importance → node size (size-by-degree generalizes to
   size-by-centrality); community → node color; bridge → a highlight; a path → a traced route.
3. **The gloss swatch** (a configurable second lens) — surface the analysis (a community legend,
   a centrality ranking, the active path) and let the swatch show any arrangement + encoding
   scoped to a subgraph.

## The semantic arrangements, reframed as signal consumers

The two "dormant" semantic adapters stop being bespoke machinery once the signals layer exists:

- **`semantic_edge_weight` is an affinity coupling, not a streaming actor.** It reads
  `IntelligenceSignals::affinity`. The affinity is a *signal* recomputed on graph mutation
  (cheap, only on change), **not** per frame. The layout is then a **gyre coupling**: feed the
  affinity in as an attraction force through gyre's existing `set_coupling_forces` seam (the same
  one fields use), and gyre's running loop does the per-frame stepping. No second physics loop,
  no actor. So in the settings menu it is a **toggle on force-directed** ("cluster by affinity"),
  not a separate layout pick. The streaming `step` adapter and its `step_with` dispatch are
  retired for this path.
- **`semantic_embedding` uses a structural embedding first.** It places nodes at precomputed
  `IntelligenceSignals::embeddings` coords. Produce those with a **structural** embedding
  (spectral / node2vec over the graph) — graph-only, deterministic, no model, recompute on
  change. A **content** embedding (a local text model via the Burn-wgpu / Candle ML lane → UMAP/PCA
  to 2D) is a drop-in upgrade later: the adapter does not care which produced the coords.

## The gloss swatch as a configurable lens

The gloss minimap (`Orrery::minimap_geometry`) draws one fixed view. Make the swatch a real
second lens, configurable on three axes, independent of the main orrery:

- **Arrangement** — force-directed / kanban / timeline / semantic, so the swatch is a
  preview/compare (the spine's "two projections of one arrangement"): try a lens here before
  applying it to the main view.
- **Encoding** — color by community, size by centrality, highlight bridges / a path (the signals
  layer surfaced).
- **Scope** — whole graph / a neighborhood / the selected node's cluster.

The settings menu *sets* the main view's arrangement; the gloss *previews* arrangements +
*surfaces* the analysis. It is the low-stakes home for the whole signals layer.

## Phases (done-conditions, not dates)

- **P0 — Phase A arrangements (done).** kanban + timeline dispatched + pickable (graph-only axes
  first pass). See the node-rep plan.
- **P1 — The signals layer + recompute hook.** A host-side `GraphSignals` computed on graph
  mutation (hook the orrery's `reconcile_derived` / the kernel mutation seam), caching an
  `IntelligenceSignals`. Start with the cheap high-leverage two: **importance** (a centrality)
  and **communities** (Leiden/Louvain). Wire `IntelligenceSignals` into `project_orrery_strategy`
  (replace the `::default()`). Done when the strategies receive real signals.
- **P2 — Encodings from signals.** Size-by-centrality (generalize size-by-degree to a pluggable
  metric) + color-by-community, as scene toggles (per the node-rep Decision 5 / styling-lens).
  Done when a node's size + color can be driven by a signal.
- **P3 — Affinity coupling (`semantic_edge_weight`).** Derive affinity on mutation; feed it as a
  gyre coupling; expose as a "cluster by affinity" toggle on force-directed. Done when affinity
  visibly clusters and costs nothing per frame beyond gyre.
- **P4 — Structural embedding (`semantic_embedding`).** A spectral / node2vec 2D embedding
  recomputed on mutation → `embeddings`; dispatch the adapter. Done when the embedding lays the
  graph out by topology. (Content embedding defers to the ML lane.)
- **P5 — The gloss configurable lens.** The swatch gains arrangement + encoding + scope controls,
  independent of the main orrery. Done when the gloss can show a different arrangement/encoding
  than the main view.

## Findings (code-verified 2026-06-22)

- **petgraph algorithms are imported and unused.** meerkat imports `astar`, `dijkstra`,
  `has_path_connecting`, `kosaraju_scc` (the build warns them unused) — the path/SCC primitives
  are already a dependency, just not run.
- **`IntelligenceSignals` is defined but never computed.** `cartography::IntelligenceSignals`
  (`signals.rs`: clusters / affinity / bridges / importance / embeddings, with `AffinityScores`
  pair lookups + `NodeEmbeddings` coord lookups) is real, but `project_orrery_strategy` and the
  adapters all pass `::default()`. The signals layer is the missing producer.
- **The coupling seam exists.** gyre's `CouplingForce` + `Simulation::set_coupling_forces` (used
  by field regions, rebuilt on mutation via `rebuild_coupling_forces`) is exactly the seam the
  affinity coupling rides — recompute-on-change, applied by the running loop. No new per-frame
  path needed.
- **The adapters split by integration, not difficulty.** kanban/timeline are one-shot
  `LayoutStrategy::project` over `axis_values` (Phase A). `semantic_embedding` is one-shot over
  `embeddings`. `semantic_edge_weight` is the only streaming one (`step` / `step_with`) — and the
  reframe drops that path in favor of the coupling.
- **The gloss draws one view.** `Orrery::minimap_geometry` returns positions + edges for the
  swatch; it is not arrangement- or encoding-parameterized yet. P5 makes it a lens.

## Progress

- 2026-06-22: **Plan drafted (with Mark).** Came out of scoping the dormant arrangements: Mark
  asked to wire them all, then pushed past the wiring into the embedding-model choice, the
  streaming-vs-coupling question, surfacing petgraph, and the under-used gloss panel. The
  unifying answer is one signals layer (recompute-on-mutation) feeding arrangements + encodings +
  the gloss lens; the semantic arrangements become consumers (affinity = a gyre coupling, not a
  streaming actor; embedding = structural now, content later). Phase A (kanban/timeline) landed in
  the node-rep plan; this plan carries P1–P5. (Authored while the meerkat build was red on an
  unrelated in-flight `palette`→`sheet` refactor, so no code landed here yet.)
