# numen

The field-primitive definitions of the conatus physics family: fields and
couplings as plain, serde-serializable data. [`quint`](../quint) evaluates these
definitions; [`seiche`](../seiche) integrates the forces they produce.

## Modules

| Module | Public items | Contents |
|---|---|---|
| `field_ast` | `ScalarField`, `VectorField`, `Falloff` | The field algebra as recursive data (`f: R² → R` and `f: R² → R²`), with `Sample(FieldId)` references to other fields. Constructors like `ScalarField::gaussian_at` / `disk_at`. |
| `field` | `Field`, `FieldId`, `CouplingId`, `FieldDefinition`, `FieldExtent`, `FieldLifecycle` | A field as truth: identity, definition, extent, lifecycle. |
| `coupling` | `Coupling`, `CouplingResponse`, `NodeSelector`, `COUPLING_VOCAB` | One coupling rule: `id`, `field`, `selector`, `response`, `strength`. |
| `edge_path` | `EdgePath`, `EdgePathRule` | Per edge-kind curve generation: `Straight`, `Spline { tension }`, `FieldLine { field, max_steps, step_size }`. |

## Field extent and lifecycle

`FieldExtent` is `Global`, `Region { min_x, min_y, max_x, max_y }`,
`AttachedToNode(Uuid)`, or `Polygon { points }`. `FieldExtent::contains` and
`FieldExtent::boundary_distance` answer point queries directly and return `None`
for `AttachedToNode`, which only a graph can resolve. `FieldLifecycle` is
`Active` or `Retired`.

`Field::new` builds a `Global` / `Active` field; `with_name`, `with_extent`, and
`is_active` follow.

## Coupling responses

`CouplingResponse` carries a recognized force core of six variants, the ones
`seiche::CouplingForce` dispatches on:

| Variant | Effect |
|---|---|
| `AttractToMin` | Move along `-grad(scalar)`. |
| `RepelFromMax` | Move along `+grad(scalar)`. |
| `AlignVelocity` | Set velocity to the vector field at this position. |
| `FlowAdvect` | `pos += dt * field(pos)`. |
| `DampenInside { factor }` | Multiplicative damping inside a positive scalar region. |
| `ContainmentWall` | Pushout where the scalar field exceeds zero. |

`Open { predicate }` carries any other response by IRI under `COUPLING_VOCAB`
(`https://mere.computer/ns/coupling#`) for the visual, navigational, selection,
semantic, and trigger families; the force integrator skips it.
`CouplingResponse::open`, `recognized_iri`, `from_iri`, `is_force`, and
`predicate` are the accessors. `NodeSelector` is `All`, `Tagged(String)`,
`Kind(String)`, or `NotTagged(String)`.

## Dependencies

serde (derive), uuid (serde; the `v4` feature only on non-wasm targets), strum
(`EnumIter` over `CouplingResponse`). Derives are serde only; rkyv archiving is
handled at the consuming DTO layer.

Everything compiles to `wasm32-unknown-unknown`. `FieldId::new` and
`CouplingId::new` are gated to non-wasm targets, so wasm hosts mint ids with
`from_uuid` and a host-supplied UUID.

## License

MIT OR Apache-2.0.
