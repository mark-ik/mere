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
use xilem::masonry::core::pointer::PointerButton;
use xilem::masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PaintLayerMode,
    PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, RegisterCtx, ScrollDelta, Widget, WidgetId, WidgetMut,
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
    /// Pointer events are queued here for the on_tick host to forward to the
    /// producer (only meaningful for a `Web` tile).
    channel: SurfaceChannel,
    size: Size,
}

impl SurfaceTileWidget {
    fn new(registry: SurfaceRegistry, content: SurfaceContent, channel: SurfaceChannel) -> Self {
        Self { registry, content, channel, size: Size::ZERO }
    }

    fn set_content(this: &mut WidgetMut<'_, Self>, content: SurfaceContent) {
        if this.widget.content != content {
            this.widget.content = content;
            this.ctx.request_render();
        }
    }

    /// Queue a pointer event (tile-local logical coords) for the host.
    fn push_input(&self, kind: TileMouseKind, pos: xilem::masonry::kurbo::Point, delta: f32) {
        if !matches!(self.content, SurfaceContent::Web { .. }) {
            return;
        }
        if let Ok(mut channel) = self.channel.lock() {
            channel.input.push(TileMouse { kind, x: pos.x as f32, y: pos.y as f32, delta });
        }
    }
}

impl Widget for SurfaceTileWidget {
    type Action = ();

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Forward pointer activity into the producer's input queue. Coords are
        // tile-local logical px; the host scales to the WebView's physical px.
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary) | None,
                state,
                ..
            }) => {
                ctx.capture_pointer();
                self.push_input(TileMouseKind::LeftDown, ctx.local_position(state.position), 0.0);
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                self.push_input(TileMouseKind::Move, ctx.local_position(current.position), 0.0);
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                self.push_input(TileMouseKind::LeftUp, ctx.local_position(state.position), 0.0);
            }
            PointerEvent::Scroll(PointerScrollEvent { state, delta, .. }) => {
                let dy = match delta {
                    ScrollDelta::LineDelta(_, y) | ScrollDelta::PageDelta(_, y) => *y as f64,
                    ScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                self.push_input(
                    TileMouseKind::Wheel,
                    ctx.local_position(state.position),
                    dy as f32,
                );
            }
            _ => {}
        }
    }

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

/// Samples a source texture into a sub-rect of the render target. Used to
/// composite an engine frame into its tile: the frame is `Bgra8Unorm` and the
/// target `Rgba8Unorm` (not copy-compatible, so `copy_texture_to_texture` can't
/// be used), but **sampling normalizes channel order**, so a render pass that
/// samples the frame and draws a viewport-filling triangle handles both the
/// positioning and the format. `LoadOp::Load` preserves Masonry's content
/// outside the tile rect.
pub struct BlitPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl BlitPipeline {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mere-blit-shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mere-blit-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mere-blit-pl"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mere-blit-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(target_format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mere-blit-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { pipeline, layout, sampler }
    }

    /// Draw `src` into `bounds` = `[x, y, w, h]` (physical px) of `target_view`.
    pub fn blit(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_view: &wgpu::TextureView,
        src: &wgpu::Texture,
        bounds: [u32; 4],
    ) {
        let [x, y, w, h] = bounds;
        if w == 0 || h == 0 {
            return;
        }
        let src_view = src.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mere-blit-bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("mere-blit") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mere-blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, w, h);
            pass.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
}

/// Fullscreen-triangle blit: sample the source into the viewport. Y flipped so
/// the texture's top-left maps to the viewport's top-left.
const BLIT_WGSL: &str = r#"
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var out: VsOut;
    out.pos = vec4(p[i], 0.0, 1.0);
    out.uv = vec2(p[i].x * 0.5 + 0.5, p[i].y * -0.5 + 0.5);
    return out;
}
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
"#;

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
    /// Producer's current physical size; re-`resize` only when the tile bounds
    /// change, so the WebView renders 1:1 in the tile (crisp + 1:1 input coords).
    #[cfg(target_os = "windows")]
    sized_to: Option<(u32, u32)>,
    /// Shared state for the web tile (see [`SurfaceChannel`]).
    channel: SurfaceChannel,
}

/// Web-tile state shared across the three main-thread sites that touch it: the
/// `SurfaceTileWidget` (writes `input`), the compositor (writes `bounds`, reads
/// `frame`), and the on_tick host (reads `bounds`/`input`, writes `frame`).
/// All run on the main thread, so the lock is uncontended.
#[derive(Default)]
pub struct SurfaceShared {
    /// Latest imported frame texture (host writes on import; compositor blits).
    /// `Some` also serves as the `resource_is_new` cache marker.
    pub frame: Option<wgpu::Texture>,
    /// The tile's current rect in physical px (compositor writes from the
    /// external layer bounds); the host sizes the producer to match.
    pub bounds: Option<[u32; 4]>,
    /// Pointer events captured by the widget (tile-local logical px), drained by
    /// the host and forwarded to the producer.
    pub input: Vec<TileMouse>,
}

/// Shared [`SurfaceShared`], `Arc`-cloned to the widget, compositor, and host.
pub type SurfaceChannel = Arc<Mutex<SurfaceShared>>;

/// A pointer event from the tile widget, in tile-local logical px. The host
/// scales to physical and maps to scrying's `MouseInput`.
#[derive(Clone, Copy, Debug)]
pub struct TileMouse {
    pub kind: TileMouseKind,
    pub x: f32,
    pub y: f32,
    /// Vertical wheel notches for [`TileMouseKind::Wheel`]; 0 otherwise.
    pub delta: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileMouseKind {
    Move,
    LeftDown,
    LeftUp,
    Wheel,
}

impl WebSurfaceHost {
    pub fn new(user_data_dir: std::path::PathBuf, channel: SurfaceChannel) -> Self {
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
            sized_to: None,
            channel,
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
        scale: f64,
    ) {
        #[cfg(target_os = "windows")]
        {
            if self.current_url.as_deref() != Some(url) {
                self.build(hwnd, url, size);
            }
            if let Some(producer) = self.producer.as_mut() {
                while let Some(event) = producer.poll_navigation_event() {
                    eprintln!("[web] nav: {event:?}");
                }
            }
            self.resize_to_bounds();
            self.forward_input(scale);
            self.pump_frame(device, queue);
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (hwnd, url, size, device, queue, scale);
        }
    }

    /// Build + navigate the producer (once per URL), suppressing its native
    /// overlay. Sets `sized_to` to the construct size.
    #[cfg(target_os = "windows")]
    fn build(&mut self, hwnd: *mut std::ffi::c_void, url: &str, size: (u32, u32)) {
        // WebView2's WinComp `Compositor` requires a DispatcherQueue on this
        // (UI/event-loop) thread. Create + hold it once before the first build.
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
            Ok(mut producer) => {
                eprintln!("[web] producer built ({}x{}) for {url}", size.0, size.1);
                if let Err(err) = producer.load_url(url) {
                    eprintln!("[web] load_url failed: {err}");
                }
                // Suppress the native composition overlay: move the visual
                // off-screen. WGC captures via `CreateFromVisual` (the visual's
                // content, position-independent), so the frame still arrives —
                // we composite it into the tile ourselves.
                if let Err(err) = producer.set_offset(-32000.0, -32000.0) {
                    eprintln!("[web] set_offset failed: {err}");
                }
                self.producer = Some(producer);
                self.current_url = Some(url.to_string());
                self.sized_to = Some((size.0.max(1), size.1.max(1)));
            }
            Err(err) => eprintln!("[web] producer build failed: {err}"),
        }
    }

    /// Resize the producer to the tile's physical bounds when they change, so
    /// the WebView renders 1:1 in the tile (crisp + 1:1 input coordinates).
    #[cfg(target_os = "windows")]
    fn resize_to_bounds(&mut self) {
        let Some([_, _, w, h]) = self.channel.lock().ok().and_then(|c| c.bounds) else {
            return;
        };
        let target = (w.max(1), h.max(1));
        if self.sized_to == Some(target) {
            return;
        }
        if let Some(producer) = self.producer.as_mut() {
            match producer.resize(dpi::PhysicalSize::new(target.0, target.1)) {
                Ok(()) => self.sized_to = Some(target),
                Err(err) => eprintln!("[web] resize failed: {err}"),
            }
        }
    }

    /// Drain queued widget pointer events and forward them to the producer.
    /// Tile-local logical coords × `scale` → physical px relative to the
    /// WebView's top-left (which, since we size the producer to the tile, is the
    /// tile's top-left).
    #[cfg(target_os = "windows")]
    fn forward_input(&mut self, scale: f64) {
        use scrying::{FocusReason, MouseEventKind, MouseInput, MouseVirtualKeys};
        let events: Vec<TileMouse> = self
            .channel
            .lock()
            .map(|mut c| std::mem::take(&mut c.input))
            .unwrap_or_default();
        let Some(producer) = self.producer.as_mut() else {
            return;
        };
        for ev in events {
            let point = ((ev.x as f64 * scale) as i32, (ev.y as f64 * scale) as i32);
            let kind = match ev.kind {
                TileMouseKind::Move => MouseEventKind::Move,
                TileMouseKind::LeftDown => MouseEventKind::LeftButtonDown,
                TileMouseKind::LeftUp => MouseEventKind::LeftButtonUp,
                TileMouseKind::Wheel => MouseEventKind::Wheel,
            };
            // Take keyboard focus when the user presses inside the tile.
            if ev.kind == TileMouseKind::LeftDown {
                let _ = producer.move_focus(FocusReason::Programmatic);
            }
            let virtual_keys = MouseVirtualKeys {
                left_button: ev.kind == TileMouseKind::LeftDown,
                ..Default::default()
            };
            // WebView2 wheel delta is in WHEEL_DELTA units (120 per notch).
            let mouse_data = if ev.kind == TileMouseKind::Wheel {
                (ev.delta * 120.0) as i32
            } else {
                0
            };
            if let Err(err) = producer.send_mouse_input(MouseInput {
                kind,
                virtual_keys,
                mouse_data,
                point,
            }) {
                eprintln!("[web] send_mouse_input failed: {err}");
            }
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
        let have_cached = self
            .channel
            .lock()
            .map(|c| c.frame.is_some())
            .unwrap_or(false);
        if !frame.resource_is_new && have_cached {
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
                if let Ok(mut cell) = self.channel.lock() {
                    cell.frame = Some(imported.texture);
                }
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
/// realize it, and queues pointer events into `channel` for the host.
pub fn surface_tile<State>(
    registry: SurfaceRegistry,
    content: SurfaceContent,
    channel: SurfaceChannel,
) -> SurfaceTile<State>
where
    State: 'static,
{
    SurfaceTile { registry, content, channel, phantom: PhantomData }
}

/// The [`View`] created by [`surface_tile`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SurfaceTile<State> {
    registry: SurfaceRegistry,
    content: SurfaceContent,
    channel: SurfaceChannel,
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
                self.channel.clone(),
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
