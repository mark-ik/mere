# Borrowed ideas through Mere's spatial / p2p / agentic lens

**Date**: 2026-06-25
**Status**: Curated idea harvest. Ideas worth borrowing from adjacent projects
(spatial canvases, local-first / p2p stacks, agentic tools), filtered through
Mere's three distinguishing properties and through Mark's curation of a longer
brainstorm. This is the architecture-level companion to the grammar-level
[carve syntax harvest](../implementation_strategy/2026-06-24_djot_editor_knot_nodes_plan.md#syntax-harvest-from-carve-extensions-over-djot).
Each item carries its source project, Mark's framing, and a status (net-new,
already-scoped-elsewhere, or dependency-gated). Same discipline as carve: take
the idea, fit it to Mere's substrate, do not import a foreign product's shape.

The richest ideas sit at the intersections of spatial, p2p, and agentic, because
little else in the landscape lives there. Grouped by axis, then the crossings.

## Spatial

### Plex re-centering (TheBrain) [open design question]

The borrow: click a node and the surface re-homes on it with its relations fanned
around it, so focus follows traversal. This is the Navigator's configurable-scope
idea expressed as an interaction. **Mark's framing:** a "center" context-menu
action that either preserves the force-directed nature but re-lays-out the nodes,
or moves them physically. Which one?

Current leaning is a spectrum of three rungs, escalating in how much they disturb
hand-placed positions, exposed as a strength (per the project's configurability
stance):

1. **Camera-center, nodes stay put.** Pan and zoom the camera to frame the node
   and its neighborhood, dimming non-neighbors through LOD. Cheapest, fully
   reversible, preserves all spatial memory. The least-destructive default, and a
   view operation rather than a mutation.
2. **Soft radial bias on the sim.** Pin the target at center and add a force term
   that pulls neighbors toward concentric target radii by graph distance, then let
   physics animate the settle. Keeps the organic force-directed feel and the
   existing inertia while borrowing TheBrain's depth-ring legibility as a soft
   constraint, and it relaxes back when released. The "gather around me" gesture.
3. **Hard radial relayout.** Deterministic concentric rings by BFS depth, nodes
   tweened to their targets. Maximally legible and most TheBrain-like, but it
   overwrites hand-placed positions, so it sits behind a modifier or setting.

Recommendation: ship rung 1 as the default "center" (it rides existing camera and
LOD and mutates nothing), offer rung 2 as an explicit "gather," and defer rung 3.
This keeps faith with the graph-canvas-as-infinite-document stance, where camera
moves are preferred over destructive node moves.

Other spatial borrows from the brainstorm (semantic zoom into a node, proximity as
a soft relation, saved viewports) were not carried into this curation.

## P2P

### Cambria schema lenses (Ink & Switch)

Bidirectional lenses let peers on different schema versions read each other's data.
Mere's engrams are schema plus data (alembic `SchemaRef`), and over federation
peers will drift engram-schema versions, so lenses are the principled alternative
to lockstep upgrades. **Net-new.** Conceptual home is the engram layer, the
[alembic implementation plan](../implementation_strategy/2026-06-24_alembic_implementation_plan.md).

### Capability-scoped subgraph sharing [p2panda-dependent]

An object-capability token that says "this peer may read this subgraph, revocably,"
with no central server. This is the sharing gesture the federation tiers need, and
it dovetails with Tessera as the trust receipt. **Mark's framing:** p2panda-dependent.
It rides the persona and federation work
([persona transport unlinkability](../implementation_strategy/2026-06-25_persona_transport_unlinkability_plan.md),
[persona wallet carry](../implementation_strategy/2026-06-25_persona_wallet_carry_layer_plan.md),
[actor constellation](../implementation_strategy/2026-06-03_actor_constellation_plan.md)),
adopted as a layered borrow there rather than standalone.

## Agentic

### MCP-native graph

Expose the graph (nodes, edges, queries, crawl, clip) as MCP tools and resources,
and let internal agents consume MCP servers. Given the ecosystem this is the
natural agent boundary, and it makes external agents first-class citizens without
bespoke glue. **Net-new.** Connects to the document-script and harness lanes
([document script substrate](../implementation_strategy/2026-06-21_document_script_substrate_plan.md),
[local models harness brief](2026-06-24_local_models_harness_brief.md)).

### Speculative branches plus provenance (Patchwork / Upwell)

Agents propose changes in an overlay you review and merge, a pull request for your
graph. It is CRDT-friendly and it is the safety valve for letting agents act at
all. **Mark's pairing:** speculative branches plus provenance on every agent
mutation, where each agent action asserts a `ProvenanceSubKind` edge (who, from
what, when). The branch is the review surface, provenance is the audit substrate,
and together they make agency safe to permit. **Net-new:** the provenance type
already exists; the review surface and the assert-on-every-mutation rule are the
work.

## Crossings

### Spatial x agentic: live query regions

A bounded area of the canvas that materializes a query and stays live (Tinderbox
agents, made spatial). **Mark's refinement:** the references and results are
related, and selectively connected depending on the graph's edge config (which
edges count). So the region's membership is the query, while its drawn connections
are filtered by the active edge lens, showing only edges that count under the
current configuration. That keeps the region honest. Ties to the
[graph signals layer](../implementation_strategy/2026-06-22_graph_signals_layer_plan.md)
for the edge config. The `=query` polyglot block is the in-note form of the same
idea, a live query rendered inline, which slots into the
[knot editor](../implementation_strategy/2026-06-24_djot_editor_knot_nodes_plan.md)
fence architecture.

### P2P x spatial: multiplayer presence [already scoped]

Peer cursors and viewports on the shared graph (Figma, but p2p). **Mark:**
discussed and scoped already, see the
[operator presence overlay plan](../implementation_strategy/2026-06-25_operator_presence_overlay_plan.md).
Recorded here only for completeness; no new work implied.

### All three: the living document

A knot node that is positioned, CRDT-synced, capability-shared, and holds live
queries and scripts. This is the thing Ink & Switch keeps circling (Webstrates
plus Potluck plus Automerge), and Mere is unusually close because the substrate
already exists. **Mark's bundle of the big constituents, each deferred and each a
feature in itself:**

- Live Potluck / Inkbase blocks: a script fence's output renders inline and stays
  manipulable, the
  [knot editor](../implementation_strategy/2026-06-24_djot_editor_knot_nodes_plan.md)
  script-block endgame over `evaluate_blocks`.
- Rich-text CRDT for collaborative knot editing: Peritext is the reference.
- Local embeddings for semantic neighbors: reachable via Burn-wgpu, though a whole
  feature; surfaces related-but-unlinked nodes as a soft relation.

## Priorities

- Cheap and high-fit: the `=query` block, provenance-on-agent-actions, the rung-1
  camera "center."
- Load-bearing for federation, painful to retrofit: Cambria lenses,
  capability-scoped sharing.
- Big, deferred with intent: speculative branches, live Potluck blocks, the
  Peritext CRDT, local embeddings.

## Cross-references

- [carve syntax harvest](../implementation_strategy/2026-06-24_djot_editor_knot_nodes_plan.md#syntax-harvest-from-carve-extensions-over-djot):
  the grammar-level companion.
- [graph signals layer plan](../implementation_strategy/2026-06-22_graph_signals_layer_plan.md):
  edge config and lens, for live query regions.
- [operator presence overlay plan](../implementation_strategy/2026-06-25_operator_presence_overlay_plan.md):
  multiplayer presence (already scoped).
- [alembic implementation plan](../implementation_strategy/2026-06-24_alembic_implementation_plan.md):
  engram schema layer (Cambria home).
- [persona transport unlinkability](../implementation_strategy/2026-06-25_persona_transport_unlinkability_plan.md),
  [persona wallet carry](../implementation_strategy/2026-06-25_persona_wallet_carry_layer_plan.md),
  [actor constellation](../implementation_strategy/2026-06-03_actor_constellation_plan.md):
  persona and federation, for capability-scoped sharing.
- [document script substrate](../implementation_strategy/2026-06-21_document_script_substrate_plan.md),
  [document script followons](../implementation_strategy/2026-06-23_document_script_followons_plan.md):
  agentic scripting, for MCP and live blocks.
- [local models harness brief](2026-06-24_local_models_harness_brief.md): local AI
  lane, for embeddings and agents.
- [djot editor and knot nodes plan](../implementation_strategy/2026-06-24_djot_editor_knot_nodes_plan.md):
  knot editor, for the `=query` block and live blocks.
