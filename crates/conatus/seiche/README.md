# seiche

Kernel-free, rapier-backed force integration for graph canvases. A host feeds
`(NodeKey, position)` pairs and edge pairs; `Simulation` owns the rapier world
that settles them and the host reads the positions back. Fields come from
[`quint`](../quint) and [`numen`](../numen); the graph itself stays host-side.

`NodeKey` is `petgraph::stable_graph::NodeIndex`, so a consumer supplies keys
from any graph or mints them directly. seiche never inspects them.

## Core types

| Item | Contents |
|---|---|
| `Simulation` | The rapier world, the `NodeKey` to `RigidBodyHandle` map, the registered forces, and the scene / fluid / emitter tiers. `Send`, so it can run on its own thread. |
| `Force` | `fn apply(&self, ctx: &mut ForceContext<'_>, dt: f32)`. Registered forces are applied in registration order each tick, before the step. |
| `ForceContext<'a>` | Per-tick view a force sees: `bodies`, `colliders`, `joints`, `bodies_by_node`, `edges`, `repulsion_solver`, `gpu_repulsion_threshold`. |
| `RepulsionSolver` | `Arc<dyn Fn(&[f32], &[f32], f32, f32) -> (Vec<f32>, Vec<f32>)>`. A host-injected pairwise-repulsion closure `NodeExclusion` routes to above a node-count threshold. |
| `NODE_BODY_RADIUS` / `NODE_BODY_DENSITY` | `18.0` and `0.001`, chosen so a node's mass is about 1.0. |
| `DEFAULT_GPU_REPULSION_THRESHOLD` | `1_000` nodes. |

## Modules

Every item below is re-exported at the crate root. `scene_spec`, `node_body`,
and `view` are private modules, so those types are reachable only as
`seiche::SceneSpec`, `seiche::NodeCollider`, `seiche::LayoutView`, and so on.

| Source | Public items | Contents |
|---|---|---|
| `forces` | `NodeExclusion`, `EdgeSpring`, `Boundary` | The built-in layout forces. |
| `barnes_hut` | `BarnesHutRepulsion`, `BarnesHutConfig`, `repulsion_forces` | Quadtree O(n log n) approximate n-body repulsion. `BarnesHutRepulsion` implements `Force`. |
| `coupling_force` | `CouplingForce` | Compiles an already-resolved field plus target set into a `Force`; quint evaluates, seiche applies. Built via `CouplingForce::new` and `with_registry`. |
| `affinity_force` | `AffinitySpring`, `DEFAULT_AFFINITY_STIFFNESS`, `DEFAULT_AFFINITY_REST_LENGTH` | Weighted attract-only spring over `(a, b, weight)` triples. |
| `anchor_force` | `AnchorSpring`, `DEFAULT_ANCHOR_STIFFNESS`, `DEFAULT_ANCHOR_SLACK` | Per-node springs toward arrangement-chosen slots. |
| `fluid` | `Fluid`, `FluidParams`, `Basin`, `FluidContact`, `ContactShape` | Position-Based Fluids solver, stepped after the rigid world and coupled two ways. |
| `scene_spec` | `SceneSpec`, `SceneBodySpec`, `SceneBodyId`, `SceneBodyType`, `SceneJoint`, `SceneJointSpec`, `JointMotorSpec`, `SceneEmitter`, `SceneField` | The declarative scene format. |
| `scenes` | `drop_bowl_scene`, `pyramid_scene`, `domino_scene`, `galton_scene`, `funnel_scene`, `drift_scene`, `chain_scene`, `ball_and_chain_scene`, `bridge_scene`, `cradle_scene`, `mixer_scene`, `whirlpool_scene`, `fountain_scene` | Ready-made `SceneSpec`s. Data, not engine code. |
| `node_body` | `NodeCollider`, `NodeMaterial` | A node body's collider shape and physical material. |
| `view` | `LayoutView`, `LayoutSnapshot`, `RectSelection`, `SceneBodyView` | The rapier-free read model. `LayoutSnapshot` is the `Send` payload a physics actor emits each tick; `LayoutView` applies it and answers picks, cull, edge segments, and marquee select on the UI thread. |

## Driving a simulation

- **Topology.** `sync_nodes` reconciles the body set to a `(NodeKey, position)`
  list, `sync_edges` sets the spring topology, `seed_positions` places bodies
  without reconciling. All idempotent.
- **Forces.** `add_force` for the built-ins, `set_coupling_forces` /
  `set_affinity_force` / `set_anchor_force` to replace those tiers wholesale,
  `set_repulsion_solver` to install a host solver. Force swaps leave body state
  untouched, so the layout stays where it is.
- **Stepping.** `tick(dt)` applies every force, steps the world, then steps the
  fluid and its coupling. `is_at_rest` and `wants_continuous_tick` answer
  whether the tick can be suspended.
- **Reading.** `positions`, `position_of`, `view`, `snapshot(generation)`, plus
  the in-thread `hit_test`, `cull_aabb`, `edge_segments`, `edge_hit_test`, and
  `rect_select`.
- **Dragging.** `pin` switches a body to kinematic-position control, `unpin`
  returns it to dynamic.
- **Scene, fluid, emitters.** `load_scene` / `add_scene_body` / `clear_scene` /
  `set_gravity` / `set_nodes_tangible`, `load_fluid` / `clear_fluid`,
  `set_scene_field`, `add_emitter` / `clear_emitters`. Scene loading and live
  emitters are both capped at 200 bodies.

Writing settled positions back into the host's own graph is the host's job.

## Features

`gpu-bench` pulls `quint/field-burn-wgpu` so an ignored settle benchmark can
inject the wgpu N-body pass as a `RepulsionSolver`. Off by default; the shipped
library does not compile burn.

## Dependencies

rapier2d 0.33, petgraph 0.8, quint (default features), numen, euclid, tracing.

## License

MIT OR Apache-2.0.
