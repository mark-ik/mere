// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! numen — field truth, evaluation, and authoring for the Seiche substrate.
//!
//! A numen is a pervading influence, the presence felt in a place. This crate is
//! that influence as data: the portable definitions of the **fields** an app lays
//! over its graph, and the **couplings** that say how elements respond to them.
//! numen owns field definitions, the analytic evaluator, registries and projection
//! composition, plus opt-in Rhai authoring and Burn lowering. Seiche owns force
//! laws and integrates them over Rapier bodies.
//!
//! Fields are the third graph primitive, beside nodes and edges. Where the node and
//! edge primitives live in the content substrate ([`chartulary`](https://github.com/mark-ik/chartulary)),
//! the field primitives live here, at the same portable tier, because a field is
//! *spatial* (it reads positions). The types are numen's; the truth is the
//! graph realm's, persisted beside nodes and edges as content.
//!
//! - [`ScalarField`] / [`VectorField`] — the field algebra, `f: R^2 -> R` and
//!   `f: R^2 -> R^2`, as recursive data with `Sample` references to other fields by
//!   [`FieldId`].
//! - [`Field`] — a field as truth: identity + a [`FieldDefinition`] + a
//!   [`FieldExtent`] (global / region / attached-to-node) + a [`FieldLifecycle`].
//! - [`Coupling`] — `field -> `[`NodeSelector`]` x `[`CouplingResponse`]` x strength`:
//!   how elements respond to a field at their position. The response vocabulary is a
//!   recognized force core (the six `seiche` integrates) plus an open IRI tail
//!   ([`COUPLING_VOCAB`]) for visual / navigational / selection / semantic / trigger
//!   families.
//! - [`EdgePath`] / [`EdgePathRule`] — how an edge's curve is drawn, including a
//!   field-traced [`EdgePath::FieldLine`].
//!
//! The default path is plain, serde-serializable data plus analytic evaluation with
//! no GPU or host dependencies. Rhai and Burn stay behind named opt-in features.

pub mod coupling;
pub mod edge_path;
pub mod eval;
pub mod field;
pub mod field_ast;
#[cfg(feature = "field-burn")]
pub mod lower_burn;
pub mod projection;
pub mod registry;
#[cfg(feature = "field-rhai")]
pub mod rhai_bindings;

pub use coupling::{COUPLING_VOCAB, Coupling, CouplingResponse, NodeSelector};
pub use edge_path::{EdgePath, EdgePathRule};
pub use eval::{eval_scalar, eval_vector, grad_scalar};
pub use field::{CouplingId, Field, FieldDefinition, FieldExtent, FieldId, FieldLifecycle};
pub use field_ast as ast;
pub use field_ast::{Falloff, ScalarField, VectorField};
pub use projection::{FieldProjection, FieldProjectionBuilder};
pub use registry::{FieldDef, FieldRegistry};
