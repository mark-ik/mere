# Spatial Compute Plan (2026-08-13)

**Status: complete 2026-08-13; amended 2026-08-14 (§0.5: lanes are
program shapes, resident-views law).** P1 through P4 all decided and
their work landed. P4 is a narrowing rather than an extraction (see below),
with its one live hazard closed in code. Every open item is closed:
slot stability ruled, the kernels promoted into `quint::resident` with
the rust-gpu carriage working, and the windowed run presented. What
remains is not this plan's: the promotion trigger for the lease
(a shipped producer and consumer) and the renderling shader-edit wall,
both with named watch conditions. The GPU
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

**The wgpu row** (netrender, renderling fork, cubecl, khal, kiss3d) is
bumped as a row, never one crate at a time.

**The row is currently split, and that is a live break (2026-08-15).**
netrender main moved to wgpu 30 (`7aa8c88d3`, pushed) while mere and the
wing still pin 29. A `wgpu::Buffer` does not cross a major version, so
every lease and resident view in this plan stops at that boundary as a
type error. Consumers are green today only where a lock still holds
netrender at a pre-30 commit. Moving to 30 costs prereleases on the
compute side (`cubecl-wgpu 0.11.0-pre.2` wants `^30.0.0`, `burn
0.22.0-pre.2`) against stable `burn 0.21` / `cubecl 0.10` on 29. The
regime is indifferent to which version wins; it is not indifferent to
two of them.

**Nexus is a quarry, not a dependency.** Harvest: Morton sorting and
Karras-style hierarchy construction and refit (shared machinery for BH
far-field and wing spatial queries; BH adds mass and centre-of-mass
aggregation plus its own opening-angle traversal), the rust-gpu kernel
organization and shared-struct CPU/GPU pattern, and the proven SPIR-V to
WGSL browser pipeline. Watch condition for adoption: tactile body count
outgrowing CPU rapier.

## 0.5 Amendment (2026-08-14): lanes are program shapes, and views are the composition

Ratified from the voxel composition discussion (Mark, 2026-08-14). Two
corrections and one law, none of which disturb P1-P4's receipts.

**Lanes divide by program shape, not by toolchain.** Section 0 drew the
tensor/explicit line as if it were the Burn-vs-rust-gpu line. CubeCL
straddles it: it is a real kernel language (shared memory, workgroup
barriers, plane ops, atomics, comptime unrolling) whose runtime is also
Burn's substrate. The Burn-handoff finding that tensor repulsion is
O(n squared) memory was a fact about tensor-graph program shape, never
about the toolchain. So the explicit lane has two carriages:

- **CubeCL-JIT**: raw `#[cube]` kernels. Composes with Burn handles
  natively (`ArrayArg::from_raw_parts` over the same allocation),
  greedy tenant, compiles at first launch.
- **rust-gpu-AOT**: committed `.spv`, minimal tenant, no runtime
  compiler. Renderling's world, and right where a consumer must not
  carry a JIT.

Render shaders on either carriage create no boundary, provided they
read the same allocation through a stable buffer contract.

**Refined by the engine composition ruling (Mark, 2026-08-16; wing
composition plan is the authority): the line between the carriages is
authored versus consumed, not ours versus theirs.** We *author* in
CubeCL, where Burn synergy pays; rust-gpu remains as *consumed
artifacts* from upstreams that maintain them (renderling's shaders;
nexus's khal kernels at its adoption gate). The local rust-gpu fork and
rebuilt cargo-gpu are maintenance tooling for consumed artifacts, no
longer an authoring lane; quint's kernels migrated to CubeCL on
2026-08-16, the perf gate having cleared the same day (same-process
control: CubeCL repulsion 11.39 ms vs the incumbent's committed-SPIR-V
15.84 ms at n=50k, equal scope, both warmed; JIT first launch
12.61 ms).

**The migration, landed.** `quint::resident::kernels` now holds
repulsion, springs, integration, and the settle reduction as `#[cube]`
source, and `Resident` allocates through the caller's `ResidentClient`
rather than raw wgpu buffers, so the tensor lane and the explicit lane
share an allocator and not merely a device. The four receipts stand
unchanged in substance: the kernel still matches
`forces::repulsion_reference`, the lane still settles, springs still
pull to rest length.

Three things retired with it, and the third is the interesting one:
the committed `.spv`, the WGSL fallback, and the receipt asserting the
artifact was what ran. That receipt existed because ".spv exists" and
".spv runs" were different claims; with one compiled-at-launch source
there is no fallback to pass on silently, so the question retires with
the mechanism rather than needing an answer. `quint-shaders` is kept,
retired for compute, because the carriage it documents is still how we
*consume* rust-gpu artifacts we do not author.

**The field tier now publishes the lease.** `Resident::positions_lease`
and `forces_lease` hand out the same `SpatialLease` the chunk bundles
do, stamped with a revision that advances on every step. The field tier
and the voxel tier say one thing to their consumers, which is what
makes a renderer's binding code indifferent to which produced its
buffer. Renderling
is a hands-on fork by the same ruling: the 0.9-to-0.10 shader-source
migration is sanctioned work, priced by the first shader edit the
engine needs.

**The allocator direction is a one-way street.** Burn/CubeCL can hand
its allocations outward (`client.get_resource` yields the wgpu buffer),
but cannot cheaply adopt a foreign buffer as a tensor. So resident
state that Burn must address is allocated through the CubeCL client and
leased outward to tracers, meshers, and render tenants: allocation
flows from the compute side out. This is the lease posture with Burn as
the producing allocator, and it is why ownership of resident
allocations sits with conatus below.

**The resident-views law.** Authoritative state is never inside any
consumer; each consumer receives a resident view of it. This is the
graph architecture applied to spatial state, and it is one doctrine:

- one identity and revision history per substrate;
- plural typed views over the same facts (tensor view, kernel view,
  tracer view, mesher input, collision input, persistence view),
  without copies, each declaring shape, type, units, validity, and
  dependencies;
- consumers produce proposals or projections; only the owning record
  commits consequences (**Burn proposes; the record disposes**);
- caches and GPU allocations are disposable; unchanged facts cause no
  recomputation; the durability boundary reads accepted deltas,
  checkpoints, hashes, and small reductions, never whole planes (the
  4-byte readback pattern from P2);
- a reusable view is promoted only after multiple real consumers prove
  it (the same promotion rule as the lease).

A resident substrate is a **tensor bundle**, not one homogeneous
tensor: exact planes (occupancy, palette, provenance, flags),
fixed-point simulation planes (replay-exact), learned and derived
planes (projections; float drift is a shrug), and temporary planes
(scratch). The plane taxonomy is also the fact-bearing ruling: the
exact and fixed-point planes are the record's side; everything else is
projection.

First application is the wing's voxel ground (mesocosm
`design_docs/2026-08-14_resident_views_composition_plan.md`, which also
carries the **conatus-side lane briefs**: the `ResidentChunk` seam as a
quint module, the nexus LBVH/radix harvest, and the CubeCL carriage
decision, each with its ownership boundary): mesocosm's `Ground` already
carries `revision` and a dirty-brick projection queue whose own doc
comment states the law ("projection work queue, not world authority").
The seam was latent in the tree before it was named.

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
record is the host's organ, never the physics capacity's.

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

**P2's opens, all closed 2026-08-13.** Slot stability is ruled under
P4 (a free list with per-slot generations, never compaction). The
kernels are promoted into `quint::resident` behind a `field-gpu`
feature, **with the rust-gpu carriage working**: `quint-shaders`
compiles Rust to a committed `quint_shaders.spv` that the lane loads
through `PASSTHROUGH_SHADERS`, falling back to the equivalent WGSL
where an adapter lacks it, with a receipt asserting the artifact is
what actually executes. And the windowed run is
`paredros/probes/ambience-lease`'s `live` binary: the same lane at
vsync in a winit window, worst steady-state frame 24.8 ms, receipt at
`Code/testing/paredros/p2_live_ambience.png`.

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

### The verdict: narrowed, not extracted (2026-08-13)

**No crate is created.** The bar is two *real* consumers, and both of
today's are probes: receipts, not shipped code. mere's resident-graph
is untracked by the probes convention; the wing's ambience-lease is a
wing-side spike. Extracting a shared crate from two probes would
declare a portable profile in advance, which is the thing the wing's
own doctrine refuses, and would bind two repositories to a struct of
three scalars before either ships a consumer of it.

What is promoted instead is **the convention, written here**, plus one
piece of enforced code (below). The contract in its post-P3 form:

    A resident spatial buffer is published, not shared.

    The producer states:
      count                      how many slots
      position format            padded 3D, vec4f, xyz meaningful
      coordinate space and units what the numbers mean
      slot stability             the rule below

    The consumer states, when it cannot bind:
      destination base and stride  where its own storage begins,
                                   and how far apart its slots sit
      allocation order             contiguity, if the adapter indexes
                                   by stride

    Either side may raise the epoch, and the reader of a stale epoch
    rebinds before publishing.

Three properties of that shape are worth stating because P3 taught
each one against expectation:

1. **Publication is asymmetric.** A consumer that cannot bind the
   buffer is not a degenerate case; it is renderling, the wing's own
   renderer. So the contract describes a buffer and leaves acquisition
   to the consumer, rather than requiring everyone to bind alike.
2. **The consumer's own layout is contract.** Allocation order,
   destination base, and stride live on the consumer's side and the
   producer must be told them. A producer-only descriptor is half a
   contract.
3. **The epoch is the only part that must be enforced rather than
   documented**, because its failure is silent. Hence the code.

**Slot stability, ruled here** (the P2 open): a slot is a free list
with a per-slot generation, never compaction. A consumer may hold a
slot index across frames, so compaction that moves subjects between
slots invalidates held indices with no signal; a free list keeps
indices stable and a generation makes reuse detectable. Compaction is
permitted only behind an epoch bump, which by rule forces every
consumer to rebind. Neither probe needs this yet (both hold count
fixed), and it is ruled now because it is a producer-side decision
that a shipped canvas will need before its first node is removed.

**Promotion trigger, named:** the contract becomes a crate when a
*shipped* producer and a *shipped* consumer exist, which in practice
means when the kernels leave the probes for conatus proper (the P2
open) and a real canvas or wing scene consumes them. At that point
the natural home is beside whatever owns the producer's state, not a
new crate wedged between the repositories, and the destination half
belongs to the consumer's own crate.

**Enforced now, in code:** the epoch, in
`paredros/probes/ambience-lease`. renderling's allocator already
publishes the signal (`SlabBuffer::is_new_this_commit`, whose craballoc
documentation says it exists so downstream bind groups can be
rebuilt); the probe commits each frame, re-attaches when the buffer was
recreated, and provokes growth deliberately with one 400,000-vertex
allocation. The control is what makes it a receipt: with `--no-reattach`
the same run skips the re-attach, and pixels changed across the growth
fall from **10.86% to 0.00%**. The producer publishes into an orphaned
buffer while renderling reads the new one, and the cloud freezes
without erroring. Both numbers come from one probe under one flag,
because an absence is evidence only when the same run can show the
positive case.

## 3. Stop rules

- No recorded decision reads settled positions except through an
  explicit commitment (the projection ruling amendment, 2026-08-13).
- Authority stays CPU and integer where the games wing constitution
  says so; this plan computes projections and ambience only.
- No second dynamics backend beside rapier in the tactile tier without a
  proven constraint need. (The Nexus-decomposition ruling in the conatus
  engine plan's consolidation map, 2026-08-28, is this rule's descendant.)
- The lease is not shared, published, or depended on across domains
  before P4.

## Findings

- **2026-08-13 (P4):** a growth receipt that never grows is worse than
  none. The first attempt to provoke reallocation allocated 40,000
  more transforms; they fit in the allocator's existing slack, so the
  epoch never fired, the assertion passed vacuously in an earlier
  draft, and the frame cost 600 ms for nothing. One large allocation
  past the slack is what actually recreates the buffer. Provoking the
  event beats waiting for it, and checking that the provocation
  *worked* beats trusting it.

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
- **2026-08-13 (P2 spike):** kernels were WGSL in the probe, because
  the probe's job was the residency numbers. **Superseded the same
  day**: the promotion carries real rust-gpu, and WGSL survives as the
  downlevel path rather than the only one. The lane taxonomy was never
  at stake either way, since a WGSL kernel is an explicit GPU program
  too.
- **2026-08-13 (carriage):** rust-gpu main fails against the toolchain
  its own `rust-toolchain.toml` pins. Its target-spec gate reads
  `rustc_version >= Version::new(1, 97, 0)` while the pinned nightly
  reports `1.97.0-nightly`, and semver sorts a pre-release *before* its
  release, so the older spec variant is chosen and emits
  `allows-weak-linkage`, the very key rustc 1.97 removed. A one-line
  fork comparing on the version triple clears it
  (`Code/crates/rust-gpu`, branch `mark-ik/prerelease-version-gate`),
  and cargo-gpu must be rebuilt against that fork because it links
  `rustc_codegen_spirv-types` directly. **Not reported upstream** by
  Mark's ruling: the window is one release wide and closes when their
  pinned nightly moves past 1.97, so it is transient build churn during
  active 0.10 work rather than a design gap. Retire the fork by
  re-checking plain upstream later, not by waiting on a fix. Full
  detail in `quint-shaders/README.md`.
- **2026-08-13 (carriage):** a receipt that a `.spv` *exists* is not a
  receipt that it *runs*. The first passing suite took the WGSL
  fallback throughout, because the test device never requested
  `PASSTHROUGH_SHADERS`. The lane now reports which source it built
  from and a test asserts the SPIR-V path where the adapter allows it.
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
