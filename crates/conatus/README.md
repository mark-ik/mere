# conatus

The portable spatial-physics family: fields and their forces for canvases,
plus the shared spatial runtime the games wing consumes. This directory is
not itself a crate; the crates below are separate members of mere's
workspace.

| Crate | Contents | Depends on |
|---|---|---|
| [`numen`](numen) | Field truth and evaluation: `ScalarField` / `VectorField`, `Field`, `Coupling`, `EdgePath`, `FieldRegistry`, `FieldProjection`, analytic evaluation, plus opt-in Rhai and Burn lowering. | serde, uuid, strum, optional burn + rhai |
| [`seiche`](seiche) | Force integration and laws: a Rapier `Simulation`, built-in layout forces, tensorized repulsion, field couplings, scenes, fluid. | rapier2d, petgraph, numen, euclid, tracing, optional burn |
| [`conatus`](conatus) | Host-neutral 3D body, collision, query, and fixed-step runtime. Its opt-in `resident` module owns `ResidentChunk`, CubeCL allocation, and the device-resident spatial lane. | nisus, rapier3d, serde, optional burn + wgpu + cubecl |
| [`nisus`](nisus) | Generic revisioned voxel chunk and edit mechanics — the *nisus formativus*, the striving by which the world's matter takes and re-takes shape; consumed by Mesocosm's `GroundVoxelProfile` as the disposable view beside its record. Renamed from `conatus-voxel` and claimed on crates.io 2026-08-28. | serde |

A sixth member is staged on the `codex/conatus-brick-lift` branch:
`modulus` (renamed from `conatus-brick` and claimed on crates.io
2026-08-28 — the classical architect's base unit of measure, and the
layout math is literally modular arithmetic), the shared sparse-brick
presentation ABI (deterministic
`BrickMap`, `BrickTraceSpace`, the camera-neutral `BRICK_DDA_WGSL`, and a
capacity-fixed retargeting mode), which Mesocosm and Paredros already
consume by pinned rev. It joins this table when the branch lands.

The resident lane is the explicit-regime half of the spatial compute
plan (`design_docs/mere_docs/technical_architecture/2026-08-13_spatial_compute_plan.md`):
positions and velocities live in GPU buffers as padded 3D `vec4f`, three
dispatches advance them, and the only per-frame readback is a four-byte
settle word. It sits beside the Burn lane rather than replacing it. Burn
serves dense field evaluation and semantic couplings; the resident lane
serves the n-body step, where the tensor formulation's `[n, n]`
intermediates are the wrong shape. The device is always the host's:
Conatus never boots one, because a second device on the same adapter
cannot share a buffer with the renderer.

Dependency direction is numen to seiche on the field side, with Conatus
resident state adjacent to its spatial runtime, and nisus to conatus on the
runtime side. Each crate publishes under its
own name and stays independent of any graph kernel or renderer. All of them
are MPL-2.0 (see LICENSE).

Fields are treated as a third canvas primitive beside nodes and edges. Node and
edge truth lives in the content substrate
([chartulary](../eidetic/chartulary)); the field *vocabulary* lives here
because a field is spatial, while field truth itself belongs to the graph
realm, persisted beside nodes and edges (the consolidation map's realms
clause in the conatus engine plan).

The name: a conatus is the instantaneous striving that, integrated over time,
becomes motion. Definition, evaluation, integration.

Merged from three sibling repos on 2026-07-21 with histories preserved, absorbed
into mere's workspace on 2026-07-23.

Background:
[`design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md`](../../design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md).
