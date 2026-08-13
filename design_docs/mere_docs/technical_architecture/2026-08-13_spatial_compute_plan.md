# Spatial Compute Plan (2026-08-13)

**Status: founded 2026-08-13; P1 landed the same day.** The GPU
architecture for conatus and its render consumers, ratified by Mark from
the conatus discussion of 2026-08-13 (the projection ruling and its
amendment in
[the field system extraction doc](2026-05-30_field_system_extraction.md)
are this plan's ground). Gates P1 to P4 below.

## 0. The regime

Two compilation regimes, one tenancy layer, physics state independent of
renderers.

- **Tensor programs: Burn/CubeCL.** ML, dense fields, force evaluation,
  reductions: anything that benefits from fusion and autotuning. Adopts
  the shared device via `init_device` (`WgpuSetup` maps onto netrender's
  `WgpuHandles`).
- **Explicit GPU programs: rust-gpu kernels as plain wgpu compute.**
  Spatial trees, collision passes, integrators, procedural geometry:
  algorithms needing exact memory and dispatch control. Carried the
  renderling-fork way: shader source in Rust (`spirv-std`), committed
  `.spv` artifacts, `cargo-gpu` only when editing, offline SPIR-V to
  WGSL for the browser.
- **Render consumers.** Mere's graph tenant for 2D and spatial graphs;
  renderling for the wing's 2.5D/3D worlds. Vello encodes scenes on the
  CPU, so netrender's rasterizer can never read a resident buffer:
  resident readers are always tenants composed through the tenancy seam
  (netrender 2026-08-10), and netrender composes what it never parses.

**Ownership follows advanced state; the regime is chosen by the
algorithm's shape.** The integrator is conatus's because it advances
conatus state, even when renderling consumes its positions.

**The host conducts the frame.** Sequence per frame: field evaluation,
spatial integration, rendering. On one device and one queue, submission
order is the synchronization, and the entity that submits in order is
the host's frame loop. Netrender stays a rasterizer-compositor and
reports its spans; budgeting against them is the host's job.

**Padded 3D positions from the start.** WGSL storage layout aligns
`vec3<f32>` to 16 bytes, so padded 3D is the natural layout and tight 2D
is the special case. A 2D graph is a constrained spatial field; mere
stays visually 2D while z is available as layer depth immediately and as
full space later.

**The two tiers stand** (projection ruling amendment, 2026-08-13): the
tactile tier stays CPU rapier (contacts, joints, stacking, the source of
commitment events); the field tier goes GPU-resident, with the CPU
solver as the downlevel tier where WebGPU compute is absent.

**The wgpu-29 row** (netrender, renderling fork, cubecl, khal, kiss3d)
is bumped as a row, never one crate at a time.

**Nexus is a quarry, not a dependency.** Harvest: Morton sorting and
Karras-style hierarchy construction and refit (shared machinery for BH
far-field and wing spatial queries; BH adds mass and centre-of-mass
aggregation plus its own opening-angle traversal), the rust-gpu kernel
organization and shared-struct CPU/GPU pattern, and the proven SPIR-V to
WGSL browser pipeline. Watch condition for adoption: tactile body count
outgrowing CPU rapier.

## 1. The lease (not yet promoted)

The shared contract for resident spatial buffers, domain-neutral,
promoted only at P4:

    SpatialBufferLease (working name)
      position format and dimension (padded 3D)
      coordinate space and units
      stable slot/count information
      buffer offset, size, and usages
      generation/frame epoch
      writer and valid-read interval

Stable slots are the load-bearing field: the NodeKey-to-slot mapping
must survive node add and remove, which forces the free-list versus
compaction decision, and the epoch guards readers on another cadence
from stale slots. Writer and valid-read interval are implicit today (one
queue is the interval) and earn their place the day async compute or a
second queue appears.

## 2. Gates

### P1. Contact-to-fact (landed 2026-08-13)

Physics proposes; an explicit gesture or standing rule commits. On
current seiche: a bin (scene body) and a card (node body); the drag is
`pin`, the release is `unpin`, and at release the host reads seiche's
containment proposals and mints a discrete attributed fact. The
trajectory is discarded.

**Done when:** a pass through the bin without release mints nothing; a
release inside mints exactly one fact naming card, bin, and tick; the
record provably carries no positions; the facts replay onto a fresh
state with no simulation present; two runs of the scenario agree on the
fact log without comparing positions.

**Landed:** `seiche::propose` (the read-only containment query;
seiche proposes geometric truth and owns no facts) plus
`tests/contact_to_fact.rs` holding the five receipts. The commitment
and the fact type live with the caller, which is the boundary: the
record is the host's organ, never the physics engine's.

### P2. Resident mere graph

Positions and velocities GPU-resident (padded 3D). quint/Burn writes
the force pass; a conatus integrator kernel (explicit regime) steps it;
a minimal mere graph tenant (dots and lines instancing, a new small
tenant owned by mere's canvas domain) reads the buffer through the
tenancy seam. Zero per-frame readback except flags (settle detection).
The existing CPU Barnes-Hut stands as the downlevel tier.

**Done when:** about 50k bodies at interactive frame time with per-frame
CPU traffic bounded to flags, receipts recorded beside the CPU path's
numbers on the same machine.

### P3. Wing projection

Renderling reads the same lease for a genuine 2.5D/3D consumer:
ambience particles whose positions are written by explicit-regime
kernels are the natural candidate. Mere must not drag renderling into
P2 merely to demonstrate sharing; the wing is the proper renderling
proof.

**Done when:** the same buffer serves two projections with no format
fork and no readback inserted to make either work.

### P4. Promote the lease

Only after P3. **Done when:** the contract is extracted with both
consumers named, or narrowed in writing with the reason.

## 3. Stop rules

- No recorded decision reads settled positions except through an
  explicit commitment (the projection ruling amendment, 2026-08-13).
- Authority stays CPU and integer where the games wing constitution
  says so; this plan computes projections and ambience only.
- No second dynamics engine beside rapier in the tactile tier without a
  proven constraint need.
- The lease is not shared, published, or depended on across domains
  before P4.

## Findings

- **2026-08-13 (P1):** the gesture vocabulary already existed. seiche's
  `pin`/`unpin` (kinematic hold during drag, dynamic release) is the
  hold-and-release the commitment doctrine needs, so P1 added a
  read-only proposal query and no new mechanics.
