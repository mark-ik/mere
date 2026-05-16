// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `MasonryEmbeddedRenderer` — `EmbeddedFrameRenderer` impl that renders
//! each Masonry tile to an offscreen wgpu texture for the substrate host
//! to composite via [`mere_spatial_prototype::ExternalTextureCompositor`].
//!
//! ## Why CPU readback (and why not GPU)
//!
//! The texture-fill path goes through `masonry_imaging`'s
//! `imaging_vello_cpu` backend — a CPU rasterizer (vello_cpu, distinct
//! from vello) that produces an RGBA8 `imaging::RgbaImage` in regular
//! memory. mere-masonry then uploads that buffer to a wgpu 29 texture
//! via `queue.write_texture`.
//!
//! The CPU path was picked over the GPU path (`imaging_vello`'s
//! `texture_render::Renderer`) because of the wgpu version skew: xilem's
//! `imaging_wgpu` pins wgpu 28, mere's workspace pins wgpu 29. Those are
//! different crates from Rust's point of view; their `Texture` and
//! `Device` types don't interoperate. CPU readback sidesteps the skew
//! entirely — the bridge between the two wgpu universes is a `Vec<u8>`.
//!
//! Performance: CPU rasterization is materially slower than GPU for
//! large surfaces. Panel-shaped tiles (text + simple shapes, a few
//! hundred pixels on a side) rasterize fast enough on modern CPUs that
//! this is fine for the prototype. When xilem upstream bumps to vello 0.9
//! + wgpu 29, this renderer can switch to the GPU path with
//! `texture_render` and the version skew dissolves; the substrate-host
//! API doesn't change.
//!
//! ## v0a scope
//!
//! Wired end-to-end:
//!
//! - GPU-resource ownership (wgpu 29 Device + Queue).
//! - CPU renderer ownership; recreated when the active producer's size
//!   changes (mask cache and internal context reset accordingly).
//! - `ensure_producer`: allocate wgpu 29 texture sized to node + construct
//!   `MasonryTile` with the host-supplied widget tree.
//! - `next_frame`: drive masonry's redraw pass → `VisualLayerPlan` →
//!   `PreparedFrame` → paint_into CPU renderer → `RgbaImage` →
//!   `queue.write_texture` → return wgpu::Texture.
//! - `deliver_input`: full input translation via [`crate::input`].
//! - `release`: clean teardown.
//!
//! Not wired yet:
//!
//! - **Resize**: the producer's texture is sized at `ensure_producer` time
//!   and not re-allocated if the node's `size` changes mid-session. Hosts
//!   currently must release+re-ensure when a panel resizes. A v0b
//!   `resize(handle, size)` method on the trait would let hosts keep the
//!   widget tree alive across resizes.
//! - **Damage tracking / partial updates**: every frame re-renders the
//!   whole tile from scratch. Fine for panels that update infrequently;
//!   needs revisiting for high-FPS panels.

use std::collections::HashMap;
use std::sync::Arc;

use masonry_core::app::VisualLayerKind;
use masonry_core::core::{DefaultProperties, NewWidget, Widget};
use masonry_core::imaging::{self, render::RenderSource};
use masonry_imaging::image_render::Renderer as CpuImageRenderer;
use masonry_imaging::{Layer, PreparedFrame};
use mere_renderer_registry::{
    CompositionMode, EmbeddedFrameRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeRenderer, ProducerHandle, ProfileBindingExpectation,
    RendererCapabilities, RendererId, SceneNodeRef,
};
use vello::peniko::Color;

use crate::input::{translate_to_masonry_pointer, translate_to_masonry_text};
use crate::tile::{MasonryTile, TileSize};

/// Factory the renderer invokes to construct each tile's root widget.
/// Cheap to clone (Arc); typically a `move ||` closure that builds a
/// fresh widget tree per producer.
pub type RootWidgetFactory = Arc<dyn Fn() -> NewWidget<dyn Widget> + Send + Sync>;

/// One masonry-rendered embedded-frame producer per node.
pub struct MasonryEmbeddedRenderer {
    id: RendererId,
    handles_kind: NodeContentKind,

    // Host-supplied GPU resources. Clone-cheap (wgpu handle types).
    device: wgpu::Device,
    queue: wgpu::Queue,

    // CPU renderer + the size it's configured for. Replaced when a
    // render at a different size is requested. VelloCpuRenderer::new is
    // cheap (no GPU init); the trade-off vs. resizing is the mask cache
    // resets, which is what `resize` does internally anyway.
    cpu_renderer: CpuImageRenderer,
    cpu_renderer_size: (u16, u16),

    // Shared widget construction context.
    default_properties: Arc<DefaultProperties>,
    root_widget_factory: RootWidgetFactory,

    // Per-producer state.
    producers: HashMap<ProducerHandle, ProducerState>,
}

struct ProducerState {
    tile: MasonryTile,
    texture: wgpu::Texture,
    /// Physical size of the texture — used by the upload step.
    size_px: (u32, u32),
}

impl MasonryEmbeddedRenderer {
    /// Construct the renderer.
    ///
    /// `id` is the renderer-registry identifier (e.g. `"mere-masonry.panel"`).
    /// `handles_kind` is the `NodeContentKind` this renderer claims —
    /// typically `NodeContentKind::Panel` for mere-domain panel content.
    ///
    /// `device` + `queue` are the host's wgpu 29 handles; the renderer
    /// clones them per allocation (wgpu's Clone is an Arc clone — cheap).
    pub fn new(
        id: &'static str,
        handles_kind: NodeContentKind,
        device: wgpu::Device,
        queue: wgpu::Queue,
        default_properties: Arc<DefaultProperties>,
        root_widget_factory: RootWidgetFactory,
    ) -> Self {
        // Construct at 1×1; first render at a real producer size will
        // replace this with one at the right dimensions.
        let cpu_renderer = CpuImageRenderer::new(1, 1);
        Self {
            id: RendererId::from_static(id),
            handles_kind,
            device,
            queue,
            cpu_renderer,
            cpu_renderer_size: (1, 1),
            default_properties,
            root_widget_factory,
            producers: HashMap::new(),
        }
    }

    /// True if the renderer has a live producer for `handle`. Test hook.
    pub fn has_producer(&self, handle: ProducerHandle) -> bool {
        self.producers.contains_key(&handle)
    }

    /// Number of live producers. Test hook.
    pub fn producer_count(&self) -> usize {
        self.producers.len()
    }

    fn allocate_texture(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mere-masonry embedded-frame target"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Rgba8Unorm matches vello's register_texture expectation
            // (peniko::ImageFormat::Rgba8) and is what
            // `imaging_vello_cpu`'s RgbaImage produces (unpremultiplied
            // RGBA8 in row-major order).
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    /// Make sure `self.cpu_renderer` is sized for the given dimensions.
    /// Replaces the renderer if not (cheap; no GPU work).
    fn ensure_cpu_renderer_size(&mut self, width: u16, height: u16) {
        if self.cpu_renderer_size != (width, height) {
            self.cpu_renderer = CpuImageRenderer::new(width, height);
            self.cpu_renderer_size = (width, height);
        }
    }

    /// Drive masonry's render pass, rasterize to CPU memory, upload to
    /// the producer's wgpu texture. Returns `None` if any step fails or
    /// the producer is missing.
    fn fill_texture(&mut self, handle: ProducerHandle) -> Option<()> {
        // Split borrow: get the producer's data we need out, then call
        // self.ensure_cpu_renderer_size / self.queue.write_texture
        // separately.
        let (width_px, height_px) = self.producers.get(&handle)?.size_px;
        if width_px == 0 || height_px == 0 {
            return None;
        }
        let width_u16 = u16::try_from(width_px).ok()?;
        let height_u16 = u16::try_from(height_px).ok()?;

        // 1. Drive masonry to refresh its VisualLayerPlan. The `ignore`
        //    scene is a vello 0.9 Scene that MasonryTile::render writes
        //    to per its signature; the scene-merge TODO inside that call
        //    means the scene stays empty — fine, we don't use it here.
        {
            let state = self.producers.get_mut(&handle)?;
            let mut ignore = vello::Scene::new();
            state.tile.render(&mut ignore, kurbo::Affine::IDENTITY);
        }

        // 2. Resize CPU renderer if needed (does nothing if size matches).
        self.ensure_cpu_renderer_size(width_u16, height_u16);

        // 3. Build the PreparedFrame and rasterize. The lifetime dance:
        //    base_scene + overlays borrow from state.tile.pending_layers(),
        //    which borrows from self.producers; cpu_renderer is a
        //    different field of self so split-borrow allows both.
        let state = self.producers.get_mut(&handle)?;
        let plan = state.tile.pending_layers()?;
        let root = plan.root_layer()?;
        let base_scene = match &root.kind {
            VisualLayerKind::Scene(s) => s,
            VisualLayerKind::External { .. } => return None,
        };
        let overlays: Vec<Layer<'_>> = plan
            .overlay_layers()
            .filter_map(|layer| match &layer.kind {
                VisualLayerKind::Scene(s) => Some(Layer {
                    scene: s,
                    transform: layer.transform,
                }),
                VisualLayerKind::External { .. } => None,
            })
            .collect();

        let mut prepared = PreparedFrame::new(
            width_px,
            height_px,
            1.0,
            Color::TRANSPARENT,
            base_scene,
            &overlays,
        );

        let mut image = imaging::RgbaImage::new(width_px, height_px);
        prepared.paint_into(&mut self.cpu_renderer);
        self.cpu_renderer.finish_into(&mut image).ok()?;

        // 4. Upload to wgpu 29 texture.
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &state.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width_px * 4),
                rows_per_image: Some(height_px),
            },
            wgpu::Extent3d {
                width: width_px,
                height: height_px,
                depth_or_array_layers: 1,
            },
        );
        Some(())
    }
}

impl NodeRenderer for MasonryEmbeddedRenderer {
    fn renderer_id(&self) -> RendererId {
        self.id.clone()
    }

    fn handles(&self) -> NodeContentKindSet {
        NodeContentKindSet::from_one(self.handles_kind)
    }

    fn composition_mode(&self) -> CompositionMode {
        CompositionMode::EmbeddedFrame
    }

    fn capabilities(&self) -> RendererCapabilities {
        RendererCapabilities {
            accepts_input: true,
            handles_ime: true,
            handles_a11y: true,
            scrollable: true,
            hit_testable_subregions: true,
            profile_binding: ProfileBindingExpectation::None,
            supports_capture: true,
        }
    }

    fn as_embedded_frame(&mut self) -> Option<&mut dyn EmbeddedFrameRenderer> {
        Some(self)
    }
}

impl EmbeddedFrameRenderer for MasonryEmbeddedRenderer {
    fn ensure_producer(&mut self, node: &SceneNodeRef) -> ProducerHandle {
        let width = (node.size.width.round() as i64).clamp(1, u32::MAX as i64) as u32;
        let height = (node.size.height.round() as i64).clamp(1, u32::MAX as i64) as u32;
        let texture = self.allocate_texture(width, height);

        let root = (self.root_widget_factory)();
        let tile = MasonryTile::new(
            self.default_properties.clone(),
            root,
            TileSize(dpi::PhysicalSize::new(width, height)),
            1.0,
        );

        let handle = ProducerHandle::next();
        self.producers.insert(
            handle,
            ProducerState {
                tile,
                texture,
                size_px: (width, height),
            },
        );
        handle
    }

    fn next_frame(&mut self, handle: ProducerHandle) -> Option<wgpu::Texture> {
        self.fill_texture(handle)?;
        let state = self.producers.get(&handle)?;
        Some(state.texture.clone())
    }

    fn deliver_input(&mut self, handle: ProducerHandle, event: &InputEvent) -> InputDisposition {
        let Some(state) = self.producers.get_mut(&handle) else {
            return InputDisposition::Passthrough;
        };
        if let Some(pointer) = translate_to_masonry_pointer(event) {
            state.tile.handle_pointer(pointer);
            return InputDisposition::Consumed;
        }
        if let Some(text) = translate_to_masonry_text(event) {
            state.tile.handle_text(text);
            return InputDisposition::Consumed;
        }
        InputDisposition::Passthrough
    }

    fn release(&mut self, handle: ProducerHandle) {
        self.producers.remove(&handle);
    }
}
