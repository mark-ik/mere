# Orrery Physics Environments Research: living backdrops, interactive scenes, and a dimensional hotswitch

**Date**: 2026-06-22
**Status**: Research / design probe (with Mark). Came out of "we have physics systems; could we
make open-source physically-defined scenes into interactive orrery backgrounds?" Mark's direction:
nodes share the scene's world; a per-node tangibility dial (interactive vs intangible); living
backdrops and interactive scenes are two features; liquid wanted; a 2D / 2.5D / 3D dimensional
hotswitch is welcome and may be its own plan. This doc frames the design space and code seams;
the two features and the hotswitch each graduate to their own `implementation_strategy/` plan when
committed.
**Code surveyed (read-only)**: `crates/orrery/gyre` (the rapier world), `crates/platen` (scene
paint / backdrop), `crates/orrery/aether` (fields / couplings).

**Related**:

- [node_representation_arrangement_plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md)
  — the per-node "scripted" representation form is *per-node* scene decoration; an environment is
  *scene-wide*, so it is a different layer. Representation stays orthogonal (a node keeps its form
  inside any environment).
- [scriptable_field_regions_plan](../implementation_strategy/2026-06-13_scriptable_field_regions_plan.md)
  — a placed field region already drives local forces via rhai (gyre couplings). A local scene
  effect (a whirlpool, a gravity well) is the same substrate; the environment is the global cousin.
- [cartography_aether_layout_seam](../technical_architecture/2026-05-29_cartography_aether_layout_seam.md)
  — gyre is already modelled as a rapier object world (hard collision, drag-pin, settle, inertia);
  an environment adds non-node bodies to that same world.
- [native_surface_compositing_plan](../../archive_docs/2026-07-03_completed_plans/2026-06-19_native_surface_compositing_plan.md)
  — `compose_external_texture` is the seam a 3D render lane would ride (a 3D scene rasterized
  elsewhere, composited under the chrome). Load-bearing for the hotswitch's 3D mode.
- [graph_projections_research](2026-06-22_graph_projections_research.md) — sibling probe (the five
  graph projections). This one is about the spatial / physical *backdrop*, not a reading of truth.

---

## Two features, one world

Mark's two anchors: nodes live in the **same** rapier world as the scene, and "living backdrops vs
interactive scenes are really two features." Both hold, and they are orthogonal to a third dial.

- **Same world.** gyre already runs one rapier2d `Simulation` per graph. Scene bodies are simply
  rapier bodies and colliders that carry no `NodeKey` (the `bodies_by_node` bimap stays
  node-only). They tick, collide, and settle in the same world the nodes do. No second engine.
- **Feature 1, living backdrop.** Ambient, atmospheric, cheap, default-off, never changes how the
  graph behaves. Drifting motes, a slow n-body orbit, falling leaves, a current. Its job is
  aliveness, not interaction.
- **Feature 2, interactive scene.** A physical place the graph shares: ramps, containers, joints,
  a drum, liquid. Its job is manipulation and play. Heavier, and the place where the tangibility
  dial earns its keep.
- **The third dial (tangibility) is orthogonal to both.** A backdrop can be tangible (nodes bump
  the leaves) or not; an interactive scene can be a thing you watch (intangible) or a thing you
  shove your graph through (tangible). So tangibility is a setting, not the feature boundary. The
  feature boundary is *intent and budget*: ambient decoration vs a physical playground.

Keeping them two features matters because they differ in content authoring, perf budget, default
tangibility, and whether they persist with the scene. Conflating them is how a cheap mood-setter
turns into a frame-killing simulation by accident.

## The tangibility dial (interactive vs intangible)

This maps cleanly onto a real, cheap rapier mechanism, and gyre does not use it yet, so it is
purely additive.

- **Mechanism.** rapier filters contacts two ways: `collision_groups` (skips contact *computation*
  in the narrow phase, the cheapest) and `solver_groups` (computes contacts and events but skips
  the *force*). The dial wants `collision_groups`: put node colliders in membership `NODE`, scene
  bodies in `SCENE`. The pairwise test passes only if each side's filter admits the other's
  membership, so flipping the **node** collider's filter alone controls node-scene contact:
  - **Interactive node** filter = `NODE | SCENE` (collides with both).
  - **Intangible node** filter = `NODE` only (passes through the scene, still collides with other
    nodes).
  Scene-scene contact is independent, so the scene stays internally physical either way.
- **Granularity.** Because it is a per-collider mask, it is a per-node toggle *or* a global mode for
  free: most nodes intangible, the one you are dragging tangible, or a scene-wide switch. It rides
  the command registry as `scene.tangibility` (global) and the node context menu / object card
  (per node).
- **Floating nodes over a scene with gravity.** gyre runs zero world gravity today (forces supply
  direction). An interactive scene that wants gravity sets it on the world and gives node bodies
  `gravity_scale(0.0)` (a per-body rapier knob) so the graph keeps floating under its force-directed
  layout while props fall. Scene props get `gravity_scale(1.0)`.
- **Code seam.** `Simulation::sync_nodes` builds each node collider with
  `ColliderBuilder::ball(NODE_BODY_RADIUS).density(..).restitution(0).friction(0)` and sets **no**
  groups (gyre/src/lib.rs). Adding `.collision_groups(..)` there, plus a `set_node_tangibility`
  method that re-masks live node colliders (the same shape `set_node_colliders` already uses to
  re-mask shapes), is the whole mechanism.

## Feature 1: living backdrop

A backdrop is the cheapest, most decoupled option, and is what most people mean by "interactive
background." Two builds, in rising cost:

- **Passive bodies, intangible.** A handful of non-node rapier bodies in the same world, in group
  `SCENE` with nodes intangible, drifting under a gentle force or low gravity. Painted under the
  nodes by `platen::scene_paint`. Almost free; reuses everything.
- **A separate ambient sim.** For motion the rapier world should not carry (an n-body orbital
  drift, particle-life, a cellular-automata field), a small standalone sim painted as a backdrop
  layer, not a rapier world at all. gyre already ships a Barnes-Hut N-body primitive
  (`barnes_hut.rs`), so a slow gravitational drift of background motes is close to home. Thematically
  apt: an orrery is a clockwork of orbiting bodies.

Open-source starting points (all permissive, verify each LICENSE before transplant): `particular`
(N-body, Barnes-Hut + wgpu) and `nbody-wasm-sim` for orbits; `par-particle-life` for particle life;
`sandspiel` / Powder Toy for falling-sand. These render *behind* the graph and never touch the
solver, so they are the safest first taste of "the orrery feels alive."

## Feature 2: interactive scene (including liquid)

The shared physical place. Because gyre is rapier2d, the richest near-free source is **rapier's own
`examples2d/` scene corpus** (Apache-2.0): the setup code builds bodies, colliders, and joints in
exactly the API gyre wraps, so a scene transplants into a gyre "environment" with no new dependency.
Evocative candidates: `drum2` (a rotating drum tumbling bodies), `s2d_card_house` / `s2d_arch` /
`s2d_bridge` / `s2d_ball_and_chain` (structures that settle and topple), `pyramid`, `one_way_platforms2`,
`heightfield2`.

- **Liquid.** Salva (dimforge, Apache-2.0) is SPH fluid with documented two-way coupling to rapier:
  fluid is pushed by rapier bodies and pushes back. A fountain or a pool the graph can stir. It is
  the heaviest option (SPH is per-particle per frame), so it is the clearest case for the perf
  budget below, and a strong default-off, opt-in scene.
- **Perf and the actor.** gyre's `Simulation` is `Send` and already runs on an off-UI-thread physics
  actor. A heavier scene or a fluid rides that actor, so a frame is never blocked, but the tick
  budget is shared with the layout forces. An interactive scene wants a body / particle cap and a
  "pause when idle" rule (gyre already has `is_at_rest`).
- **Authoring.** A scene is a set of non-node bodies / colliders / joints plus optional forces.
  Worth a small declarative scene format (positions, shapes, joints, gravity, tangibility default)
  so scenes are data, not code, and the field-regions rhai substrate can drive local effects.

## The dimensional hotswitch (its own plan)

Mark asked whether 2D (default) / 2.5D (isometric, free cam) / 3D could be a selectable mode, and
guessed it might be a separate plan. It should be, and here is why, plus the model that keeps it
tractable.

The hotswitch conflates two axes that are better separated:

1. **Physics dimensionality.** rapier2d and rapier3d are **separate crates with separate types**
   (Vector2 vs Vector3, Isometry2 vs Isometry3). There is no runtime "switch dimension" on one
   world. So a real 2D-to-3D switch is either: run rapier3d always and lock bodies to a plane via
   `LockedAxes` for the flat modes (unlock for 3D), or keep parallel worlds and rebuild on switch.
2. **Camera / projection.** Top-down orthographic (classic 2D), isometric (the 2.5D look), or
   perspective free-cam (the 3D look). This is a render concern, independent of whether the physics
   is 2D or 3D.

The crux, and the reason it is a separate plan: **our render stack is 2D** (platen `scene_paint` to
netrender to a vello-style 2D scene). The three modes therefore cost very differently:

- **2D mode** = rapier2d + orthographic. Today.
- **2.5D mode** = an **isometric camera with fake height over the existing 2D world**. Physics stays
  rapier2d; the painter projects positions isometrically, depth-sorts, and adds drop-shadows for
  height. Shippable in-stack, no rapier3d, no new renderer. This is the cheap, high-value middle
  rung, and it likely satisfies "2.5D isometric, free cam" if 2.5D means "looks dimensional, orbits
  freely, but motion is planar."
- **3D mode** = rapier3d + perspective free-cam + a **new 3D render lane**. The 2D painter cannot
  draw a general 3D scene; this needs either a 3D pipeline (wgpu directly, or a 3D crate) composited
  in through the existing `compose_external_texture` seam, or projecting simple 3D geometry to 2D
  painted polygons (fine for stylized scenes, not general). This is the long pole.

So the plannable shape is: treat the **camera / projection as the primary, cheap switch** (2D ortho
to 2.5D isometric over the same 2D physics), and treat **physics dimensionality + a 3D render lane
as the secondary, gated switch**. The near-term win is 2.5D isometric; full 3D is its own milestone
behind the render lane. The dimensional hotswitch graduates to
`implementation_strategy/` as its own plan; this section is the framing, not that plan.

## Mounting points (code-verified 2026-06-22)

- **One rapier2d world per graph**: `gyre::Simulation` (rapier2d 0.22). Non-node scene bodies live
  in the same `RigidBodySet` without a `bodies_by_node` entry.
- **Colliders carry no interaction groups today** (`sync_nodes`, gyre/src/lib.rs:527). The
  tangibility dial adds `collision_groups` there plus a live re-mask method (mirrors
  `set_node_colliders`).
- **Non-uniform colliders already supported**: `NodeCollider` (Ball / Square / RoundedSquare /
  Hull) lowers to parry shapes; scene props reuse the same lowering.
- **Pluggable forces**: the `Force` trait + `ForceContext` (bodies, colliders, joints, query index);
  built-ins NodeExclusion / EdgeSpring / Boundary; `barnes_hut` N-body primitive present. A scene
  force or a per-body `gravity_scale` slots here.
- **Off-thread**: `Simulation` is `Send`, runs on the physics actor; `is_at_rest` gives the
  pause-when-idle hook a heavy scene needs.
- **Backdrop home**: `platen::scene_paint` paints under nodes; the apparatus theme already keeps an
  orrery backdrop palette (where the background visually lives).
- **Licensing**: gyre is MPL-2.0; rapier2d / salva are Apache-2.0 deps (compatible). rapier scene
  code is Apache-2.0; preserve attribution on any transplanted setup code.

## Sequencing (cheapest first)

1. **Tangibility dial** — additive collision-groups change + the global / per-node toggle. Useful on
   its own (and prerequisite for any tangible scene).
2. **Living backdrop, passive bodies** — a few intangible scene bodies in the same world, painted
   behind the nodes. Proves the world-sharing and the backdrop paint path.
3. **Living backdrop, separate ambient sim** — an N-body / particle-life / CA layer for motion the
   solver should not carry.
4. **Interactive scene** — transplant one rapier `examples2d` scene as data; add the body cap and
   pause-when-idle.
5. **Liquid** — salva, opt-in, budgeted on the actor.
6. **Dimensional hotswitch** — its own plan: 2.5D isometric camera first (no rapier3d), full 3D
   render lane gated behind it.

## Open questions

- **Scene format.** Code, or a declarative data file (shapes / joints / gravity / tangibility
  default)? Lean data, so scenes are shareable and the rhai field substrate can drive local effects.
- **Per-graph vs per-scene persistence.** Does a chosen environment persist with the session (the
  cartography sidecar, where sizes / positions already live), and is it per graph or a global vibe?
- **Tangibility default per feature.** Backdrops default intangible, interactive scenes default
  tangible? Confirm, and whether the dial is global, per-node, or both (the mechanism supports all).
- **Budget policy.** A hard body / particle cap and an idle-pause are needed; what are the numbers,
  and does a fluid get its own lower cap?
- **2.5D scope.** Is "2.5D, free cam" satisfied by an isometric camera over 2D physics, or does Mark
  want true planar-locked 3D? Decides whether rapier3d enters at the 2.5D rung or only at full 3D.
- **3D render lane.** A composited 3D pipeline (via `compose_external_texture`) vs 3D-to-2D polygon
  projection. The defining choice of the hotswitch plan.

## Grounding (code-verified 2026-06-22)

- `gyre::Simulation` is one rapier2d 0.22 world; `sync_nodes` spawns ball colliders with no
  interaction groups; `NodeCollider` + `set_node_colliders` already re-mask collider shapes live.
- rapier filtering: `collision_groups` (skip contact computation) vs `solver_groups` (skip force);
  16-bit membership + 16-bit filter, pairwise-AND test. Confirmed against rapier docs.
- rapier2d and rapier3d are separate crates; `LockedAxes` locks translation / rotation axes (the
  planar-in-3D mechanism); per-body `gravity_scale` lets nodes float in a gravity world.
- salva (Apache-2.0) two-way couples SPH fluid to rapier; heaviest option, actor-budgeted.
- Mere render path is 2D (scene_paint to netrender to vello-style scene); 3D needs a new lane via
  `compose_external_texture` or 3D-to-2D projection. This is the hotswitch's load-bearing cost.

## Progress

- 2026-06-22: **Created from Mark's "physics scenes as interactive backgrounds" direction.**
  Code-verified the gyre world (one rapier2d Simulation, no collider groups today, Send / off-thread,
  Barnes-Hut present, NodeCollider re-masking). Confirmed the tangibility dial is a clean additive
  `collision_groups` change (per-node or global) with `gravity_scale(0)` keeping nodes afloat over a
  gravity scene. Surveyed open-source scene sources (rapier `examples2d` transplant since gyre is
  rapier; salva for liquid; particular / nbody-wasm-sim / par-particle-life / sandspiel for ambient
  backdrops). Framed the two features (living backdrop vs interactive scene) with tangibility as an
  orthogonal dial, and decomposed the dimensional hotswitch into physics-dimensionality vs
  camera-projection, with the 2D render stack as the reason full 3D earns its own plan (2.5D
  isometric is the cheap in-stack rung). No code; the two features and the hotswitch each spin out to
  their own `implementation_strategy/` plan when picked up.
