# Conatus Shared Game Engine

**Date:** 2026-08-22  
**Status:** active; body/runtime foundation implemented  
**Scope:** Build the reusable spatial game engine. Mesocosm, Paredros,
Isometry, and Mere projections become consumers of it instead of incubating
engine features in product-local probes.

## Direction

Conatus is the engine, not a collection of physics experiments.

The engine owns reusable advancement and spatial machinery: the fixed-step
clock, body identities, transforms, collision, fields, resident allocations,
procedural geometry, scene preparation, and system execution. A game owns its
rules, durable world meaning, assets, and presentation choices. Netrender owns
frame composition. Renderling is a 3D render tenant. Rapier is the current CPU
collision and dynamics implementation. Nexus and its Khal kernels are source
material for the scale at which the CPU backend stops being the right
implementation.

Ordinary unit, property, and integration tests remain part of engine work.
Windowed demos, screenshots, receipt applications, and benchmark gates are
not the work queue. Performance work starts from a named engine workload and
changes the engine path itself.

## Existing pieces

| Piece | Role in the engine |
|---|---|
| `numen` | Serializable field and coupling vocabulary |
| `quint` | Field evaluation, CubeCL kernels, resident tensor/chunk allocations |
| `seiche` | Existing 2D graph-physics specialization; eventually an adapter over shared Conatus facilities |
| `conatus` | Shared 3D body world and host-neutral runtime |
| Netrender | One-device tenancy and final frame composition |
| Renderling | 3D scene/render implementation, consumed as a tenant |
| Mesocosm voxel types | First source material for generic voxel storage, revision, dirty-region, collision, and meshing features |

The dependency direction is engine to implementation only. Product crates
depend on Conatus. Conatus must not depend on a game.

## Implemented foundation

`crates/conatus/conatus` now supplies:

- stable generational body and collider identities;
- fixed, dynamic, and position/velocity-kinematic 3D bodies;
- compound colliders with sphere, box, capsule, cylinder, and sparse voxel
  shapes;
- materials, collision layers, sensors, gravity, damping, CCD, force, torque,
  and impulse operations;
- ray casts and shape-overlap queries using Conatus identities;
- contact and sensor entry/exit events;
- in-place sparse voxel collision edits with revisions;
- configurable kinematic character movement with wall sliding, slope limits,
  ground snap, and autostep;
- a drift-free fixed-step engine clock with explicit catch-up policy;
- ordered input, gameplay, field, before/after-physics, materialization, and
  render-preparation system phases;
- typed shared resources and deterministic registration order;
- a serializable structural command buffer applied between phases, with
  correlated command results;
- current-step physics changes and interaction events available to
  after-physics systems;
- frame changes containing final body transforms and removals, including host
  frames on which simulation takes zero steps;
- effective sparse voxel edit streams for materialization and render systems;
- serializable body, collider, voxel, filter, and character configuration.

Rapier types stay private. Replacing the backend does not change game-facing
ids, descriptors, queries, or frame changes.

## Engine growth

### 1. Runtime and systems (foundation implemented)

The ordered engine schedule now has concrete phases:

1. ingest commands;
2. run game systems;
3. evaluate fields;
4. advance tactile physics;
5. materialize voxel and geometry changes;
6. prepare render views;
7. publish frame changes.

Systems receive typed resources, current-step output after physics, and a
command buffer. Structural mutations
apply between phases, so scripts and parallel systems cannot invalidate a
live iteration. Tick-local events and durable game facts remain different
types.

The remaining runtime work is parallel system access declarations, command
sources with explicit authority, and a renderer-facing frame resource. A game
can already register systems, insert resources, queue structural commands, and
advance without writing a physics loop or borrowing Rapier types.

### 2. Voxel world

Promote the generic parts already present across Mesocosm and Quint:

- sparse chunk addressing and stable chunk ids;
- exact occupancy/material planes with per-chunk revision;
- dirty brick/region tracking and bounded edit batches;
- derived collision, surface, SDF, light, navigation, and mesh products;
- dependency stamps so each product updates only from changed source regions;
- streaming budgets and explicit residency states;
- body volumes using the same plane and product vocabulary as ground volumes.

Games define what a material or voxel means. The engine owns storage,
revision, locality, and derived spatial products.

Feature complete when terrain edits, body-volume edits, collision, queries,
and mesh/SDF preparation use one chunk/revision path and game crates carry no
second voxel cache protocol.

### 3. Resident spatial world

Join the CPU body world to Quint's resident allocation model:

- stable GPU slots with generations and free-list reuse;
- padded 3D position, rotation, velocity, force, and flags planes;
- changed-slot uploads for the Rapier tier;
- direct resident advancement for field/particle tiers;
- small reductions and accepted deltas as the only routine readback;
- leases with allocation epoch, source revision, shape, units, and valid read
  interval.

Burn/CubeCL handles dense fields and authored kernels. Khal/rust-gpu artifacts
are adopted where their explicit spatial algorithms are useful. Tool choice
follows the operation, while allocations and scheduling remain Conatus's.

Feature complete when CPU tactile bodies and GPU field bodies publish one
stable spatial-view vocabulary and can coexist in one engine world.

### 4. Scene and rendering

Add an engine-owned render view containing instances, cameras, lights,
sprites, voxel surfaces, particles, and visibility changes. It is a projection
of engine state, not a renderer scene graph.

Renderling consumes the 3D portion through a tenant adapter. Mere's spatial
renderer can consume the same view. Netrender receives each tenant's frame
entry and composes them on the host's device and queue.

Feature complete when a product selects render tenants and presentation
settings while body transforms, visibility, and geometry revisions continue
to originate in Conatus.

### 5. Procedural and field systems

Turn Numen and Quint into registered engine systems rather than utilities a
product calls by hand:

- scalar/vector field sampling;
- force and material couplings;
- particle and fluid systems;
- SDF composition and extraction;
- procedural terrain, bodies, vegetation, and scatter;
- spatial trees, neighborhood queries, and large-body broad phase.

Feature complete when these systems compose through shared resources,
commands, and revisions, and adding one changes the products available to
every game rather than one executable.

### 6. Scripting and content

Expose engine commands, queries, events, system parameters, body/voxel
descriptors, and scene recipes to the script host. Scripts cannot obtain raw
backend or GPU handles. Content catalogs select and combine registered engine
features instead of growing fifty bespoke enums per category.

Feature complete when a data/script-defined rule can spawn and alter bodies,
edit voxel regions, query space, react to interactions, and configure fields
through the same command vocabulary native game systems use.

## Immediate implementation order

1. Keep extending `conatus` as the shared package; adapt `seiche` later rather
   than making the 2D graph API the 3D engine core.
2. Move generic voxel chunk/revision/dirty-region machinery under Conatus and
   connect it to the voxel collider already implemented.
3. Publish stable resident body/chunk views through Quint allocations.
4. Add the renderer-neutral scene view and the Renderling/Netrender tenant
   adapter.
5. Register fields, procedural products, and scripting against those engine
   resources.

New work belongs in a product only when its meaning is genuinely product
specific. Reusable mechanics move into the engine as they are written, rather
than after another probe duplicates them.
