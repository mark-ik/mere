# Graph Signals Layer Plan: a producer layer feeding arrangements, encodings, and the gloss lens

**Date**: 2026-06-22 (rev 3, 2026-06-24: grounded + decisions settled, building P1)
**Status**: Planning + **starting P1** (with Mark). Phase A (kanban/timeline arrangements) landed;
P1-P6 open. **Rev 2 corrects rev 1**; **rev 3 (2026-06-24)** grounds the cache + channels against
the live tree, settles the open decisions (edges-first, multi-edge multiplicity, size precedence,
the two-layer seam), and corrects two stale claims (the overlay vocabulary mostly already exists;
the ring channel is not built). See the Decisions + grounding section.
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
- [meaningful_physics_signals_plan](2026-06-24_meaningful_physics_signals_plan.md): the
  **physics-side consumer**. It maps runtime signals (content / sync / observability) *and* the
  graph-structural signals this plan produces onto physical parameters (material / size) via
  configurable presets. Clean seam, confirmed 2026-06-24: this plan **produces** the structural
  signals + their **non-physics** encodings (size, arrangements, gloss lens, the affinity force);
  that plan **consumes** them + runtime signals and binds both onto physics. Two layers of
  consumption, one producer.
- [gloss_outline_lens_plan](2026-06-23_gloss_outline_lens_plan.md): a **consumer of P1-P3**. Its
  outline metrics read importance + community (with a degree / connected-components fallback until
  the cache lands). Distinct from P6 (the gloss swatch's own `Projection`); both honour the gloss
  no-split rule (one surface, form-factor toggle).
- [node_body_face_model_plan](2026-06-23_node_body_face_model_plan.md): owns the encoding
  **rendering hardware** (per-node size, and the body's rings). This plan owns the **signal + the
  channel decision** (importance to size, community to a ring). The size + ring channel occupancy
  is grounded in the Decisions section below.

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
  `projection.nodes.iter().map(|n| (n.node, n.position))` (`cartography_scene.rs:235`). No live path
  consumes `Projection::overlays`. **Correction (2026-06-24): the overlay *vocabulary* mostly
  already exists** — `cartography/src/overlay.rs` defines `ClusterHalo`, `BridgeEmphasis`,
  `ImportanceScale`, `ActivityHeat`, `EdgeWeight`. So P2 is **consume + arbitrate**, not "build the
  vocabulary"; only a `PathHighlight` kind is genuinely missing. The gloss bypasses cartography
  entirely via `Orrery::minimap_geometry`. The consumer plumbing is the bulk of the work.
- **The "rings" channel is not built yet (2026-06-24).** The plan's headline encoding (community to
  a ring) assumes a ring channel, but selection is still a **colour** recolour today (`#f7a440`,
  `lib.rs:949-958`), node-rep Decision 2's selection ring is unimplemented, and no halo/ring
  renderer exists. The **edge** channel, by contrast, is completely free (the `EdgeWeight` overlay
  is defined but unconsumed; every edge is a uniform grey stroke). This reorders P2 (below).

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
- **P1 — cheap signals + the cache contract.** `intel/signals` with the generation/dirty-bit cache
  (hooked at the `GraphDelta` mutation sites; see Decisions), **degree** computed synchronously
  (articulation defers), the snapshot handed to `project_orrery_strategy` (replacing `::default()`)
  plus a size encoding reading importance (= degree for now). Done when degree flows producer ->
  cache -> snapshot -> size + collider with no new rendering (the thinnest spine-prover above).
- **P2 — `Projection` plumbing + the first new encoding (edges).** Stop discarding the projection;
  the overlay *vocabulary* mostly exists (consume + arbitrate, not build). **First new encoding =
  edge-weight to thickness** (multiplicity now, affinity later — the free channel); encoding
  arbitration is a **scene-wide setting** on the orrery scene page (settings-lane), not a per-tile
  picker. **Rings (community / bridge) defer behind a ring renderer + the selection-to-ring
  conversion** (node-rep Decision 2), since the ring channel is unbuilt. Done when edge-weight
  drives a visible thickness end to end.
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

## Decisions + grounding (2026-06-24, with Mark)

A development pass grounded the plan against the live tree and settled the open calls.

**Decisions:**

1. **Edges first.** The first *visible* signal-driven encoding is **edge-weight to edge thickness**,
   not community-to-ring: the edge channel is unoccupied (a clean end-to-end win), whereas the ring
   channel does not exist yet (selection is still colour; see below). Community-to-ring follows once
   the ring renderer + the selection-to-ring conversion land.
2. **Multi-edges are the first edge signal.** The kernel is a **multigraph** (one petgraph edge per
   statement / predicate, per the RDF kernel plan); the orrery collapses to one visual edge per pair
   (`dedup_edges`, `build.rs:102`). Edge-weight rides the **collapsed per-pair edge**, and the
   cheapest first weight is **multiplicity** — the number of `relations()` connecting a pair (more
   statements = thicker edge), which makes the multigraph truth legible for free. The computed
   *affinity* weight (`semantic_edge_weight`) is the later, richer edge signal on the same channel.
3. **Size precedence:** manual override > **importance signal** > size-by-degree > uniform default.
   You can always pin a node's size; unpinned nodes scale by computed importance; degree is the
   fallback when no importance signal is present. (`node_size`, `lib.rs:1255-1265`; importance slots
   in as a new input, applied only to nodes absent from `node_sizes`.)
4. **Two signal layers, one producer.** Confirmed the seam with
   [meaningful_physics_signals_plan](2026-06-24_meaningful_physics_signals_plan.md): this plan
   produces graph-structural signals + non-physics encodings; that plan consumes them (plus runtime
   signals) and binds onto physics.

**Cache grounding (P1):**

- **No generation counter exists in the kernel** — the cache introduces one, bumped at the
  `Graph::apply` / `GraphDelta` mutation sites (`graph/mod.rs:312-357`, `apply.rs`): node + edge
  add / remove = **topology-dirty**; `SetNodeTitle` / `Url` / tags = **content-dirty**.
- **Async stable-key rebinding is already wired:** `NodeKey` is an unstable petgraph index, but
  `id_to_node` + `get_node_key_by_id` (`query.rs:75-83`) let a background result keyed by **UUID**
  rebind on arrival; stamp the generation, drop the stale.
- **Lifecycle mirrors `reconcile_derived` + the gyre actor:** detect mutation -> bump generation +
  set dirty bits; on read, cheap signals compute inline, expensive ones queue to a background actor
  (armillary, the same off-thread discipline gyre physics uses) and return the last-good snapshot
  until a fresh one lands.
- **Cheap signals reuse existing code:** degree is already computed (`neighbors_undirected`);
  `ImportanceWeights` is already a field on the `IntelligenceSignals` contract. Articulation points
  are the only net-new cheap compute, and they defer.

**Channel occupancy (P2 arbitration, grounded):**

- **Size** — three inputs today (manual override > size-by-degree > default); importance slots in
  per decision 3. Size drives the **collider** too (`push_node_geometry`), so importance-to-size
  makes important nodes physically weighty, not just visually large.
- **Colour** — reserved for activation state (`node_state_color`); community genuinely cannot use
  it, so community is a **ring**.
- **Rings** — **not built**: selection is still colour, no halo renderer. Community / bridge rings
  need (a) a ring-rendering layer on the gnode and (b) the selection-to-ring conversion (a node-rep
  Decision 2 deferral) so the channels do not collide. This is the prerequisite that pushes rings
  behind edges.
- **Edges** — **free**: `EdgeWeight` overlay defined, unconsumed; uniform grey stroke. Multiplicity
  then affinity ride this channel end to end.

**The thinnest first slice (P1, the spine-prover):** `intel/signals` wraps **degree** in the cache
and hands a real `IntelligenceSignals` (degree as `ImportanceWeights`) into `project_orrery_strategy`,
replacing the hardcoded `::default()`; a size encoding reads importance (= degree for now). This
proves producer -> cache -> snapshot -> encoding -> render + physics with **zero new rendering**
(it reuses the live size + collider path), and the metric is pluggable to betweenness later. The
first *new* visual is then the edge channel (multiplicity -> thickness).

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
- 2026-06-24: **Rev 3 — grounded against the live tree (3-reader dev pass) + decisions settled with
  Mark; starting P1.** Verified the status is current (P0 done, P1-P6 unstarted, no signals code
  landed; the `palette`/`sheet` blocker resolved). Corrected two stale claims: the overlay
  *vocabulary* mostly already exists (so P2 is consume + arbitrate), and the **ring channel is not
  built** (selection is still colour), which pushes community-to-ring behind a ring renderer + the
  selection-to-ring conversion. Settled with Mark: (1) edges first (the free channel); (2) multi-edge
  **multiplicity** is the first edge-weight signal (count `relations()` per pair, which `dedup_edges`
  collapses), affinity later on the same channel; (3) size precedence manual > importance > degree >
  default; (4) the two-layer seam with the physics-signals plan. Grounded the cache: no kernel
  generation counter (introduce one at the `GraphDelta` sites), UUID rebind already wired
  (`get_node_key_by_id`), lifecycle mirrors `reconcile_derived` + the gyre actor, cheap signals reuse
  `neighbors_undirected`. Added cross-refs (physics-signals, gloss-outline, node-body-face). Next: P1
  thinnest slice.
- 2026-06-24: **P1 spine slice landed — the producer reaches the consumer.** New `crates/intel/signals`
  crate (`signals`): `produce_cheap_signals(graph)` + `degree_importance(graph)` (undirected neighbour
  count, normalized 0..=1, matching the existing size-by-degree count so it is behaviour-preserving).
  `project_orrery_strategy` (`cartography_scene.rs:189`) now calls it, replacing the hardcoded
  `IntelligenceSignals::default()` — so strategies receive a real `importance` signal (the others
  ignore it; additive contract). 3 unit tests green (normalization, no-edges -> 0, only-importance-
  produced); platen builds. **Invisible by design** (the dispatched strategies do not read importance
  yet), but the spine is live: producer -> snapshot -> consumer, with the metric pluggable to
  betweenness later. **Remaining in P1:** (a) the generation + dirty-bit cache that gates the
  per-frame recompute; (b) the first *visible* encoding — importance -> size (precedence manual >
  importance > degree > default), the orrery/host wiring. Then P2 (edges -> thickness).
- 2026-06-24: **Importance -> size encoding — orrery mechanism landed + tested.** The orrery now
  consumes the importance signal on the **size channel**: a `size_by_importance` scene toggle +
  a cached `node_importance` map (refreshed from `signals::degree_importance` in `push_node_geometry`
  while the mode is on, since normalization needs all nodes); `node_size` precedence is **manual >
  importance > size-by-degree > default** (normalized importance maps to `36..=88`, so the most
  important node hits the cap and the rest scale relative). `set_size_by_importance` / accessor mirror
  the size-by-degree knobs. A unit test locks it (off -> uniform; on -> hub at the cap, a 0.5-leaf at
  62; manual override wins; off -> uniform). orrery 59 tests green; the `orrery -> signals` dep is
  acyclic. **Deferred (mechanical, mirror size-by-degree):** the in-app toggle (`ToggleSizeByImportance`
  context action + command-registry entry) so it is enable-able, and cartography-sidecar persistence.
  Until the toggle lands it is exercised only by the test, not yet in-app. Next: the toggle, then P2
  edges -> thickness, then the cache.
- 2026-06-24: **Size-by-importance is in-app now — the toggle landed.** `ToggleSizeByImportance`
  wired across all three size-by-degree surfaces: the selection context menu, the command palette
  (registry id `size_by_importance`), and the `pelt/orrery` settings page, all routing to
  `set_size_by_importance`. meerkat 78 tests green, including the registry-consistency checks
  (`default_menu_actions_are_all_known_registry_ids`, `registry_ids_are_unique...`). So the importance
  encoding is fully usable: flip it on and the most-connected node grows to the cap, the rest scale
  relative. **Still deferred:** cartography-sidecar persistence of the toggle (in-memory for now,
  like size-by-degree began). Next: P2 edges -> thickness, then the generation/dirty cache.
- 2026-06-24: **Size-by-importance persistence + P2 edges -> thickness landed.** (1) The toggle now
  rides the cartography sidecar: `CartographyGeometry.size_by_importance` (serde-defaulted) +
  `with_/accessor`, exported in `cartography_geometry`, restored via `apply_cartography_sizing` (now a
  3rd param) at boot + session-switch; the size-persistence round-trip test asserts it. (2) **P2's
  first new encoding — edge-weight -> stroke thickness.** `PositionedEdge` gained a `weight` field;
  `projected_undirected_edges` counts the **multigraph multiplicity** (relations per collapsed pair —
  more statements between two nodes => heavier), and `paint_projection_filtered` scales the stroke
  `width = edge_width * weight` (capped 4x). Default-preserving: a single-relation pair stays at the
  1.5px default, so only multi-statement / reciprocal pairs thicken. A platen test locks it (unit
  weight per single-link pair; weight 2 for a reciprocal pair). orrery 59 / platen 83 / cartography 14
  / arrangements 99 green; meerkat builds. **Scope note:** weighting is on the underlay (force-directed)
  path; arrangement-strategy edges stay uniform (weight 1) for now. The computed *affinity* weight is
  the later, richer signal on this same channel. Next: the generation/dirty cache.
- 2026-06-24: **The cheap-signal cache landed (the cheap half of P1's cache).** The orrery's degree-
  importance now recomputes **only on a topology change**, not on every geometry push: an
  `importance_dirty` flag set in `reconcile_derived` (the universal orrery topology hook) + on
  enabling the mode, and gated in `recompute_importance` (early-return when clean). So a size-only
  push (a manual resize) no longer redoes the O(N) degree pass. An invalidation test proves it:
  adding an edge drops a node's importance-size (a stale cache would leave it). orrery 60 green.
  **Design note (deferred deliberately):** the plan's full cache (a kernel generation counter +
  per-signal dirty bits + the debounced background lane + stale-rejection + UUID rebinding) is the
  **expensive-signal** substrate (betweenness / Leiden / spectral). For the one cheap synchronous
  signal we have, the dirty flag keyed on the known reconcile hook is correct + robust + zero kernel
  surgery (8 scattered `inner.add_edge`/`remove_edge` sites would risk a missed bump). The generation
  and background machinery is built **with** the first expensive signal, its real validation point. So
  **P1 is complete** (spine + importance->size + toggle + persistence + the cheap cache); P2's first
  encoding (edges->thickness) done. The frontier is the first *expensive* signal (betweenness or
  community) + the background cache it rides, then P2's overlay consumer (community ring), gated on a
  ring renderer + the selection-to-ring conversion. **Adversarial review (2 lenses) then caught a real
  bug in the just-added persistence:** `apply_cartography_sizing` restored `size_by_importance = true`
  but did not dirty the cache, so on a *reused* orrery (session switch, cache already clean) the
  restore left every node at the default size. Fixed (force-dirty on restore) + a regression test
  (cycle off, restore on a reused orrery, assert recompute). The round-trip test had missed it by
  restoring onto a fresh orrery. orrery 61 green.
- 2026-06-24: **First *expensive* signal — betweenness centrality, as a pluggable importance metric.**
  `intel/signals` gained `betweenness_importance` (Brandes' algorithm, undirected/unweighted, parallel
  edges collapsed, normalized 0..=1) + an `ImportanceMetric { Degree, Betweenness }` enum +
  `importance(graph, metric)` dispatch. The orrery holds `importance_metric` + `set_importance_metric`
  (dirties + recomputes); `recompute_importance` reads the chosen metric; re-exported as
  `orrery::ImportanceMetric`. The pelt/orrery settings page shows **by degree / by betweenness**
  sub-toggles when size-by-importance is on (drained to `set_importance_metric`). So size now
  encodes *structural brokerage*: a bridge node between two clusters grows to the cap even at modest
  degree, where degree alone would not single it out — the compelling payoff the metric pluggability
  was for. Tests: betweenness marks the path-broker (max) + the bowtie bridge (max, > peripheral); the
  metric switch re-sizes (a peripheral 62 under degree -> 36 under betweenness). signals 5 / orrery 62
  green; meerkat builds. A 2-lens adversarial review **confirmed Brandes' correct** (BFS/accumulation,
  the undirected halving, the NaN-guarded normalization, and the disconnected / isolated / single /
  empty / parallel-edge edge cases all check out; a 4-cycle hand-trace confirms symmetry) and caught
  one real gap, now closed (below). **Metric persistence (closed):** the metric now rides the
  cartography sidecar as a string code (`degree` / `betweenness`, serde-defaulted, `ImportanceMetric::
  as_code`/`from_code`); the orrery exports it and restores it via `apply_cartography_importance_metric`
  **before** the sizing restore (so the size recompute uses the restored metric), wired at both meerkat
  reload sites. A round-trip test locks it (a betweenness-sized scene re-opens betweenness-sized). So a
  reload no longer silently reverts to degree. This is the plan's first **advanced** analysis (the gap
  it named: "centrality beyond degree"). **Deferred (one):** the **off-thread background lane** —
  betweenness is O(V·E), cheap at current scale and synchronous through the existing dirty-flag cache;
  the background lane is the scale-trigger work (when betweenness on a large graph exceeds a frame
  budget), its real validation point. Next: community detection (-> the ring encoding, gated on a ring
  renderer), or wire betweenness into the `bridges` overlay contract.
