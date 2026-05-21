// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `MasonryEmbeddedRenderer` — `EmbeddedFrameRenderer` impl that renders
//! each Masonry tile directly into a wgpu texture, returned to the
//! substrate host for compositing via
//! [`spatial_substrate::ExternalTextureCompositor`].
//!
//! ## Pipeline
//!
//! 1. `ensure_producer` allocates a wgpu 29 `Texture` (Rgba8Unorm,
//!    RENDER_ATTACHMENT + TEXTURE_BINDING) sized to the node, plus a
//!    matching `TextureView`. Constructs a `MasonryTile` for the panel's
//!    widget tree.
//! 2. `next_frame` drives `MasonryTile::render` to refresh the
//!    `VisualLayerPlan`, extracts base + overlay scenes into
//!    `masonry_imaging::PreparedFrame`, then calls
//!    `masonry_imaging::texture_render::Renderer::render_to_texture` —
//!    which uses xilem's `imaging_vello::VelloRenderer` (vello 0.9
//!    internally) to paint masonry's content into the producer's wgpu
//!    texture on the GPU. No CPU round-trip.
//! 3. The host registers the returned `wgpu::Texture` with its own
//!    `vello::Renderer::register_texture` and composites via
//!    `Scene::draw_image` at the node's placement transform.
//!
//! ## Why this works now (and didn't initially)
//!
//! Earlier in development, xilem's workspace pinned wgpu 28 + vello 0.8
//! while mere's workspace pins wgpu 29 + vello 0.9. Those versions are
//! different crates from Rust's point of view; a `wgpu::Texture` from
//! one can't cross to the other. The interim implementation used
//! `imaging_vello_cpu` (CPU rasterizer; vello_cpu doesn't touch wgpu)
//! + `queue.write_texture` to bridge the gap.
//!
//! With the imaging fork at `repos/imaging` (branch
//! `mere-wgpu-29-vello-0-9`) and xilem's pins bumped to match mere's,
//! both stacks live in the same wgpu 29 + vello 0.9 universe. The CPU
//! detour is no longer needed; the GPU path is the obvious choice.

use std::collections::HashMap;
use std::sync::Arc;

use masonry_core::app::VisualLayerKind;
use masonry_core::core::{DefaultProperties, NewWidget, Widget};
use masonry_imaging::texture_render::{RenderTarget, Renderer as TextureRenderer};
use masonry_imaging::{Layer, PreparedFrame};
use register_renderer::{
    CompositionMode, EmbeddedFrameRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeRenderer, ProducerHandle, ProfileBindingExpectation,
    RendererCapabilities, RendererId, SceneNodeRef,
};
use vello::peniko::Color;

use crate::input::{translate_to_masonry_pointer, translate_to_masonry_text};
use crate::panel_tile::PanelTile;
use crate::tile::{MasonryTile, TileSize};

/// Factory the renderer invokes to construct each tile's root widget.
/// Cheap to clone (Arc); typically a `move ||` closure that builds a
/// fresh widget tree per producer.
pub type RootWidgetFactory = Arc<dyn Fn() -> NewWidget<dyn Widget> + Send + Sync>;

/// Factory the renderer invokes to construct each producer's panel.
/// Receives the node being produced (so the host can route content by
/// pane role / identity), plus its physical size + scale factor, and
/// returns a boxed [`PanelTile`] — a plain [`MasonryTile`] or a
/// reactive [`crate::XilemPanel`]. Cheap to clone (Arc).
pub type PanelFactory =
    Arc<dyn Fn(&SceneNodeRef, TileSize, f64) -> Box<dyn PanelTile> + Send + Sync>;

/// One masonry-rendered embedded-frame producer per node.
pub struct MasonryEmbeddedRenderer {
    id: RendererId,
    handles_kind: NodeContentKind,

    // Host-supplied GPU resources. Clone-cheap (wgpu handle types).
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,

    // Shared GPU renderer. `texture_render::Renderer::new` is a tiny
    // stateless wrapper; it caches the underlying VelloRenderer keyed
    // by device on first render and reuses it after.
    texture_renderer: TextureRenderer,

    // Builds each producer's panel (plain or reactive).
    panel_factory: PanelFactory,

    // Per-producer state.
    producers: HashMap<ProducerHandle, ProducerState>,
}

struct ProducerState {
    tile: Box<dyn PanelTile>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Physical size of the texture.
    size_px: (u32, u32),
}

impl MasonryEmbeddedRenderer {
    /// Construct the renderer.
    ///
    /// `id` is the renderer-registry identifier (e.g.
    /// `"mere-masonry.panel"`). `handles_kind` is the `NodeContentKind`
    /// this renderer claims — typically `NodeContentKind::Panel` for
    /// mere-domain panel content.
    ///
    /// `adapter` + `device` + `queue` are the host's wgpu 29 handles;
    /// the renderer clones them per allocation (wgpu's Clone is an Arc
    /// clone — cheap).
    /// Construct from a [`PanelFactory`] — the general path. The
    /// factory builds each producer's panel (plain [`MasonryTile`] or
    /// reactive [`crate::XilemPanel`]) given its size + scale.
    pub fn new(
        id: &'static str,
        handles_kind: NodeContentKind,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        panel_factory: PanelFactory,
    ) -> Self {
        Self {
            id: RendererId::from_static(id),
            handles_kind,
            adapter,
            device,
            queue,
            texture_renderer: TextureRenderer::new(),
            panel_factory,
            producers: HashMap::new(),
        }
    }

    /// Convenience constructor for plain (non-reactive) masonry tiles:
    /// wraps a [`RootWidgetFactory`] (build a `NewWidget` per producer)
    /// into a [`PanelFactory`] that produces [`MasonryTile`]s.
    pub fn with_widget_factory(
        id: &'static str,
        handles_kind: NodeContentKind,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        default_properties: Arc<DefaultProperties>,
        root_widget_factory: RootWidgetFactory,
    ) -> Self {
        let panel_factory: PanelFactory = Arc::new(move |_node, size, scale| {
            let widget = (root_widget_factory)();
            Box::new(MasonryTile::new(
                default_properties.clone(),
                widget,
                size,
                scale,
            )) as Box<dyn PanelTile>
        });
        Self::new(id, handles_kind, adapter, device, queue, panel_factory)
    }

    /// True if the renderer has a live producer for `handle`. Test hook.
    pub fn has_producer(&self, handle: ProducerHandle) -> bool {
        self.producers.contains_key(&handle)
    }

    /// Number of live producers. Test hook.
    pub fn producer_count(&self) -> usize {
        self.producers.len()
    }

    fn allocate_texture_and_view(
        &self,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
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
            // `imaging_vello`'s GPU renderer writes.
            format: wgpu::TextureFormat::Rgba8Unorm,
            // STORAGE_BINDING: vello's compute pipeline writes here.
            // TEXTURE_BINDING: host's vello samples this when compositing
            // via `register_texture` + `Scene::draw_image`.
            // RENDER_ATTACHMENT: required by some wgpu backends as a
            // companion to STORAGE_BINDING for color-format textures.
            // COPY_SRC + COPY_DST: masonry_imaging's
            // `texture_render::Renderer` composites layers through a
            // copy inside its `render_to_texture` command encoder; wgpu
            // validates COPY_SRC is present (observed on Vulkan). Both
            // copy flags are included so the layer-composite path
            // type-checks regardless of copy direction.
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Drive masonry's render pass, paint into the producer's wgpu
    /// texture via xilem's `texture_render::Renderer`. Returns `None`
    /// if any step fails or the producer is missing.
    fn fill_texture(&mut self, handle: ProducerHandle) -> Option<()> {
        let (width_px, height_px) = self.producers.get(&handle)?.size_px;
        if width_px == 0 || height_px == 0 {
            return None;
        }

        // 1. Reactive update (if any) + drive masonry to refresh its
        //    VisualLayerPlan. `tick` re-runs a reactive panel's logic
        //    against current state and rebuilds; plain tiles no-op.
        //    `render_layers` runs masonry's pass chain and stashes the
        //    layer plan the texture path below extracts.
        {
            let state = self.producers.get_mut(&handle)?;
            state.tile.tick();
            state.tile.render_layers();
        }

        // 2. Build the PreparedFrame from the layer plan. The lifetime
        //    dance: base_scene + overlays borrow from
        //    `state.tile.pending_layers()`, which borrows from
        //    `self.producers`. `texture_renderer` + adapter/device/queue
        //    are different fields of self so split-borrow allows both.
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

        let prepared = PreparedFrame::new(
            width_px,
            height_px,
            1.0,
            Color::TRANSPARENT,
            base_scene,
            &overlays,
        );

        // 3. GPU-render masonry's content into the producer's texture.
        let target = RenderTarget {
            adapter: &self.adapter,
            device: &self.device,
            queue: &self.queue,
            texture: &state.texture,
            view: &state.view,
        };
        self.texture_renderer
            .render_to_texture(target, prepared)
            .ok()?;
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
        let (texture, view) = self.allocate_texture_and_view(width, height);

        let tile = (self.panel_factory)(node, TileSize(dpi::PhysicalSize::new(width, height)), 1.0);

        let handle = ProducerHandle::next();
        self.producers.insert(
            handle,
            ProducerState {
                tile,
                texture,
                view,
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

    fn take_accesskit_subtree(&mut self, handle: ProducerHandle) -> Option<accesskit::TreeUpdate> {
        // MasonryTile produces a TreeUpdate whenever the widget tree
        // produces one (typically the first render and on widget-tree
        // mutations). The substrate's `collect_accesskit_updates`
        // rewrites the `tree_id` field to the substrate's minted
        // per-producer subtree id before forwarding to the host's
        // AccessKit adapter — masonry's update arrives here with
        // `tree_id == TreeId::ROOT`.
        let state = self.producers.get_mut(&handle)?;
        state.tile.take_accesskit_update()
    }
}
