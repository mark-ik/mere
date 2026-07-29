# quint

Field algebra for graph canvases: scalar and vector fields over canvas space,
node and edge couplings, and evaluation — the portable field source whose
forces [`seiche`](https://github.com/merely-made/mere) integrates.

A `FieldProjection` bundles fields, couplings, and edge-path rules. Fields are a
small AST (`ScalarField` / `VectorField`) evaluated in closed form with
finite-difference gradients; couplings bind a field to a set of nodes with a
response. Authoring is available through a Rhai surface (`field-rhai`), and the
algebra lowers to a fused Burn tensor program (`field-burn`, with a wgpu backend
under `field-burn-wgpu`) for GPU evaluation.

The default build is portable and dependency-light: the field-primitive
definitions live in [`numen`](https://github.com/merely-made/mere); quint is the
runtime algebra over them. It knows nothing about any host graph — a consumer
feeds it fields, couplings, and node positions.

## License

MIT OR Apache-2.0.
