# Physics Scenes and Tangibility Plan: shared-world environments for the orrery

**Date**: 2026-06-22
**Status**: Planning (with Mark). The build plan for the
[orrery physics environments research](../research/2026-06-22_orrery_physics_environments_research.md):
non-node physics scenes sharing the orrery's rapier world, the interactive/intangible tangibility
lever, and the research's two features (living backdrop, interactive scene) plus liquid. The
dimensional modes are owned by the
[isometric orrery camera plan](2026-06-22_isometric_orrery_camera_plan.md); this plan is the scene
*content and physics*, that one is the *view*.
**Code**: `crates/orrery/gyre` (the `Simulation`; scene bodies are the gap), `crates/orrery/orrery`
(`frame.rs` paint + the scene paint pass), `crates/platen/platen/src/scene_paint.rs` (the ground
layer), `crates/meerkat` (the tangibility command + scene picker), optional new `crates/orrery/scene`
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
- [command_registry_configurable_menus_plan](2026-06-21_command_registry_configurable_menus_plan.md)
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
- **Scene crate vs in-orrery.** A new `crates/orrery/scene` for the `SceneSpec` + transplanted
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
