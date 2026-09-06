# Physics Scenes and Tangibility Plan: shared-world environments for the orrery

**Date**: 2026-06-22
**Status**: Planning (with Mark). The build plan for the
[orrery physics environments research](../research/2026-06-22_orrery_physics_environments_research.md):
non-node physics scenes sharing the orrery's rapier world, the interactive/intangible tangibility
lever, and the research's two features (living backdrop, interactive scene) plus liquid. The
dimensional modes are owned by the
[isometric orrery camera plan](2026-06-22_isometric_orrery_camera_plan.md); this plan is the scene
*content and physics*, that one is the *view*.
**Code**: `crates/orrery/gyre` *(historical citation)* <!-- doc-audit: historical-path --> (the `Simulation`; scene bodies are the gap), `crates/orrery/orrery` *(historical citation)* <!-- doc-audit: historical-path -->
(`frame.rs` paint + the scene paint pass), `crates/canvas/canvas/src/scene_paint.rs` (the ground
layer), `crates/meerkat` *(historical citation)* <!-- doc-audit: historical-path --> (the tangibility command + scene picker), optional new `crates/orrery/scene` *(planned target)* <!-- doc-audit: planned-path -->
(scene format + transplanted scenes), `salva2d` (liquid).

**Related**:

- [orrery_physics_environments_research](../research/2026-06-22_orrery_physics_environments_research.md)
  — the parent probe (the two features, the tangibility dial, the option survey). This plan builds it.
- [isometric_orrery_camera_plan](2026-06-22_isometric_orrery_camera_plan.md) — scene bodies render
  in the **ground layer** that plan defines, so they shear isometrically (correct: a scene sits on
  the floor); a scene prop with a face can be a billboard like a node. The two plans compose.
- [node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md) — `gyre`
  already supports non-uniform colliders (`NodeCollider` Ball/Square/RoundedSquare/Hull); scene props
  reuse that lowering. Nodes are the billboards a tangible scene pushes around.
- [scriptable_field_regions_plan](2026-06-13_scriptable_field_regions_plan.md) — a placed field
  region's rhai rules already drive local forces; a localized scene effect (a whirlpool, a
  gravity well) rides that substrate. The environment is the global cousin.
- [cartography_aether_layout_seam](../technical_architecture/2026-05-29_cartography_aether_layout_seam.md)
  — gyre is already the rapier object world; this adds non-node citizens to it.
- [command_registry_configurable_menus_plan](../../archive_docs/2026-09-02_retired_plans/2026-06-21_command_registry_configurable_menus_plan.md)
  — the tangibility toggle and the scene picker are registry commands.

---

## Two features, one substrate

Per the research, a **living backdrop** (ambient, cheap, default-intangible) and an **interactive
scene** (a shared physical place, tangible, with props and liquid) are two features. They are two
*configurations* of one substrate: non-node bodies in the orrery's rapier world, a tangibility
filter on whether nodes touch them, a paint pass, and a perf budget. This plan builds the substrate
once and ships both features on it. Tangibility is an orthogonal dial, not the feature boundary, so
a backdrop can be tangible and an interactive scene can be intangible if the user wants.

## The scene world: non-node bodies in the same Simulation (the core gap)

Mark's anchor is that nodes share the scene's world. gyre runs one rapier2d `Simulation` per graph,
but it only tracks **node** bodies: `bodies_by_node` is the sole registry, and `sync_nodes`
reconciles the body set to exactly the node list (gyre/lib.rs:511-567). There is no API to add,
iterate, or remove a non-node body. So the foundational work is a **scene-body** concept in gyre:

- `add_scene_body(SceneBodySpec) -> SceneBodyId` / `remove_scene_body` / `clear_scene` and an
  iterator `scene_bodies() -> (SceneBodyId, transform, NodeCollider-ish shape)` for the paint pass.
  A `SceneBodyId` distinct from `NodeKey`, tracked in a parallel `scene_bodies` map so `sync_nodes`
  never reaps them (it iterates `bodies_by_node` only, so scene bodies already survive a sync; this
  just gives them a home and a paint handle).
- Scene bodies reuse the existing `NodeCollider` lowering (Ball / Square / RoundedSquare / Hull,
  gyre/lib.rs:90-133) for shape, plus body type (dynamic / fixed / kinematic), density, restitution,
  friction, and joints (rapier joints already live in the `Simulation`).
- **The layout forces must ignore scene bodies.** `NodeExclusion` and the cull-based neighbor
  queries read the shared `query_index`, which after this sees scene colliders too. Filter those
  queries to the `NODE` collision group (below) so a scene prop never enters the graph's
  force-directed layout. Forces already iterate `bodies_by_node`, so per-node force application is
  fine; only the spatial-query neighbor lookups need the group filter.

This is the one genuinely new mechanism. Everything else composes existing parts.

## The tangibility lever (collision groups + gravity scale)

The interactive/intangible dial maps onto rapier's cheapest filter, and gyre sets no groups today
(`ColliderBuilder::ball(..).density(..).restitution(0).friction(0)`, gyre/lib.rs:527), so it is
additive.

- **Groups.** Node colliders join membership `NODE`, scene colliders join `SCENE`. rapier's pairwise
  test passes only if each side's filter admits the other's membership, so flipping the **node**
  collider's filter alone controls node-scene contact:
  - **Interactive node** filter = `NODE | SCENE` (collides with the scene).
  - **Intangible node** filter = `NODE` (passes through the scene, still collides with nodes).
  Scene-scene contact is independent (`SCENE` admits `SCENE`), so the scene stays internally
  physical either way. Use `collision_groups` (skips contact computation, cheapest), not
  `solver_groups`.
- **Granularity.** A per-collider mask gives a per-node toggle and a global mode for free: drag-one
  tangible, the rest intangible, or a scene-wide switch. `Simulation::set_node_tangibility(node,
  bool)` re-masks one live collider (mirrors `set_node_colliders`, gyre/lib.rs:278); a global form
  re-masks all node colliders.
- **Floating nodes over a gravity scene.** gyre runs zero world gravity (forces supply direction,
  gyre/lib.rs:236). An interactive scene that wants gravity sets the world `gravity` and gives node
  bodies `gravity_scale(0.0)` (a per-body rapier knob set in the `RigidBodyBuilder`, gyre/lib.rs:521)
  so the graph keeps floating under its force-directed layout while props fall. Scene props get
  `gravity_scale(1.0)`.

## Painting scenes (the ground layer, tied to the iso camera)

`scene_paint` paints a cartography `Projection` (graph nodes + edges); scene bodies are not graph
nodes, so they need their own pass. Add a `paint_scene(scene_bodies, camera, style) ->
CanvasPaintList` that draws each scene body's shape at its transform, composited as a layer between
the orrery backdrop and the graph underlay (the layer vec, frame.rs:254-274). Under the
[isometric camera](2026-06-22_isometric_orrery_camera_plan.md), scene bodies live in the **ground
layer** (they sit on the floor, so the iso shear is correct); a scene prop with a face (a textured
crate) can opt into the billboard path like a node. Scene props can carry textures via the same
favicon/sprite `ImageResource` path the nodes use.

## The scene content menu (all options, licenses code-checked)

Surfacing the research's survey as the concrete content menu. gyre is rapier2d 0.22, so the
same-engine sources transplant with no new dependency.

**Tier 1, direct transplant (rapier `examples2d`, Apache-2.0, no new dep).** Setup code builds
bodies / colliders / joints in exactly the API gyre wraps. Candidate scenes (verified to exist in
the corpus): `drum2` (a rotating drum tumbling bodies), `s2d_card_house`, `s2d_arch`, `s2d_bridge`,
`s2d_ball_and_chain` (structures that settle and topple), `pyramid` / `inv_pyramid2`,
`one_way_platforms2`, `heightfield2`, `joints2` / `rope_joints2`. The transplant is harvesting the
setup into a `SceneBodySpec` list. Preserve attribution on transplanted setup code.

**Tier 1b, liquid (salva2d, Apache-2.0).** SPH fluid with documented two-way coupling to rapier:
fluid pushed by bodies and pushing back. A fountain or a pool the graph stirs. Heaviest option (SPH
per particle per frame), so it is opt-in, capped, and budgeted on the physics actor (below).

**Tier 2, ambient separate sims (the living backdrop, not a rapier world).** For motion the solver
should not carry, a small standalone sim painted as a backdrop layer: `particular` (N-body,
Barnes-Hut + wgpu) or `nbody-wasm-sim` for an orbital drift (thematically apt, and gyre already
has a Barnes-Hut primitive, gyre/lib.rs:57); `par-particle-life` for particle life; `sandspiel` /
Powder Toy for falling-sand. Verify each LICENSE before transplant (sandspiel's was not clearly
permissive at a glance).

**Tier 3, design reference (port, do not depend).** The Matter.js demo gallery (MIT, JS) is the
richest catalogue of interactive 2D scenes; read for ideas, reimplement in rapier.

## Perf and the actor budget

`Simulation` is `Send` and runs on the off-UI-thread physics actor (gyre/lib.rs:701-708), so a heavy
scene never blocks a frame, but it shares the tick budget with the layout forces. An interactive
scene or a fluid needs: a **body / particle cap** (refuse or LOD past it), **pause-when-idle**
(`is_at_rest` already exists, gyre/lib.rs:651), and a fluid getting its own lower cap. Default-off,
opt-in.

## Phases (done-conditions, not dates)

### P1 — Scene-world substrate + the living-backdrop MVP

Add the scene-body concept to gyre (`add_scene_body` / `remove_scene_body` / `clear_scene` /
`scene_bodies`), the `NODE` / `SCENE` collision groups on node and scene colliders (nodes default
**intangible**: filter `NODE` only), node `gravity_scale(0)` wiring, and the `paint_scene` pass
composited under the graph underlay. Ship a couple of passive, intangible backdrop bodies (drifting
under a gentle force or low gravity) as the living-backdrop proof.

Done when an intangible backdrop of a few bodies drifts behind the graph, the graph's layout and
drag are visibly unaffected by it, and the layout forces ignore the scene bodies (verified by a
headed drive).

### P2 — The tangibility lever

`set_node_tangibility` (per node) + a global mode; surface as the `scene.tangibility` registry
command and a per-node toggle on the context menu / object card. Add an optional world-gravity scene
preset with node `gravity_scale(0)` so nodes float while props fall.

Done when toggling a node (or the scene) to interactive makes it collide with scene bodies while
intangible passes through, node-node collision unaffected either way, and a gravity preset drops
props while the graph floats.

### P3 — Scene format + transplant one interactive scene

A declarative `SceneSpec` (bodies: shape / transform / body-type / material; joints; gravity;
default tangibility) so scenes are data, not code, loadable per scene and shareable. Transplant one
rapier `examples2d` scene (lean `drum2` or `s2d_card_house`) into a `SceneSpec`, selectable from a
scene picker (registry command). Apply the body cap + pause-when-idle.

Done when a user picks an interactive scene, it loads as data, the graph can knock its bodies around
when tangible, and the cap / idle-pause hold.

### P4 — Richer scenes: catalog now, extensions next, own fluid later

A workflow investigation (2026-06-23) reframed the original "liquid (salva)" rung. salva is out: it
lags rapier badly (it needs rapier 0.23 while gyre is now on 0.33), so adopting it would re-pin rapier
backward. So liquid means our own small SPH. And the cheapest richness is declarative scenes, which
subsumes "import the easy ones" and "make from the JS demos" into one thing: a `fn -> SceneSpec`. P4 is
therefore a sequence.

**P4a, declarative scene catalog (done).** A library of ready-made scenes in `gyre::scenes`, plus a
`perpetual` flag on `SceneSpec` and the keep-ticking rider so a backdrop that never settles keeps the
actor ticking instead of parking at rest.

**P4b, scene-spec extensions.** The highest unlock-per-effort: wire the already-owned but currently
dead `ImpulseJointSet` into `load_scene` so `SceneSpec` can express joints (Newton's cradle, rope
bridge, the rotating drum, ragdolls, spring-mesh cloth), plus a one-line per-body `gravity_scale`
(buoyancy, mixed falling/floating). A shape-aware scene paint pass belongs here too: square and hull
scene bodies currently paint as round orbs, so the pyramid and domino read poorly.

**P4c, own-SPH fluid (the big rock).** Position-Based Fluids (PBF), our own `gyre::fluid` module,
unconditionally stable at the 1/60s tick with no salva dep. A particle pool with uniform-grid
neighbours, two-way coupling gated by the same tangibility lever, painted as soft metaballs behind the
graph. Four sub-phases: settling pool, render, coupling, emitter/drain.

Done (the rung as a whole) when a liquid scene simulates and couples within the actor budget; P4a and
P4b land first as the cheap richness.

### P5 — Ambient separate-sim backdrop

A non-rapier ambient layer (start with an N-body orbital drift over gyre's Barnes-Hut, or particle
life) painted as the bottom backdrop, for liveliness the solver should not carry. Default-off, a
backdrop picker option.

Done when an ambient sim renders behind the graph as atmosphere, independent of the rapier world,
within budget.

## Open questions

- **Where the scene world lives.** Scene bodies in the node `Simulation` (one world, simplest,
  Mark's same-world ask) vs a sibling `Simulation` for separation. Lean one world (the ask), with
  collision groups keeping the layout clean.
- **Scene crate vs in-orrery.** A new `crates/orrery/scene` *(planned target)* <!-- doc-audit: planned-path --> for the `SceneSpec` + transplanted
  scenes, or a module in `orrery`? Lean a small crate so scenes are shareable and testable headless.
- **Tangibility default per feature.** Backdrop intangible, interactive scene tangible? Confirm, and
  whether the lever is per-node, global, or both (the mechanism supports all).
- **Scene persistence.** Does a chosen scene persist with the session (the cartography sidecar, where
  sizes / positions already ride) and is it per graph or a global vibe?
- **Budget numbers.** The body cap, the fluid particle cap, and the idle-pause threshold.
- **Scene-prop billboards.** Which scene props are ground-shear (most) vs upright billboards (a
  textured crate, a standee)? Ties to the iso camera plan's billboard path.

## Grounding (code-verified 2026-06-22)

- gyre runs one rapier2d 0.22 `Simulation`; `bodies_by_node` is the only body registry; `sync_nodes`
  reaps only node bodies (gyre/lib.rs:511-567), so scene bodies need a new add/iterate/paint API but
  are not at risk from a sync.
- Node colliders are built with no interaction groups (gyre/lib.rs:527); the tangibility lever adds
  `collision_groups` there + a live re-mask (mirrors `set_node_colliders`, gyre/lib.rs:278).
- World gravity is zero (gyre/lib.rs:236); per-body `gravity_scale` (set in the `RigidBodyBuilder`)
  floats nodes over a gravity scene.
- `NodeCollider` (Ball / Square / RoundedSquare / Hull) lowers to parry shapes (gyre/lib.rs:90-133);
  scene props reuse it. rapier joints already live in the `Simulation`.
- The layout forces' cull-based neighbor queries read the shared `query_index` (gyre/lib.rs, the
  `ForceContext.query_index`), so they must filter to the `NODE` group to skip scene bodies.
- `Simulation` is `Send`, off-thread; `is_at_rest` (gyre/lib.rs:651) gives the idle-pause hook.
- `scene_paint` paints a graph `Projection` only (scene_paint.rs); scene bodies need a sibling
  `paint_scene` pass composited as a layer (frame.rs:254-274), in the iso ground layer.
- rapier `examples2d` corpus + salva2d confirmed Apache-2.0; `collision_groups` semantics
  (membership | filter, 16 bits each, pairwise-AND) confirmed against rapier docs.

## Progress

- 2026-06-22: **Plan written (with Mark), code-verified against gyre + the orrery paint path.**
  Carried the physics-environments research's two features + option survey into a build plan. Key
  finding: gyre tracks only node bodies (`bodies_by_node`; `sync_nodes` reaps to the node set), so a
  **scene-body API is the foundational gap**, while the tangibility lever is additive (rapier
  `collision_groups`, none set today, flip the node filter; `gravity_scale(0)` floats nodes over a
  gravity scene; the layout forces must group-filter their spatial queries to skip scene bodies).
  Sequenced P1 scene-world substrate + intangible living-backdrop MVP → P2 the tangibility lever
  (per-node + global, gravity preset) → P3 scene format + transplant a rapier `examples2d` scene
  (the interactive-scene feature) → P4 liquid (salva) → P5 ambient separate-sim backdrop. Surfaced
  the full content menu with licenses (rapier examples2d transplant, salva, particular / nbody /
  particle-life / sandspiel, Matter.js reference). Scenes render in the iso camera plan's ground
  layer. No code yet.
- 2026-06-23: **P1 substrate landed + tested (committed `5ad0bb8`); the paint + drifting-backdrop
  demo is the next slice.** Added the gyre scene-body API (`add_scene_body` / `remove_scene_body` /
  `clear_scene` / `scene_bodies` / `scene_body_count`, minting a `SceneBodyId`) plus NODE/SCENE
  `collision_groups` — node colliders join `NODE`/filter-`NODE` (intangible to the scene by default);
  scene colliders join `SCENE`/filter-`SCENE|NODE`, so a node opts into contact by adding `SCENE` to
  its own filter (the P2 lever). **Correction to the plan + Grounding**: the layout forces do **not**
  need a group-filter — `NodeExclusion` / `EdgeSpring` / `Boundary` already key off `bodies_by_node`
  and skip any collider whose `collider_to_node` returns `None`, so scene bodies (no `NodeKey`) are
  invisible to them automatically; only the collision groups (for hard-collision intangibility) were
  required. A unit test (`scene_body_is_intangible_to_nodes_and_ignored_by_forces`) proves an
  overlapping scene body never pushes a node and the scene clears cleanly; gyre 32 tests green, the
  orrery builds on it. **Remaining P1 (next slice)**: the orrery **paint pass** — extend
  `LayoutSnapshot` / `LayoutView` with scene positions so they ride the off-thread snapshot, paint
  them as a layer under the graph underlay (`frame.rs`), and add a few drifting backdrop bodies in
  the bin (added pre-offload, so no `PhysicsCommand` is needed) — then headed-verify the drift behind
  an unperturbed graph. `gravity_scale(0)` for a gravity scene rides that slice / P2.
- 2026-06-23: **P1 complete — the paint slice landed + headed-verified (committed `0f4569e`).** Scene
  bodies ride the off-thread snapshot: `LayoutSnapshot` / `LayoutView` carry `(SceneBodyId, position,
  radius)`, `Simulation::snapshot` emits them and `apply_snapshot` refreshes them, so the actor's
  per-tick snapshot flows the drifting positions with no extra channel work (plus a
  `PhysicsCommand::AddSceneBody` for post-offload adds). `frame.rs` paints each as a soft
  radial-gradient orb behind the graph (a layer between the backdrop and the edge underlay),
  projected through `Camera::to_screen` so it reclines with the iso ground. `Orrery::add_scene_body
  (position, radius, velocity)` is the host seam; the bin seeds four drifting backdrop orbs.
  Headed-verified (scry-shots/scene-00..02): the orbs render behind the graph, drift between frames,
  graph unperturbed. **Known limitation**: the physics actor parks at rest, so the backdrop freezes
  once the layout settles — a continuous-tick-while-scene mode (or a perpetual drift force) is a
  follow-on, traded against the idle-CPU saving.
- 2026-06-23: **P2 complete — the tangibility lever (committed `a5ff3c5`).** `set_node_tangibility`
  (per node) and `set_nodes_tangible` (scene-wide) re-mask the node collider filter (`NODE`
  intangible / `NODE|SCENE` tangible) via a `remask_node` helper; node-node collision is unaffected
  either way. Routed through the orrery + `PhysicsCommand::SetNodesTangible`; the bin's `t` key flips
  it. A unit test (`node_tangibility_toggles_scene_collision`) proves an overlapping scene body pushes
  a tangible node but not an intangible one. The P1 groups made this a pure filter flip.
  **Follow-ons**: a scene-wide toggle applies to the current nodes (new nodes spawn intangible —
  re-apply on add); the meerkat `scene.tangibility` command + per-node menu toggle wait for the
  command-registry pass.
- 2026-06-23: **P3 complete — scenes as data + a gravity scene (committed `63c6313`); headed-verified.**
  A declarative `SceneSpec` (bodies: shape / position / velocity / `SceneBodyType` Dynamic|Fixed /
  restitution; world gravity; default tangibility) loads via `Simulation::load_scene` (clears any
  prior scene, capped at `SCENE_BODY_CAP`). Nodes carry `gravity_scale(0)`, so scene gravity acts only
  on scene bodies — the graph layout is untouched. A transplanted `drop_bowl_scene` (a bumpy fixed
  floor + dynamic balls falling onto it) ships via `Orrery::load_demo_scene`; the bin's `1` / `0`
  load / clear it (routed through `PhysicsCommand::LoadScene` / `ClearScene`). A unit test
  (`load_scene_falls_under_gravity_while_nodes_float`) proves the balls fall while the node floats;
  gyre 34 + orrery 44 green. Headed-verified (scry-shots/p3-00..01): the balls fall and pile on the
  floor behind an unperturbed graph. **Follow-ons**: joints (to transplant the drum / card-house
  scenes), a serde scene-file format (shareable scenes, not just code), the meerkat scene-picker
  command, then P4 liquid (salva) and P5 the ambient separate-sim backdrop.
- 2026-06-23: **rapier 0.22 to 0.33 migration (committed `e01a6f4`).** Bumped gyre (the workspace's
  only rapier consumer) to upstream-current rapier, unblocking own-SPH fluid on a modern base and
  settling the salva question (salva caps at rapier 0.23, so it is out). rapier 0.33 moved to glam
  vectors, added `InteractionTestMode` to `InteractionGroups::new`, and made the `QueryPipeline`
  ephemeral, so `hit_test` / `cull_aabb` now read live body positions (matching the `LayoutView` the
  orrery picks from) and `NodeExclusion` went all-pairs. 34 gyre + 44 orrery tests green; headed
  re-verified (drop-bowl still falls and piles).
- 2026-06-23: **P4a complete, declarative scene catalog + keep-ticking rider; headed-verified.** Five
  new scenes join `drop_bowl` in a new `gyre::scenes` module (kept out of the over-ceiling `lib.rs`):
  `pyramid_scene` (topple-able stack), `domino_scene` (nudged cascade), `galton_scene` (Plinko
  peg-field bell curve), `funnel_scene` (hourglass pour), and `drift_scene` (perpetual gravity-free
  orbs). All port demo-gallery mechanics (matter.js / planck.js / box2d) to today's `SceneSpec`, no new
  dep. A `perpetual: bool` on `SceneSpec` (plus `Simulation::scene_perpetual` and
  `PERPETUAL_SCENE_DAMPING`) drives the keep-ticking rider: the physics actor and the inline path keep
  ticking instead of parking once a perpetual scene is live, so the drift never freezes. The bin loads
  them on keys `1`-`6` (`0` clears); `Orrery::load_scene(SceneSpec)` plus the re-exported catalog are
  the host seam. gyre 36 + orrery 44 green; headed-verified (scry-shots/p4a-*): pyramid stacks, galton
  scatters and piles, and the two drift frames differ (perpetual motion confirmed). **Follow-ons**:
  P4b joints + `gravity_scale` + a shape-aware scene paint pass (square/hull bodies paint as round orbs
  today, so pyramid/domino read poorly); drift orbs slowly disperse without a centering force (a P4b
  force-field item); then P4c own-PBF fluid.
- 2026-06-23: **P4b part 1 complete, joints + per-body `gravity_scale` / `rotation`; headed-verified.**
  The scene format moved to its own `gyre::scene_spec` module (the format grows independently of the
  `Simulation` core). `SceneBodySpec` gains `gravity_scale` (floats pegs / buoyant props, negative
  rises) and `rotation` (place a tilted prop), with ergonomic `dynamic` / `fixed` + chaining builders.
  The headline unlock: the already-owned-but-dead `ImpulseJointSet` is now wired — `SceneSpec` carries
  `joints: Vec<SceneJointSpec>` (Fixed / Revolute-with-optional-motor / Rope / Spring, indexing bodies
  by spawn order), and `load_scene` builds + inserts them via the rapier 0.33 joint builders (reaped
  with their bodies, so no extra bookkeeping). A `chain_scene` (eight rope-linked balls hung from a
  fixed anchor) is the proof + the demo, on bin key `7`. gyre 37 + orrery 44 green; headed-verified
  (scry-shots/p4b-chain-*): the chain falls from horizontal and hangs as a connected vertical line of
  links from the anchor (without joints the links would fall away). **Still pending in P4b**: the
  shape-aware scene paint pass — the orrery paint API here is axis-aligned `RectItem` + radial gradients
  only, so rotated squares / tipped dominoes / hull polygons need a new render primitive (a rotated-rect
  or polygon `PaintCmd`); until then square/hull scene bodies still paint as round orbs (balls, incl.
  the chain beads, read fine). Also pending: `lib.rs` is 1044 LOC (the `Simulation` core, over the 600
  ceiling pre-existing) — a `Simulation` decomposition is its own follow-on.
- 2026-06-23: **P4b part 2 complete, shape-aware scene paint; headed-verified.** No new render
  primitive was needed after all — netrender's `PaintCmd::DrawPath` (filled Bezier path) was already
  there. Scene bodies now carry their collider shape + live rotation through the snapshot (a
  `SceneBodyView { id, position, rotation, collider }` replaces the old `(id, pos, radius)` tuple on
  `LayoutSnapshot` / `LayoutView`). `frame.rs` paints a ball as the soft radial-gradient orb (the calm
  backdrop look, unchanged) and a square / rounded-square / hull as a filled polygon: each body-local
  corner is rotated by the body's angle, translated to world, and projected per-corner through the
  camera, so the shape reclines with the iso ground and shows its true orientation. gyre 37 + orrery 44
  green; headed-verified (scry-shots/p4a-*): the pyramid reads as a stepped stack of square blocks and
  the dominoes render as tilted bars (the cascade reads — upright on one end, fallen at angles on the
  other). Ball scenes (galton, drift, chain) keep the orb look. **P4b done** bar the deferred
  `Simulation` decomposition (`lib.rs` still over the 600 ceiling, pre-existing). Next rung: P4c
  own-PBF fluid.
- 2026-06-23: **P4c complete, our own Position-Based Fluids (a settling pool the graph can stir);
  headed-verified.** Salva stays ruled out (caps at rapier 0.23), so gyre grows its own small SPH
  liquid in three steps. **P4c-a** (`bf34f14`): a standalone PBF solver in `gyre::fluid`
  (Macklin/Mueller 2013): predict, density constraint via the poly6 kernel, lambda, spiky-gradient
  position correction (Jacobi), velocity update, XSPH viscosity, analytic `Basin` boundary; rest
  density auto-set from spawn spacing. **P4c-b** (`80386e5`, after the in-flight-tree checkpoint
  `277751d`): wired into the live `Simulation` (`load_fluid` / `clear_fluid` / `has_fluid`, stepped
  each `tick`, riding the snapshot as fluid particle positions + radius), painted in `frame.rs` as
  soft cyan metaballs (a layer above the scene, below the graph), routed through
  `PhysicsCommand::LoadFluid` / `ClearFluid`; bin key `8` drops a pool, `0` clears. The keep-ticking
  rider generalised to `wants_continuous_tick` (perpetual scene or fluid). **P4c-c** (`971f089`):
  two-way coupling, the pool pushes out of every scene body and (when the graph is tangible) every
  node body, then shoves the dynamic ones back with a gentle impulse, so a tangible graph stirs the
  pool and floating props bob (circle-approximated; true collider-shape coupling is a refinement).
  Tests prove the pool settles finite in the basin, contacts report a reaction, and a tangible node
  displaces the pool to ~its radius. Headed-verified (scry-shots/p4c-fluid-*, p4cc-*): a cyan pool
  settles in the bowl and parts around a tangible node.
- 2026-06-23: **Force-field tier, a whirlpool vortex over the scene (committed `6945026`);
  headed-verified.** `SceneField::Vortex { center, strength, inward }` applies a tangential swirl
  plus inward pull to every dynamic scene body each tick (`apply_scene_field`, after the force loops,
  before the step); set via `Simulation::set_scene_field` and `PhysicsCommand::SetSceneField`, and it
  joins `wants_continuous_tick` so the swirl never freezes. `whirlpool_scene` (two rings of loose
  balls over a centred vortex) is the demo on bin key `9`. A unit test proves a CCW vortex swirls a
  +x-axis body toward +y and that clearing the scene drops the field. Headed-verified
  (scry-shots/p4w-*): the rings orbit the centre. **Follow-on**: reuse aether's `CouplingForce` for
  richer fields (scalar potentials, n-body wells) on scene bodies.
- 2026-06-23: **Emitters, a fountain that sprays and recirculates (committed `b5fbcf2`);
  headed-verified.** A `SceneEmitter` (collider, position + velocity with deterministic jitter, rate,
  lifetime, max-alive) spawns dynamic scene bodies over time and reaps each past its lifetime
  (oldest-first), so the live count holds a bounded steady state near rate*lifetime. `Simulation`
  owns a `LiveEmitter` list (`add_emitter` / `clear_emitters` / `emitter_count`; `step_emitters` runs
  before the step), with a tiny xorshift PRNG for reproducible jitter (no `rand` dep); emitters join
  `wants_continuous_tick`. Routed through `PhysicsCommand::AddEmitter` / `ClearEmitters`.
  `fountain_scene` (a catch basin) plus `Orrery::load_fountain` (basin + an upward emitter) ship on
  bin key `f`. A unit test proves the count rises then holds bounded and that clearing removes the
  spawned bodies. Headed-verified (scry-shots/p4f-*): a spray of droplets fans across the basin.
  **Follow-on**: a bounds-drain (reap by region, not only age) and a fluid emitter (spray PBF
  particles, not rigid droplets).
- 2026-06-23: **`Simulation` decomposition, the deferred file-size cleanup (committed `9bed860`).**
  The over-ceiling `gyre/lib.rs` (1441 LOC) split by concern into impl-spanning modules; pure
  refactor, no behaviour change. `lib.rs` (557 LOC) keeps the struct, `new`, force registration, the
  rapier-free read model (`view` / `snapshot` / `hit_test` / `cull_aabb`), and the `tick` heartbeat.
  The tiers moved out: `node_body` (collider shape + material), `sync` (body lifecycle + accessors),
  `scene_sim` (rigid scene bodies, `load_scene` + joints, gravity, tangibility), `fluid_coupling`
  (load + two-way coupling; the solver stays in `fluid`), `field` (vortex + `wants_continuous_tick`),
  `emitter` (PRNG + spawn/reap), and `scene_tests` (the cross-tier integration tests). Cross-module
  helpers (`to_shared_shape`, `step_emitters`, `apply_scene_field`, `couple_fluid_to_bodies`) took
  `pub(crate)`; private struct fields stay reachable from the child modules. Every gyre source file
  is now under the 600 ceiling; 43 gyre + 45 orrery green, zero new warnings. **The physics-scenes
  feature arc is complete** bar the meerkat command-palette / scene-settings binding, which waits on
  the command-registry pass.
- 2026-06-23: **Scene settings page in the host (committed `06eae2b`) — the "scene settings" half of
  the binding.** The orrery's physics-backdrop scenes are now loadable from meerkat's pelt settings
  lane: a new `pelt/scene` page (`scene_settings.rs`) lists the `SceneSpec` catalog (drop bowl,
  pyramid, dominoes, Galton, funnel, drift, chain), the whirlpool / fountain effects, a liquid-pool
  load / clear, the graph-tangibility lever, and clear-scene. Each control drains a `scene:<...>` key
  through the existing pelt-activation path (`apply_pelt_activation` → `apply_scene_key`) to the
  matching `Orrery` method (which forwards to the physics actor), the same shared-builder + host-drain
  split the theme / engine / physics / orrery pages use. Kept self-contained in one new file so the
  already-over-ceiling `input.rs` / `settings_lane.rs` don't grow (they gain only a page ref + resolve
  arm + one delegating arm). Tangibility is two explicit buttons rather than one reflecting toggle —
  the physics is off-thread, so its live state isn't synchronously readable here; a reflecting toggle
  waits on a getter. Unit-tested (the page is listed in the pelt index and every control carries the
  expected `scene:`-prefixed key); meerkat compiles + links, scene tests green; the scene methods
  were each headed-verified earlier via the orrery bin. **Remaining**: the command-palette half — a
  `>scene <name>` omnibar verb (the parameterized-command pattern, a side-channel on the shell outcome
  like `relate("cites")`). That centrally edits the already-over-ceiling `command.rs`, so it likely
  wants a `command.rs` split first (a Mark-coordinated refactor of his registry). A headed pass through
  the lane UI also waits on a settings-navigation drive harness (none exists yet).
- 2026-06-24: **Bucket A joint scenes (committed `f0ce2f0`); headed-verified.** Four more catalog
  scenes exercising the P4b joints, all pure `SceneSpec` (no new engine code): `cradle_scene`
  (Newton's cradle, five elastic balls on rigid revolute-rod pendulums, the end one launched so the
  momentum clicks through the row), `bridge_scene` (a nine-plank suspension bridge revolute-hinged end
  to end and pinned to fixed posts, sagging under two dropped weights), `ball_and_chain_scene` (a
  wrecking ball on a five-link rope chain swung through a block tower it scatters), and `mixer_scene`
  (a paddle bar on a motorised revolute joint spinning inside a ring-of-balls bowl, flinging loose
  balls - the one scene that drives the revolute motor). Wired through the catalog re-exports (gyre +
  orrery), the orrery bin keys (c / b / k / m), and the meerkat Scene settings page. gyre 45 (+2
  behaviour tests: the motor spins the paddle, the loaded bridge sags), orrery 45, meerkat green;
  headed-verified (scry-shots/p4ba-*): the bridge sags into a clean catenary, the mixer paddle spins
  flinging balls round the bowl, the wrecking ball scatters the tower, the cradle hangs and swings
  (subtle - small balls behind the graph). Bucket B (the non-rapier ambient-sim backdrop layer: Game
  of Life / N-body / particle-life) is the next rung. The scenes paint as flat soft orbs + grey
  polygons today; textured / sprite scene props is an open question (see the Open questions list).
- 2026-06-24: **Opt-in sprite textures on scene props (committed `6477436`); headed-verified.** A
  scene prop can now wear a texture (a crate, a barrel) instead of the abstract orb / polygon - the
  groundwork for tangible / interactive props that read as real objects, while ambient backdrops stay
  calm by default (a prop without a sprite is unchanged). gyre stays physics-pure: `SceneBodySpec`
  gains an opaque `sprite: Option<String>` handle (builder `.sprite(h)`) that rides through to
  `SceneBodyView` via a sparse parallel map, exactly as the collider shape rides through for paint -
  gyre never interprets it. The orrery owns the pixels: a `handle -> RGBA` registry
  (`register_scene_sprite`, persisting across scene loads) and a `frame.rs` paint branch billboarding
  a `DrawImage` quad over a prop whose handle resolves (favicon-style `ImageResource`, image namespace
  2 so it never collides with favicons (0) / genet images (1)), sized to the prop's collider
  half-extent; an unregistered handle falls back to the polygon. A bin demo (a procedural crate
  texture and a crate-drop scene on key `x`) proves it. Billboards are axis-aligned for now (no
  rotation as a prop
  tumbles); rotating textured quads is the iso-billboard refinement (the open question on ground-shear
  vs upright props). gyre 46 + orrery 47 green (carry-through; registered prop emits an image op,
  unregistered falls back); headed-verified (scry-shots/p4cr-*): a pile of wood-textured crates behind
  the graph. Decision recorded: keep ambient backdrops abstract, expose sprites as the opt-in just
  landed - they earn their keep on foreground / interactive props, not background ambiance.
- 2026-06-24: **P5 opened - Game of Life ambient backdrop (Bucket B, committed `8f77fdb`);
  headed-verified.** The first non-rapier ambient sim: a sim painted behind the graph for liveliness
  the rapier solver should not carry. `ambient.rs` (new, orrery-side since it is not rapier physics,
  so not in gyre) holds a `GameOfLife` CA: `step()` is pure Conway (B3/S23) on a toroidal grid so it
  tests cleanly, `step_living()` the host wrapper that reseeds a dead / thinning field so the backdrop
  never freezes (deterministic random soup, no `rand` dep). The orrery holds `ambient:
  Option<GameOfLife>` + `load_game_of_life` / `clear_ambient`; `frame()` steps it every 7th frame (a
  watchable ~8 gen/s), paints the live cells as the bottom backdrop layer (above the bg fill, below
  the scene) with per-row run-merged rects in a muted low-alpha green, and keeps redrawing while
  loaded so it animates. Bin key `g` (and `0` clears it); meerkat Scene settings gains an Ambient
  section. orrery 52 tests (a blinker oscillates period-2, a block is a still life, the backdrop keeps
  the orrery redrawing until cleared) + meerkat green; headed-verified (scry-shots/p4gol-*): the CA
  grid evolves behind the graph between frames. **The ambient seam is now proven**; siblings (n-body
  drift over gyre's Barnes-Hut, particle-life) slot into the same `ambient` module / paint path next.
  Tuning follow-on: the cells read light-grey (the green tint washes out at 0.16 alpha) and are a
  touch prominent - the colour / alpha is one constant in `frame.rs` to taste.
- 2026-06-24: **`>scene <name>` omnibar verb (committed `96b1ebc`) - the command-palette half of the
  scene binding, with no `command.rs` touch.** No registry split was needed after all:
  `scene("pyramid")` / `>scene pyramid` rides the pure side-channel pattern `sparql` / `attach_script`
  use (a `ShellOutcome` field + a desugar so the bare two-token form works), not the `Command` enum.
  The page and the verb are unified on one flat-name vocabulary: a single
  `WindowCtx::load_named_scene(name) -> bool` (with aliases life->gol, wreck->ballchain, pool->fluid,
  galaxy->nbody, plife->particles) that both the Scene page buttons (now keyed `scene:<name>`) and the
  verb route through, so they cannot drift. meerkat lib + bin tests green (14 shell_eval incl. the
  scene verb).
- 2026-06-24: **Ambient-sim seam + tincture + two more sims (committed `1be0936` / `57005a0` /
  `366e4f0`); headed-verified.** Generalised the lone Game of Life into an `AmbientSim` trait
  (advance + paint + default_tincture) the orrery holds as `Box<dyn AmbientSim>`, and gave each
  backdrop a **tincture** - its base paint colour, set from the sim's default on load, overridable via
  `set_ambient_tincture` (the tint pass). Three sims ride the seam, each tinted: **Game of Life**
  recoloured from a grey wash to a phosphor green (a low alpha washed the hue out; a moderate alpha
  reads green while staying behind the graph); **n-body drift** (`load_nbody`, key `n`) - a cloud
  orbiting a central harmonic well (stable: no fly-away / collapse) with weak softened mutual gravity
  for clumping, painted as warm-gold star-dots, a galaxy-like swirl; **particle-life**
  (`load_particle_life`, key `p`) - a few species under an asymmetric attraction matrix
  self-organising into clusters / chains, each species a hue rotated from the tincture (a coherent
  re-tintable palette). All wired to bin keys + the meerkat Scene settings Ambient section + the
  `>scene` verb. orrery ambient tests green (Conway blinker / block; n-body bounded over 900 steps;
  particle-life wrapped + finite; hsv round-trips). Headed-verified (scry-shots/p4gol-*, p4nb-*,
  p4pl-*). Follow-on tuning (taste knobs, not blockers): a tighter n-body spiral (lower G or a
  Keplerian well for differential rotation), more dramatic particle-life structure (the attraction
  matrix / force scale).
- 2026-06-24: **Polish round - sim tuning, falling sand, scene paint, ambient split (committed
  `72febe8` / `63418f0` / `d4d04b6` / `49ef3a8`); headed-verified.** The taste-knob follow-ups, a
  fourth ambient sim, and a scene-look pass:
  - **n-body** swapped to a softened Keplerian well (differential rotation), so it reads as a centred
    galaxy rather than a uniform scatter; **particle-life** biased the attraction-matrix diagonal
    positive so per-species blobs always form (`72febe8`).
  - **Falling sand** (`load_sand`, key `s`, `>scene sand`): a fourth `AmbientSim` - grains pour from
    the top, pile into an angle-of-repose dune, and the grid resets when full so it cycles forever
    (`63418f0`).
  - **Scene paint polish** (`d4d04b6`): the rapier scene polygons (blocks, planks, floor slabs) gained
    a lit-edge stroke + a softened fill, and balls a brighter core - a block now reads as a defined,
    lightly-dimensional object and the big floor slabs stop reading as solid walls.
  - **ambient.rs split** (`49ef3a8`): the four-sim file had hit 814 LOC, over the ceiling - split into
    `ambient/` with one file per sim + the shared trait / `Tincture` / helpers in `mod.rs`.
  orrery 56 tests green throughout; headed-verified (scry-shots/p4nb-*, p4pl-*, p4sd-*, p4po-*). The
  ambient catalog is now **four** sims (Game of Life, n-body, particle-life, falling sand), the rapier
  scenes have form, and every orrery source file is under 600.
- 2026-06-24: **Capstone - multi-material sand, true-shape fluid coupling, oriented sprites (committed
  `7d05b85` / `23c6293` / `f1f05dc`); headed-verified.** The three deferred refinements:
  - **Multi-material falling sand**: water joins sand - water falls + spreads sideways to find its
    level while sand piles steep, and sand sinks through water (the denser). A `moved` buffer prevents
    same-step cascades; paint merges per-material runs (sand tincture, water blue). (`7d05b85`)
  - **True collider-shape fluid coupling**: `FluidContact` carries a `ContactShape` (circle | OBB); a
    square / rounded-square body couples at its true oriented-box face + rotation (the pool sits flat
    on a block and fills its corners) instead of an inscribed circle; ball / hull stay circles.
    (`23c6293`)
  - **Oriented sprite billboards**: a textured prop's sprite now rotates with the prop (a
    `PushTransform` spin about its projected anchor, identity at rest), so a tumbling crate's texture
    tumbles - the iso-billboard path for the open billboard question. (`f1f05dc`)
  gyre 47 + orrery 57 tests green (a blocked-water-spreads test, an OBB push-out test); headed-verified
  (scry-shots/p4sd-*, p4bf-*, p4cr-*): sand + water settle side by side, a pool drapes over the
  pyramid's blocks, crates settle at varied angles with their texture tilting to match. The
  physics-scenes arc is feature-complete.
