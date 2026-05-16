// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `MasonryEmbeddedRenderer` — `EmbeddedFrameRenderer` impl that renders
//! each Masonry tile to an offscreen wgpu texture for the substrate host
//! to composite via [`mere_spatial_prototype::ExternalTextureCompositor`].
//!
//! ## Why EmbeddedFrame, and why the texture isn't filled yet
//!
//! EmbeddedFrame is the composition mode that *should* let masonry render
//! to a wgpu texture which the host then composites via
//! `vello::Renderer::register_texture`. The mode itself is right; the
//! implementation is blocked on the **wgpu version skew** between mere
//! (wgpu 29) and xilem (wgpu 28). See the crate-level "Two-vello + wgpu
//! version skew" docs in [`crate`](crate) — wiring
//! `masonry_imaging::texture_render::Renderer::render_to_texture` from
//! this renderer's `next_frame` will not compile until the skew resolves
//! (patch bump in xilem's local clone, or xilem upstream catching up,
//! are the two clean paths).
//!
//! ## v0a scope (works today, despite the skew)
//!
//! Wires the parts that don't cross the wgpu version boundary:
//!
//! - GPU-resource ownership (wgpu 29's `Device` + `Queue` — host-side).
//! - `ensure_producer`: allocate a wgpu 29 `Texture` sized to the node,
//!   construct a `MasonryTile` for the panel's widget tree.
//! - `deliver_input`: full input translation via [`crate::input`] (no
//!   wgpu crossing — input is pure data).
//! - `release`: clean teardown.
//! - `NodeRenderer` + `EmbeddedFrameRenderer` trait surfaces wired
//!   end-to-end so the substrate-host's dispatch path can call us today.
//!
//! ## What's deferred until the wgpu skew resolves
//!
//! `next_frame` drives masonry's redraw pass (the AccessKit tree update
//! drains correctly; `VisualLayerPlan` is stashed) but returns the
//! producer's wgpu 29 `Texture` *unfilled*. The host registers it with
//! vello, composites it, users see a transparent rectangle where the
//! masonry panel should be. The architecture is end-to-end exercised;
//! the pixels just don't arrive yet.
//!
//! When the skew is resolved, `next_frame` gains an
//! `masonry_imaging::texture_render::Renderer::render_to_texture` call
//! that fills the allocated texture from the `VisualLayerPlan`.

use std::collections::HashMap;
use std::sync::Arc;

use masonry_core::core::{DefaultProperties, NewWidget, Widget};
use mere_renderer_registry::{
    CompositionMode, EmbeddedFrameRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeRenderer, ProducerHandle, ProfileBindingExpectation,
    RendererCapabilities, RendererId, SceneNodeRef,
};

use crate::input::{translate_to_masonry_pointer, translate_to_masonry_text};
use crate::tile::{MasonryTile, TileSize};

/// Factory the renderer invokes to construct each tile's root widget.
/// Cheap to clone (Arc); typically a `move ||` closure that builds a
/// fresh widget tree per producer.
pub type RootWidgetFactory = Arc<dyn Fn() -> NewWidget<dyn Widget> + Send + Sync>;

/// One masonry-rendered embedded frame producer per node.
pub struct MasonryEmbeddedRenderer {
    id: RendererId,
    handles_kind: NodeContentKind,

    // Host-supplied GPU resources. Clone-cheap (wgpu handle types).
    device: wgpu::Device,
    // Reserved for the upcoming `texture_render::Renderer::render_to_texture`
    // call inside `next_frame` (see the TODO there). The field is held
    // now so the constructor signature is stable across the fill-step
    // landing.
    #[allow(dead_code)]
    queue: wgpu::Queue,

    // Shared widget construction context.
    default_properties: Arc<DefaultProperties>,
    root_widget_factory: RootWidgetFactory,

    // Per-producer state.
    producers: HashMap<ProducerHandle, ProducerState>,
}

struct ProducerState {
    tile: MasonryTile,
    texture: wgpu::Texture,
}

impl MasonryEmbeddedRenderer {
    /// Construct the renderer.
    ///
    /// `id` is the renderer-registry identifier (e.g. `"mere-masonry.panel"`).
    /// `handles_kind` is the `NodeContentKind` this renderer claims — typically
    /// `NodeContentKind::Panel` for mere-domain panel content.
    ///
    /// `device` + `queue` are the host's wgpu handles; the renderer clones
    /// them per allocation (wgpu's Clone is an Arc clone — cheap).
    pub fn new(
        id: &'static str,
        handles_kind: NodeContentKind,
        device: wgpu::Device,
        queue: wgpu::Queue,
        default_properties: Arc<DefaultProperties>,
        root_widget_factory: RootWidgetFactory,
    ) -> Self {
        Self {
            id: RendererId::from_static(id),
            handles_kind,
            device,
            queue,
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
            // Rgba8Unorm matches what vello's register_texture expects
            // (peniko::ImageFormat::Rgba8) and what masonry_imaging's
            // vello backend writes.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
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
        let width = node.size.width.round() as u32;
        let height = node.size.height.round() as u32;
        let texture = self.allocate_texture(width, height);

        let root = (self.root_widget_factory)();
        let tile = MasonryTile::new(
            self.default_properties.clone(),
            root,
            TileSize(dpi::PhysicalSize::new(width.max(1), height.max(1))),
            1.0,
        );

        let handle = ProducerHandle::next();
        self.producers
            .insert(handle, ProducerState { tile, texture });
        handle
    }

    fn next_frame(&mut self, handle: ProducerHandle) -> Option<wgpu::Texture> {
        let state = self.producers.get_mut(&handle)?;

        // Drive masonry's render pass — populates VisualLayerPlan +
        // AccessKit tree update. The `ignore_scene` here is the vello 0.9
        // Scene; we hand it to MasonryTile::render purely to satisfy the
        // signature. The scene-merge TODO in tile.rs is what would
        // actually paint into it (path A from the crate's "Two-vello
        // version skew" notes). EmbeddedFrame mode (this renderer)
        // sidesteps that by rendering to a wgpu texture instead.
        let mut ignore_scene = vello::Scene::new();
        state.tile.render(&mut ignore_scene, kurbo::Affine::IDENTITY);

        // TODO(masonry-texture-fill): the next step is wiring
        // `masonry_imaging::texture_render::Renderer::render_to_texture`
        // here — it takes a `RenderTarget { adapter, device, queue,
        // texture, view }` plus a `PreparedFrame` built from
        // `state.tile.pending_layers()`. Pending: store an
        // `Arc<wgpu::Adapter>` on the renderer (today's constructor
        // only takes Device + Queue), and write the VisualLayerPlan →
        // PreparedFrame conversion (extract first Scene-kind layer as
        // base; map remaining Scene-kind layers into
        // `masonry_imaging::Layer { scene, transform }`).
        //
        // Until the fill lands the texture stays at its initial state
        // (zeros — fully transparent under Rgba8Unorm). The host's
        // compositor still registers and composes it; users see a
        // transparent rectangle where the masonry panel should be.

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
