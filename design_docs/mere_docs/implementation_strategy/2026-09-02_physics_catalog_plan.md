# Physics Catalog Plan

**Date:** 2026-09-02
**Status:** in progress (P1 landed 2026-09-02, P1b the petgraph sources and P2's web half 2026-09-03; P2's native half waits on the picker decision, then P3).
**Scope:** A catalog of *distinct physics layout laws* — dynamical systems
over the graph's bodies that produce different layouts because they are
different physics — as a lever beside the arrangement catalog, plus the
composable extra forces the donor's presets were made of, in the canvas
and both Graphshell hosts, extended to the remote board, with receipts.
Not in scope: new integrators (seiche stays rapier), GPU tiers (the spatial
compute plan), the ambient backdrop sims (already a catalog), the donor's
WASM layout mods.

**Related:** [cartography layer brief](../research/2026-05-10_cartography_layer_brief.md)
(§5, the strategy catalogue and the helper-era preset portfolio),
[the cartography–gyre layout seam](../technical_architecture/2026-05-29_cartography_aether_layout_seam.md)
(arrangements compute, physics simulates; the seed/read-back bridge),
[physics scenes and tangibility plan](2026-06-22_physics_scenes_and_tangibility_plan.md)
(the scene and ambient catalogs this one sits beside),
[browser WebRTC carrier plan](2026-08-25_browser_webrtc_carrier_plan.md)
(the web host and the scenario lane the receipts run on).
Donor sources, read 2026-09-02 from the archive (`mark-ik/graphshell`,
branch `webrender-wgpu-branch`, `design_docs/graphshell_docs/implementation_strategy/`):
`canvas/2026-02-24_physics_engine_extensibility_plan.md` (the Layout
Algorithm Reference Table and the Ten Thematic/Topological Physics
Presets), `canvas/layout_algorithm_portfolio_spec.md`,
`canvas/force_layout_and_barnes_hut_spec.md`,
`canvas/layout_behaviors_and_physics_spec.md`,
`system/register/physics_profile_registry_spec.md`.

## 1. The question, and what the code answers today

Mark's framing (2026-09-02): there are catalogs of arrangements, of
node/edge/field/canvas presentations, of scenes — so there should be a
catalog of physics layouts too, an additional lever a scene author reaches
for; and it must not collapse into tuned versions of one force-directed
graph. Verified against the crates:

- **cartography** owns two contracts: `LayoutStrategy` (analytic, one
  shot) and `StreamingLayoutStrategy` (iterative, host-owned serializable
  state). The canvas picker lists eight graph-only analytic arrangements
  (`CANVAS_LAYOUT_STRATEGIES`, `cartography_scene.rs:146`); the seiche
  force-directed default is the host's `None`, unlisted; the pure streaming
  strategies the donor had (`ForceDirected`, `BarnesHut`,
  `SemanticEdgeWeight`) no longer exist in mere's crates — they left with
  `arrangements`.
- **seiche** owns the physics: a rapier world with `forces: Vec<Box<dyn
  Force>>` (`lib.rs:375`) and `add_force` only — no replace, no clear
  (`lib.rs:507`). Seven `Force`s: `NodeExclusion`, `EdgeSpring`, `Boundary`
  (installed at build, `seiche_bridge.rs:58`), `AnchorSpring` (the
  arrangement-as-attractor pull), `AffinitySpring` (semantic affinity),
  `CouplingForce` (fields — the donor's magnetic zones, landed), and
  `BarnesHutRepulsion` (built, exported, never added). A `Force` sees
  `ForceContext { bodies, colliders, joints, bodies_by_node, edges,
  repulsion_solver }` (`lib.rs:343`): positions and topology, no node
  attributes. Tunables: linear damping, pause/play, settle budgets. Two
  catalogs already live in this tier: thirteen declarative scenes
  (`scenes.rs`) and four ambient sims.
- **persistence** (`SavedSceneV1`, `product.rs:213`) carries
  `physics_paused` and `physics_damping`, nothing that names a law.
- **the web host** runs seiche inline through settle budgets (~6 s), same
  fixed force set; the remote board is drawn from the endpoint's score with
  no physics.

## 2. Terms, and the donor's catalog read in them

Three words are in play and they are not the same thing; the donor's docs
use all three, and the plan's first draft conflated the second and third.

- **Layout algorithm** (the graph-drawing literature's word): a procedure
  that computes positions from structure. Two kinds — *analytic*
  (grid, tree/Sugiyama, radial, phyllotaxis, Penrose, L-system, timeline,
  kanban, spectral, embedding) and *energy minimization* (spring-electrical
  à la Fruchterman–Reingold, Kamada–Kawai stress, LinLog/ForceAtlas2,
  annealing). Mere calls the analytic kind **arrangements**; the picker is
  their catalog. The energy kind, run to convergence, is what the donor's
  `arrangements` crate did (`ForceDirected`, `BarnesHut`); run live under
  rapier, it is the next thing.
- **Physics law** (this plan's word; the donor's "Dynamic physics"
  category): a dynamical system over bodies — what force each feels, from
  whom, as a function of what — integrated over time. Live FR is one;
  so are n-body gravitation, particle life, flocking, phase
  synchronization, fluids, rigid bodies with collision. This is seiche's
  tier, and the catalog this plan founds.
- **Physics profile / preset** (the donor's word, `physics_profile_registry_spec`):
  a *named parameter set and extra-force composition over one law* —
  `Liquid`, `Gas`, `Solid` were "semantic parameter sets over the
  Fruchterman–Reingold force model". That is the tuning tier; it is real
  and it stays, but it is not the catalog.

So the lever has three parts, and the donor had all three in some form:

1. **Laws** — distinct dynamics. The catalog.
2. **Overlays** — the donor's Level-2 "post-physics extra forces", each a
   seiche `Force` composable onto any law: `DegreeRepulsion` (hubs spread),
   `DomainCluster` (pull toward the centroid of same-site nodes),
   `HubGravity` (gravity scaling with log degree), `DepthGravity` (BFS depth
   drives one axis: roots up, leaves down), `GridSnap` (spring to the
   nearest grid point), `GravityLocus` (pull toward a point, optionally
   oscillating), and the ones mere already has: `CouplingForce` (fields,
   the donor's zones), `AffinitySpring` (semantic clustering),
   `AnchorSpring` (the arrangement's pull).
3. **Tunables** — damping, settle policy, pause/play, anchor strength,
   overlay strengths. A **profile** is a saved (law, overlays, tunables)
   triple with a name — the donor's ten presets are profiles, and they map
   cleanly:

| donor preset | law | overlays |
|---|---|---|
| liquid | Springs | weak locus |
| gas | Springs | none, no anchor |
| solid | Springs | domain cluster + degree repulsion |
| archipelago | Springs | strong domain cluster + degree repulsion |
| constellation | Springs | degree repulsion + hub gravity |
| crystal | Springs | grid snap |
| tide | Springs | oscillating locus (never settles) |
| sediment | Springs | depth gravity |
| magnet | Springs | fields (landed as `CouplingForce`) |
| void | Still | none |

Every one is the same law. That is the collapse Mark refused, and the
donor's own reference table already pointed past it: its "Constraint-Based
/ Elastic (rapier)" and "Semantic Embedding" rows are different physics,
not presets.

### The laws (v1 — all of them, ruled 2026-09-02; plain labels)

| id | label | dynamics | what it reveals | needs |
|---|---|---|---|---|
| `spring.rapier` | Springs | today's law: rigid-body exclusion, Hooke springs on edges, boundary | local structure, no overlap; the interactive default | edges |
| `charge.barnes-hut` | Charge | Coulomb repulsion between all bodies (1/d, Barnes-Hut O(n log n)) + edge springs: the Fruchterman–Reingold shape | evenly spread neighbourhoods, the classic force picture | edges |
| `stress.kamada-kawai` | Stress | every pair a spring whose rest length is graph distance × L | global distance fidelity: paths unroll to true length, far is far | all-pairs shortest paths, cached per topology |
| `energy.linlog` | Energy | attraction ∝ d on edges, repulsion ∝ 1/d overall, degree-weighted (LinLog / ForceAtlas2) | communities as islands, hubs central | edges, degree |
| `orbit.gravity` | Orbit | n-body gravitation, mass by degree, tangential initial velocity, no rest | the graph as a solar system: leaves orbit hubs | degree |
| `kinds.particle-life` | Kinds | particle life: each node has a kind; an asymmetric kind×kind attract/repel matrix (the ambient sim's law over the graph) | sorting, chasing and fleeing by kind; never at rest | a kind per node — the host's choice per scene (relation family, domain, facet), recorded in the saved scene |
| `flock.boids` | Flock | separation / alignment / cohesion; edge-neighbours are flockmates | constellations that move as groups | edges |
| `sync.kuramoto` | Sync | phase oscillators coupled along edges; angle = phase, radius = distance from focus | communities as phase clusters on a ring | edges, a focus |
| `flow.magnetic` | Flow | directed edges align to a field direction (magnetic springs); with depth gravity, the donor's sediment made a law | hierarchy and direction by physics, Sugiyama's reading without its layers | directed edges |
| `anneal.davidson-harel` | Anneal | stochastic descent on a general energy under a temperature schedule | a balanced settled picture; stochastic | edges |
| `still.default` | Still | no forces; positions are the arrangement's (the donor's void) | the arrangement alone | — |

### The overlays (v1, from the donor's Level 2)

`DegreeRepulsion`, `DomainCluster`, `HubGravity`, `DepthGravity`,
`GridSnap`, `GravityLocus` (with an optional oscillation, the donor's
tide) — six new seiche `Force`s, each composable onto any law; plus the
three already landed. Overlays are what a profile toggles; the donor's ten
presets become the first ten profiles, expressed as (law, overlays,
tunables) and named as they were — the evocative names are the product
tier and belong to profiles, not laws.

In code: seiche gains `set_forces` / `clear_forces`; laws and overlays are
seiche `Force`s (seiche is the published home, so mer3ly's live seiche
path can pick them up); the canvas owns the catalogs
(`CANVAS_PHYSICS_LAWS`, `CANVAS_PHYSICS_OVERLAYS`, `CANVAS_PHYSICS_PROFILES`,
mirroring `CANVAS_LAYOUT_STRATEGIES`), builds forces from graph attributes
(degree, kind, depth, domain, the distance table), and switches the live
simulation — inline or actor — with `Canvas::set_physics_law(id)`,
`set_physics_overlays(ids)`, `apply_physics_profile(id)`.

## 3. Phases

**P1 — laws and overlays in seiche, the catalogs in the canvas.** seiche
gains `set_forces` / `clear_forces` (additive) and the `Force`s: laws
`StressSpring`, `LinLogForce`, `Gravity` (n-body), `ParticleLife`,
`Boids`, `Kuramoto`, `MagneticSpring`, `Anneal`; overlays
`DegreeRepulsion`, `DomainCluster`, `HubGravity`, `DepthGravity`,
`GridSnap`, `GravityLocus`. Each law carries a unit test stating what it
reveals — Stress: a 4-node path settles with end-to-end distance ≈ 3L;
Energy: two cliques joined by one edge sit further apart than under
Springs; Orbit and Kinds and Flock: kinetic energy above a floor after
600 ticks; Kinds: intra-kind distance below inter-kind; Sync: two
communities end in two phase clusters; Flow: a directed chain ends
monotone along the field; Anneal: energy after the schedule below energy
before. Each overlay carries one (degree repulsion spreads hubs; depth
gravity orders a tree top-down; grid snap lands on grid points). The
canvas gains `PhysicsLaw` / `PhysicsOverlay` / `PhysicsProfile` values,
the three catalogs, the setters, the `PhysicsCommand::SetForces` mirror
for the actor, attribute builders (degree, kind by the host's choice,
depth from a focus, domain from URL, the distance table, rebuilt on
topology change as couplings are), and the persisted `physics_law`,
`physics_overlays`, `physics_kind_source` fields on `SavedSceneV1`
(optional, defaults `spring.rapier`, none, `relation-family`). Field
couplings stay under every law; the anchor pull is a tunable. *Done when*
a law or overlay switch on a live canvas replaces the force set without
moving a body until the next tick, every combination round-trips through
the saved scene, the law and overlay tests are green, and a catalog test
proves every id builds and every label is plain.

*Landed 2026-09-02.* seiche: `set_forces` / `clear_forces` /
`velocity_of` / `kinetic_energy`; `laws/` (`StressSpring` +
`graph_distances`, `LinLogForce`, `Gravity`, `ParticleLife`, `Boids`,
`Kuramoto`, `MagneticSpring`, `Anneal`) and `overlays/`
(`DegreeRepulsion`, `DomainCluster`, `HubGravity`, `DepthGravity`,
`GridSnap`, `GravityLocus` with `tidal`), one test each stating what it
reveals; 73 seiche tests green. Canvas: `physics_catalog.rs` with
`PhysicsLaw` / `PhysicsOverlay` / `PhysicsKindSource` / `PhysicsProfile`,
the three catalogs plus `CANVAS_PHYSICS_KIND_SOURCES`, the setters
(`set_physics_law`, `set_physics_overlays`, `toggle_physics_overlay`,
`set_physics_kind_source`, `apply_physics_profile`, `physics_profile_id`),
`LawInputs` (the attribute builders: degree, site groups, cluster groups,
BFS depth, the distance table), `PhysicsCommand::SetForces` for the
actor, and the graph-bound rebuild on `reconcile_derived`; four catalog
tests green (every id round-trips, every law and overlay builds on the
sample graph without moving a body, every profile applies and names
itself back, a living law keeps ticking and Stress survives a topology
change). Graphshell: `SavedSceneV1` gains `physics_law`,
`physics_overlays`, `physics_kind_source` with serde defaults (a legacy
scene opens as Springs); the web host saves them from the canvas and
re-applies them on restore. The web-side restore/save edit
(`web_product.rs`) is wasm-only and could not be compiled this session:
the clean genet worktree the web build reads from
(`worktrees/genet-head`) is gone from disk (see Findings). *2026-09-03:*
the worktree was recreated at the `577e2471e97` pin and the wasm check
passed; P1 is fully verified.

**P1b — the petgraph sources (ruled 2026-09-03, landed the same day).**
The kernel's graph is a petgraph 0.8.3 `StableGraph` behind chartulary,
and petgraph's algorithm shelf maps onto the attributes the laws and
overlays already snapshot, so it joins the catalog as *sources* (where an
attribute is read from — tunables, not laws). The canvas builds a
[`TopologyView`] over the **visible** edges (a directed multigraph, one
edge per relation cell; an undirected simple graph with cost
`1 / multiplicity`), so hidden relations relax the physics as they relax
the springs, and runs: `dsatur_coloring` and `tarjan_scc` for two more
kind sources (**By colouring**: a kind never touches its own kind; **By
island**: each component a kind); `page_rank` as a **mass source** for
Orbit and the hub overlays (`PhysicsMassSource::{Degree, PageRank}`;
seiche's `HubGravity` / `DegreeRepulsion` gain `with_weights`);
`greedy_feedback_arc_set` + `toposort` longest-path layering and
`dominators::simple_fast` as two more **depth sources**
(`PhysicsDepthSource::{Roots, Layers, Focus}`); `min_spanning_tree` as
the **Skeleton** overlay (tree edges stiff, via `StressSpring` at one
hop) with a `skeleton` profile over Charge; and per-node `dijkstra` for
the Stress law's distance table, so a pair joined by three relations sits
at a third of a hop (seiche's `StressSpring::from_weighted_distances`).
`SavedSceneV1` gains `physics_mass_source` and `physics_depth_source`
with defaults. Four more catalog tests (a path two-colours and islands
separate; the hub outranks its linkers and weights average one; layers
survive a cycle and dominators count from the focus, the unreachable node
one below the deepest; the tree takes the thrice-joined pair and Stress
reads it as a third of a hop). seiche 74/74, canvas 192/192, graphshell
scene tests 6/6, wasm check green.

**P2 — the levers in both hosts, with a receipt per law.** The product
panel gains a law picker, overlay toggles and a profile picker beside the
arrangement picker (web: `physics-select`, `overlay-<id>` checkboxes,
`profile-select`; native: the same commands); the chrome hint reads
`<arrangement> · <law>`; the saved scene carries all three. The web
snapshot exposes `physics-law`, `physics-overlays`, `physics-energy`
(kinetic energy). Receipts: `physics_<law>.scn` per law with capture pairs
across `settle 60`, asserting the law's own signature (Stress: two nodes
three edges apart end further than adjacent; Orbit/Kinds/Flock: energy
above a floor at the end; Charge/Energy: no overlapping bodies; Springs
and Still: at rest), and one `physics_profiles.scn` applying each of the
ten donor profiles and asserting its overlays are the ones installed.
*Done when* eleven law receipts and the profile receipt are green and the
captures show what the labels say.

*Landed 2026-09-03 (web half).* The panel gains `physics-select`, an
`overlay-<id>` checkbox per overlay, `kind-source-select` /
`mass-source-select` / `depth-source-select`, `profile-select`, and the
`apply-physics` / `apply-profile` commands; the selects are filled from
the canvas catalogs the first time the panel is seen empty, and follow the
canvas after a profile or a reopened scene. The arrangement picker gains
**Free (physics alone)**, the canvas's `None`. The chrome hint reads
`<arrangement> · <law> · physics <state>`. The snapshot exposes
`physics-law`, `physics-overlays`, `physics-profile` (`custom` when no
profile names the pair), the three sources, `physics-paused` (now the
canvas's own state), and the layout's signature numbers from
`Canvas::layout_stats` — `physics-energy`, `layout-spread`,
`layout-overlaps`, `layout-stretch`. The scenario lane gains `select
<css> <value>` and `check <css> on|off`. Twelve receipts green
(`Code/testing/mere/physics_p2_receipt.md`), and five defects found by
them fixed: the pause toggle, the anchors under every law, Orbit under
damping, Charge's calibration, Still's explosion (see Findings). The
native half is unbuilt: no native surface carries an arrangement picker
today, so the open decision below is where the first one goes.

**P3 — physics on the remote board.** The web host seeds seiche bodies
from the remote scene's items — the projection's score is the seed and,
through the anchor pull, an attractor — and runs the chosen law over them,
reading positions back for drawing (the seam doc's seed/read pair, on the
remote side). A card appended by the host enters with a settle burst.
Host interaction policy; the endpoint's score is never written back.
*Done when* `c4b1_live_board` under Charge shows the second card settle
away from the first (positions and a capture pair) and under Orbit the
cards keep moving.

**P4 — drag and add.** The web pointer path reaches `pointer_down/up`;
verify drag pins and re-settles under each law, and adding a node fans it
out and settles (or joins the motion under the restless laws); one receipt
row per law; the donor's reheat-on-structural-change and
place-near-parent contracts (`layout_behaviors_and_physics_spec.md` §2)
adopted as the add behaviour. *Done when* the rows are green on the web
and the native host shows the same by hand.

## 4. Findings

- 2026-09-02: `BarnesHutRepulsion` has been one `add_force` from live since
  2026-06 (`2026-07-03_archived_plan_tails_plan.md:170`); Charge is its
  honest home, because it replaces `NodeExclusion`'s pairwise exclusion
  rather than adding to it — a different physics, not a tuning.
- 2026-09-02: the donor's ten presets are one law with different
  overlays (table above); its reference table's genuinely different physics
  were rapier constraints and semantic embedding, and its
  `force_layout_and_barnes_hut_spec` §5 said in as many words that
  Barnes-Hut "is a scaling implementation choice, not a new user-facing
  semantics model". The donor never had a law catalog; it had a profile
  registry over FR. This plan is the first to separate the two.
- 2026-09-02: a `Force` sees positions and topology but no node
  attributes (`lib.rs:343`); laws and overlays that need degree, kind,
  depth, domain or graph distance take them at construction from the
  canvas, the way `CouplingForce` snapshots its targets, and are rebuilt on
  topology change as couplings are.
- 2026-09-02: `Force` is `&self` and forces run in registration order;
  laws with interior state (Sync's phases, Anneal's temperature, the
  oscillating locus's clock) keep it behind a `Mutex`; a law plus its
  overlays is one ordered `Vec<Box<dyn Force>>` that `set_forces`
  installs in the declared order (law first, overlays after, couplings
  last).
- 2026-09-02: on the web the simulation is inline and ticks only through
  settle budgets (`input.rs:590` is the continuous run the play control
  enters); Orbit, Kinds, Flock, Sync and the oscillating locus never rest,
  so a law declares `wants_continuous_tick`, the rider the physics scenes
  plan added for perpetual scenes. Landed as `PhysicsLaw::never_rests` /
  `PhysicsOverlay::never_rests` on the canvas side: a switch to a living
  law settles for `u32::MAX` (the play control's continuous run) instead
  of the ordinary budget, so no seiche-side rider was needed.
- 2026-09-02 (P1): the node snapshot every law starts from iterated
  `bodies_by_node`, a `HashMap`, so a seeded law (Anneal's walk) drew its
  random numbers in a different node order per process and a seeded run
  did not reproduce. `laws::node_positions` now sorts by node index; every
  law and overlay reads through it.
- 2026-09-02 (P1): rapier weighs every node body the same (density-scaled
  to mass ≈ 1), so Orbit's degree masses must be gravitational only: the
  law applies `m_inertial · G · m_other / d²`, and the one-time kick is
  the circular speed for the mass *inside* each body's radius (sorted,
  prefix-summed), or the outer leaves start unbound.
- 2026-09-02 (P1): `EdgeSpring` holds spokes at 170 with stiffness 10;
  an inverse-square hub push sized safely for close range is under a
  pixel of displacement at that reach, so `DegreeRepulsion` falls off as
  `1/d`. The overlay tests are with/without comparisons on the same graph
  for this reason: an overlay that does nothing measurable is a defect.
- 2026-09-02 (P1): the profile catalog test found Gas ≡ bare Charge,
  Magnet ≡ bare Flow, Void ≡ bare Still; a picker cannot name the live
  choice when two profiles coincide, so the bare-law profiles exist only
  for the eight laws the donor's ten do not already offer bare.
- 2026-09-02 (P1): the web build's genet worktree
  (`C:/Users/mark_/Code/worktrees/genet-head`, the machine-local
  `.cargo/config.toml` redirect) no longer exists — genet's worktree list
  shows only `repos/genet` (main at `76a47850946`, 34 commits past the
  `577e2471e97` buckram pin, mid crate-split and dirty in cambium) and a
  Codex worktree. Recreating it at `577e2471e97` (or wherever the config
  was last proven) is the way back to a wasm build; that is a git action
  in a repo this plan does not own, so it waits for Mark.
- 2026-09-02 (P1): `tests::fold_and_source_time::source_time_canvas_scrubs_every_canvas_arrangement_without_rewriting_live_truth`
  failed once in the full canvas run and passed three times alone; the
  compared bytes are a graph snapshot that carries `timestamp_secs`, so a
  second boundary between the two serializations breaks it. Pre-existing,
  not touched here.
- 2026-09-03 (P1b): the kernel's petgraph is `pub(crate)` inside
  `chartulary::Graph`, and the signals crate reaches it only through a
  two-method `TopologyView` trait (keys + undirected neighbours), so the
  canvas builds its own petgraph view from the visible edge list rather
  than borrowing the kernel's. Linear in the graph, rebuilt per force
  rebuild, and it respects hidden relations, which the kernel's inner
  graph could not.
- 2026-09-03 (P1b): the Focus depth source snapshots the focused node at
  build; a selection change alone does not rebuild the forces (only a
  topology change or a source switch does). Open: whether a focus change
  under the Depth overlay should rebuild — a small P2 item if the receipt
  shows it matters.
- 2026-09-03 (P1b): asserting the same relation kind twice between a pair
  is idempotent in the kernel, so a multiplicity fixture must lay distinct
  kinds; the catalog tests' `wired` helper cycles through four.
- 2026-09-03 (P2): the web host's pause toggle flipped a remembered flag
  that the canvas's own pause (an arrangement pick pauses it) had left
  behind, so the first click paused a paused sim; every first-run law
  switch happened frozen in the spiral seed. The canvas is the authority
  now, for the toggle and the snapshot field both.
- 2026-09-03 (P2): the arrangement's anchor springs act under every law
  (by design — the anchor pull is a tunable), so a law receipt under the
  boot Spiral measures the anchors: Stress stretched 1.45 instead of ~3.5,
  Flow was squeezed into seven overlaps. The web picker had no way to say
  "no arrangement"; it has **Free** now, and the receipts pick it first.
- 2026-09-03 (P2): Orbit died under the host's damping within six
  seconds — gravity conserves energy only in a frictionless world, and the
  seiche test ran too short under too little damping to see it. `Gravity`
  cancels each body's damping with `m·d·v` along its velocity
  (`counter_damping`), and the receipt reads energy at one and six seconds.
- 2026-09-03 (P2): `BarnesHutRepulsion`'s default strength (2 400, `1/d`)
  is a third of `NodeExclusion` at a node diameter, exactly the
  recalibration its own docs deferred; Charge is built at 6 000 in the
  canvas, matched at contact. Two touching pairs became none.
- 2026-09-03 (P2): Still as an *empty* force set exploded (energy 10⁶,
  bodies off the canvas): rapier's contact solver alone met the tight free
  seed and accumulated separating velocity every tick, with no repulsion to
  spread the bodies first as every other law has. Still is the seiche
  `Hold` force now — velocity zeroed each tick — so contacts nudge.
- 2026-09-03 (P2): under Springs with no arrangement the fixture never
  reaches "energy ≤ 5": a probe read the spring ring down in four seconds
  and then a steady outward drift, the two disconnected components pushing
  apart against the weak `Boundary` at a few pixels a second. The trio's
  free equilibrium on a disconnected graph is slow; nobody saw it because
  the canvas has always run under an arrangement. The receipt asserts the
  honest signature (ring-down gone, no overlaps, not flying apart); the
  drift itself is a `Boundary` / exclusion-cutoff tuning left open.
- 2026-09-03 (P2): the driver's exit code reports the driver, not the
  scenario; `result.json`'s `scenario.state` is the receipt.
- 2026-09-03 (P2): `Canvas::layout_stats` runs a BFS from every node per
  frame for `stretch`; trivial on the fixture, `O(n·m)` on a large graph.
  Worth gating on node count if the web host ever carries thousands.

## 5. Decisions

Ruled 2026-09-02: laws are distinct dynamics, never tunings of one; all
eleven laws in v1 ("plenty"); labels plain, ids technical; the kind for
Kinds is the host's choice per scene; the catalogs live in the canvas and
the forces in seiche; the DOC_README index line added this session; the
donor's ten preset names return as the first ten profiles. Open: where the
native picker sits (the product panel, or the scene settings page the
physics-scenes plan founded).

Taken in P1, for Mark to confirm or overturn: the kind sources v1 offers
are **by site** (URL host), **by cluster** (the Louvain partition) and
**by degree** (isolated / leaf / connected / hub), default *site* — not
the `relation-family` default P1's text named, because a node's relation
family is a graphshell-tier taxonomy (`RelationFamilyFilter`) the canvas
does not see; adding it means threading the family per node down as an
attribute, which is a small P2 item if wanted. A long tail of sites folds
into eight kinds (particle life reads best with a handful).

Ruled 2026-09-03: the petgraph shelf joins as sources, all four packs
(kinds by colouring + island, mass by PageRank, depth by layers +
dominators, the Skeleton overlay + weighted Stress), landed before the
pickers as P1b so P2 exposes every source in one pass. Not taken:
matching, a Steiner tree over the selection, cliques as groups — no
inference carried.

## Progress

- 2026-09-02: assessed against the crates and the donor's archived docs;
  plan written; the first draft's collapse into tunings rejected by Mark
  and rewritten as laws × overlays × tunables.
- 2026-09-02: P1 landed — eight laws and six overlays in seiche, the
  catalogs, setters and attribute builders in the canvas, the three saved-
  scene fields in graphshell; seiche 73/73, canvas 188/188 (one
  pre-existing clock flake, see Findings), graphshell scene tests 6/6
  under `web,personal-sync,native`. The wasm check of the web host's
  restore/save edit is blocked on the missing genet worktree.
- 2026-09-03: the genet worktree recreated at the pin, the P1 wasm check
  green; P1b landed — the petgraph sources (two kind sources, a mass
  source, two depth sources, the Skeleton overlay, weighted Stress) and
  their saved-scene fields; seiche 74/74, canvas 192/192, graphshell
  scene tests 6/6, wasm check green.
- 2026-09-03: P2's web half landed — the panel's law / overlay / source /
  profile controls, the Free arrangement, the chrome hint, the signature
  snapshot fields, the `select` / `check` verbs, twelve receipts green
  after four driver runs that found and fixed five defects (the pause
  toggle, the anchors, Orbit under damping, Charge's calibration, Still's
  explosion); `Hold` joins the laws. seiche 76/76, canvas 193/193. Receipt:
  `Code/testing/mere/physics_p2_receipt.md`. Native half open on the
  picker decision.
