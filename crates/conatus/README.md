# conatus

The portable spatial-runtime family. This directory is not itself a crate;
each row below is a separate member of Mere's workspace with one narrow owner.

| Crate | Contents | Depends on |
|---|---|---|
| [`numen`](numen) | Field definitions as plain data: `ScalarField` / `VectorField`, `Field`, `Coupling`, `EdgePath`. | serde, uuid, strum |
| [`quint`](quint) | The runtime algebra: `FieldRegistry`, `FieldProjection`, `eval_scalar` / `eval_vector` / `grad_scalar`, optional Rhai authoring, and two GPU lanes: Burn lowering (`field-burn`) and the **resident lane** (`field-gpu`), whose kernels advance positions that never leave the device. | numen, serde, uuid, optional burn + rhai + wgpu |
| [`quint-shaders`](quint-shaders) | The resident lane's kernels in Rust, for rust-gpu. Not built today; see its README for the toolchain blocker and what ships instead. | spirv-std |
| [`seiche`](seiche) | Force integration: a rapier `Simulation`, built-in layout forces, field couplings, scenes, fluid. | rapier2d, petgraph, quint, numen, euclid, tracing |
| [`voxel`](voxel) | Generic revisioned voxel chunks, bounded patches, dirty regions, and occupancy lowering. Product identity and material meaning stay outside. | serde |
| [`conatus`](conatus) | Host-neutral 3D bodies, collision, queries, fixed-step advancement, and voxel-collider realization. | conatus-voxel, rapier3d, serde |
| [`brick`](brick) | Product-neutral sparse pointer/atlas ABI and ray-in WGSL voxel DDA. Camera, appearance, and composition stay in product lenses. | bytemuck |

The resident lane is the explicit-regime half of the spatial compute
plan (`design_docs/mere_docs/technical_architecture/2026-08-13_spatial_compute_plan.md`):
positions and velocities live in GPU buffers as padded 3D `vec4f`, three
dispatches advance them, and the only per-frame readback is a four-byte
settle word. It sits beside the Burn lane rather than replacing it. Burn
serves dense field evaluation and semantic couplings; the resident lane
serves the n-body step, where the tensor formulation's `[n, n]`
intermediates are the wrong shape. The device is always the host's:
quint never boots one, because a second device on the same adapter
cannot share a buffer with the renderer.

The field lane runs numen to quint to seiche. `conatus-voxel`, `conatus`, and
`conatus-brick` are sibling organs rather than another chain: value mechanics,
body runtime, and presentation traversal respectively. Each package stays
independent of product authority. Existing portable mechanics retain their
MIT OR Apache-2.0 grants; new product-neutral brick traversal follows Mere's
MPL-2.0 default.

Fields are treated as a third canvas primitive beside nodes and edges. Node and
edge truth lives in the content substrate
([chartulary](../eidetic/chartulary)); field truth lives here because a field
reads positions.

The name: a conatus is the instantaneous striving that, integrated over time,
becomes motion. Definition, evaluation, integration.

Merged from three sibling repos on 2026-07-21 with histories preserved, absorbed
into mere's workspace on 2026-07-23.

Background:
[`design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md`](../../design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md).
