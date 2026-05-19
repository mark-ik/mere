// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `NodeRenderer` + the three composition-mode sub-traits.
//!
//! Renderers run on the host's UI thread; not `Send + Sync`.

use register_renderer_types::{
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
///
/// ## Dispatching to mode-specific traits
///
/// [`RendererRegistry`](crate::RendererRegistry) stores renderers as
/// `Box<dyn NodeRenderer>`; the substrate doesn't know which composition
/// mode a renderer implements without asking. The `as_*` accessors below
/// are the dyn-trait downcast seam: each composition-mode impl overrides
/// the matching one to return `Some(self)`. The default `None` is correct
/// for renderers that don't implement that mode.
///
/// Implementers typically write:
///
/// ```ignore
/// impl NodeRenderer for MyRenderer {
///     // ...identity/handles/composition_mode/capabilities...
///     fn as_in_scene_paint(&mut self) -> Option<&mut dyn InScenePaintRenderer> {
///         Some(self)
///     }
/// }
/// impl InScenePaintRenderer for MyRenderer { /* ... */ }
/// ```
pub trait NodeRenderer {
    fn renderer_id(&self) -> RendererId;
    fn handles(&self) -> NodeContentKindSet;
    fn composition_mode(&self) -> CompositionMode;
    fn capabilities(&self) -> RendererCapabilities;

    /// Return `Some(self)` if this renderer paints into the host's vello
    /// scene; otherwise `None`. Default `None`.
    fn as_in_scene_paint(&mut self) -> Option<&mut dyn InScenePaintRenderer> {
        None
    }

    /// Return `Some(self)` if this renderer produces wgpu textures on its
    /// own clock; otherwise `None`. Default `None`.
    fn as_embedded_frame(&mut self) -> Option<&mut dyn EmbeddedFrameRenderer> {
        None
    }

    /// Return `Some(self)` if this renderer renders into out-of-band OS
    /// overlays; otherwise `None`. Default `None`.
    fn as_overlay(&mut self) -> Option<&mut dyn OverlayRenderer> {
        None
    }
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
///
/// `next_frame` returns `Option<wgpu::Texture>` (not `TextureView`) because
/// vello's `Renderer::register_texture` takes a `wgpu::Texture` and creates
/// its own view internally. Hosts using a separate-pass compositor (e.g.
/// netrender's external-texture pipeline) call `.create_view(...)` on the
/// returned Texture. `wgpu::Texture` is a cheap Clone handle.
pub trait EmbeddedFrameRenderer: NodeRenderer {
    fn ensure_producer(&mut self, node: &SceneNodeRef) -> ProducerHandle;
    fn next_frame(&mut self, handle: ProducerHandle) -> Option<wgpu::Texture>;
    fn deliver_input(&mut self, handle: ProducerHandle, event: &InputEvent) -> InputDisposition;
    fn release(&mut self, handle: ProducerHandle);

    /// Drain the renderer's pending AccessKit `TreeUpdate` for
    /// `handle`, if any. The substrate's
    /// `SubstrateHost::collect_accesskit_updates` grafts the returned
    /// sub-tree onto the scene-level tree via AccessKit's `TreeId`
    /// mechanism.
    ///
    /// The renderer may return a `TreeUpdate` whose `tree_id` is
    /// `TreeId::ROOT` (the renderer thinks it's the root of its own
    /// tree). The substrate rewrites `tree_id` to its minted
    /// per-producer subtree id before forwarding to the host's
    /// AccessKit adapter.
    ///
    /// Default impl returns `None` — renderers that don't produce
    /// AccessKit content (e.g. `SolidRectRenderer`) take this default.
    fn take_accesskit_subtree(&mut self, handle: ProducerHandle) -> Option<accesskit::TreeUpdate> {
        let _ = handle;
        None
    }
}

/// Renderers that render into out-of-band OS surfaces.
pub trait OverlayRenderer: NodeRenderer {
    fn ensure_overlay(&mut self, node: &SceneNodeRef) -> OverlayHandle;
    fn position(&mut self, handle: OverlayHandle, rect: ScreenRect, z_order: i32);
    fn deliver_input(&mut self, handle: OverlayHandle, event: &InputEvent) -> InputDisposition;
    fn release(&mut self, handle: OverlayHandle);
}
