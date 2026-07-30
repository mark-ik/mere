# numen

The portable field-primitive definitions of the **quint** / **seiche** physics
substrate.

A *numen* is a pervading influence, the presence felt in a place. This crate is that
influence as data: the definitions of the **fields** an app lays over its graph, and
the **couplings** that say how elements respond to them. numen holds the definitions;
it does not evaluate them (that is [`quint`](https://github.com/merely-made/mere)) and
it does not integrate the forces (that is [`seiche`](https://github.com/merely-made/mere)).

Fields are the third graph primitive, beside nodes and edges. The node and edge
primitives live in the content substrate ([chartulary](https://github.com/merely-made/mere));
the field primitives live here, at the same portable tier, because a field is
*spatial* (it reads positions) and so stays out of the position-free content graph.

- `ScalarField` / `VectorField` — the field algebra (`f: R² → R` and `f: R² → R²`) as
  recursive data, with `Sample` references to other fields by `FieldId`.
- `Field` — a field as truth: identity + a `FieldDefinition` + a `FieldExtent`
  (global / region / attached-to-node) + a `FieldLifecycle`.
- `Coupling` — `field → NodeSelector × CouplingResponse × strength`. The response
  vocabulary is a recognized force core (the six `seiche` integrates) plus an open IRI
  tail for visual / navigational / selection / semantic / trigger families.
- `EdgePath` / `EdgePathRule` — how an edge's curve is drawn, including a field-traced
  `FieldLine`.

Everything is plain, serde-serializable data with no host dependencies, so it compiles
to `wasm32-unknown-unknown` and travels wherever the substrate does.

## License

MIT OR Apache-2.0.
