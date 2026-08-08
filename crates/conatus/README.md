# conatus

The portable canvas-physics family: three crates that define fields, evaluate
them, and integrate the forces they produce. This directory is not itself a
crate; the three below are separate members of mere's workspace.

| Crate | Contents | Depends on |
|---|---|---|
| [`numen`](numen) | Field definitions as plain data: `ScalarField` / `VectorField`, `Field`, `Coupling`, `EdgePath`. | serde, uuid, strum |
| [`quint`](quint) | The runtime algebra: `FieldRegistry`, `FieldProjection`, `eval_scalar` / `eval_vector` / `grad_scalar`, optional Rhai authoring and Burn lowering. | numen, serde, uuid, optional burn + rhai |
| [`seiche`](seiche) | Force integration: a rapier `Simulation`, built-in layout forces, field couplings, scenes, fluid. | rapier2d, petgraph, quint, numen, euclid, tracing |

Dependency direction is numen to quint to seiche. Each crate publishes under its
own name and stays independent of any graph kernel or renderer. All three are
MIT OR Apache-2.0.

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
