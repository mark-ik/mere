// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Cross-crate integration test: full substrate architecture
//! exercised end-to-end.
//!
//! Wires together:
//! - `spatial_substrate::SubstrateHost` (per-frame dispatch)
//! - `spatial_substrate::ExternalTextureCompositor` (vello
//!   register_texture cache + Scene::draw_image)
//! - `mere_renderer_registry::RendererRegistry` (renderer lookup)
//! - `mere_masonry::MasonryEmbeddedRenderer` (EmbeddedFrameRenderer)
//! - `vello::Renderer` (host-side scene renderer)
//! - Real wgpu 29 device
//!
//! Verifies the registered renderer's texture flows through the entire
//! chain: substrate-host dispatches → registry resolves → renderer
//! produces a `wgpu::Texture` → compositor registers it with vello and
//! emits `Scene::draw_image` against the target scene. No panics, the
//! frame report shows `painted = 1`, the compositor holds one
//! registered texture.

use std::sync::Arc;

use kurbo::Size;
use masonry_core::core::{DefaultProperties, NewWidget};
use masonry_testing::ModularWidget;
use mere_masonry::{MasonryEmbeddedRenderer, RootWidgetFactory};
use mere_renderer_registry::{NodeContentKind, Placement, RendererRegistry};
use spatial_substrate::{
    ExternalTextureCompositor, SubstrateHost, SubstrateNode, SubstrateScene,
};
use vello::{AaSupport, RendererOptions};

fn boot_wgpu() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
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
        Some((adapter, device, queue))
    })
}

fn build_factory() -> RootWidgetFactory {
    Arc::new(|| {
        let widget: ModularWidget<()> = ModularWidget::new(());
        NewWidget::new(widget).erased()
    })
}

#[test]
fn substrate_host_dispatches_embedded_frame_through_full_chain() {
    let Some((adapter, device, queue)) = boot_wgpu() else {
        eprintln!("skipping: no wgpu adapter available");
        return;
    };

    // 1. Construct the renderer registry + register a MasonryEmbeddedRenderer
    //    that handles NodeContentKind::Panel nodes.
    let masonry = MasonryEmbeddedRenderer::new(
        "test.mere-masonry.panel",
        NodeContentKind::Panel,
        adapter.clone(),
        device.clone(),
        queue.clone(),
        Arc::new(DefaultProperties::new()),
        build_factory(),
    );
    let mut registry = RendererRegistry::with_default_selector();
    registry
        .register(Box::new(masonry))
        .expect("register masonry renderer");

    // 2. Substrate host + scene with one panel node.
    let mut host = SubstrateHost::new(registry);
    let mut scene = SubstrateScene::new();
    let node_id = scene.insert(SubstrateNode::new(
        NodeContentKind::Panel,
        Placement::translate(10.0, 20.0),
        Size::new(64.0, 32.0),
    ));

    // 3. Compositor + vello renderer (host-side).
    let mut compositor = ExternalTextureCompositor::new();
    let mut vello_renderer = vello::Renderer::new(
        &device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::area_only(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .expect("vello renderer init");

    // 4. Render the scene through the unified dispatch path.
    let mut target_scene = vello::Scene::new();
    let report = host.render_scene(
        &scene,
        &mut target_scene,
        &mut compositor,
        &mut vello_renderer,
        1.0,
    );

    assert_eq!(report.painted, 1, "panel node painted");
    assert_eq!(report.missing_renderer, 0);
    assert_eq!(report.not_ready, 0);
    assert!(report.renderer_failed.is_empty());
    assert!(report.wrong_mode.is_empty());
    assert_eq!(report.overlay_skipped, 0);
    assert!(report.is_clean(), "frame report clean: {:?}", report);

    // 5. Compositor should hold exactly one registration (for this node).
    assert_eq!(
        compositor.len(),
        1,
        "compositor registered the panel texture"
    );
    assert!(compositor.is_registered(node_id));

    // 6. Second render reuses the existing registration. Painted count
    //    stays 1.
    let report2 = host.render_scene(
        &scene,
        &mut target_scene,
        &mut compositor,
        &mut vello_renderer,
        1.0,
    );
    assert_eq!(report2.painted, 1);
    assert!(report2.is_clean());
    assert_eq!(
        compositor.len(),
        1,
        "no duplicate registration on second render"
    );

    // 7. release_node tears down both the producer (in the renderer)
    //    and the compositor's registration.
    host.release_node(node_id, &mut compositor, &mut vello_renderer);
    assert_eq!(
        compositor.len(),
        0,
        "compositor cleaned up after release_node"
    );
    assert!(!compositor.is_registered(node_id));
}

#[test]
fn mixed_scene_dispatches_inscene_and_embedded_correctly() {
    let Some((adapter, device, queue)) = boot_wgpu() else {
        eprintln!("skipping: no wgpu adapter available");
        return;
    };

    // Register one EmbeddedFrame renderer for Panel + one InScenePaint
    // renderer (the recording stub) for DocumentTile.
    let masonry = MasonryEmbeddedRenderer::new(
        "test.mere-masonry.panel",
        NodeContentKind::Panel,
        adapter.clone(),
        device.clone(),
        queue.clone(),
        Arc::new(DefaultProperties::new()),
        build_factory(),
    );
    let recording = spatial_substrate::RecordingRenderer::for_kind(
        "test.recording",
        NodeContentKind::DocumentTile,
    );
    let paint_log = recording.shared_paint_log();
    let mut registry = RendererRegistry::with_default_selector();
    registry.register(Box::new(masonry)).unwrap();
    registry.register(Box::new(recording)).unwrap();

    let mut host = SubstrateHost::new(registry);
    let mut scene = SubstrateScene::new();
    let panel_id = scene.insert(SubstrateNode::new(
        NodeContentKind::Panel,
        Placement::translate(10.0, 20.0),
        Size::new(64.0, 32.0),
    ));
    let doctile_id = scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::translate(100.0, 50.0),
        Size::new(120.0, 80.0),
    ));

    let mut compositor = ExternalTextureCompositor::new();
    let mut vello_renderer = vello::Renderer::new(
        &device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::area_only(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .unwrap();

    let mut target_scene = vello::Scene::new();
    let report = host.render_scene(
        &scene,
        &mut target_scene,
        &mut compositor,
        &mut vello_renderer,
        1.0,
    );

    // Both nodes painted, each through its own path.
    assert_eq!(report.painted, 2, "both nodes painted");
    assert!(report.is_clean(), "{:?}", report);

    // EmbeddedFrame path: panel registered with compositor.
    assert_eq!(compositor.len(), 1);
    assert!(compositor.is_registered(panel_id));

    // InScene path: recording renderer captured the doctile paint call.
    let records = paint_log.borrow();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].identity, doctile_id);
    assert_eq!(records[0].node_transform.translation().x, 100.0);
    assert_eq!(records[0].node_transform.translation().y, 50.0);
}
