# conatus

The portable canvas-physics family. *Conatus* is Leibniz's word for the
instantaneous striving that, integrated over time, becomes motion; Spinoza's
for a body's perseverance in its own state. The stack is that idea as crates:

| Crate | Role |
|---|---|
| [`numen`](crates/numen) | Defines what bodies strive under: scalar/vector field algebra as plain data, the `Field` truth primitive, and `Coupling` (field, node selector, response). WASM-clean, kernel-free. |
| [`quint`](crates/quint) | Evaluates the instantaneous striving: field algebra over canvas space, node and edge couplings. |
| [`seiche`](crates/seiche) | Integrates striving into motion: rapier-backed force-directed layout with themable forces. |

Definition, evaluation, integration. Each crate publishes individually under
its own name; this workspace is their shared home. Fields are the third graph
primitive beside nodes and edges, and none of this depends on any particular
graph kernel.

Formerly three sibling repos (numen, quint, seiche), merged 2026-07-21 with
histories preserved.

## License

MIT OR Apache-2.0.
