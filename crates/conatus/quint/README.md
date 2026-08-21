# quint

Field algebra for graph canvases: the runtime that evaluates
[`numen`](../numen)'s field and coupling definitions into values at requested
points. [`seiche`](../seiche) integrates the forces those values produce. quint
is fed fields, couplings, and positions by its host and knows nothing about the
host's graph.

## Modules

| Module | Public items | Contents |
|---|---|---|
| `ast` | re-export of `numen::field_ast` (`ScalarField`, `VectorField`, `Falloff`) | The field AST, re-exported so `quint::ast::` paths resolve onto the canonical types. |
| `coupling` | re-export of `numen`'s `Coupling`, `CouplingResponse`, `NodeSelector`, `EdgePath`, `EdgePathRule` | Same, for the coupling and edge-path types. |
| `eval` | `eval_scalar`, `eval_vector`, `grad_scalar` | Pure-Rust evaluator. Closed forms for the known kernels (Gaussian, Linear, Disk) and analytic gradients where available, with a finite-difference fallback for arbitrary compositions. |
| `registry` | `FieldRegistry`, `FieldId`, `FieldDef` | Per-canvas store of named scalar and vector fields. `insert_scalar` / `insert_vector` / `insert_with_id` / `get` / `lookup` / `name_of` / `remove` / `iter`. |
| `projection` | `FieldProjection`, `FieldProjectionBuilder` | Per-canvas bundle: a `FieldRegistry`, `Vec<Coupling>`, `Vec<EdgePathRule>`, and an optional `z_field` driving per-node z in 2.5D. |
| `lower_burn` (`field-burn`) | `lower_scalar`, `lower_vector`, `LowerError` | Lowers an AST to a Burn tensor program over rank-1 `xs` / `ys` batches, with the execution device chosen at runtime. |
| `forces` (`field-burn`) | `repulsion`, `node_exclusion`, parameter types | Tensorized N-body laws: smooth field repulsion and the exact hard-floor/cutoff layout law. `*_wgpu_roundtrip` helpers are explicit CPU-GPU-CPU staging paths, not resident simulation. |
| `rhai_bindings` (`field-rhai`) | `build_engine`, `build_from_script`, `BuildError` | Rhai authoring surface; a script's final expression must be a `FieldProjection`. |

`FieldId`s minted by `FieldRegistry` are registry-local, assigned from a counter
via `Uuid::from_u128`; `insert_with_id` seeds the registry with a host's own
canonical ids. Re-inserting an existing name overwrites the definition and keeps
the id, so dependent `Sample` expressions stay valid.

`FieldProjectionBuilder` is a unit struct today, held as a namespace for future
preset constructors.

## Features

| Feature | Effect |
|---|---|
| default | numen, serde, and uuid only. |
| `field-burn` | Pulls Burn 0.22.0-pre.2 with the NdArray backend; enables `lower_burn` and `forces`. |
| `field-burn-wgpu` | `field-burn` plus `burn/wgpu` for GPU evaluation, and getrandom's `wasm_js` backend on wasm. |
| `field-rhai` | Pulls rhai 1.20 and enables `rhai_bindings`. |

## Dependencies

numen (path, version 0.1), serde, uuid. burn and rhai are optional and off by
default. `rust-version` is 1.92.0.

Lowering coverage and its `LowerError` cases (`UnsupportedOperator`,
`UnknownField`, `SampleTypeMismatch`) are documented on `lower_burn` itself.
Background:
[`design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md`](../../../design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md).

## License

MIT OR Apache-2.0.
