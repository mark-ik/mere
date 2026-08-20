# Graph Signals Layer Plan: a producer layer feeding arrangements, encodings, and the gloss lens

**Date**: 2026-06-22 (rev 3, 2026-06-24: grounded + decisions settled, building P1)
**Status**: complete 2026-06-24; see the closing note. Phase A (kanban/timeline arrangements) landed;
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

- [node_representation_arrangement_plan](../../mere_docs/implementation_strategy/2026-06-18_node_representation_arrangement_plan.md):
  the arrangement half of node-rep (Phase A = kanban/timeline lives there). **Decision 2 reserves
  node face color for activation state** — the encoding collision below.
- [graph_cluster_namespaces_brief](../../mere_docs/implementation_strategy/2026-05-10_graph_cluster_namespaces_brief.md): already states
  recomputing community decomposition on every insertion is impractical — the cache below answers it.
- [scriptable_field_regions_plan](../../mere_docs/implementation_strategy/2026-06-13_scriptable_field_regions_plan.md): gyre's
  `CouplingForce` is a **field** mechanism (per-node field sample), *not* the pairwise affinity
  force — the affinity force is new.
- [document_script_substrate_plan](../2026-07-03_completed_plans/2026-06-21_document_script_substrate_plan.md): the
  wasmtime-async lane is the **scriptable** path for user-authored analyses (below).
- [meaningful_physics_signals_plan](../../mere_docs/implementation_strategy/2026-06-24_meaningful_physics_signals_plan.md): the
  **physics-side consumer**. It maps runtime signals (content / sync / observability) *and* the
  graph-structural signals this plan produces onto physical parameters (material / size) via
  configurable presets. Clean seam, confirmed 2026-06-24: this plan **produces** the structural
  signals + their **non-physics** encodings (size, arrangements, gloss lens, the affinity force);
  that plan **consumes** them + runtime signals and binds both onto physics. Two layers of
  consumption, one producer.
- [gloss_outline_lens_plan](../../mere_docs/implementation_strategy/2026-06-23_gloss_outline_lens_plan.md): a **consumer of P1-P3**. Its
  outline metrics read importance + community (with a degree / connected-components fallback until
  the cache lands). Distinct from P6 (the gloss swatch's own `Projection`); both honour the gloss
  no-split rule (one surface, form-factor toggle).
- [node_body_face_model_plan](../../mere_docs/implementation_strategy/2026-06-23_node_body_face_model_plan.md): owns the encoding
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
2. **Multi-edges are the first edge signal.** The kernel is a **logical multigraph** (statement
   records enumerated inside the pair-local `EdgePayload` bucket, per the RDF kernel plan's
   statement-bucket revision 2026-07-04); the orrery draws one visual edge per pair
   (`dedup_edges`, `build.rs:102`). Edge-weight rides the **per-pair edge**, and the
   cheapest first weight is **multiplicity** — the number of statements in the pair's bucket (more
   statements = thicker edge), which makes the statement-bucket truth legible for free. The computed
   *affinity* weight (`semantic_edge_weight`) is the later, richer edge signal on the same channel.
3. **Size precedence:** manual override > **importance signal** > size-by-degree > uniform default.
   You can always pin a node's size; unpinned nodes scale by computed importance; degree is the
   fallback when no importance signal is present. (`node_size`, `lib.rs:1255-1265`; importance slots
   in as a new input, applied only to nodes absent from `node_sizes`.)
4. **Two signal layers, one producer.** Confirmed the seam with
   [meaningful_physics_signals_plan](../../mere_docs/implementation_strategy/2026-06-24_meaningful_physics_signals_plan.md): this plan
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
  / arrangements 99 green; meerkat builds. **Scope note (corrected 2026-06-24, slice 4):** this note
  originally claimed arrangement-strategy edges stay uniform — that was WRONG. The orrery renders ALL
  modes through one underlay path (`orrery_paint_list_demoted_from_arrangement` -> `project_keys` ->
  `projected_undirected_edges` -> `paint_projection_filtered`); a layout strategy only changes node
  positions, never the edge derivation. So multiplicity thickness already shows in analytic layouts
  too. The uniform-weight edges are the discarded arrangement *adapter* outputs, not what renders. The
  computed *affinity* weight is the later, richer signal on this same channel. Next: the generation/
  dirty cache.
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
- 2026-06-24: **"Go all in" — community detection (P3) + the generation-gated cache, slices 1-3 of
  4** (Mark chose the full scope: P2 plumbing + the real cache + community + community->kanban). A
  seam-mapping workflow grounded all four targets first.
  - **Slice 1 — community detection.** `signals::community_louvain` (single-level Louvain modularity
    local-moving on the multiplicity-weighted graph; self-loops dropped; every node lands in one
    cluster). 4 tests: two triangles + a bridge stay two communities; a clique collapses to one;
    disconnected components separate; an edgeless graph is all singletons. The genuinely expensive
    structural signal the cache exists for. signals 9 green.
  - **Slice 2 — community -> kanban columns.** A sibling strategy `kanban.community` ("Kanban (by
    cluster)") joins `kanban.default` ("by site") in the picker (configurable, not a replacement),
    reusing `KanbanAdapter` with a Louvain-derived column axis. Test: two triangles lay out as two
    columns. Community computes only when this strategy is active. platen 84 green.
  - **Slice 3 — the generation-gated cache (the real P1 cache, minus the thread move).** A
    `topology_generation` counter bumped in `reconcile_derived` (the universal topology hook the
    importance cache already trusts); an orrery `community_cache` recomputed by
    `refresh_community_cache` only when the active strategy needs it AND the generation advanced.
    The host (both render.rs sites) refreshes before projecting and threads `community()` into
    `project_orrery_strategy` (inline fallback when absent), so **Louvain no longer runs per frame** —
    it ran every frame before, since the projection dispatch is per-frame. Test: the cache fills on a
    cluster strategy, no-ops on a non-cluster one, and invalidates on a topology change (the added
    node joins the recomputed partition). orrery 64 / platen 84 / signals 9 / meerkat 80 green.
  - **Remaining (slice 3b + 4):** (3b) the **off-thread armillary lane** — `armillary::spawn` is
    native-thread-based, so the compute stays inline on wasm/tests and offloads on native, exactly
    like physics's Inline/Actor split; it is structured as a drop-in behind `refresh_community_cache`.
    Native-only and a no-op for UX at current scale (Louvain is sub-ms), so it is sequenced as its own
    careful pass, not skipped. (4) **P2 overlay plumbing** — stop `project_orrery_strategy` discarding
    `Projection::overlays`; emit `EdgeWeight` from arrangement adapters so edge thickness shows in
    arrangement modes (not just the force-directed underlay); wire an overlay consumer.
- 2026-06-24: **Generalized the cache (A + B) — the redundant-recompute fix Mark asked for.** A
  review of slice 3 caught a real bug (clear_selection bumped the orrery's hand-rolled generation via
  reconcile_derived, spuriously invalidating the community cache), which proved reconcile_derived is
  not a clean topology hook. The fix generalizes:
  - **B — a kernel structural revision.** `Graph::revision()` (crates/graph/graph-kernel) bumped at
    the mutation **source** (`bump_revision` in add_node_with_id / remove_node / assert_relation /
    assert_semantic_predicate / retract_relations / push_traversal-when-it-creates-an-edge), NOT on
    content edits (a url/title change) or a re-visit's traversal append. The replay/dissolve variants
    delegate, so they inherit it. A kernel test locks coverage (structure advances it; a url edit does
    not; remove advances it). The orrery's community cache now gates on `graph.revision()` instead of
    the reconcile-bumped counter, so a selection change can no longer invalidate it — the bug is fixed
    at the root, and any consumer holding the graph can gate on the same truth.
  - **A — an arrangement-result cache.** Analytic layouts (grid, phyllotaxis, penrose, lsystem,
    timeline, radial, kanban.community) were recomputed **every frame**; now the orrery's
    `needs_strategy_recompute(id, w, h, focus)` gates the host's per-frame `project_orrery_strategy`
    on `(strategy, revision, viewport, focus)`, recomputing once per real change. Focus is in the key
    only for radial (`strategy_uses_focus`), so a selection change does not invalidate the others;
    `kanban.default` (by site, URL-content-dependent + cheap) stays uncached and always recomputes;
    `set_layout_strategy` resets the key so a re-activation recomputes. The community refresh is now
    nested inside this gate, so Louvain runs only when the layout actually recomputes. A test covers
    first-compute / unchanged-skip / viewport-change / revision-change / focus-ignored-for-grid /
    focus-matters-for-radial. kernel 252 / orrery 65 / platen 84 / signals 9 / meerkat 80 green.
  - **C (kernel-query memos)** stays gated on profiling evidence, not built. Mark endorsed the
    future-proofing direction.
  - **Adversarial review (2 lenses) — caught 2 real missed-bump bugs, both fixed.** The
    arrangement-cache lens came back clean (every input-completeness point sound: kanban.default
    uncached, timeline structural, community gated, focus scoped to radial, revert reset, viewport,
    both host sites). The bump-coverage lens found two structural mutators that wrote `inner`
    directly and skipped the bump — `copy_node_from_with_id` (cross_graph.rs) and
    `rebuild_derived_containment_relations` (query.rs); a revision-keyed cache would have gone stale
    on either. Fixed (bump on copy; bump-once-if-changed on the rebuild) + a copy-bumps-revision
    test. The "pub inner" footgun the review flagged is now closed too: `inner` is `pub(crate)`
    (verified no external access), so every topology mutation must go through a bumping method.
    kernel 252 green.
- 2026-06-24: **Slice 4 (P2 overlay plumbing) — investigated; the visible half is already done, the
  rest is genuinely premature.** Tracing the render path settled the P2 question the audit raised:
  - **Edge-thickness in arrangement modes already works.** The audit said it was invisible there, but
    that was wrong (and so was this plan's own scope note, now corrected): the orrery renders every
    mode through one underlay path that re-derives edges with multiplicity, so analytic layouts get
    thickness too. A layout strategy only changes positions. A new paint-level test
    (`edge_weight_scales_the_stroke_width`) proves the last untested link (a weight-2 edge paints 2x),
    closing the chain `projected_undirected_edges` (multiplicity, tested) -> `project_keys` ->
    `paint_projection_filtered` (scaling, now tested). platen 85 green.
  - **The overlay-consumer plumbing is deferred, deliberately.** Stopping `project_orrery_strategy`
    from discarding `Projection::overlays` only pays off when something *produces and consumes* an
    overlay. The `EdgeWeight` overlay is redundant (weight already flows on the `PositionedEdge.weight`
    field the paint path reads); `ClusterHalo` / `BridgeEmphasis` / `ImportanceScale` are the real
    future consumers, and every one is gated on the **ring renderer + the selection-to-ring
    conversion** that P2 itself defers. Building the overlay pipe now, with no producer or consumer,
    is the YAGNI trap the cache work just avoided. So the overlay plumbing rides with the community
    ring (its first real consumer), not ahead of it.
  - **Net:** P2's user-facing goal (edge-weight encoding visible everywhere) is met; the structural
    overlay pipe waits for a consumer. "Go all in" is substantively complete (community + cache +
    revision + arrangement cache + edge-weight everywhere). Remaining: **slice 3b** (the off-thread
    armillary lane) as the architectural future-proofing.
- 2026-06-24: **Community -> ring (the headline encoding) landed.** Mark asked to just build it.
  **Ordering decision** (his question — pipe-first vs ring-first): the ring renderer is the
  load-bearing primitive with an *immediate* consumer (the orrery already holds the partition in its
  cache, so it draws rings directly); the overlay pipe is data-transport the main view does not need
  (it has the cache). So ring-renderer-first lights up immediately and reveals the overlay pipe was
  never on the critical path for the visible feature. Built the ring path, not the pipe:
  - **Ring renderer:** `cluster_color` (an 8-hue categorical palette), `circle_path` (a stroked-circle
    polyline), and `community_ring_overlay(view, community, radius_of)` — a world-space halo per node
    in its community's colour, spliced into the orrery underlay exactly like the selected-edge overlay
    (so it tracks the camera + shows under EVERY layout, force-directed and analytic).
  - **Toggle:** orrery `show_community_rings` + setter/accessor; `refresh_community_cache` now also
    computes when the rings are on (sharing the same generation-gated `community_cache`); `frame()`
    refreshes + splices the rings when on. A `Show community rings` toggle on the pelt/orrery page +
    the `orrery:communityrings` input handler.
  - A test proves the toggle drives the partition + runs the ring paint path in `frame()`. orrery 66
    green; meerkat builds. The **overlay pipe** (stop discarding `Projection::overlays`) stays
    deferred to its first real off-orrery consumer (a standalone projection view / the gloss lens),
    not built speculatively. Remaining: **slice 3b** (the off-thread armillary lane).
- 2026-06-24: **Slice 3b — the off-thread community lane (the background substrate the plan named).**
  The last of the "go all in" scope; the *correct architecture* (Mark's words) for the expensive
  signal.
  - **The Send boundary (signals):** `community_louvain` split into `CommunitySnapshot::from_graph`
    (extract the parallel-edge-collapsed weighted adjacency, sorted rows — `Send` + deterministic) +
    `community_louvain_on_snapshot` (the algorithm, `Graph`-free) + `community_louvain` (delegates).
    The `Graph` (not `Send`, borrowed) never crosses a thread; the snapshot does. Behavior-preserving
    (the existing community tests pass unchanged).
  - **The actor (orrery `community_lane.rs`):** an armillary actor mirroring the physics one — loops
    `recv` -> Louvain-on-snapshot -> `emit(CommunityUpdate { clusters, revision })`. `request` skips a
    duplicate in-flight revision; `drain` takes the freshest result. A real-thread round-trip test
    proves spawn -> request -> compute -> drain with the right revision + partition.
  - **Wiring (no new host call):** `offload_physics` now captures the host's wake as the orrery's
    off-thread wake; the community lane reuses it (the render loop is on-demand, so a result needs a
    wake to be drained promptly). `refresh_community_cache` dispatches off-thread when the wake is set
    (native + offloaded), inline otherwise (wasm / tests). `frame()` calls `drain_community` each
    frame, which accepts a result **only if its revision still matches the live graph**
    (stale-rejection — a partition computed against a since-mutated graph is dropped and re-dispatched,
    so a stale NodeKey never reaches the cache). The worker spins up lazily on first need (no idle
    thread for orreries that never use community).
  - **Honest scope note:** this is native-only and changes nothing observable at current graph scale
    (Louvain is sub-millisecond); its value is the validated async round-trip + future-proofing for
    large graphs, the architecture the cache plan called for. kernel 252 / signals 9 / orrery 67 /
    platen 85 / meerkat 80 green. **"Go all in" is complete** — community detection, community->kanban,
    community->rings, the generation-gated cache, the kernel revision (B), the arrangement cache (A),
    edge-weight everywhere, and now the off-thread lane.
  - **Review (2 lenses, both sound).** Integration/refactor: behavior-preserving, `CommunitySnapshot`
    is `Send`, gating correct, fallbacks robust, per-pane isolated (one readability nit fixed: frame()
    now calls `ensure_community_fresh()` instead of `refresh_community_cache("")`). Async: liveness
    (in-flight always clears), stale-rejection (a partition for an old revision is dropped + re-
    dispatched, never reaching the cache), freshest-wins drain, one-worker-per-orrery lifecycle with
    safe drop, correct wake/drain timing — all verified. The only noted weakness is theoretical (a
    worker panic would serve stale data, but the compute is bounds-safe + division-guarded, and it
    degrades without deadlock).
- 2026-06-24: **P5 — spectral arrangement.** A new `SpectralAdapter` ("Spectral" in the layout
  picker): node positions are the two non-trivial smallest eigenvectors of the (multiplicity-weighted)
  graph Laplacian, so the layout reflects connectivity — clusters separate, a path unrolls into a
  line. Dependency-free + deterministic: power iteration with deflation on `B = cI - L` (whose largest
  eigenvectors are L's smallest), `c = 2·max_degree` (the tight Gershgorin bound, so B is PSD),
  cosine starts, auto-fit into the viewport, and a circle fallback for an edgeless / degenerate graph
  so nodes never pile up. It is the expensive analytic layout the **arrangement cache (A)** exists for
  (recomputed only on a structural change), and it has real **synergy with P3**: spectral separates
  the clusters spatially, the community rings colour them. Tests: a path is monotonic along the
  Fiedler axis; two triangles + a bridge separate (inter-cluster gap > intra-cluster spread);
  deterministic; empty. arrangements 104 / platen 85 / orrery 67 / meerkat 80 green. The
  `semantic_embedding` content-coords lane stays reserved for the intel/embed model (not built).
  Remaining: **P6** (independent gloss projection, carrying the overlay pipe), **P4** (affinity force),
  and the small extensions (**bridges->ring** is now an unblocked warm-up).
- 2026-06-25: **Extension — bridges -> ring (the warm-up).** Betweenness was already computed and the
  ring renderer already existed, so highlighting the structural brokers was a small reuse, not a
  phase. `signals::bridge_nodes(graph, threshold)` thresholds normalized betweenness into the
  cartography `BridgeNodes` contract (the "detected by graph-structural betweenness" notion the
  contract names); a clique / structureless graph yields none. The orrery gained a `show_bridge_rings`
  toggle drawing a **bold near-white ring** on the brokers (distinct from the per-cluster community
  rings and the orange selection, larger radius so both overlays read together), over a revision-gated
  **inline** cache (betweenness is cheap, so no off-thread lane — `ensure_bridges_fresh`). A `Show
  bridge rings` toggle on the pelt/orrery page. Tests: the bowtie broker is the only bridge at
  threshold 0.5; a clique has none; the toggle drives the computation on `frame()`. signals 11 /
  orrery 68 / meerkat 80 green. Remaining: **P6** (gloss projection + the overlay pipe), **P4**
  (affinity force).
- 2026-06-25: **P6a — the independent gloss projection (the "different lens" core).** The gloss
  swatch was a minimap (it mirrored the main view's live positions via `minimap_geometry`); now it
  can show its **own arrangement**, independent of the main view. The orrery gained `gloss_geometry`
  (assemble the swatch's node/edge geometry from *arbitrary* positions), a `gloss_strategy`
  (`None` = mirror, `Some(id)` = an independent lens), and a gloss arrangement cache
  (`gloss_needs_recompute` / `set_gloss_positions` / `gloss_geometry_cached`) keyed on
  `(strategy, graph revision, viewport)` so an expensive lens like spectral is not recomputed per
  frame. The host (render.rs) computes the gloss lens through `project_orrery_strategy` only when the
  cache says to, then draws it; with no lens set it mirrors as before. A `Gloss: independent lens`
  toggle on the pelt/orrery page flips it to a **spectral** gloss (a clearly different lens than the
  force-directed main view). Test: the gloss nodes sit at the *lens* positions (not the live layout),
  and the cache re-triggers on a viewport / topology change. orrery 69 / meerkat 81 green. P6's
  done-condition (the gloss shows a different lens than the main view) is **met**.
  **Deferred (P6b/c):** the **overlay pipe** (gloss showing community/bridge rings + other overlays
  via `Projection::overlays` — its first real consumer, the reason that pipe waits for the gloss) and
  richer gloss **scope / encoding / lens-picker** controls (the toggle is on/off spectral for now).
  Remaining on the plan: **P4** (the affinity force) + those P6b/c refinements. The rest of the
  graph-signals plan — P1, P2, P3, P5, the A/B cache generalization, community + bridge rings, the
  off-thread lane — is complete and reviewed.
- 2026-06-25: **P4 — the affinity force.** The second semantic adapter, done as a *force* (not a
  streaming projection): a new gyre `AffinitySpring`, a weighted attract-only pairwise spring that
  pulls structurally-similar nodes together on top of force-directed, so communities draw into tight
  clusters ("cluster by affinity"). The signal is `signals::structural_affinity(graph, min)`: the
  Jaccard similarity of node neighbourhoods (shared-neighbour count over neighbourhood union),
  computed by common-neighbour accumulation so the cost tracks the graph's clustering not n², sorted
  for determinism, thresholded (0.1) to stay sparse. It is the cheap dependency-free structural
  stand-in for the later content-embedding cosine; both ride the same `AffinityScores` channel. The
  force is **attract-only** (pulls a stretched pair in, never pushes a close pair apart: that is
  `NodeExclusion`'s job) and composes on top of `EdgeSpring` rather than replacing it. It installs as
  a wholesale-replaceable slot on the sim (`set_affinity_force(Option<AffinitySpring>)`, mirroring the
  field couplings, position-preserving, `Send` across the physics actor via a new
  `SetAffinityForce` command). The orrery caches the signal revision-gated (inline, like betweenness)
  and `sync_affinity_force` (re)installs or clears the force once per real change with a settle, never
  per frame. A `Cluster by affinity` toggle on the pelt/orrery page. Tests: the force pulls a
  high-affinity pair in and leaves a zero / close pair alone, a stronger pair beats a weaker one
  against exclusion, clearing is position-preserving (gyre, 5); the producer gives 1/3 on a triangle,
  1/2 on a clique, clusters within bridged triangles not across, and prunes by threshold (signals,
  5); the toggle installs the clustered pairs and clears them, and a topology change refreshes the
  live force (orrery, 2). gyre 52 / signals 16 / orrery 71 / meerkat 81 green. Done-condition
  (affinity clusters at gyre's cost) **met**. Two honest caveats: the force is only *visible* under
  force-directed (an analytic strategy overwrites the physics snapshot), and the streaming
  `SemanticEdgeWeightAdapter` is now redundant for clustering but **left in place** (a registered
  strategy) pending Mark's go-ahead to retire it. Remaining: the **P6b/c** refinements (overlay pipe +
  richer gloss controls).
- 2026-06-25: **P6b — the overlay pipe (the gloss's first consumer).** `project_orrery_strategy` used
  to discard the whole `Projection` except `.nodes`; now the gloss consumes the **overlay channel**.
  The dispatch is factored into a private `project_orrery_dispatch` (returns the full `Projection`),
  a `signal_overlays(clusters, bridges)` pure builder (a `ClusterHalo` per community in cluster order
  plus a `BridgeEmphasis` per broker, in the existing cartography `Overlay` vocabulary —
  **position-independent**, just node references, so the same overlays serve any layout), and `project_orrery_lens`
  (= dispatch + overlays). `project_orrery_strategy` stays a thin positions wrapper, so its three
  render.rs callers + tests are untouched. The orrery stores the lens overlays beside its positions
  (`set_gloss_positions(positions, overlays, w, h)`) and `gloss_geometry` resolves them into **rings
  at the lens's own positions** (cluster halos coloured per cluster matching the main view, bridge
  emphasis in bold near-white); `minimap_scene` paints the rings under the nodes in swatch space. The
  gloss cache key gained the two ring-toggle booleans (flipping a ring toggle re-fetches the lens, so
  its overlays track the toggles; rare, so paying a spectral recompute then is fine), and the host
  gates the lens's clusters/bridges on the same `show_community_rings` / `show_bridge_rings` toggles
  as the main view. So "Show community rings" now halos the clusters in **both** the main view and the
  gloss, the gloss at its own (e.g. spectral) layout. Tests: `signal_overlays` builds halos in cluster
  order then bridge emphasis, and is empty without signals; `project_orrery_lens` carries the two
  community halos while the positions wrapper still returns every node (platen, +3); the gloss resolves
  stored overlays into rings at the lens positions (orrery, +1). The `Projection::overlays` channel is
  now a live, consumed seam (its first real consumer), so future encodings (importance scale, edge
  weight, activity heat) have an established path.
- 2026-06-25: **P6c (down-payment) — the gloss lens-picker.** The gloss control was an on/off spectral
  toggle; it is now a **picker** mirroring the main layout one: "Mirror main view" (a minimap) plus a
  row per `ORRERY_LAYOUT_STRATEGIES` entry (spectral, grid, phyllotaxis, penrose, l-system, kanban,
  timeline, radial), the active lens checked, each draining `orrery:gloss:<id>` (empty id = mirror) to
  `set_gloss_strategy`. So the second lens can now be *any* arrangement, not just spectral. platen 88 /
  orrery 72 / signals 16 / gyre 52 / meerkat 81 green. **Remaining P6c:** the deeper gloss controls —
  an independent **scope** (the gloss previewing a sub-graph) and independent **encoding** (its own
  size-by-importance / edge-thickness, decoupled from the main view). With P6b done, those are
  additive on the same lens projection, not new plumbing. The graph-signals plan is now complete
  through P6b + the lens-picker; P4's `SemanticEdgeWeightAdapter` retirement and the P6c scope/encoding
  controls are the only open threads.
- 2026-06-25: **Retired the streaming `SemanticEdgeWeightAdapter`.** The affinity force (P4) reached
  parity, so the cartography-side streaming adapter is gone: deleted
  `arrangements/adapters/semantic_edge_weight.rs`, its module decl + re-export, and the one platen test
  that exercised it (it was never in the live `ORRERY_LAYOUT_STRATEGIES` / `project_orrery_strategy`
  dispatch — only that test used it). Affinity now clusters at gyre's cost, not as an iterative
  projection. arrangements 98 / platen 87 green. **Left in place (separate layer, flagged):** the
  underlying `SemanticEdgeWeight` *primitive* (a registered `arrangements` `Layout<N>` builtin, with
  Grid/Radial/etc., exercised by the registry's own tests, not host-wired) and the generic
  `step_with` / `StreamingLayoutStrategy` streaming harness (now consumer-less but reserved for a
  future content-embedding streaming lane). Removing either is an `arrangements`-registry decision,
  not "the streaming adapter," so it waits for an explicit call.
- 2026-06-25: **P6c (rest) — the independent gloss scope + encoding.** The gloss is now a fully
  configurable second lens (arrangement + scope + encoding). **Scope:** `gloss_scope_selection` crops
  the lens to the current selection (+ induced edges + halos over selected members); `minimap_scene`
  auto-refits, so the swatch zooms to the selection. It is a pure render-time filter in
  `gloss_geometry` (no position-cache impact), so changing the selection re-crops live; an empty
  selection falls back to the whole graph. **Encoding:** `gloss_size_by_importance` sizes each gloss
  node by the importance signal (the same `node_importance` the main view reads, mapped to a
  0.7..=1.9 factor), independent of the main view's own sizing; the gloss node tuple gained a per-node
  size factor the swatch multiplies in, and `frame` ensures the (dirty-gated) importance cache fresh
  when the encoding is on. Two toggles on the pelt/orrery "Gloss lens" section ("Gloss: selection
  only", "Gloss: size by importance"). Tests: scope crops to one selected node with no out-of-scope
  edges and restores on toggle-off; the encoding leaves sizes uniform when off and scales a hub above
  a leaf when on (orrery, +2). gyre 52 / signals 16 / arrangements 98 / platen 87 / orrery 74 /
  meerkat 81 green. **The graph-signals plan is complete** (P1–P6 + the A/B cache generalization +
  community/bridge rings + the off-thread lane + the affinity force + the overlay pipe + the full
  gloss lens). Only-if-wanted follow-ups: subgraph *re-layout* for the gloss scope (today it crops the
  whole-graph layout), gloss *edge-thickness* encoding, multi-level Louvain, articulation points,
  kernel-query memos, and pruning the orphaned `SemanticEdgeWeight` primitive / streaming harness.
- 2026-06-25: **Polish pass — all six follow-ups done.** (1) **Articulation points:** a new
  `signals::articulation_points` (iterative Hopcroft–Tarjan low-link DFS, distinct undirected
  adjacency, disconnected-graph-safe) plus a `BridgeMetric { Betweenness, Articulation }` choice on the
  bridge ring — so the ring highlights either betweenness brokers (high traffic) or cut vertices
  (single points of failure). A `by betweenness / by cut-vertex` sub-toggle on the pelt page;
  `set_bridge_metric` invalidates the bridge cache. (2) **Multi-level Louvain:** the single-level
  local-moving is now the inner loop of a true hierarchical Louvain (`louvain_local_moving` +
  `louvain_aggregate` with self-loop folding that preserves total degree / modularity across levels);
  the `ClusterSet` contract is unchanged, the partition is at least as good. (3) **Gloss
  edge-thickness:** gloss edges carry their multiplicity weight (`dedup_edges_weighted`) and the swatch
  draws denser pairs thicker — the same free channel as the main view, always-on. (4) **Gloss subgraph
  re-layout:** `project_orrery_subgraph` lays out the *induced subgraph* of the selection (nodes
  re-added by stable id, positions remapped back), so a scoped gloss reflects the selection's own
  structure rather than a crop; the scope joined the gloss cache key so a selection change re-projects.
  (5) **Kernel-query memo (cache C):** the per-frame gloss `dedup_edges_weighted` is now revision-gated
  (`weighted_edges_cache`, refreshed in `frame`, read with a fallback) so a static frame reuses the
  collapsed edge topology instead of re-deduping. (6) **Pruned the orphaned streaming stack:** deleted
  the `SemanticEdgeWeight` primitive (arrangements `Layout<N>` + config/state + tests + registry
  registration), the `step_with` / `run_one_step` runners, and the `StreamingLayoutStrategy` cartography
  trait + its tests + re-exports, with the doc sweep. `SemanticEmbedding` / `EmbeddingFallback` /
  `LayoutStrategy` stay. Tests: articulation (path/clique/cycle/bowtie + metric dispatch), multi-level
  Louvain (3-triangle line + determinism), gloss edge multiplicity, subgraph-only projection, scope
  cache key, the memo's static-frame reuse. Final: signals 23 / gyre 52 / cartography 11 /
  arrangements 94 / platen 88 / orrery 78 / meerkat 81 green. **Nothing left on the graph-signals
  plan.**
- 2026-06-25: **Adversarial review of the polish diff + fixes.** A read-only six-lens review (the two
  algorithms came back clean; five issues confirmed, one refuted) found, and this fixes: (1) the gloss
  cache key lacked the main view's `kanban.default` always-recompute guard, so a "Kanban (by site)"
  gloss lens went stale after a URL-host edit (content the structural revision does not track) —
  added the guard. (2) The gloss cache key omitted **focus**, so a focus-driven lens (radial) would
  not re-center on a selection change — threaded focus in, gated by `strategy_uses_focus`, mirroring
  the main view (latent: radial is not an offered gloss lens yet). (3) The weighted-edge memo could
  serve a stale weight because `dedup_edges_weighted` counted **traversal** rows, which the kernel
  deliberately does not bump the revision for — it now counts only *statements* (semantic / structural
  relations), which both restores memo/revision coherence and corrects the encoding (thickness =
  statements, not navigations); a traversal-only pair still draws at weight 1. (4) Two broken
  intra-doc links to `gyre::AffinitySpring` / `gyre::EdgeSpring` from crates that do not depend on
  gyre — dropped to plain code spans. (5) Disabling main-view size-by-importance left the importance
  cache clean-empty, so a later gloss-only size encoding rendered every node at the floor — the
  disable now re-dirties the cache. Regression tests added for the kanban-gloss recompute and the
  size-survives-disable path. The refuted finding (the now-unread `LayoutExtras::semantic_similarity`
  field) is intentional — a generic similarity input, harmless to populate. All suites green after the
  fixes (signals 23 / arrangements 94 / orrery 80 / meerkat compiles).

## Closing note, archived 2026-08-20

Complete 2026-06-24 (P1 through P6, the A/B cache generalization, community
and bridge rings, the off-thread lane, the affinity force, the overlay pipe,
the full gloss lens), and the 2026-06-25 polish pass then closed every entry
on the only-if-wanted list too. The header's "P1-P6 open" was stale from rev
3 and contradicted by the log below it. Re-checked on the way out:
`crates/intel/signals` exists as `mere-signals`, exactly where this plan put
it.
