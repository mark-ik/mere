// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! External-surface tiles — the verso realization seam for engine content that
//! paints itself on the GPU (a system WebView via `scrying`, later).
//!
//! Per the [verso adoption plan](../../../design_docs/mere_docs/implementation_strategy/2026-05-27_verso_adoption_plan.md)
//! Phase 3: an engine surface isn't drawn by Masonry — it's an external wgpu
//! texture composited into the tile's on-screen rect. The seam has three parts,
//! all here:
//!
//! 1. [`SurfaceTileWidget`] — a Masonry widget that reserves an **external
//!    layer** for its rect (`PaintLayerMode::External`) and registers its
//!    content key in the [`SurfaceRegistry`]. It paints nothing itself.
//! 2. [`SurfaceRegistry`] — the decoupler: maps each tile widget's `WidgetId`
//!    to its content. Masonry's composite hook gives the embedder only widget
//!    ids + bounds, so the widget writes here and the compositor reads here.
//! 3. [`composite_color`] — the per-layer GPU op the app's
//!    `with_external_compositor` hook calls: fill the layer's bounds in the
//!    shared render target.
//!
//! **This is the stub cut**: the registry holds a solid color and
//! `composite_color` fills the rect with it — enough to prove the whole pipe
//! (widget → external layer → registry → driver composite → on screen) end to
//! end on the GPU, before a real `scrying` producer replaces the color with a
//! live WebView texture (a contained follow-up: swap the registry value type
//! and `composite_color`'s body for a `copy_texture_to_texture` from the
//! producer's frame).

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::accesskit::{Node as AccessNode, Role};
use xilem::masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, PaintCtx, PaintLayerMode, PropertiesRef,
    RegisterCtx, Widget, WidgetId, WidgetMut,
};
use xilem::masonry::imaging::Painter;
use xilem::masonry::kurbo::{Axis, Size};
use xilem::masonry::layout::{LenReq, Length};
use xilem::{Pod, ViewCtx};

const DEFAULT_LENGTH: Length = Length::const_px(100.0);

/// Maps each [`SurfaceTileWidget`]'s `WidgetId` to its current content. The
/// widget writes its entry; the app's external-compositor hook reads it by the
/// `widget_id` masonry reports for each external layer. Cheap, low-frequency
/// (one insert per tile paint), shared by `Arc` clone between the widgets and
/// the compositor closure.
///
/// Stub content is a solid `[r, g, b, a]`; the real lane swaps this for a
/// producer-backed texture handle.
#[derive(Clone, Default)]
pub struct SurfaceRegistry(Arc<Mutex<HashMap<WidgetId, [u8; 4]>>>);

impl SurfaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn set(&self, id: WidgetId, color: [u8; 4]) {
        if let Ok(mut map) = self.0.lock() {
            map.insert(id, color);
        }
    }

    /// The content registered for `id`, if any. Called by the compositor hook.
    pub fn color(&self, id: WidgetId) -> Option<[u8; 4]> {
        self.0.lock().ok().and_then(|map| map.get(&id).copied())
    }
}

// =============================================================================
// Widget
// =============================================================================

/// A tile whose content is realized as an external GPU layer, not Masonry paint.
pub struct SurfaceTileWidget {
    registry: SurfaceRegistry,
    color: [u8; 4],
    size: Size,
}

impl SurfaceTileWidget {
    fn new(registry: SurfaceRegistry, color: [u8; 4]) -> Self {
        Self { registry, color, size: Size::ZERO }
    }

    fn set_color(this: &mut WidgetMut<'_, Self>, color: [u8; 4]) {
        if this.widget.color != color {
            this.widget.color = color;
            this.ctx.request_render();
        }
    }
}

impl Widget for SurfaceTileWidget {
    type Action = ();

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::FitContent(space) => space,
            _ => DEFAULT_LENGTH,
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _painter: &mut Painter<'_>) {
        // Reserve an external layer for this widget's rect and publish our
        // content key. Masonry hands the embedder this widget's id + bounds in
        // `composite_external_layers`; the compositor looks the color up here.
        // We deliberately paint nothing — the compositor fills the region.
        self.registry.set(ctx.widget_id(), self.color);
        ctx.set_paint_layer_mode(PaintLayerMode::External);
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut AccessNode,
    ) {
        node.set_description("External engine surface".to_string());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

// =============================================================================
// GPU compositing (the embedder hook's per-layer op)
// =============================================================================

/// Fill `bounds` of the shared render `target` with a solid `color`, on the
/// shared wgpu device. The app's `with_external_compositor` hook calls this for
/// each external layer. Stub: a solid fill via a staging texture +
/// `copy_texture_to_texture` (no pipeline/shader); the real lane copies from a
/// `scrying` producer's frame texture instead.
///
/// `target` is `Rgba8Unorm` with `COPY_DST` usage (masonry's contract), so the
/// copy needs no readback — both textures live on the same device.
pub fn composite_color(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    bounds: [u32; 4],
    color: [u8; 4],
) {
    let [x, y, w, h] = bounds;
    if w == 0 || h == 0 {
        return;
    }

    let staging = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mere-surface-stub"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        pixels.extend_from_slice(&color);
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &staging,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mere-surface-composite"),
    });
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &staging,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit(std::iter::once(encoder.finish()));
}

// =============================================================================
// Xilem view
// =============================================================================

/// A reactive external-surface tile. `color` is the stub content; the widget
/// registers it in `registry` so the app's compositor hook can realize it.
pub fn surface_tile<State>(registry: SurfaceRegistry, color: [u8; 4]) -> SurfaceTile<State>
where
    State: 'static,
{
    SurfaceTile { registry, color, phantom: PhantomData }
}

/// The [`View`] created by [`surface_tile`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SurfaceTile<State> {
    registry: SurfaceRegistry,
    color: [u8; 4],
    phantom: PhantomData<fn() -> State>,
}

impl<State> ViewMarker for SurfaceTile<State> {}

impl<State, Action> View<State, Action, ViewCtx> for SurfaceTile<State>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<SurfaceTileWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(SurfaceTileWidget::new(self.registry.clone(), self.color)),
            (),
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        SurfaceTileWidget::set_color(&mut element, self.color);
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}
