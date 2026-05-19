// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! register-renderer-types — wasm-friendly data types for the renderer
//! registry contract.
//!
//! Split from [`register-renderer`](https://docs.rs/register-renderer)
//! so wasm32-clean consumers (notably `host-runtime`) can consume the
//! input vocabulary + scene contract + renderer-identity types without
//! pulling vello / wgpu deps.
//!
//! See [README](../README.md) for the split rationale.
//!
//! ## Re-export shape
//!
//! `register-renderer` re-exports everything from this crate, so
//! consumers of the full registry can `use register_renderer::*` and
//! get the data types too. Direct consumers of *just* this crate use
//! `use register_renderer_types::*`.

#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

mod composition;
mod diagnostics;
mod handles;
mod identity;
mod input;
mod permissions;
mod scene;

pub use composition::CompositionMode;
pub use diagnostics::{
    DiagnosticEvent, DiagnosticSink, NoopSink, RecordingSink, RouteDegradedReason,
};
pub use handles::{OverlayHandle, ProducerHandle};
pub use identity::{ProfileBindingExpectation, RendererCapabilities, RendererId, ScreenRect};
pub use input::{
    ImeEvent, InputDisposition, InputEvent, KeyCode, KeyEventKind, ModifiersState, NamedKey,
    PointerButton, PointerEventKind,
};
pub use permissions::{
    CapabilityAction, DenyEverythingGate, PermissionDecision, PermissionGate, PermitEverythingGate,
};
pub use scene::{
    LodLevel, NodeContentKind, NodeContentKindSet, NodeIdentity, Placement, SceneNodeRef,
};

// Re-export kurbo so consumers of the data types share a version with the
// registry crate.
pub use kurbo;
