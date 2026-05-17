// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! mere-renderer-registry — vello/wgpu-bound trait + dispatch surface for
//! Mere's renderer registry contract.
//!
//! Data-only types (`InputEvent`, `NodeContentKind`, `RendererId`,
//! `RendererCapabilities`, etc.) live in
//! [`mere-renderer-registry-types`](https://docs.rs/mere-renderer-registry-types)
//! so wasm32-clean consumers (notably `mere-host-runtime`) can consume them
//! without pulling vello/wgpu transitively. This crate re-exports the types
//! crate's full surface, so consumers of the full registry can
//! `use mere_renderer_registry::*` and get everything.
//!
//! ## Public surface
//!
//! - [`NodeRenderer`] — common trait every renderer implements.
//! - [`InScenePaintRenderer`] / [`EmbeddedFrameRenderer`] / [`OverlayRenderer`]
//!   — composition-mode sub-traits.
//! - [`RendererRegistry`] — substrate-owned dispatcher.
//! - [`PaintCtx`] / [`PaintResult`] — paint contract for in-scene-paint
//!   renderers.
//! - All data types from `mere-renderer-registry-types` re-exported.

#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

mod paint;
mod registry;
mod renderer;

pub use paint::{PaintCtx, PaintError, PaintResult};
pub use registry::{
    DefaultSelector, DispatchError, RegistryError, RendererRegistry, RendererSelector,
};
pub use renderer::{
    EmbeddedFrameRenderer, InScenePaintRenderer, NodeRenderer, OverlayRenderer,
};

// Re-export the data-types surface so callers don't need to add a separate
// dep on `mere-renderer-registry-types` just to name the types.
pub use mere_renderer_registry_types::{
    CompositionMode, DiagnosticEvent, DiagnosticSink, ImeEvent, InputDisposition, InputEvent,
    KeyCode, KeyEventKind, LodLevel, ModifiersState, NamedKey, NodeContentKind, NodeContentKindSet,
    NodeIdentity, NoopSink, OverlayHandle, Placement, PointerButton, PointerEventKind,
    ProducerHandle, ProfileBindingExpectation, RecordingSink, RendererCapabilities, RendererId,
    RouteDegradedReason, SceneNodeRef, ScreenRect,
};

// Re-export shared dependencies for version alignment.
pub use kurbo;
pub use vello;
pub use wgpu;
