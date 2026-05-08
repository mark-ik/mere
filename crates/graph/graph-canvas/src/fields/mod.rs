/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Field algebra for the graph canvas.
//!
//! Defines a small AST for scalar and vector fields over canvas coordinates,
//! coupling rules describing how nodes and edges respond to those fields, an
//! evaluator that produces values at requested points, and a per-canvas
//! [`FieldRegistry`] keyed by [`FieldId`].
//!
//! Architectural anchor: see
//! `repos/mere/design_docs/graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md`.
//!
//! ## Backends
//!
//! The default evaluator in [`eval`] is a pure-Rust analytic + finite-difference
//! pass: closed forms are used for known kernels (Gaussian, Linear, Disk,
//! analytic gradient where available), with a finite-difference fallback for
//! arbitrary compositions. A future Burn-wgpu backend (gated by the
//! `field-burn` feature) will lower an entire field expression to a fused
//! tensor program for vectorised evaluation; see `lower_burn`.

pub mod ast;
pub mod coupling;
pub mod eval;
#[cfg(feature = "field-burn")]
pub mod lower_burn;
pub mod projection;
pub mod registry;
#[cfg(feature = "field-rhai")]
pub mod rhai_bindings;

pub use ast::{Falloff, ScalarField, VectorField};
pub use coupling::{Coupling, CouplingResponse, EdgePath, EdgePathRule, NodeSelector};
pub use eval::{eval_scalar, eval_vector, grad_scalar};
pub use projection::{FieldProjection, FieldProjectionBuilder};
pub use registry::{FieldDef, FieldId, FieldRegistry};
