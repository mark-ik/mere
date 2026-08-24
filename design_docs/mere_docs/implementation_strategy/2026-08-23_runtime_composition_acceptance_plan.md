# Runtime Composition Acceptance Plan

**Date:** 2026-08-23
**Status:** active; Conatus foundation and first product proofs complete;
host adoption and second-consumer gates open
**Scope:** Name how Merely games compose runtime organs, who owns each seam,
and which executable receipts are required before a local shape becomes a
shared contract.

## Direction

The engine is the composition a product runtime profile conducts. It is not a
crate and does not need one universal owner. A profile chooses its clocks,
triggers, input-to-intent mapping, authorization, source bindings, spatial and
inference organs, render tenants, audio adapters, persistence route, and
diagnostics. Each organ retains its own state and authority boundary.

Conatus advances spatial state. It does not own the product's clock, rules,
input, durable identities, assets, scenes, renderer policy, audio policy,
checkpoint lifecycle, or packaging. This ledger exists to keep those absences
from being mistaken for either missing Conatus features or permission to found
a second application engine.

Two extraction rules govern every row:

1. Reusable mechanics may move after one forcing consumer when ownership is
   already settled and the move creates no second authority.
2. Cross-product contracts for orchestration, identity, authority, device
   ownership, frames, triggers, or leases remain product-local until a second
   heterogeneous consumer tests the shape.

## Ownership and acceptance ledger

| Concern | Authority and natural owner | Product-profile seam | First executable proof | Promotion gate | State |
|---|---|---|---|---|---|
| Durable rules and consequences | Each product core and ordered record | Accepted product intent or event enters adapters | Isometry `Intent -> Resolved`; Mesocosm ordered `Intent` and replay | Never promoted as one game-rule vocabulary | Established product-local |
| Runtime cadence | Product profile | Exact step, elapsed frame, turn, epoch, or event trigger | Isometry profile uses event-driven zero-step spatial publication | A second game must need the same trigger vocabulary | Provisional product-local |
| Input and actions | Genet/Cambium captures input; product maps it to intents | Authorized intent lowering | Isometry protocol refusal and application tests | Two products must share semantics, not merely devices | No shared action contract |
| Tactile bodies and spatial queries | Conatus owns the state it advances | Product source bindings to runtime `BodyId`s | Isometry product profile mirrors accepted map tokens | Paredros or Mesocosm must consume and challenge the profile shape | First profile proven product-local |
| CPU voxel patch mechanics | `conatus-voxel` owns generic value mechanics; product owns voxel identity, material meaning, and durable authority | Product adapter supplies admitted source revisions | Mesocosm `GroundVoxelProfile` preserves `Ground`, replay, refusal, occupancy lowering, and silence without resolving Rapier | A second product tests mechanics; identity/frame contracts remain local regardless | Narrow package adopted locally |
| Resident fields and chunks | Quint owns resident allocations and typed views for its advanced state | Profile orders passes and binds product source revisions | Mesocosm resident-ground and `ResidentChunk` receipts | Second product proves any shared lease or identity vocabulary | Mechanics proven; contract provisional |
| DDA traversal | Product-free traversal kernel/data contract, final crate selected by adoption | Product lens owns camera, material, and composition | Mesocosm and Paredros headed profiles share WGSL/brick ABI in receipts | Permanent tracked adoption by the second product | Reusable core proven; permanent owner open |
| Scene facts and transitions | Sceno/scenomise/scenotime; product adapter owns meaning | Profile chooses scene recipe and realization | Existing Mere and game consumers | Governed by Scenograph's own two-consumer receipts | Outside Conatus |
| Device tenancy and final composition | Netrender | Profile selects tenants on host device and queue | Existing same-device composition receipts | New contract only if a second host needs it | Established seam |
| 3D body realization | Renderling or another selected tenant | Product-local spatial-frame adapter | First adapter remains open | Second renderer or product challenges the frame | Open |
| Inference and learned proposals | ESP/Burn/CubeCL or product-selected model organ | Proposal plus provenance enters product adjudication | Mesocosm bounded fauna and field proposals | Shared proposal envelope needs two products | Product authority settled; composition local |
| Assets and procedural content | Product campaign/content packs; shared cache only when forced | Profile binds durable content refs to runtime instances | Isometry campaign and voxel-appearance packs | Shared asset lifecycle needs heterogeneous pack consumers | No universal asset manager |
| Audio | Hocket/Firewheel realization, product-owned cues and routing | Profile maps accepted events to voices | Open | Two games challenge the adapter vocabulary | Open, outside Conatus |
| Persistence, checkpoints, and replay | Product record plus Mere storage/replication organs | Profile checkpoints product truth and rebuilds disposable organs | Existing Isometry and Mesocosm replay suites | Never serialize Conatus as a second product record | Established law; profile lifecycle open |
| Diagnostics and packaging | Product host/tooling | Profile reports selected organs, revisions, refusals, and build provenance | Focused crate receipts exist | One useful cross-product report shape must be consumed twice | Open |

## Product profiles

### Isometry

The first profile is deliberately narrow and host-neutral. Its source binding
is `(map source, TokenId) -> BodyId`, so equal token numbers on different maps
cannot alias. It accepts an already-applied `MapDocument`, mirrors tokens as
fixed Conatus bodies, and publishes changes with `advance(0)`. Movement and
facing remain ordered Isometry events. Rejected events never reach the
profile. Its tile span, elevation step, and collider extent are configuration,
not shared defaults.

The first slice lives in the product-owned `isometry-runtime` crate rather than
`isometry-core`: core stays pure record/geometry, while the runtime crate may
compose shared organs. It remains a standalone, root-excluded workspace with
its own lock until adoption, because the desktop graph still carries Genet's
deleted layout compatibility cone. Desktop-host wiring waits for the active
protocol H2 gate and that Genet migration; the profile must not become a reason
to reorder either prerequisite.

### Mesocosm

Mesocosm's first composition slice is a voxel adapter, not yet its complete
runtime conductor. `Ground` remains the only serialized voxel authority.
`GroundVoxelProfile` holds disposable Conatus chunks, separates the global
source revision from local chunk revisions, and emits accepted spatial work.
The epoch clock, ecology rules, renderer selection, and durable history remain
Mesocosm-owned.

### Paredros

Paredros continues its product sequence. F3 memory/belief/standing does not
wait for engine adoption. Its first Conatus use should be pulled by an embodied
spatial requirement and then serve as the likely second challenge to
Isometry's body/profile seam. A shared conductor is not declared in advance.

## Gates and done-conditions

### C0 — Spatial foundation

**Complete at Mere commit `5767563c`.** Conatus names spatial phases and the
runtime-local command trust boundary, carries tested voxel mechanics, and
documents product-owned profiles and source bindings.

### C1 — First voxel-mechanics consumer

**Implemented locally at Mesocosm commit `b112931`; remote integration open.**
Done when the commit is integrated without crossing the concurrent dirty lane,
the focused adapter tests and full core suite remain green, and the real
resident-ground example resolves Quint from the same Mere provenance.

### C2 — First runtime profile

**Complete in the product at Isometry commit `303e347`.** `isometry-runtime`
proves map-qualified source bindings, accepted-event-only synchronization,
stable body identity across moves, distinct identity across maps, unchanged-
frame silence, and warnings-denied checks. Four focused tests and Clippy pass
under Rust 1.96.0. Desktop wiring remains a separate adoption gate.

### C3 — First host adoption

**Open behind existing Isometry prerequisites.** Done when the desktop host's
current Genet migration and protocol H2 gate are closed, the host constructs
the product profile, and an accepted event yields one consumed spatial frame
without any peer or Conatus re-derivation.

### C4 — Second consumer challenge

**Open.** Paredros or Mesocosm consumes the relevant Isometry profile shape and
forces at least one real comparison of source identity, trigger cadence,
authorization, and frame consumption. Only the common minimum may then move to
a shared contract. Product-specific fields remain local.

## Stop rules

- Product work continues while profile contracts are provisional; engine
  adoption is never a prerequisite for unrelated game progress.
- Conatus receives admitted spatial work, never raw remote commands or rules.
- A disposable spatial or resident view never enters the product's durable
  record merely to make replay convenient; rebuild it from product truth.
- `ResidentChunk` stays Quint-owned. DDA renderer policy stays product-owned.
  Voxel CPU mechanics do not imply a universal voxel-world service.
- A first consumer may establish mechanics. It cannot establish a shared
  conductor, source-binding schema, spatial frame, or lease contract.
- An open row is a boundary map and receipt queue, not permission to found a
  catch-all engine crate.

## Progress

- **2026-08-23:** Founded from the engine-stack review and its adjudication.
  Recorded the pushed Conatus foundation, the local Mesocosm voxel adoption,
  the first Isometry profile slice, and the explicit second-consumer stop.
- **2026-08-23:** The first Isometry profile proof closed without host wiring.
  It remains product-local and root-excluded; C3 and C4 stay open, and no
  conductor, source-binding, frame, trigger, or lease contract was promoted.
- **2026-08-24:** The incumbent Rapier implementation moved behind a structural
  private boundary, and reusable voxel value mechanics moved into
  `conatus-voxel`. Mesocosm's adapter now depends on that narrow package. These
  are mechanics and dependency corrections; they do not promote a shared
  backend selector, voxel identity, conductor, frame, or lease contract.
