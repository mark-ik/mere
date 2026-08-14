# Spatial Compute Plan (2026-08-13)

**Status: founded 2026-08-13; P1, P2's spike, the Burn handoff, and P3
all landed the same day. P4 is next, and now has its evidence.** The GPU
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

**Spike landed 2026-08-13** (`crates/probes/resident-graph`, standalone
probe). Positions and velocities resident as padded 3D `vec4f`; three
WGSL dispatches per frame (tiled all-pairs repulsion, springs by CSR
gather with no float atomics, damped symplectic Euler with the settle
reduction as one `atomicMax` over speed bits); the dots-and-lines tenant
reads the same buffer in its vertex stages; netrender composes the
master through the tenancy seam, booted with `TenantNeeds` as the seam's
second consumer after paredros-room. The per-frame readback is four
bytes.

The numbers, RTX 4060 Laptop, 300-frame averages, frame = sim + draw +
flag readback with full sync:

    n        gpu resident   cpu barnes-hut (one force pass)
    10,000       0.89 ms        26.3 ms
    50,000      12.8  ms       318    ms
    100,000     49.6  ms       813    ms

The GPU column is honest O(n squared) (10k to 50k scales 14x, 50k to
100k scales 3.9x) and still beats the CPU's O(n log n) at every
measured size; the crossover where a GPU Barnes-Hut becomes necessary
sits in the several-hundred-thousand range, which is the LBVH harvest's
job when a canvas gets there. Receipt picture at
`Code/testing/mere/p2_resident_graph.png` (50k dots, 149,990 edges, lit
fraction 26.5 percent).

**The Burn handoff landed 2026-08-13, same probe (`--burn` mode).**
Burn adopts the shared device (`init_device` over the same handles
netrender booted), a `[n, 4]` tensor's storage is filled from the
resident position buffer by device-local copy, quint's own
`forces::repulsion` runs in its own vocabulary, and the output tensor's
raw `wgpu::Buffer` (via `TensorPrimitive::Float` to
`client.get_resource`) is copied device-local into the resident forces
buffer, which springs and integrate then consume. Ordering rides the
one queue: copy-in submits, `client.flush()` submits Burn's recorded
ops after it, copy-out submits after the flush. No force byte touches
the CPU.

The receipts, n = 4096, same constants both paths:

    equivalence   mean relative error 1.05e-6, worst 1.38e-5
    kernel        0.52 ms/frame
    quint/burn   12.25 ms/frame

The equivalence number is the handoff proven: quint's tensor program
and the explicit kernel compute the same field to float precision
through a chain the CPU never sees. The 24x gap is the two-regime
taxonomy proven: n-body is exactly the algorithm shape that needs
exact memory control (the tensor formulation materializes [n, n]
intermediates, O(n squared) memory, and cannot reach 50k at all on an
8 GB card), so the explicit lane owns the large-n field tier and the
tensor lane serves small-to-mid canvases and the semantic-field
couplings it was built for.

**Still open before P2 closes:** the slot-stability decision (the
probe holds n fixed); promotion of the kernels out of the probe into
conatus proper with the rust-gpu carriage; and a windowed rather than
offscreen run.

### P3. Wing projection

Renderling reads the same lease for a genuine 2.5D/3D consumer:
ambience particles whose positions are written by explicit-regime
kernels are the natural candidate. Mere must not drag renderling into
P2 merely to demonstrate sharing; the wing is the proper renderling
proof.

**Done when:** the same buffer serves two projections with no format
fork and no readback inserted to make either work.

**Landed 2026-08-13** (`paredros/probes/ambience-lease`, a wing-side
probe with the kernel shape copied rather than shared, per the plan).
20,000 ambience motes in a resident padded-3D buffer; explicit-regime
WGSL kernels drift them on a swirl-plus-updraft field wrapped to a
torus; renderling draws them as instanced octahedra under a real
perspective camera and a PBR sun; netrender composes. 300 frames at
**3.6 ms average** (worst 5.9). Cloud bounds x [-120, 120], y
[-110, 10], z [-120, 120]: **z carries real extent**, the axis no 2D
canvas exercised and the reason padded 3D was chosen before any
consumer needed it. Receipt at
`Code/testing/paredros/p3_ambience_lease.png`, 73 distinct colours
(PBR-shaded, so a colour-count guard is right here where mere's flat
2D graph needed coverage instead).

**The done-condition holds, and the format did not fork.** Both
consumers read the same `vec4f` padded-3D layout, and no readback was
inserted to make either work: mere's tenant binds the buffer in its
vertex stage, the wing's consumer receives it through an adapter
dispatch, and in both cases the CPU sees nothing per frame.

**The consumers are not symmetric, and that is the finding.**

### P4. Promote the lease

Only after P3. **Done when:** the contract is extracted with both
consumers named, or narrowed in writing with the reason.

**P3's evidence for the shape.** The draft lease had six fields. Two
were used as data, one became an assertion, and one that mattered was
missing:

- `count` — used by both consumers.
- `coordinate space and units` — used, as the extent the wing consumer
  frames its camera from.
- `position format and dimension` — used as an assertion (the adapter
  kernel indexes `vec4f`; a different stride needs a different kernel)
  rather than as data anything branches on.
- `buffer offset, size, usages` — **producer-side, and insufficient.**
  A consumer that cannot bind the buffer needs a *destination*
  descriptor instead: where its own storage begins and how far apart
  its slots sit. The draft had no such field.
- `stable slot/count information` — validated in an unexpected place.
  Not only the producer's slots must be stable; the **consumer's
  allocation order** is part of the contract (see the findings).
- `generation/frame epoch` — validated by contact rather than design.
  renderling replaces its slab buffer when the allocator grows, so a
  bind group built against the old one goes stale. The probe attaches
  once after commit and would break on growth; the epoch is what makes
  that detectable rather than silent.

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

- **2026-08-13 (P3):** a second consumer may be unable to bind the
  lease at all. renderling addresses geometry through a craballoc slab
  rather than caller-supplied buffers, so the wing consumer reads the
  lease through an **adapter kernel that writes into renderling's own
  storage** (three `bitcast` stores into a transform's first three
  words). The mismatch cost one small kernel, and the neutrality claim
  survives in a better form: the producer publishes a readable lease
  and each consumer adapts, rather than every consumer being obliged to
  bind the same way.
- **2026-08-13 (P3):** the consumer's allocation order is part of the
  contract. Allocating a transform and its primitive in one loop put a
  primitive descriptor between every pair of transforms, so the slab
  stride became 27 words instead of the transform's own 10 and the
  adapter's indexing broke. Allocating all transforms in one unbroken
  pass fixes it. An assertion that checked contiguity rather than
  trusting it turned a corrupted-scene hunt into a two-minute fix.
- **2026-08-13 (P3):** renderling shades through PBR, so a lit picture
  needs a material *and* a light, and its intensities are physical.
  `Lux::OUTDOOR_OVERCAST_HIGH` (1000) rendered the motes nearly black;
  `OUTDOOR_DIRECT_SUNLIGHT_HIGH` (130,000) reads correctly. Unlit
  vertex colours draw fine without either, which is what the probe did
  first and why its first colour-count guard misfired.
- **2026-08-13 (P3):** an unnormalized radial swirl grows with radius,
  so outer motes were flung into the clamp and the cloud collapsed onto
  its own boundary. Normalizing the tangential force and wrapping every
  axis to a torus gives a cloud that fills its volume. Ambience physics
  is presentation under the projection ruling, so this is taste rather
  than truth, but the failure mode is worth naming: a force whose
  magnitude scales with position will find the boundary.

- **2026-08-13 (P1):** the gesture vocabulary already existed. seiche's
  `pin`/`unpin` (kinematic hold during drag, dynamic release) is the
  hold-and-release the commitment doctrine needs, so P1 added a
  read-only proposal query and no new mechanics.
- **2026-08-13 (P2 spike):** kernels are WGSL in the probe, not
  rust-gpu: cargo-gpu is not installed on this machine and the probe's
  job was the residency numbers. The rust-gpu carriage is the promotion
  step, when the kernels move into conatus proper. The lane taxonomy is
  untouched: a WGSL kernel is still an explicit GPU program.
- **2026-08-13 (P2 spike):** the cloud drifts. Across 2.5 billion
  float pair-sums per frame, non-associativity leaves a net momentum
  bias, and 300 frames walked the 50k cloud about 450 units off origin.
  Under the projection ruling this is a shrug rather than a bug: the
  capture fits its camera to measured bounds (a one-time presentation
  readback, outside the flags-only frame discipline), and no fact-plane
  consumer ever reads these floats. The same drift on the CPU path
  would have been a determinism incident.
- **2026-08-13 (P2 spike):** the honest frame includes a full CPU/GPU
  sync per frame (the flag readback maps a staging buffer). A real host
  double-buffers the flag and hides that latency; the probe pays it
  openly so the numbers are conservative.
- **2026-08-13 (P2 spike):** a flat-shaded graph is exactly three
  colours, so a distinct-colour blank-guard (calibrated on shaded 3D)
  false-alarms; the probe's guard is coverage (lit fraction between
  bounds) instead.
- **2026-08-13 (Burn handoff):** a JIT compute runtime is a **greedy
  tenant**. CubeCL boots its own devices with every adapter feature
  (minus `MAPPABLE_PRIMARY_BUFFERS`) and full adapter limits, and its
  WGSL compiler emits against adapter capability (u64 addressing when
  the adapter has `SHADER_INT64`), so a shared device holding less than
  the adapter fails shader validation at kernel launch. The tenancy
  seam grew `TenantNeeds::greedy` for this shape (netrender
  `1ce733be6`, receipt included): adapter features minus the traps,
  adapter limits, netrender's minimum still raised. The third consumer
  taught the seam what the first two could not.
- **2026-08-13 (Burn handoff):** the interop chain is
  `TensorPrimitive::Float` to `CubeclTensor { client, handle }` to
  `client.get_resource(handle)` to `WgpuResource { buffer, offset }`,
  with `client.flush()` as submit-without-wait. Burn tensor storage is
  ordinary wgpu buffers on the shared queue, so device-local copies and
  submission order are the whole synchronization story.
- **2026-08-13 (Burn handoff):** quint's tensor repulsion is O(n
  squared) **memory**, not just compute: `[n, n]` pairwise tensors put
  50k out of reach entirely and cost 24x at 4096 against the tiled
  kernel's O(n) working set. The taxonomy's line between the lanes is
  measurable, and the crossover logic in seiche (Burn above 1000 nodes)
  holds for the CPU-naive comparison it was written against but not
  against a resident explicit kernel, which wins at every n measured.
