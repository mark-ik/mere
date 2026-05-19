// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! register-renderer — vello/wgpu-bound trait + dispatch surface for
//! Mere's renderer registry contract.
//!
//! Data-only types (`InputEvent`, `NodeContentKind`, `RendererId`,
//! `RendererCapabilities`, etc.) live in
//! [`register-renderer-types`](https://docs.rs/register-renderer-types)
//! so wasm32-clean consumers (notably `host-runtime`) can consume them
//! without pulling vello/wgpu transitively. This crate re-exports the types
//! crate's full surface, so consumers of the full registry can
//! `use register_renderer::*` and get everything.
//!
//! ## Public surface
//!
//! - [`NodeRenderer`] — common trait every renderer implements.
//! - [`InScenePaintRenderer`] / [`EmbeddedFrameRenderer`] / [`OverlayRenderer`]
//!   — composition-mode sub-traits.
//! - [`RendererRegistry`] — substrate-owned dispatcher.
//! - [`PaintCtx`] / [`PaintResult`] — paint contract for in-scene-paint
//!   renderers.
//! - All data types from `register-renderer-types` re-exported.

#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

mod paint;
mod registry;
mod renderer;

pub use paint::{PaintCtx, PaintError, PaintResult};
pub use registry::{
    DefaultSelector, DispatchError, HotSwapError, RegistryError, RendererRegistry, RendererSelector,
};
pub use renderer::{EmbeddedFrameRenderer, InScenePaintRenderer, NodeRenderer, OverlayRenderer};

// Re-export the data-types surface so callers don't need to add a separate
// dep on `register-renderer-types` just to name the types.
pub use register_renderer_types::{
    CapabilityAction, CompositionMode, DenyEverythingGate, DiagnosticEvent, DiagnosticSink,
    ImeEvent, InputDisposition, InputEvent, KeyCode, KeyEventKind, LodLevel, ModifiersState,
    NamedKey, NodeContentKind, NodeContentKindSet, NodeIdentity, NoopSink, OverlayHandle,
    PermissionDecision, PermissionGate, PermitEverythingGate, Placement, PointerButton,
    PointerEventKind, ProducerHandle, ProfileBindingExpectation, RecordingSink,
    RendererCapabilities, RendererId, RouteDegradedReason, SceneNodeRef, ScreenRect,
};

// Re-export shared dependencies for version alignment.
pub use kurbo;
pub use vello;
pub use wgpu;
