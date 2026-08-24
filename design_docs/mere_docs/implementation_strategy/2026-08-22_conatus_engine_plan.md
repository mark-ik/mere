# Conatus Shared Spatial Runtime

**Date:** 2026-08-22  
**Status:** active; body/runtime foundation implemented; private-backend
integrity pass in progress 2026-08-24; scope corrected 2026-08-23
**Scope:** Build the shared spatial runtime. Mesocosm, Paredros, Isometry,
and Mere projections consume it through product-owned runtime profiles
instead of incubating spatial machinery in product-local probes.

## Scope correction (2026-08-23)

Ruled by Mark after the 2026-08-23 engine-stack review chain (stack review,
verdict, adjudication). The settled answer:

> Product profiles conduct. Conatus advances spatial state. Reusable
> mechanics can move early under settled ownership. Cross-product contracts
> remain provisional until a second game proves them.

The corrections, each carried into the body text below:

- Conatus is the shared spatial runtime, not the application engine. "The
  engine" is the composition: a product-owned runtime profile conducting
  clocks, triggers, input mapping, authorization, source bindings, and
  subsystem selection. Product clocks are not interchangeable
  (`mesocosm/design_docs/2026-08-18_engine_ecology_rulings_and_review.md`
  §2.8), and the implemented runtime already has the right shape:
  host-driven `step(steps)` advances exact steps, and frame changes publish
  on zero-step host frames.
- Seiche remains the graph-oriented 2D specialist, not an eventual adapter.
- The host/profile orders passes; allocation ownership follows advanced
  state (the host-conducts ruling,
  [spatial compute plan](../technical_architecture/2026-08-13_spatial_compute_plan.md)).
  Netrender's tenancy seam stays the device seam.
- The engine-owned render view becomes a lean spatial frame with no cameras,
  lights, sprites, or presentation policy (§4).
- Schedule phases are renamed around spatial work (§1).
- Scripts and peers submit product intents; raw remote `BodyCommand`s leave
  the architecture (§1).
- Source bindings are profile-owned; Conatus identities stay runtime-only
  (§1).
- The closing extraction rule splits into early mechanics and
  second-consumer contracts, reconciling this plan with the wing's
  two-consumer law.

## Direction

Conatus is the shared spatial runtime, not a collection of physics
experiments — and not the application engine. The engine is the composition
a product's runtime profile conducts: the profile owns clocks, triggers,
input mapping, authorization, source bindings, and subsystem selection, and
decides when to request a Conatus step, a Seiche relaxation, a field
evaluation, or an inference job.

Conatus owns reusable spatial machinery: the fixed-step spatial clock, body
identities, transforms, collision, field effects on the state it advances,
resident allocations for that state, generic procedural geometry,
spatial-frame preparation, and spatial system execution. A game owns its
rules, durable world meaning, assets, procedural content choices, and
presentation choices; its profile owns orchestration. Netrender owns device
tenancy and frame composition. Renderling is a 3D render tenant. Rapier is the
current CPU collision and dynamics implementation. Nexus and its Khal kernels
are source material for the scale at which the CPU backend stops being the
right implementation.

Ordinary unit, property, and integration tests remain part of runtime work.
Windowed demos, screenshots, receipt applications, and benchmark gates are
not the work queue. Performance work starts from a named engine workload and
changes the engine path itself.

## Existing pieces

| Piece | Role in the stack |
|---|---|
| `numen` | Serializable field and coupling vocabulary |
| `quint` | Field evaluation, CubeCL kernels, resident tensor/chunk allocations |
| `seiche` | Graph-oriented 2D dynamics specialist; shares contracts with the runtime where real, never an adapter over the 3D body API |
| `conatus` | Shared 3D body world and host-neutral runtime |
| Netrender | One-device tenancy and final frame composition |
| Renderling | 3D scene/render implementation, consumed as a tenant |
| Mesocosm voxel types | First source material for generic voxel storage, revision, dirty-region, collision, and meshing features |

The dependency direction is runtime to implementation only. Product crates
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
- ordered ingest, field, before/after-physics, materialization, and publish
  system phases;
- typed shared resources and deterministic registration order;
- a serializable structural command buffer applied between phases, with
  correlated command results;
- current-step physics changes and interaction events available to
  after-physics systems;
- frame changes containing final body transforms and removals, including host
  frames on which simulation takes zero steps;
- effective sparse voxel edit streams for materialization and render systems;
- serializable body, collider, voxel, filter, and character configuration.

Rapier types stay private at the public API. That is insulation, not yet
interchangeability: the initial `BodyWorld` directly embedded Rapier handles,
lowerings, queries, character movement, voxel mutation, stepping, and event
translation in one implementation. The 2026-08-24 integrity pass first makes
that private implementation boundary structural. A future backend must still
prove which game-facing behaviors it supports; unsupported behavior may not be
silently approximated.

### Private backend integrity pass (in progress 2026-08-24)

Keep Conatus identities, generations, revision order, dirty publication, and
normalized events as the stable spatial mechanics. Move Rapier-specific world
state, handles, conversions, queries, character controller, voxel shape edits,
and stepping behind a crate-private implementation module. This pass adds no
public backend trait, backend selector, or Nexus dependency.

Done when the existing public API and all behavior tests remain unchanged,
Rapier imports and handles are confined to the private implementation, and
warnings-denied checks pass. This proves a code boundary only. Nexus earns a
backend seat later through an isolated lifecycle/query receipt and the exact
host-device receipt; it does not inherit one from opacity alone.

## Runtime growth

### 1. Runtime and systems (foundation implemented)

The ordered spatial schedule now has concrete phases:

1. run ingest adapters over already-admitted spatial work;
2. evaluate fields;
3. run before-physics spatial systems;
4. advance tactile physics;
5. run after-physics spatial systems;
6. materialize voxel and geometry changes;
7. publish derived spatial changes;

Systems receive typed resources, current-step output after physics, and a
command buffer. Structural mutations
apply between phases, so scripts and parallel systems cannot invalidate a
live iteration. Tick-local events and durable game facts remain different
types.

Three corrections (2026-08-23) narrow this machinery to spatial work:

**Phase names.** On 2026-08-23, `Input` became `Ingest`, `Gameplay` merged
into the existing `BeforePhysics` phase, and `Prepare` became `Publish`.
The public schedule now names only spatial work and does not imply that
Conatus owns user input, rules, or rendering.

**Command trust boundary.** Scripts and peers submit product intents; only
authorized product code — the profile and its registered systems — lowers
accepted consequences into local `BodyCommand`s. Raw remote `BodyCommand`s
leave the architecture, and `command.rs`'s doc line advertising the command
vocabulary to "scripts and remote inputs" is corrected with the rename. The
first product profile retains the request provenance alongside its admitted
intent; only the product decides whether the request is allowed. Conatus does
not invent a universal provenance field before that consumer names what it
needs.

**Source bindings stay profile-owned.** `BodyId` is generational and
runtime-only, and `BodyDesc` deliberately carries no durable source
reference. One durable product source may materialize as several bodies,
scene instances, resident slots, and audio voices, so the binding table
(`ProductSourceId -> RuntimeBindings { bodies, scene instances, resident
slots, audio voices }`) belongs to the runtime profile, not to Conatus and
not inside `BodyDesc`. Sceno's `SourceRef` is the pattern reference, not
automatically the universal type: it belongs to semantic scenes and lacks
revision and materialization information. The first Isometry profile
defines a neutral-shaped binding table locally; the shared minimum is
extracted when Paredros or Mesocosm needs the same vocabulary.

The remaining runtime work is parallel system access declarations, enforcing
the intent-lowering command boundary at the first product profile, and the
lean spatial-frame resource (§4). A game can already register spatial
systems, insert resources, queue structural commands, and advance without
writing a physics loop or borrowing Rapier types.

### 2. Voxel world

Promote the generic parts already present across Mesocosm and Quint:

- sparse chunk addressing and stable chunk ids;
- exact occupancy/material planes with per-chunk revision;
- dirty brick/region tracking and bounded edit batches;
- derived collision, surface, SDF, light, navigation, and mesh products;
- dependency stamps so each product updates only from changed source regions;
- streaming budgets and explicit residency states;
- body volumes using the same plane and product vocabulary as ground volumes.

The first CPU mechanics slice now lives in the Rapier-free `conatus-voxel`
package: Euclidean world cell addressing; dense opaque chunks in the incumbent
Mesocosm Y/Z/X order; validated serialization; revision-gated, caller-bounded
patch batches; effective changes; disposable dirty boxes; and lowering of
material changes into backend-neutral occupancy edits. The full `conatus`
runtime re-exports this vocabulary and lowers accepted occupancy edits into its
voxel collider. Product chunk identity, material meaning, admission, and
durable authority remain outside either package. Mesocosm's `Ground` and
Quint's `ResidentChunk` are unchanged.

Games define what a material or voxel means. Conatus owns storage,
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
follows the operation. Allocation ownership follows advanced state: Conatus
owns the allocations whose state it advances, the profile orders passes on
the shared device and queue (the host-conducts ruling, spatial compute plan
2026-08-13), and Netrender's tenancy seam stays the device seam. Conatus is
not the global allocator for ESP, Quint, Renderling, or future subsystems.

Feature complete when CPU tactile bodies and GPU field bodies can be joined
through one versioned profile-local spatial view and coexist in one spatial
world. Its cross-product vocabulary becomes stable only after a second game
consumes it.

### 4. Spatial frame

Publish a lean spatial frame: transforms addressable by runtime identity,
removals, contacts, activity, sleeping and residency changes, voxel and
geometry revisions, and resident products. No cameras, no lights, no sprites,
no visibility or presentation policy: those belong to the product's rendering
profile, which selects and configures tenants — Scenograph lanes for semantic
2D, Renderling for 3D bodies, brick DDA for live volumes, several composed by
Netrender.

Renderling consumes the 3D portion through a profile-owned tenant adapter.
Mere's spatial renderer can consume the same frame. Netrender receives each
tenant's frame entry and composes them on the host's device and queue.

The frame vocabulary is a cross-product contract, so its shared form stays
provisional until a second game consumes it. Until then it is the Isometry
profile's seam, kept neutral in shape.

Feature complete when a product selects render tenants and presentation
settings in its profile while body transforms, activity, and geometry
revisions continue to originate in Conatus.

### 5. Procedural and field systems

Keep Numen and Quint independently callable, including by Seiche and product
profiles. Add Conatus adapters only for field effects on state Conatus
advances:

- scalar/vector field sampling over Conatus bodies and volumes;
- force and material couplings applied to Conatus state;
- particle and fluid systems whose state Conatus advances;
- SDF composition and extraction for Conatus geometry;
- spatial trees, neighborhood queries, and large-body broad phase.

Generic procedural algorithms may live in shared crates. Terrain, body,
vegetation, and scatter recipes remain product content. Feature complete when
a profile can select a field adapter for Conatus while Seiche and another
consumer continue using the same Numen definitions and Quint evaluation
without depending on Conatus.

### 6. Scripting and content

Expose spatial queries, events, system parameters, body/voxel descriptors,
and spatial body or voxel recipes to the script host. Scripts submit product
intents; authorized product code lowers accepted consequences into commands.
Scripts cannot obtain raw backend or GPU handles, and they do not receive the
raw command buffer. Content catalogs select and combine registered features
instead of growing fifty bespoke enums per category.

Feature complete when a data/script-defined rule can spawn and alter bodies,
edit voxel regions, query space, react to interactions, and configure fields
through product intents that lower into the same command vocabulary native
product systems use.

## Immediate implementation order

1. Make the current private Rapier implementation a real internal boundary;
   keep backend selection private until a named product workload forces a
   second implementation.
2. Keep extending `conatus` as the shared spatial package; `seiche` stays the
   2D graph specialist rather than the 2D graph API becoming the 3D core or
   being forced through it.
3. Adopt the generic voxel chunk/revision/dirty-region mechanics in one product
   without moving product identity or authority, then feed accepted occupancy
   changes into the voxel collider already implemented.
4. Publish versioned profile-local resident body/chunk views through Quint
   allocations.
5. Add the lean spatial frame and the Renderling/Netrender tenant adapter,
   seamed in the first product profile.
6. Add optional field adapters and spatial scripting against the shared
   resources without making Numen or Quint depend on Conatus.

New work belongs in a product only when its meaning is genuinely product
specific. The extraction rule is split (ruled 2026-08-23):

- **Mechanics may move early.** A reusable implementation whose authority is
  already settled — collision algorithms, voxel revision machinery, spatial
  queries — enters its natural shared crate after one forcing consumer, as
  it is written, rather than after another probe duplicates it.
- **Contracts wait for the second consumer.** Public contracts governing
  orchestration, identity, authority, device ownership, or cross-product
  frames — the conductor, source bindings, the spatial frame, shared trigger
  vocabulary, resident lease contracts — stay provisional until a second
  game proves them. The first implementation uses a neutral-shaped seam in
  its profile; the second consumer tests that shape before it becomes stack
  law.

This reconciles the plan with the wing's two-consumer law (mesocosm
`CLAUDE.md`): no deliberate duplication of machinery, and no cross-product
contract declared in advance.

## Progress (2026-08-23 product adoption pass)

- Conatus's corrected spatial-runtime foundation and first voxel mechanics
  landed at Mere commit `5767563c` with 24 tests and warnings-denied Clippy.
- Mesocosm became the first tracked mechanics consumer locally at `b112931`:
  `Ground` remains authoritative while a product adapter uses `VoxelChunk`
  patch and occupancy machinery, with replay, refusal, snapshot-silence, and
  unchanged-source receipts. Its divergent main and concurrent dirty lane keep
  remote integration open.
- Isometry's first profile landed product-side at `303e347` in
  `isometry-runtime`: accepted map events drive an event-cadenced, zero-step
  Conatus projection with map-qualified source bindings. Host wiring remains
  gated by Isometry's active protocol work and Genet host migration.
- The cross-product ownership and receipt queue now lives in the
  [runtime composition acceptance plan](2026-08-23_runtime_composition_acceptance_plan.md).
  No conductor, source-binding, spatial-frame, trigger, or resident-lease
  contract was promoted in this pass.

## Progress (2026-08-24 private-boundary pass)

- `BodyWorld` now retains Conatus identity, revision, and publication
  bookkeeping while all Rapier state, handles, conversions, queries, character
  movement, voxel mutation, stepping, and interaction normalization live in a
  crate-private implementation module. This is an internal boundary, not a
  public backend ecosystem.
- Generic chunk, patch, dirty-region, serialization, and occupancy-edit
  mechanics moved into `conatus-voxel`; `conatus` re-exports the same public
  vocabulary. Mesocosm now names the narrow package directly, so its
  `GroundVoxelProfile` does not acquire Rapier merely to maintain a disposable
  chunk view.
- The next Nexus evidence is an isolated same-device rigid-body probe. It must
  translate the public Conatus descriptor vocabulary, wrap Netrender's exact
  device and queue through Khal, advance one body on the GPU, and read a changed
  pose. It does not add Nexus to Conatus or establish shared buffer ownership.
