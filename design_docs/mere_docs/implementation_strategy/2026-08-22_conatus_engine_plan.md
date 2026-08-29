# Conatus Shared Spatial Runtime

**Date:** 2026-08-22  
**Status:** active; body/runtime foundation, private-backend integrity, first
profile-local resident body-position publication, and first product renderer
tenant implemented; Nexus admission probe blocked at its upstream Windows
shader build; scope corrected 2026-08-23. 2026-08-26: Mesocosm's runtime
became the first product tactile consumer (terrarium picking over
`BodyWorld`, Rapier private), quint's `ResidentChunk` join was proven with
per-brick patches, tracer-validated read epochs, and allocator-observed
bytes (V1b), and `conatus-brick` — the shared sparse-brick ABI both game
vessels pin — advanced on `codex/conatus-brick-lift` to `bd8f0044`.
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
| Mesocosm voxel types | First source material for generic voxel storage, revision, dirty-region, collision, and meshing features; since 2026-08-26 Mesocosm is also a runtime consumer, holding `BodyWorld` tactile advice through its own `mesocosm-runtime` adapter |

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

### Private backend integrity pass (complete 2026-08-24)

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

Complete at Mere commit `339e8567`: six `conatus-voxel` tests, eighteen
Conatus unit tests, the cross-package voxel-collider integration test, and
warnings-denied Clippy passed.

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

The first position-plane slice is complete at Mere commit `c382e734` and
Isometry commit `15f5da2`. Quint can validate and publish several disjoint
ranges under one stamp. Isometry uses that mechanism to project an accepted
`IsometrySpatialFrame` into one fixed-capacity `[x, y, z, occupied]` plane.
The product owns capacity, coordinates, source bindings, generation handling,
and tenant selection. Conatus's existing `FrameUpdate` remains unchanged.
This disposable product projection does not settle allocation ownership for
state advanced directly on the GPU, or establish a shared frame or lease.

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
2D, Renderling for 3D bodies, brick DDA for live volumes (owned by
`conatus-brick`, currently on the `codex/conatus-brick-lift` branch and
consumed by pinned rev), several composed by Netrender.

The first realized adapter is Isometry's product-local, fixed-isometric body
marker tenant at commit `7d45c40`. It binds the stamped Quint position
suballocation directly, projects through configurable basis and appearance
settings, renders into its own same-device texture, and gives Netrender an
explicit external-composition boundary. Netrender learns neither Conatus body
semantics nor Quint allocation policy.

Renderling remains a candidate for a later 3D portion through another
profile-owned adapter. It was not pulled into the 2D proof because its current
stage cannot attach the external resident plane without a sidecar pass. Mere's
spatial renderer may consume a later common frame if another product proves
one. Netrender receives each tenant's frame entry and composes them on the
host's device and queue.

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
   allocations. The first body-position view is complete; rotation, velocity,
   forces, flags, voxel/chunk joins, and direct-GPU advancement remain open.
5. The first profile-owned Netrender tenant adapter is complete in Isometry:
   it realizes stamped resident positions as fixed-isometric body markers.
   Add a Renderling or other 3D adapter only when a product lens demands it,
   and keep shared frame vocabulary provisional until a second game challenges
   it.
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
- The isolated Nexus probe resolves Netrender, Vello, Khal, and Nexus onto one
  wgpu-30 row after restating Netrender's temporary Vello patch. Its Windows
  build reaches `nexus_rbd3d`, then the Khal build script's `cargo-gpu 0.1.0`
  invocation fails while removing `Cargo.lock`. No Nexus kernel executed, so
  Nexus remains outside Conatus and shared buffer ownership remains unproven.

## Progress (2026-08-25 resident-position pass)

- Quint commit `c382e734` added an atomic sparse batch-patch mechanism to
  `ResidentChunk`. Its real-adapter receipt changes nonadjacent rows under one
  advancing stamp, retains the allocation, and proves malformed batches write
  nothing.
- Isometry commit `15f5da2` added the first product-local consumer: accepted
  Conatus body changes populate a retained position plane, silent frames do
  not write, capacity refusal leaves the allocation untouched, and same-slot
  generation reuse remains distinct in the product binding. The accepted
  `MapDocument` stays authoritative and unchanged.
- Shared frame, source, lease, and conductor contracts remain provisional.
  The next critical slice is the product-owned renderer tenant adapter; host
  construction still waits for Isometry's protocol and Genet migration gates.

## Progress (2026-08-25 renderer-tenant pass)

- Isometry commit `7d45c40` added the first product-owned resident renderer
  tenant. Its production path reads the stamped Quint storage suballocation
  directly, draws configurable fixed-isometric markers into a tenant texture,
  and exposes that texture to Netrender at an explicit scene boundary.
- The real-device receipt proves scene content below and above the tenant,
  silent-frame skipping, stable-allocation moves, stale-view refusal, removal,
  generation-aware slot reuse, and capacity and target-limit refusal. Default
  profile tests remain GPU-free.
- This closes the first renderer-adapter slice, not host adoption or a shared
  tenant contract. Isometry desktop construction still waits for protocol H2
  and Genet migration. Renderling remains a later 3D candidate, and shared
  frame, source, lease, tenant, and conductor vocabulary still waits for a
  second product.

## Consolidation map (ruled 2026-08-28, Mark; amended the same day from a three-engines sketch)

The consolidation goal, stated plainly: functional, modular, nonredundant
components, consolidated by decomposing incumbents into owned organs behind
stable vocabularies — never by umbrella crates. The map:

- **Realms own truth.** Chartulary is the graph realm; the Ground-pattern
  with `nisus` is the voxel realm; esp's corpora are the semantic realm.
  Each realm owns authority, revision, and edit mechanics, and nothing else
  does. Analytic products form **derived realms** beside authority —
  revisioned, disposable, refusal-gated, the `GroundVoxelProfile` and
  tactile-advice shape — never a second authority.
- **Two engines operate over realms.** The **projection engine** shows
  realms, primary or derived: sceno/scenomise, the graph canvas (a graph is
  a projection of data — the earlier sketch's "graph engine" dissolves here
  as chartulary's lens), `modulus` with the product tracers over it,
  Renderling tenancy, Netrender composition. The **inference engine**
  (esp's semantic lane plus quint's Burn lanes) couples twice and only
  twice: **analytically**, reading realms into derived data; and
  **generatively**, writing proposals into realms under
  propose-constrain-commit with authority disposing (the resident-ground
  receipt and Mesocosm's B1 bounded policy are the standing proofs at two
  scales). Inference never renders; a lens that "depends on the inferences
  upon the data" is just projection over a derived realm, in games and in
  turnstone alike.
- **Physics is a stratified capacity, not an engine.** In-projection
  dynamics (`seiche` over layout space; quint's `z_field` at the 2.5D
  rung), in-realm dynamics (`conatus`, and the decomposed Nexus backend
  when its consumer fires), and fields serving either altitude (quint).
- **The host grows out of genet/cambium.** The game runtime shell is the
  same lane as Isometry's pending Genet host migration; the hand-rolled
  winit hosts in the vessel receipts retire into it.

**Nexus is decomposed, never adopted.** The runtime's body vocabulary
already keeps its backend private precisely so "a later Nexus or
resident-GPU backend can replace that machinery" — the ruling sharpens
that seam: when the first vessel gate needs dynamic bodies (Paredros F5
material life and F7 danger are the expected pulls), Nexus's useful parts
arrive as a Conatus backend behind the same `BodyWorld` vocabulary, on the
one-device CubeCL/wgpu lane the mesocosm R2 receipt proved. Nexus never
becomes a peer engine with its own vocabulary. This retires both standing
hazards at once: the upstream Windows shader-build blocker stops mattering
(only the kernels that serve this stack are taken), and the Parry-`Voxels`
admission gap closes from our side (its solver meets the world through
Conatus's voxel colliders rather than its own geometry path). The missing
quadrant this fills is GPU *dynamics*: the family already has GPU fields
and GPU residency, and its rigid bodies are CPU-only until then. License
diligence on the upstream source happens at decomposition time, before any
kernel is taken.
