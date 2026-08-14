//! # quint — the field algebra
//!
//! Defines a small AST for scalar and vector fields over canvas coordinates,
//! coupling rules describing how nodes and edges respond to those fields, an
//! evaluator that produces values at requested points, and a per-canvas
//! [`FieldRegistry`] keyed by [`FieldId`].
//!
//! A *quint* is the quintessence: the fifth element the classical world named as
//! the field-bearing medium, and in modern cosmology a scalar field. quint is that
//! medium here, the runtime that evaluates [`numen`]'s field and coupling definitions
//! into values. Force couplings compile to forces the `seiche` integrator applies to
//! bodies; visual couplings feed paint. quint stays platform-free and WASM-portable
//! (numen + serde + optional Rhai/Burn), never pulling in a host or renderer.
//!
//! Architectural anchor: the
//! [field-system extraction brief](../../../../design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md).
//!
//! ## Backends
//!
//! The default evaluator in [`eval`] is a pure-Rust analytic + finite-difference
//! pass: closed forms are used for known kernels (Gaussian, Linear, Disk,
//! analytic gradient where available), with a finite-difference fallback for
//! arbitrary compositions. A future Burn-wgpu backend (gated by the
//! `field-burn` feature) will lower an entire field expression to a fused
//! tensor program for vectorised evaluation; see `lower_burn`.

// The scalar/vector field AST is a portable `numen` primitive (`numen::field_ast`).
// quint re-exports it as `ast` so internal `crate::ast::` paths and external
// `quint::ast::` callers keep resolving onto the canonical types.
pub use numen::field_ast as ast;
pub mod coupling;
pub mod eval;
// The force laws. The burn-lowered passes inside are feature-gated;
// the module itself is not, because `repulsion_reference` is the
// burn-free anchor both GPU lanes are checked against, and an anchor
// that needs the thing it anchors is no anchor.
pub mod forces;

#[cfg(feature = "field-burn")]
pub mod lower_burn;
pub mod projection;
pub mod registry;
/// The explicit-regime lane: resident buffers advanced by kernels the
/// host dispatches. Feature-gated because it needs wgpu; see the
/// module docs for how it relates to the Burn lane beside it.
#[cfg(feature = "field-gpu")]
pub mod resident;
#[cfg(feature = "field-rhai")]
pub mod rhai_bindings;

pub use ast::{Falloff, ScalarField, VectorField};
pub use coupling::{Coupling, CouplingResponse, EdgePath, EdgePathRule, NodeSelector};
pub use eval::{eval_scalar, eval_vector, grad_scalar};
pub use projection::{FieldProjection, FieldProjectionBuilder};
pub use registry::{FieldDef, FieldId, FieldRegistry};
