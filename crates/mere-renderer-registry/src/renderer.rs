// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `NodeRenderer` + the three composition-mode sub-traits.
//!
//! Renderers run on the host's UI thread; not `Send + Sync`.

use mere_renderer_registry_types::{
    CompositionMode, InputDisposition, InputEvent, NodeContentKindSet, OverlayHandle,
    ProducerHandle, RendererCapabilities, RendererId, SceneNodeRef, ScreenRect,
};

use crate::paint::{PaintCtx, PaintResult};

/// Common surface every renderer implements.
///
/// All return types are wasm-friendly data; the `NodeRenderer` trait itself
/// could in principle live in `mere-renderer-registry-types`. It lives here
/// instead because its sub-traits (below) reference vello / wgpu types and
/// it's cleaner to keep all four traits in one crate.
pub trait NodeRenderer {
    fn renderer_id(&self) -> RendererId;
    fn handles(&self) -> NodeContentKindSet;
    fn composition_mode(&self) -> CompositionMode;
    fn capabilities(&self) -> RendererCapabilities;
}

/// Renderers that paint scene ops into the host's vello scene during the
/// host's paint pass.
pub trait InScenePaintRenderer: NodeRenderer {
    /// Paint into the host scene at the node's placement transform.
    fn paint(&mut self, node: &SceneNodeRef, ctx: &mut PaintCtx<'_>) -> PaintResult;

    /// Dispatch an input event to this node.
    ///
    /// Position-bearing events (Pointer) are in tile-local logical
    /// coordinates — the substrate input router has already mapped from
    /// host coordinates via the inverse-placement transform.
    fn input(&mut self, node: &SceneNodeRef, event: &InputEvent) -> InputDisposition;
}

/// Renderers that produce wgpu textures on their own clock; the host
/// composites them as external textures.
pub trait EmbeddedFrameRenderer: NodeRenderer {
    fn ensure_producer(&mut self, node: &SceneNodeRef) -> ProducerHandle;
    fn next_frame(&mut self, handle: ProducerHandle) -> Option<wgpu::TextureView>;
    fn deliver_input(&mut self, handle: ProducerHandle, event: &InputEvent) -> InputDisposition;
    fn release(&mut self, handle: ProducerHandle);
}

/// Renderers that render into out-of-band OS surfaces.
pub trait OverlayRenderer: NodeRenderer {
    fn ensure_overlay(&mut self, node: &SceneNodeRef) -> OverlayHandle;
    fn position(&mut self, handle: OverlayHandle, rect: ScreenRect, z_order: i32);
    fn deliver_input(&mut self, handle: OverlayHandle, event: &InputEvent) -> InputDisposition;
    fn release(&mut self, handle: OverlayHandle);
}
