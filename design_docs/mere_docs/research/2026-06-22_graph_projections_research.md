# Graph Projections Research: five new ways to read the browsing graph

**Date**: 2026-06-22
**Status**: Research / design probe (with Mark). Five projection ideas, code-grounded but
uncommitted. Each graduates to its own `implementation_strategy/` plan if and when it is picked
up; this doc is the menu and the shared framing, not a build schedule.
**Code touched (read-only survey)**: `crates/graph/graph-kernel` (node + edge taxonomy),
 `crates/graph/node-lineage` *(historical citation)* <!-- doc-audit: historical-path --> (visit trees), `crates/orrery/cartography` *(historical citation)* <!-- doc-audit: historical-path --> (signal contract),
 `crates/forme` (lens + graphlet), `crates/platen` (projection assembly), `crates/meerkat` *(historical citation)* <!-- doc-audit: historical-path --> (host).

**Related**:

- [modular_integration_plan §1](../implementation_strategy/2026-06-02_modular_integration_plan.md)
  — the graph-rooted projection model. The graph is the sole root; orrery / workbench / gloss /
  apparatus / tiles are contingent projections; no projection becomes the root. Every idea below
  obeys this.
- [gloss_navigator_design](../design/2026-06-07_gloss_navigator_design.md) — the Navigator is one
  configurable summary surface (scope x form factor), never split. Three of the five are better
  framed as gloss lenses or orrery modes than as new panes, so the no-split rule holds.
- [graph_signals_layer_plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md)
  — the producer layer (communities / affinity / bridges / importance / embeddings). #5 below is a
  *consumer* surface of that layer, not a second producer; #4 borrows its importance signal.
- [node_representation_arrangement_plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md)
  — representation (a node's look) and arrangement (a layout) are orthogonal to which projection is
  reading the truth. A node's chosen form carries across all five.
- [command_registry_configurable_menus_plan](../../archive_docs/2026-09-02_retired_plans/2026-06-21_command_registry_configurable_menus_plan.md)
  — each projection is one registry command (`projection.trail`, `projection.claim-map`, ...),
  applicability-gated, palette- and script-reachable.
- [edge_system_audit](2026-06-13_edge_system_audit.md) — the kernel relation model is complete and
  mostly auto-populated; `assert_relation` / `retract_relations` are the write API a curation
  gesture calls; the Provenance family has zero consumers; the relation-kind picker is the one
  open creation gap.

---

## The frame: projection vs arrangement vs lens

Three words are easy to conflate, so they are pinned here.

- An **arrangement** is a layout *inside* the orrery: grid, radial, penrose, phyllotaxis, kanban,
  timeline, spectral. It positions the current nodes. The `crates/orrery/arrangements` *(historical citation)* <!-- doc-audit: historical-path --> registry
  owns these.
 - A **lens** (`forme::ProjectionLens`, `crates/forme/forme/src/lens.rs`) already enumerates which
  *topology* drives a view's hierarchy: `Traversal`, `Arrangement`, `Containment`, `Semantic`,
  `Recency`, `All`. A lens picks which edge family is load-bearing.
- A **projection** is a whole surface or mode that reads the graph a particular way. The orrery
  (spatial root), the workbench (tiled media), gloss (the configurable summary Navigator),
  apparatus (system inspector), and the roster (node list) are the projections that exist.

The five below are projections in that third sense. None lays the same nodes out differently and
calls it new; each exploits a primitive cluster the orrery currently flattens or ignores. Where one
is really a new mode of an existing surface (an orrery overlay, a gloss lens), it says so, so we do
not accidentally grow a second Navigator.

## The contract all five honor

1. **Read over truth, never a second store.** A projection derives from the kernel graph and the
   lineage visit tree; it does not hold its own authority. This is the lineage crate's standing
   rule (`node-lineage`: visits own the tree, edges are projected) and the projection model's root
   rule.
2. **No projection is the root.** Each is summonable and dismissable; the graph (session) outlives
   all of them.
3. **Curation writes back as new assertions.** When a projection lets you act (merge two nodes,
   accept a claim, mark a derivation), the write goes through the kernel's typed edge API
   (`assert_relation` / `retract_relations`, per the edge audit), append-only and provenance-bearing,
   not as projection-local state.
4. **Representation is orthogonal.** A node keeps its chosen form (tile / card / textured / shape)
   across every projection; the projection decides position, grouping, and which relations show, not
   what a node looks like.

---

## The five

### 1. Trail — replay the session, not the map

The orrery is a map of *what* you have; Trail is a record of *when and how* you moved. It is the
fuller surface behind the already-named "Trail" shellbar button (the Tier-A sidequest in
[lane0_sidequests_plan](../../archive_docs/2026-09-02_retired_plans/2026-06-16_lane0_sidequests_plan.md)) and the
substrate from
[node_navigation_lineage_wiring_plan](../implementation_strategy/2026-06-05_node_navigation_lineage_wiring_plan.md).

**Reads.** `node-lineage` `VisitRecord` (the branching visit tree with `parent` / `children` /
`created_at_ms` / `inbound` transition), `TransitionKind` (LinkClick / Back / Forward / TabSpawn /
Redirect / Restore), `OwnerRecord` cursors (per pane / tab / graph view), and the two derived
projections already implemented in `node-lineage/src/queries.rs`: `owner_branch_projection` (the
linear path plus the branches you did not take) and `edge_views` / `aggregated_entry_edges`
(traversal counts per pair). Time-window recall rides eidetic's `nodes_visited_in_window`.

**Surface.** A horizontal time ribbon of real sessions. Where you went back then forward to a
different link, the ribbon forks (the visit tree stores this today; nothing renders it). A scrubber
selects a window and the orrery dims to the nodes alive in it; a transport control steps the
session forward, following `inbound` transitions. The branches you abandoned hang as faint
alternates (`OwnerBranchVisit.alternate_children`).

**Lives as.** A new peripheral pane variant beside gloss / roster / apparatus (the slot in
[peripheral_panes_architecture](../technical_architecture/2026-06-06_peripheral_panes_architecture.md)),
driving the orrery's visible set. The "chronological graphlet" gloss already names is the swatch-scale
cousin; Trail is the full-scale scrubber.

**New vs. existing.** The lineage substrate and both derived projections exist; the time-ribbon
render, the scrubber, and the orrery dim-to-window are new. Distinct from the `timeline`
*arrangement*, which only sorts current nodes on an axis. Trail is a replay surface over the
branching visit authority, not a node layout.

**Gaps.** The live navigation path must actually populate the visit tree (the wiring plan's job);
the branch render and scrubber are the net-new UI.

### 2. Claim map — the research session as a stance graph

When you have been chasing a question, the orrery shows every node flat and every edge family at
once. Claim map keeps only the epistemic relations and arranges them by stance.

**Reads.** The Semantic sub-kinds that are invisible noise in the orrery today
(`edge_taxonomy.rs`): `Supports`, `Contradicts`, `Questions`, `Cites`, `Quotes`, `Elaborates`,
`ExampleOf`; plus `decay_progress` on the Semantic assertion (a faded claim renders thin),
`AgentDerived` (agent-asserted relations), and `NodeClassification` for the node badges.

**Surface.** Pick a focal question or claim node. The projection hides Traversal and Containment
entirely and partitions the rest: supports to one side, contradicts to the other, questions below,
cites threaded under each source. It is a debate map you can read and defend. The agent populates it
as it reads, since it already emits `AgentDerived` edges.

**Lives as.** An orrery mode (a stance-partitioned arrangement plus an edge-family filter), not a
new pane. It reuses platen's relation-level edge-visibility predicate (`Fn(&RelationView) -> bool`,
already used to hide / show edge families) and the reveal / derive machinery of
[graphlet_derivation_from_selection](../design/2026-06-13_graphlet_derivation_from_selection.md)
(select nodes, reveal latent edges, read the shape). The new piece is the stance partition layout,
kin to the `kanban` adapter's axis logic.

**New vs. existing.** The relation vocabulary and the edge-visibility filter exist. The stance
layout is new. This is the most novel of the five and the one that most rewards the rich Semantic
taxonomy that ingest and the agent already write.

**Gaps.** Most user-asserted semantic edges are `UserGrouped` today because the **discoverable
relation-kind picker** is the one open edge-creation gap (edge audit). Claim map is the surface that
makes that picker worth finishing: without a way to assert `Supports` vs `Contradicts` by hand, the
map is agent- and ingest-only.

### 3. Provenance trail — show your work

The inverse of the claim map. Not what argues with what, but what was made from what.

**Reads.** The entire `Provenance` family, which has zero consumers today (edge audit):
`ClippedFrom`, `ExcerptedFrom`, `SummarizedFrom`, `TranslatedFrom`, `RewrittenFrom`, `GeneratedFrom`,
`ExtractedFrom`; plus the eidetic content store (the stored bytes at each hop) and the open
`NodeProperty` literals.

**Surface.** Select an artifact you produced (a clip, a summary, an agent draft). The projection
walks the Provenance edges downward into a derivation tree: this summary was `GeneratedFrom` these
four pages, which were `ExcerptedFrom` these sources. Each hop opens to its stored bytes, so
attribution is verifiable, not asserted.

**Lives as.** A pane variant rendered through platen's existing Tree projection (the workbench
already proves Tree mode), or a gloss swatch at small scale. The render reuses the same `EdgeView`
projection shape `node-lineage` uses, retargeted from visit parentage onto Provenance edges.

**New vs. existing.** The family is fully modeled and the tree renderer exists; the projection that
walks Provenance edges is new. It pairs with the
[statement_kernel_brief](../technical_architecture/2026-06-19_statement_kernel_brief.md)'s
per-predicate lifecycle: Provenance is a durable-keep, accumulating predicate, exactly the kind of
recorded fact this surface reads.

**Gaps.** The real gap is upstream: almost nothing *asserts* Provenance edges yet (knot clips are
the nearest candidate). The clip / excerpt / summarize / agent-generate gestures must write the
family before there is anything to project. So this projection is a forcing function for wiring
those write paths, and is the least shovel-ready until they exist.

### 4. Facet matrix — a pivot table over the graph

The graph holds rich classifications and tags that are decoration in the orrery. The facet matrix
makes the history navigable by intent.

**Reads.** The kernel's **already-implemented** PMEST facet projection
(`graph-kernel/src/graph/facet_projection.rs`, pure, spec'd against a faceted-filter surface spec):
it derives per node a queryable facet map over domain, mime, in / out degree, edge kinds, traversal
count, frame memberships, UDC classes (tags plus `NodeClassification` values merged), `last_visited`,
and lifecycle.

**Surface.** Choose two axes (classification scheme on rows, domain or tag on columns). The grid
fills cells with the matching nodes. "Everything classified rust, from this domain, in the last 30
days." Click a cell to push that subset into the orrery or open it as a workbench split.

**Lives as.** A gloss lens (a tabular form factor alongside outline and swatch) for browsing, or a
dedicated pane for heavy analysis. The cell layout reuses the `grid` arrangement; the importance
signal from
[graph_signals_layer_plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md) can
rank within a cell.

**New vs. existing.** The facet projection is built and tested. The matrix UI (axis pickers, the
cross-tab, cell-to-subset action) is new. Distinct from the SPARQL query facet in
[graph_query_layer_plan](../implementation_strategy/2026-06-18_graph_query_layer_plan.md), which is a
text query language; the matrix is direct manipulation over the same truth. This is the cheapest of
the five: the kernel primitive already exists.

**Gaps.** A small amount of facet-value enumeration (the distinct values per scheme, to build the
axes) and the cell-render UI. No new kernel work.

### 5. Neighborhoods — named topic islands and their bridges

The discovery surface: what have you been into, and what connects interests you did not realize were
adjacent.

**Reads.** The full `cartography::IntelligenceSignals` contract (`cartography/src/signals.rs`):
`ClusterSet` (named clusters with confidence), `AffinityScores` (pairwise), `BridgeNodes` (community
connectors), `ImportanceWeights`, `NodeEmbeddings`. Plus `SameEntityAs` / `CanonicalMirrorOf` /
`DuplicateOf` and node `addresses` aliases to collapse duplicates so the map shows distinct things,
not distinct URLs.

**Surface.** The graph as named topic islands with drawn hulls, bridge nodes highlighted where two
islands touch, and a quiet suggestion channel ("these two clusters share one bridge; want to connect
them"). Renaming a cluster or accepting a suggested link writes back through the kernel edge API.

**Lives as.** An orrery overlay mode (hulls and bridge highlights over the spatial scene). This is
the important reconciliation: Neighborhoods is a **consumer** of the signals layer Mark planned the
same day, not a second producer. The signals plan owns producing communities / bridges / affinity
and the encodings (community as a ring, importance as size); Neighborhoods is the surface that
*names, draws as regions, and acts on* them. It needs the signals plan's consumer plumbing (the
`Projection::overlays` path that `project_orrery_strategy` currently discards, and an overlay
vocabulary with community / bridge kinds).

**New vs. existing.** The signal *contract* exists; the *producer* is the signals plan; the
named-region surface plus the connection-suggestion action are what Neighborhoods adds beyond
encodings. Distinct from the `semantic_embedding` and `SpectralLayout` *arrangements*, which only
place nodes at coordinates; Neighborhoods adds regions, bridges, and suggestions on top.

**Gaps.** Gated on the signals plan reaching P3 (background community) and P2 (overlay plumbing).
Until then the surface has nothing to draw.

---

## Map

| Projection | Primitive cluster (read) | Lives as | Adjacent plan / substrate | Readiness |
|---|---|---|---|---|
| Trail | `node-lineage` visit tree + transitions | new pane, drives orrery | lineage wiring; Trail button exists | substrate built, UI new |
| Claim map | Semantic argument sub-kinds + decay | orrery mode | graphlet-derivation; edge audit | vocab + filter built, layout new |
| Provenance trail | Provenance family + content store | pane / swatch (Tree) | statement-kernel; lineage EdgeView | family modeled, writers missing |
| Facet matrix | `facet_projection` + tags + classifications | gloss lens / pane | graph-query layer; facet spec | projection built, UI new |
| Neighborhoods | `IntelligenceSignals` + identity edges | orrery overlay | graph-signals-layer (consumer) | gated on signals producer |

## Shared substrate gaps and sequencing

Three substrate items unblock more than one projection, so they are worth naming once:

- **The relation-kind picker** (edge audit's one open creation gap) unblocks hand-authored Claim
  maps and any by-hand Provenance assertion.
- **The lineage write path** (the nav-lineage wiring plan) unblocks Trail.
- **The signals producer** (Mark's same-day signals plan) unblocks Neighborhoods and the
  importance ranking inside Facets.

Cheapest first, given the above:

1. **Facet matrix** — the kernel projection is built and tested; only the matrix UI is new, and it
   needs no upstream substrate.
2. **Trail** — the substrate and both derived projections exist; the time-ribbon and scrubber are
   the work, and the named button already anticipates it.
3. **Claim map** — reuses the edge-visibility filter and the reveal machinery; the stance layout is
   the net-new piece, and it pulls the relation-kind picker into usefulness.
4. **Neighborhoods** — rides the signals plan; arrives when that plan's P2 / P3 land.
5. **Provenance trail** — last, because the Provenance write gestures must exist before there is
   anything to walk.

## How they compose

These are five lenses on one truth, not five apps. The same node, the same selection, seen five
ways: where it sits in time (Trail), what it argues (Claim map), what it was made from (Provenance),
how it classifies (Facets), what it clusters with (Neighborhoods). A selection carried from the
orrery into any of them stays the same selection (shared focus, the gloss precedent). Each is one
applicability-gated command in the registry, so the palette, a script, the agent, and the context
menu all summon them by id. Representation choices ride along untouched.

## Open questions

- **Pane budget vs. the no-split rule.** Trail and Provenance want pane real estate; Claim map and
  Neighborhoods are orrery modes; Facets is a gloss lens. Is Trail a true new pane variant, or a
  gloss form factor (the chronological scope)? Lean: Trail earns a pane (its scrubber and transport
  are interactive, not a summary), the rest are modes / lenses.
- **Write-back scope.** Claim map (assert a stance), Neighborhoods (name a cluster, accept a link),
  and Provenance (confirm a derivation) all write. Confirm each goes through `assert_relation` with
  honest provenance (a human gesture is `UserGrouped`-grade or its semantic kind, an accepted
  suggestion records the signal that proposed it), never projection-local state.
- **Cluster identity over time.** A named Neighborhood must survive a recompute (the signals plan's
  stable-key concern). Does a user-named cluster pin to a member set, a centroid, or a tag?
- **Decay authority.** Claim map renders `decay_progress`, but the edge audit's thread 4 (who ticks
  decay, what reinforces it) is unsettled. Claim map is a reason to settle it.
- **Provenance writers.** Which gestures assert which Provenance sub-kind, and where (inker clip,
  agent generate, knot excerpt)? This is the precondition for projection 3.

## Grounding (code-verified 2026-06-22)

- **Node truth** (`graph-kernel/src/graph/node.rs`): `tags`, `classifications`
  (scheme / value / confidence / status / provenance), open `properties`, `last_visited`,
  `thumbnail_png`, `favicon_rgba`, `addresses` (Primary plus Alias claims), `lifecycle`
  (Active / Warm / Cold / Tombstone). All durable, all projectable.
- **Edge taxonomy** (`graph-kernel/src/graph/edge_taxonomy.rs`): six families
  (Semantic / Traversal / Containment / Arrangement / Imported / Provenance), each multi-family per
  edge; the deep `SemanticSubKind` and `ProvenanceSubKind` vocabularies the claim map and provenance
  trail read; `decay_progress` on Semantic; `RelationDurability` (Durable / Session).
- **Lineage** (`node-lineage/src/lib.rs`, `queries.rs`): `VisitRecord` branching tree,
  `TransitionKind`, `OwnerRecord`, and the built derived projections `owner_branch_projection`,
  `edge_views`, `aggregated_entry_edges`. Append-only; edges projected, never stored twice.
- **Facets** (`graph-kernel/src/graph/facet_projection.rs`): a pure PMEST facet map per node, built
  and tested (domain / mime / degree / edge-kinds / traversal-count / udc-classes / last-visited /
  lifecycle).
- **Signals** (`cartography/src/signals.rs`): `IntelligenceSignals` with ClusterSet / AffinityScores
  / BridgeNodes / ImportanceWeights / NodeEmbeddings; a snapshot contract, producer owned by the
  signals-layer plan.
- **Lens vocabulary** (`forme/src/lens.rs`): `ProjectionLens` already names Traversal / Arrangement
  / Containment / Semantic / Recency / All; the five projections extend this vocabulary rather than
  invent one.

## Progress

- 2026-06-22: **Created from a chat pitch Mark accepted ("I like 'em all").** Code-verified the
  graph primitives each projection reads (node fields, edge taxonomy, lineage visit tree, facet
  projection, signal contract, lens enum). Reconciled the five against the same-day
  [graph_signals_layer_plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md) (so
  Neighborhoods is a consumer surface, not a duplicate producer, and Facets borrows its importance
  signal) and the [gloss_navigator_design](../design/2026-06-07_gloss_navigator_design.md) (so the
  no-split rule holds: three of the five are orrery modes or gloss lenses, only Trail and Provenance
  earn panes). Noted the shared substrate gaps (relation-kind picker, lineage write path, signals
  producer, Provenance writers) and a cheapest-first order (Facets, Trail, Claim map, Neighborhoods,
  Provenance). No code; each projection spins out to its own `implementation_strategy/` plan when
  picked up.
