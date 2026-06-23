# Graph Signals Layer Plan: a producer layer feeding arrangements, encodings, and the gloss lens

**Date**: 2026-06-22 (rev 2, critique incorporated)
**Status**: Planning (with Mark). Phase A (kanban/timeline arrangements) landed; this plan is
the backbone for the rest. **Rev 2 corrects rev 1**, which collapsed producer lifecycle,
layout, force physics, and rendering into one "missing layer" — those boundaries are drawn
explicitly here.
**Code**: `kernel` (truth + neutral queries), a new `intel/signals` (computation + cache +
invalidation), `crates/orrery/cartography` (the snapshot contract), `crates/orrery/arrangements`
(analytic layouts), `crates/orrery/gyre` (live forces), `crates/platen` (assembly), `meerkat`
(lens config), `orrery`/gloss (consume projections + overlays).

The thesis holds: **one signal snapshot feeding arrangements, encodings, and the gloss lens.**
But the producer is a *lifecycle*, not a clock; affinity is a *force*, not a field; a spectral
layout is an *arrangement*, not a signal; and the consumer plumbing is largely unbuilt. Rev 1's
errors are corrected below, each verified against the code.

Sibling / converging docs:

- [node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md):
  the arrangement half of node-rep (Phase A = kanban/timeline lives there). **Decision 2 reserves
  node face color for activation state** — the encoding collision below.
- [graph_cluster_namespaces_brief](2026-05-10_graph_cluster_namespaces_brief.md): already states
  recomputing community decomposition on every insertion is impractical — the cache below answers it.
- [scriptable_field_regions_plan](2026-06-13_scriptable_field_regions_plan.md): gyre's
  `CouplingForce` is a **field** mechanism (per-node field sample), *not* the pairwise affinity
  force — the affinity force is new.
- [document_script_substrate_plan](2026-06-21_document_script_substrate_plan.md): the
  wasmtime-async lane is the **scriptable** path for user-authored analyses (below).

---

## What actually exists (verified 2026-06-22, correcting rev 1)

- **The basic graph algorithms are live, not asleep.** `kernel::graph::query` uses `dijkstra`
  (`hop_distances_from`), `astar` (`shortest_path`), `has_path_connecting` (`is_reachable`), and
  `kosaraju_scc`. The unused-import warning is **stale duplicate imports in `graph/mod.rs`**, not
  evidence the algorithms are unused. So path / reachability / SCC are done; the gap is the
  **advanced** analyses with no producer: centrality beyond degree (betweenness / PageRank),
  communities (Leiden / Louvain), articulation / bridges, pairwise affinity, embeddings.
- **`cartography::IntelligenceSignals` is a snapshot contract, never produced.** Its README scopes
  it to a *narrow snapshot* a strategy reads; every call site passes `::default()`. The missing
  piece is a producer, and it should live **outside** cartography (cartography keeps the contract).
- **`CouplingForce` is per-node field sampling.** It "evaluate[s] the field at each target node's
  position and apply[s] the response" over a `FieldDefinition` (scalar/vector). It is **not** a
  pairwise node-node attraction. Affinity needs a new gyre force.
- **`project_orrery_strategy` discards everything but positions.** It returns
  `projection.nodes.iter().map(|n| (n.node, n.position))`. No live path consumes
  `Projection::overlays`, and the overlay vocabulary has no community-color / path-highlight kinds.
  The gloss bypasses cartography entirely via `Orrery::minimap_geometry`. The consumer plumbing is
  the bulk of the work, not an afterthought.

## Ownership (drawn explicitly — rev 1 blurred these)

- **`kernel`** — graph truth + neutral queries (the live path/SCC/reachability already here).
- **`intel/signals` (new)** — signal *computation*, *invalidation*, *cache*, algorithm config.
  Owns the lifecycle. Produces snapshots.
- **`cartography::IntelligenceSignals`** — the narrow snapshot *contract* only (per its README).
  A produced snapshot is handed to a `ProjectionRequest`.
- **`platen`** — assembles `graph + signal snapshot + ViewIntent` into a `ProjectionRequest`.
- **`arrangements` / `gyre`** — analytic layouts (incl. a new `SpectralLayout`) and live forces
  (incl. a new affinity force).
- **`meerkat`** — lens configuration + channel arbitration (which encoding owns color, etc.).
- **`orrery` / gloss renderer** — consume the full `Projection` + overlays (not just positions).

## The signals + their lifecycle (a cache, not a clock)

A monolithic recompute-on-every-mutation is wrong: degree is cheap, betweenness / Leiden /
spectral are not, and tags / traversal / topology / content invalidate for *different* reasons.
A per-session **signals cache**:

- **A source generation / fingerprint** stamped on every snapshot.
- **Per-signal dirty bits**, invalidated by the relevant cause (topology vs tags vs content).
- **Cheap signals synchronous** (degree, articulation on small graphs) — computed inline on read.
- **Expensive signals debounced + backgrounded** (betweenness, Leiden, spectral) — never block a
  frame; the UI uses the last good snapshot until a fresh one lands.
- **Stale-result rejection**: a background result computed against an older generation is dropped.
- **Stable keys for async**: `NodeKey` is snapshot-local, so a backgrounded result must be
  generation-tagged or computed against stable UUIDs and rebound on arrival.

**Where "background" runs:** first pass = a native background thread / the armillary actor
substrate (the same off-thread discipline gyre's physics uses). The **wasmtime-45 async** lane
(being implemented now) is the *scriptable* path: a user-authored analysis as an async WASM
component (a registered command per the command-registry plan) that the cache schedules like any
other expensive signal. So the cache's "expensive/background" slot is extension-ready — native
now, scriptable-async later — but neither is needed for the cheap first signals.

## Consumers

1. **Arrangements** — community → kanban columns (replace Phase A's host-axis); the rest below.
2. **Encodings** — importance → node size (generalize size-by-degree to a pluggable metric);
   community → **a halo / ring, not face color** (face color is reserved for activation state,
   node-rep Decision 2 — or an explicit mode that *replaces* activation color); bridge / path →
   highlight. **This needs real plumbing**: `project_orrery_strategy` must stop discarding the
   projection, the overlay vocabulary must gain community / path kinds, and the renderer must
   consume them. Encoding *arbitration* (which signal owns which channel) is a meerkat concern.
3. **The gloss swatch** — a configurable second lens that **consumes its own `ProjectionRequest` /
   `Projection`** (not a parameterized `minimap_geometry`): its own arrangement + encoding + scope,
   independent of the main orrery, to preview a lens before applying it.

## The two semantic adapters, corrected

- **`semantic_edge_weight` → a new gyre affinity force (not a coupling, not kept streaming
  forever).** Affinity is recomputed on mutation (a signal, not per-frame). The layout is a **new
  pairwise gyre force** (an `AffinitySpring` / weighted-edge attraction) applied by gyre's running
  loop — `CouplingForce` cannot serve (it samples a field per node, not a pair). **Keep the
  existing streaming adapter until the gyre force reaches parity**, then retire it. In the settings
  menu this is a toggle on force-directed ("cluster by affinity").
- **`SpectralLayout` is an arrangement; `semantic_embedding` stays content.** A 2D spectral /
  topology projection *is a layout result*, so it belongs directly in `arrangements` as
  `SpectralLayout` — feeding it through `IntelligenceSignals::embeddings` for `semantic_embedding`
  to copy is a circular layer. Reserve `semantic_embedding` for **statistical / content** coords
  from the `intel/embed` lane (a local text model later). node2vec is not inherently deterministic
  or 2D without extra policy, so spectral (deterministic, directly 2D) is the first arrangement.

## Phases (revised sequence — boundaries first)

- **P0 — kanban / timeline (done, node-rep).** Host-derived axes, first pass.
- **P1 — cheap signals + the cache contract.** `intel/signals` with the generation/dirty-bit cache,
  degree + articulation computed synchronously, the snapshot handed to `project_orrery_strategy`
  (replacing `::default()`). Done when a strategy receives a real (if cheap) snapshot.
- **P2 — full `Projection` plumbing + encoding arbitration.** Stop discarding the projection; extend
  the overlay vocabulary (community, path); the renderer consumes overlays; meerkat arbitrates the
  channels (size by importance, community as a ring). Done when one cheap signal drives a visible
  encoding end to end.
- **P3 — background community analysis.** Leiden / Louvain on the background lane with debounce +
  stale-rejection; community → kanban columns + the ring encoding. Done when community survives
  mutation without a frame stall.
- **P4 — the affinity force.** A new gyre pairwise force from the affinity signal; the streaming
  adapter retired once it reaches parity. Done when affinity clusters at gyre's cost.
- **P5 — spectral arrangement (+ content embedding later).** `SpectralLayout` in `arrangements`;
  `semantic_embedding` reserved for the content lane. Done when spectral lays the graph by topology.
- **P6 — the independent gloss projection.** Gloss builds its own `ProjectionRequest`; the swatch
  gains arrangement + encoding + scope controls. Done when gloss shows a different lens than the
  main view.

## Findings (code-verified 2026-06-22)

- `kernel::graph::query` runs `dijkstra` / `astar` / `has_path_connecting` / `kosaraju_scc` live;
  the unused warning is stale `graph/mod.rs` imports (`cargo check -p kernel` confirms).
- `gyre::CouplingForce::apply` samples a `FieldDefinition` at each target node — field, not pair.
- `platen::project_orrery_strategy` returns positions only; `Projection::overlays` has no consumer,
  and the overlay vocabulary lacks community / path kinds.
- `cartography/README` scopes `IntelligenceSignals` to a snapshot contract; producer belongs elsewhere.
- node-rep Decision 2 reserves face color for activation state (the community-color collision).
- the cluster-namespace brief already rejects recompute-on-every-insertion (the cache answers it).

## Progress

- 2026-06-22: **Rev 1 drafted, then corrected (rev 2) against an agent critique Mark relayed.** Rev 1
  had the right unifying thesis but four real errors, each verified and fixed: (1) the premise
  overstated the gap — path/SCC/reachability are live in `query.rs`; the real gap is advanced
  analyses; (2) the recompute model was a monolithic clock — replaced with a per-signal cache
  (generation + dirty bits + cheap-sync / expensive-background + stale-rejection + stable-key
  rebinding); (3) affinity-as-`CouplingForce` was wrong (that samples a field per node) — it needs a
  new gyre pairwise force, keeping the streaming adapter until parity; (4) spectral-as-embedding-signal
  was circular — spectral is a `SpectralLayout` arrangement, `semantic_embedding` reserved for content
  coords. Plus: the consumer plumbing (full `Projection` + overlays, gloss-via-`Projection`) is the
  bulk of the work, and community color collides with activation color (use a ring). Ownership redrawn
  (`intel/signals` owns the lifecycle; cartography keeps the contract). The wasmtime-45 async lane is
  the scriptable-analysis path for the cache's background slot, not the first-pass native one. No code
  landed (the meerkat build was red on an unrelated in-flight `palette`→`sheet` refactor).
