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

/// What a [`SurfaceTileWidget`] wants realized into its external layer.
#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceContent {
    /// Fill the layer with a solid color (the original stub; still used as a
    /// placeholder for a web tile whose first frame hasn't arrived).
    Solid([u8; 4]),
    /// A live system WebView at this URL. The [on_tick host](WebSurfaceHost)
    /// drives the producer; the compositor copies its frames.
    Web { url: String },
}

/// Maps each [`SurfaceTileWidget`]'s `WidgetId` to its current content. The
/// widget writes its entry; the app's external-compositor hook + the on_tick
/// host read it by the `widget_id` masonry reports for each external layer.
/// Cheap, low-frequency (one insert per tile paint), shared by `Arc` clone
/// between the widgets, the compositor closure, and the tick closure.
#[derive(Clone, Default)]
pub struct SurfaceRegistry(Arc<Mutex<HashMap<WidgetId, SurfaceContent>>>);

impl SurfaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn set(&self, id: WidgetId, content: SurfaceContent) {
        if let Ok(mut map) = self.0.lock() {
            map.insert(id, content);
        }
    }

    fn remove(&self, id: WidgetId) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(&id);
        }
    }

    /// The content registered for `id`, if any. Called by the compositor hook.
    pub fn content(&self, id: WidgetId) -> Option<SurfaceContent> {
        self.0.lock().ok().and_then(|map| map.get(&id).cloned())
    }

    /// All registered `(id, content)` pairs — the tick host scans these to find
    /// the web tiles it must keep a producer for.
    pub fn entries(&self) -> Vec<(WidgetId, SurfaceContent)> {
        self.0
            .lock()
            .map(|map| map.iter().map(|(id, c)| (*id, c.clone())).collect())
            .unwrap_or_default()
    }
}

// =============================================================================
// Widget
// =============================================================================

/// A tile whose content is realized as an external GPU layer, not Masonry paint.
pub struct SurfaceTileWidget {
    registry: SurfaceRegistry,
    content: SurfaceContent,
    size: Size,
}

impl SurfaceTileWidget {
    fn new(registry: SurfaceRegistry, content: SurfaceContent) -> Self {
        Self { registry, content, size: Size::ZERO }
    }

    fn set_content(this: &mut WidgetMut<'_, Self>, content: SurfaceContent) {
        if this.widget.content != content {
            this.widget.content = content;
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
        // content. Masonry hands the embedder this widget's id + bounds in
        // `composite_external_layers`; the compositor + tick host look the
        // content up here. We deliberately paint nothing — the compositor fills
        // the region (solid color, or the WebView frame).
        self.registry.set(ctx.widget_id(), self.content.clone());
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
        // Written by `write_texture` (needs COPY_DST) and read by
        // `copy_texture_to_texture` (needs COPY_SRC).
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
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
// WebView producer host (driven from on_tick — outside render)
// =============================================================================

/// Owns the live system-WebView producer(s) and drives them from the on_tick
/// hook (main thread, outside `render()`), where pumping the OS message loop is
/// safe. The compositor only ever *copies* an already-prepared frame; it never
/// touches the producer.
///
/// v1 is **single-producer**, keyed by URL (not widget id — the surf tile's
/// `WidgetId` churns across view rebuilds, which would otherwise spawn a fresh
/// WebView each frame). Multi-tile + per-tile lifecycle is a later step.
/// Windows-only for now (WebView2); other platforms no-op until their producer
/// is wired.
pub struct WebSurfaceHost {
    #[allow(dead_code)]
    user_data_dir: std::path::PathBuf,
    #[cfg(target_os = "windows")]
    producer: Option<scrying::PlatformWebSurfaceProducer>,
    #[cfg(target_os = "windows")]
    current_url: Option<String>,
    /// WebView2's composition path requires a `DispatcherQueue` on this thread
    /// before `Compositor::new`. Created once on first build and held alive for
    /// the producer's lifetime.
    #[cfg(target_os = "windows")]
    dispatcher_queue: Option<windows::System::DispatcherQueueController>,
    /// Imports the producer's `Dx12SharedTexture` frames into our wgpu (DX12)
    /// device. Created once the device is known.
    #[cfg(target_os = "windows")]
    importer: Option<scrying::WgpuTextureImporter>,
    /// Log the first successful import once, not every frame.
    #[cfg(target_os = "windows")]
    logged_import: bool,
    /// The most-recently imported frame texture. Re-imported only when the
    /// producer reports `resource_is_new`; otherwise the producer reused its
    /// underlying allocation and re-opening the (now-stale) shared handle fails
    /// — we keep this cached texture instead. Step 3b composites it.
    #[cfg(target_os = "windows")]
    imported_texture: Option<wgpu::Texture>,
}

impl WebSurfaceHost {
    pub fn new(user_data_dir: std::path::PathBuf) -> Self {
        Self {
            user_data_dir,
            #[cfg(target_os = "windows")]
            producer: None,
            #[cfg(target_os = "windows")]
            current_url: None,
            #[cfg(target_os = "windows")]
            dispatcher_queue: None,
            #[cfg(target_os = "windows")]
            importer: None,
            #[cfg(target_os = "windows")]
            logged_import: false,
            #[cfg(target_os = "windows")]
            imported_texture: None,
        }
    }

    /// Ensure a producer parented to `hwnd`, navigated to `url`, at `size`
    /// physical px, and pump one frame: acquire the producer's Dx12 shared
    /// texture and import it into our wgpu (DX12) device. Builds on first call
    /// (or when `url` changes). Safe to call every tick (acquire is non-blocking
    /// after the first frame). Step 3a: import + log; 3b composites it.
    pub fn ensure(
        &mut self,
        hwnd: *mut std::ffi::c_void,
        url: &str,
        size: (u32, u32),
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        #[cfg(target_os = "windows")]
        {
            if self.current_url.as_deref() == Some(url) {
                if let Some(producer) = self.producer.as_mut() {
                    while let Some(event) = producer.poll_navigation_event() {
                        eprintln!("[web] nav: {event:?}");
                    }
                }
                self.pump_frame(device, queue);
                return;
            }

            // WebView2's WinComp `Compositor` requires a DispatcherQueue on
            // this (UI/event-loop) thread. Create + hold it once before the
            // first producer build.
            if self.dispatcher_queue.is_none() {
                use windows::Win32::System::WinRT::{
                    CreateDispatcherQueueController, DQTAT_COM_STA, DQTYPE_THREAD_CURRENT,
                    DispatcherQueueOptions,
                };
                self.dispatcher_queue = unsafe {
                    CreateDispatcherQueueController(DispatcherQueueOptions {
                        dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                        threadType: DQTYPE_THREAD_CURRENT,
                        apartmentType: DQTAT_COM_STA,
                    })
                }
                .ok();
            }

            let config = scrying::PlatformWebSurfaceConfig::new(
                dpi::PhysicalSize::new(size.0.max(1), size.1.max(1)),
                self.user_data_dir.clone(),
            );
            // SAFETY: `hwnd` is the live top-level window handle from
            // `TickCtx::windows`, valid for the app's lifetime.
            match unsafe { scrying::PlatformWebSurfaceProducer::new(hwnd, config) } {
                Ok(producer) => {
                    eprintln!("[web] producer built ({}x{}) for {url}", size.0, size.1);
                    if let Err(err) = producer.load_url(url) {
                        eprintln!("[web] load_url failed: {err}");
                    }
                    self.producer = Some(producer);
                    self.current_url = Some(url.to_string());
                }
                Err(err) => eprintln!("[web] producer build failed: {err}"),
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (hwnd, url, size, device, queue);
        }
    }

    /// Acquire the producer's latest frame (non-blocking after the first) and
    /// import its `Dx12SharedTexture` into our wgpu device. Step 3a logs the
    /// first successful import; 3b stores the texture for the compositor.
    #[cfg(target_os = "windows")]
    fn pump_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        use scrying::{
            HostWgpuContext, ImportOptions, TextureImporter, WebSurfaceFrame, WgpuTextureImporter,
        };

        if self.importer.is_none() {
            self.importer = Some(WgpuTextureImporter::new(HostWgpuContext::new(
                device.clone(),
                queue.clone(),
            )));
        }

        let Some(producer) = self.producer.as_mut() else {
            return;
        };
        let frame = match producer.try_acquire_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => return,
            Err(err) => {
                eprintln!("[web] acquire failed: {err}");
                return;
            }
        };
        let WebSurfaceFrame::Native(native) = &frame.frame else {
            return;
        };
        // Re-import only on a fresh allocation. When the producer reused its
        // texture (`resource_is_new == false`) the shared handle is stale —
        // keep the cached import (its backing memory was just overwritten).
        if !frame.resource_is_new && self.imported_texture.is_some() {
            return;
        }
        match self
            .importer
            .as_ref()
            .expect("importer created above")
            .import_frame(native, &ImportOptions::default())
        {
            Ok(imported) => {
                if !self.logged_import {
                    self.logged_import = true;
                    eprintln!(
                        "[web] imported frame {}x{} ({:?}) gen {}",
                        imported.size.width, imported.size.height, imported.format, imported.generation
                    );
                }
                self.imported_texture = Some(imported.texture);
            }
            Err(err) => eprintln!("[web] import failed: {err}"),
        }
    }
}

// =============================================================================
// Xilem view
// =============================================================================

/// A reactive external-surface tile. The widget registers `content` in
/// `registry` so the compositor hook (and, for `Web`, the on_tick host) can
/// realize it.
pub fn surface_tile<State>(registry: SurfaceRegistry, content: SurfaceContent) -> SurfaceTile<State>
where
    State: 'static,
{
    SurfaceTile { registry, content, phantom: PhantomData }
}

/// The [`View`] created by [`surface_tile`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SurfaceTile<State> {
    registry: SurfaceRegistry,
    content: SurfaceContent,
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
            ctx.create_pod(SurfaceTileWidget::new(
                self.registry.clone(),
                self.content.clone(),
            )),
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
        SurfaceTileWidget::set_content(&mut element, self.content.clone());
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        // Drop our registry entry so the compositor + tick host stop tracking a
        // tile that's no longer shown. (Producer teardown is step 5.)
        self.registry.remove(element.ctx.widget_id());
    }

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
