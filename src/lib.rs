//! numen — the field-primitive definitions of the aether/gyre physics substrate.
//!
//! A numen is a pervading influence, the presence felt in a place. This crate is
//! that influence as data: the portable definitions of the **fields** an app lays
//! over its graph, and the **couplings** that say how elements respond to them.
//! numen holds the definitions; it does not evaluate them (`aether` does, closed
//! forms and finite differences, optionally on Burn) and it does not integrate the
//! forces (`gyre` does, over rapier bodies).
//!
//! Fields are the third graph primitive, beside nodes and edges. Where the node and
//! edge primitives live in the content substrate ([`chartulary`](https://github.com/mark-ik/chartulary)),
//! the field primitives live here, at the same portable tier, because a field is
//! *spatial* (it reads positions) and so stays out of the position-free content
//! graph.
//!
//! - [`ScalarField`] / [`VectorField`] — the field algebra, `f: R^2 -> R` and
//!   `f: R^2 -> R^2`, as recursive data with `Sample` references to other fields by
//!   [`FieldId`].
//! - [`Field`] — a field as truth: identity + a [`FieldDefinition`] + a
//!   [`FieldExtent`] (global / region / attached-to-node) + a [`FieldLifecycle`].
//! - [`Coupling`] — `field -> `[`NodeSelector`]` x `[`CouplingResponse`]` x strength`:
//!   how elements respond to a field at their position. The response vocabulary is a
//!   recognized force core (the six `gyre` integrates) plus an open IRI tail
//!   ([`COUPLING_VOCAB`]) for visual / navigational / selection / semantic / trigger
//!   families.
//! - [`EdgePath`] / [`EdgePathRule`] — how an edge's curve is drawn, including a
//!   field-traced [`EdgePath::FieldLine`].
//!
//! Everything here is plain, serde-serializable data with no host dependencies, so
//! it compiles to `wasm32-unknown-unknown` and travels wherever the substrate does.

pub mod coupling;
pub mod edge_path;
pub mod field;
pub mod field_ast;

pub use coupling::{COUPLING_VOCAB, Coupling, CouplingResponse, NodeSelector};
pub use edge_path::{EdgePath, EdgePathRule};
pub use field::{CouplingId, Field, FieldDefinition, FieldExtent, FieldId, FieldLifecycle};
pub use field_ast::{Falloff, ScalarField, VectorField};
