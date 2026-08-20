# Meaningful physics: signals, presets, and the ambient pulse

Builds on the [physics-scenes plan](2026-06-22_physics_scenes_and_tangibility_plan.md) (the mechanics:
scenes, fluid, fields, emitters, ambient sims, tangibility). That work made the orrery *delightful*.
This plan is the next phase: making the physics *meaningful*, so a behavior either carries a true
quantity (you read it at a glance) or affords a real action (pushing on it changes your work).

The test for every physical behavior: **does it carry a true quantity, or afford a real action?** If
not, it is eye candy. Two halves pass that test, both backed by real signals (never a faked
animation, per the no-placebo rule):

- **Ambient pulse** — the backdrop sims read the system's live state, so the field becomes a calm
  instrument you glance at, not a screensaver.
- **Per-node truth** — a node body's weight / size / heat reflects facts about *that node*, so the
  layout itself tells you what matters. (Mark's priority: surface the nodes, not just system state.)

## Signal inventory (code-verified 2026-06-24)

What we can feed, grounded in the actual surfaces. Split by whether a real producer exists *today*.

### Real now — per-node

- **Content / fetch state** ([`meerkat::fetch::ContentState`](../../../crates/meerkat/src/fetch.rs)):
  per node `Loading | Ready(Fetched) | Failed(String)` (plus a `tag()` → 0 none / 1 loading / 2 ready
  / 3 failed). The live "this node is working / done / errored" pulse. Aggregates to active-fetch and
  failed counts (a system signal too).
- **Degree** ([`graph-kernel` query](../../../crates/graph/graph-kernel/src/graph/query.rs)):
  `out_neighbors` / `in_neighbors` per node → in/out degree. Cheap, always available.
- **Favicon presence / node state**: `set_node_favicon`, `set_node_states` already flow per-node.
- **Recency**: derivable from the per-node nav lineage / history (within-node history exists; needs a
  last-touched timestamp surfaced).

### Real now — system

- **Sync** ([`SyncStatus`](../../../crates/meerkat/src/sync.rs)): per lane `syncing: bool`,
  `ops_received`, `last_activity_ms`. The sync indicator is explicitly "a real operation count ... no
  placebo." Number of syncing lanes, total ops, recency.
- **Observability spine** ([`observability.rs`](../../../crates/meerkat/src/observability.rs)): a
  capped buffer of diagnostics / actor / probe / trace records with severities. Yields a recent event
  rate and a recent-error count; actor `started` / `ended` records = operation activity.
- **Live operations** (the steward): `steward_rows()` enumerates live operations + the graph count.
- **Graph size**: `node_count` / `edge_count`.

### Contract exists, producer pending — per-node + per-edge

[`cartography::signals::IntelligenceSignals`](../../../crates/orrery/cartography/src/signals.rs) is
already a narrow contract carrying exactly the richer per-node truth, consumed today by the
`arrangements` layout adapters:

- **`ImportanceWeights::lookup(node) -> Option<f32>`** — per-node importance. The natural driver for
  weight / size.
- **`AffinityScores::lookup(from, to) -> Option<f32>`** — per-edge affinity. The natural driver for
  edge-spring tension.
- **`ClusterSet` / `Cluster`** — groupings (drive a clustering force / tint).
- **`BridgeNodes`** — structural bridges (highlight / anchor).
- **`NodeEmbeddings::lookup(node) -> Option<(f32, f32)>`** — 2D embedding (seed positions / a field).

These are `Option` by design: a strategy that lacks a signal ignores it. Until an intelligence
*producer* runs, importance / affinity read `None` and the physics that depends on them sits quiet.
We do not fake them. (Degree is a real, cheap stand-in for importance in the meantime.)

**Boundary with the graph signals layer.** Producing these graph-structure signals (the `intel/signals`
cache, the community / centrality / affinity computation) and their *non-physics* encodings (size,
arrangements, the gloss lens, a new affinity force) is owned by the
[graph signals layer plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md). This plan does **not** re-derive
them. It owns (1) the **runtime / system** signals above (content / sync / ops / observability, which
graph_signals does not cover) and (2) the **physics-binding layer** that maps *any* signal, graph or
runtime, onto a physical parameter. When graph_signals produces importance / affinity, this plan
consumes them through that binding layer.

## Architecture: a mapping over seams that already exist

Most of this is wiring real signals into parameters that are already there. Two pieces:

### Per-node truth reuses the node-material / size seams

The orrery already exposes per-node physical seams cartography drives:
[`set_node_material`](../../../crates/orrery/orrery/src/lib.rs) /
`apply_cartography_materials` (restitution / friction / **density** = mass),
`apply_cartography_sizing`, `set_node_states`, and the gyre `Simulation::set_node_materials` /
`set_node_colliders` re-apply-to-live-bodies path. So "weight = importance," "size = degree," "heat =
content state" is a **mapping table** (signal → `NodeMaterial` / size) pushed through these seams on
signal change. No new physics; a new binding layer.

### Ambient pulse adds a small metrics feed

A new `AmbientMetrics` snapshot (a handful of real counts / rates) + an
`AmbientSim::set_metrics(&AmbientMetrics)` trait method (default no-op); meerkat gathers it from the
sources above and pushes it into the active sim at **low frequency**. One pulse, many dial faces:
each sim interprets the same metrics in its idiom (n-body body count = active ops; sand source rate =
ingest throughput, dune = backlog; Game of Life density = event rate; particle-life species
populations = the item mix). The data underneath is one honest thing; the sim is the chosen gauge.

## Design commitments (from the design conversation)

- **Node physics presets.** Named, reusable physical profiles (a `NodeMaterial` + size + behavior
  bundle), so a graph has *predictable* physics expectations and users can author + reuse their own.
  Two binding modes: (1) the user picks which **quantity drives which physical parameter** (e.g.
  importance → density), and (2) **conditional presets** — when a condition holds (a query, a tag, a
  state), the node adopts a preset. This is the configurable face of the mapping layer above.
- **Collision as an ephemeral relation.** An optional **collision edge** between two nodes that grows
  fractionally stronger each time they collide and fades with time; a gesture substantiates it into a
  real relation ("knock twice to be sure"). Display-only until substantiated.
- **Safety default: physics events × graph truth, confirmation by default.** Any physics gesture that
  would change *graph truth* (delete, tag, relate, archive via a scene-prop sink) requires
  confirmation by default, behind a single toggleable option. Functional scene props obey it. Physics
  may *suggest* truth changes freely; it commits them only through confirmation.
- **Tags bestow physical traits, scriptably.** A tag can carry physical characteristics, defined in a
  script lane (rhai today; lua / js / rune / etc. as the scripting map allows). Configurable
  **autotagging** becomes important here (tags are how presets attach at scale).

## Efficiency (the pulse must not starve the task it reports)

Mark's caution: do not choke the system with simulation when it is mid-heavy-task. Principles:

- **Low-frequency metric push** (a few Hz), not per-frame; the sources update slowly anyway.
- **Per-node material updates only on change**, not every frame (the seams are re-apply-on-set).
- **Cap sim cost**: bounded particle / cell counts; the O(n^2) sims stay small.
- **Self-throttle under load**: an "active ops high" metric can dial the ambient sim's own tick rate
  down, so the busier the system, the calmer (and cheaper) the backdrop. The rapier actor already
  parks when idle (`wants_continuous_tick`); the ambient sim should too under pressure.

## Phased slices (real-now signals first)

1. **System pulse MVP.** `AmbientMetrics { active_fetches, failed, syncing_lanes, ops_rate,
   node_count, edge_count }` + `set_metrics`; wire one sim (n-body body count = active operations) end
   to end. Cheap, real now, proves the feed.
2. **Per-node truth MVP.** Per-node content state → node heat (Loading pulses, Failed warns, Ready
   calm) via the material / size seam. Real now, per-node, the priority steer.
3. **The mapping layer + presets.** A configurable signal → parameter table and named presets;
   conditional presets (tag / query / state → preset).
4. **Consume the graph signals layer.** Once
   [graph_signals_layer_plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md) produces `ImportanceWeights` /
   `AffinityScores` / clusters, bind them as physical parameters (weight = importance, tension =
   affinity, cluster forces) through the preset / mapping layer. That plan owns producing them + the
   size encoding + the affinity force; this plan owns the physics-binding surface that maps them
   (alongside the runtime signals) onto materials. Degree is the stand-in until then.
5. **Collision-relations + scriptable tags** (each optional, behind the confirmation default).

## Progress

- 2026-06-24: **Plan written; surfaces scouted, signals scoped.** Read the meerkat observability /
  sync / fetch / steward surfaces, the graph-kernel node + query API, the orrery per-node seams, and
  the cartography `IntelligenceSignals` contract. Key findings: the per-node physical seams already
  exist (`set_node_material` / `apply_cartography_*`), so per-node "meaningful physics" is a mapping
  layer, not new physics; `IntelligenceSignals` is the right home for richer per-node quantities
  (importance / affinity / clusters), consumed by arrangement adapters but awaiting a live producer;
  content state + degree + sync are the strongest real-now signals. No code yet.
