// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `scrying-embedded-renderer` — `EmbeddedFrameRenderer` for the
//! renderer-registry contract, backed by `scrying`'s system-WebView
//! producers (WebView2 on Windows, WKWebView on macOS, WPE/WebKitGTK
//! on Linux).
//!
//! Per the spatial-chrome modular adoption plan §Phase 4, this crate
//! is the consolidation point where Mere's substrate composes web
//! content. Pipeline:
//!
//! 1. Host registers a `ScryingEmbeddedRenderer` against
//!    `NodeContentKind::WebPage` (or another configured kind).
//! 2. `ensure_producer` invokes the host-supplied
//!    [`ProducerFactory`] to construct a concrete
//!    `scrying::WebSurfaceProducer` for the node (platform-specific
//!    setup: HWND, NSView, GTK widget, etc. — the host owns those).
//! 3. `next_frame` calls `WebSurfaceProducer::acquire_frame`. If the
//!    result is `WebSurfaceFrame::Native(_)`, the renderer's shared
//!    `WgpuTextureImporter` imports the platform-native frame
//!    (DX12 shared texture / Metal IOSurface / dmabuf) into a
//!    `wgpu::Texture` on the host's wgpu 29 device. The renderer
//!    caches that texture and returns it.
//! 4. Non-native frame variants (`CpuRgba`, `PngSnapshot`,
//!    `OverlayOnly`) cause `next_frame` to return the last cached
//!    texture (or `None` if no texture has ever been produced).
//!    Overlay-only producers truly can't produce textures via this
//!    path and are best served by a separate `OverlayRenderer`.
//!
//! ## Why no built-in input wiring (yet)
//!
//! `WebSurfaceProducer` accepts scrying-shaped `MouseEvent` /
//! `PointerEvent` / `KeyboardEvent` types (distinct from
//! `register_renderer::InputEvent`). A faithful translation is
//! ~150 LOC of mechanical mapping similar to
//! `mere-masonry/src/input.rs` — out of scope for v0a. The
//! [`EmbeddedFrameRenderer::deliver_input`] impl currently returns
//! `Passthrough` for every event; hosts wanting real keyboard /
//! mouse routing into web content should drive the producer directly
//! via `ScryingEmbeddedRenderer::producer_mut`.

#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

use std::collections::HashMap;
use std::sync::Arc;

use register_renderer::{
    CompositionMode, EmbeddedFrameRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeRenderer, ProducerHandle, ProfileBindingExpectation,
    RendererCapabilities, RendererId, SceneNodeRef,
};
use scrying::native_frame::{ImportOptions, TextureImporter, WgpuTextureImporter};
use scrying::{WebSurfaceFrame, WebSurfaceProducer};

/// Host-side hook the renderer invokes to construct a fresh scrying
/// producer per node. The closure receives the `SceneNodeRef` so the
/// host can extract size / placement / identity-tagged URLs / etc.
///
/// Cheap to clone (Arc); panic-on-failure is the v0a contract — the
/// trait's `ensure_producer` is infallible, so factories that can't
/// build a producer should return one that emits
/// `WebSurfaceFrame::OverlayOnly` from `acquire_frame` (effectively
/// a degraded stub) rather than panic.
pub type ProducerFactory = Arc<dyn Fn(&SceneNodeRef) -> Box<dyn WebSurfaceProducer> + Send + Sync>;

/// EmbeddedFrameRenderer backed by scrying.
pub struct ScryingEmbeddedRenderer {
    id: RendererId,
    handles_kind: NodeContentKind,
    importer: WgpuTextureImporter,
    factory: ProducerFactory,
    producers: HashMap<ProducerHandle, ProducerState>,
}

struct ProducerState {
    producer: Box<dyn WebSurfaceProducer>,
    /// Last successfully-imported wgpu texture. The substrate
    /// re-composites this until the producer publishes a new frame —
    /// gives "frozen frame while waiting" behavior instead of
    /// flicker-to-empty.
    last_texture: Option<wgpu::Texture>,
}

impl ScryingEmbeddedRenderer {
    /// Construct the renderer.
    ///
    /// `importer` is the per-device `WgpuTextureImporter` — share one
    /// across renderers/producers on the same wgpu device. Construct
    /// via `WgpuTextureImporter::new(HostWgpuContext::new(device,
    /// queue))`.
    ///
    /// `factory` is the host-side hook that builds a concrete scrying
    /// producer for each node. Typically wraps platform handle
    /// resolution (HWND / NSView / GTK widget) + producer
    /// instantiation.
    pub fn new(
        id: &'static str,
        handles_kind: NodeContentKind,
        importer: WgpuTextureImporter,
        factory: ProducerFactory,
    ) -> Self {
        Self {
            id: RendererId::from_static(id),
            handles_kind,
            importer,
            factory,
            producers: HashMap::new(),
        }
    }

    /// True if a producer exists for `handle`. Test hook.
    pub fn has_producer(&self, handle: ProducerHandle) -> bool {
        self.producers.contains_key(&handle)
    }

    /// Number of live producers. Test hook.
    pub fn producer_count(&self) -> usize {
        self.producers.len()
    }

    /// Borrow a producer's underlying scrying producer for direct
    /// access (navigation, input forwarding, settings). Returns
    /// `None` if `handle` isn't a registered producer.
    ///
    /// This is the v0a escape hatch for input routing: hosts that
    /// want real mouse/keyboard delivery into web content call this
    /// to reach the scrying-shaped input methods on `WebSurfaceProducer`.
    pub fn producer_mut(
        &mut self,
        handle: ProducerHandle,
    ) -> Option<&mut (dyn WebSurfaceProducer + 'static)> {
        let state = self.producers.get_mut(&handle)?;
        Some(state.producer.as_mut())
    }
}

impl NodeRenderer for ScryingEmbeddedRenderer {
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
            accepts_input: false, // not yet wired — see crate-level docs
            handles_ime: false,
            handles_a11y: false,
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

impl EmbeddedFrameRenderer for ScryingEmbeddedRenderer {
    fn ensure_producer(&mut self, node: &SceneNodeRef) -> ProducerHandle {
        let producer = (self.factory)(node);
        let handle = ProducerHandle::next();
        self.producers.insert(
            handle,
            ProducerState {
                producer,
                last_texture: None,
            },
        );
        handle
    }

    fn next_frame(&mut self, handle: ProducerHandle) -> Option<wgpu::Texture> {
        let state = self.producers.get_mut(&handle)?;
        match state.producer.acquire_frame() {
            Ok(WebSurfaceFrame::Native(native)) => {
                match self
                    .importer
                    .import_frame(&native, &ImportOptions::default())
                {
                    Ok(imported) => {
                        state.last_texture = Some(imported.texture.clone());
                        Some(imported.texture)
                    }
                    Err(_) => state.last_texture.clone(),
                }
            }
            Ok(_) | Err(_) => state.last_texture.clone(),
        }
    }

    fn deliver_input(&mut self, _handle: ProducerHandle, _event: &InputEvent) -> InputDisposition {
        // v0a: input translation to scrying's vocab not wired. Hosts
        // route input directly via `producer_mut(handle)` instead.
        InputDisposition::Passthrough
    }

    fn release(&mut self, handle: ProducerHandle) {
        self.producers.remove(&handle);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kurbo::Size;
    use register_renderer::{
        LodLevel, NodeContentKind, NodeIdentity, Placement, SceneNodeRef,
    };
    use scrying::native_frame::{
        CapabilityStatus, HostWgpuContext, NativeFrameKind, UnsupportedReason, WgpuTextureImporter,
    };
    use scrying::{
        SystemWebviewBackend, WebSurfaceCapabilities, WebSurfaceError, WebSurfaceFrame,
        WebSurfaceMode, WebSurfaceProducer,
    };

    use super::*;

    /// Minimal producer stub — returns OverlayOnly + Unsupported. Used
    /// to drive the lifecycle methods of `ScryingEmbeddedRenderer`
    /// without standing up real WebView2/WKWebView/WebKitGTK platform
    /// state. Mirrors scrying-tile-engine's OverlayStub.
    struct OverlayStub;

    impl WebSurfaceProducer for OverlayStub {
        fn capabilities(&self) -> WebSurfaceCapabilities {
            WebSurfaceCapabilities {
                backend: SystemWebviewBackend::Unknown,
                preferred_mode: WebSurfaceMode::NativeChildOverlay,
                imported_texture: CapabilityStatus::Unsupported(
                    UnsupportedReason::PlatformNotImplemented,
                ),
                native_child_overlay: CapabilityStatus::Supported,
                cpu_snapshot: CapabilityStatus::Unsupported(
                    UnsupportedReason::PlatformNotImplemented,
                ),
                supported_frames: vec![NativeFrameKind::Dx12SharedTexture],
                reason: "test stub",
            }
        }
        fn acquire_frame(&mut self) -> Result<WebSurfaceFrame, WebSurfaceError> {
            Ok(WebSurfaceFrame::OverlayOnly)
        }
    }

    fn boot_wgpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .ok()?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .ok()?;
            Some((device, queue))
        })
    }

    fn test_node() -> SceneNodeRef {
        SceneNodeRef {
            identity: NodeIdentity::next(),
            placement: Placement::IDENTITY,
            lod: LodLevel::FullPane,
            size: Size::new(640.0, 480.0),
            content_kind: NodeContentKind::WebPage,
            renderer_pin: None,
        }
    }

    #[test]
    fn ensure_producer_creates_state_and_release_clears_it() {
        let Some((device, queue)) = boot_wgpu() else {
            eprintln!("skipping: no wgpu adapter");
            return;
        };
        let importer = WgpuTextureImporter::new(HostWgpuContext::new(device, queue));
        let factory: ProducerFactory = Arc::new(|_node| Box::new(OverlayStub));
        let mut renderer = ScryingEmbeddedRenderer::new(
            "test.scrying.web",
            NodeContentKind::WebPage,
            importer,
            factory,
        );

        let handle = renderer.ensure_producer(&test_node());
        assert!(renderer.has_producer(handle));
        assert_eq!(renderer.producer_count(), 1);

        renderer.release(handle);
        assert!(!renderer.has_producer(handle));
        assert_eq!(renderer.producer_count(), 0);
    }

    #[test]
    fn next_frame_with_overlay_only_returns_none() {
        let Some((device, queue)) = boot_wgpu() else {
            eprintln!("skipping: no wgpu adapter");
            return;
        };
        let importer = WgpuTextureImporter::new(HostWgpuContext::new(device, queue));
        let factory: ProducerFactory = Arc::new(|_node| Box::new(OverlayStub));
        let mut renderer = ScryingEmbeddedRenderer::new(
            "test.scrying.web",
            NodeContentKind::WebPage,
            importer,
            factory,
        );
        let handle = renderer.ensure_producer(&test_node());
        // OverlayStub returns OverlayOnly → no texture available.
        // Since this is the first frame, last_texture is None too.
        assert!(renderer.next_frame(handle).is_none());
    }

    #[test]
    fn node_renderer_surface_matches_embedded_frame_mode() {
        let Some((device, queue)) = boot_wgpu() else {
            eprintln!("skipping: no wgpu adapter");
            return;
        };
        let importer = WgpuTextureImporter::new(HostWgpuContext::new(device, queue));
        let factory: ProducerFactory = Arc::new(|_node| Box::new(OverlayStub));
        let mut renderer = ScryingEmbeddedRenderer::new(
            "test.scrying.web",
            NodeContentKind::WebPage,
            importer,
            factory,
        );
        assert_eq!(renderer.composition_mode(), CompositionMode::EmbeddedFrame);
        assert!(renderer.handles().contains(NodeContentKind::WebPage));
        assert!(renderer.as_embedded_frame().is_some());
    }
}
