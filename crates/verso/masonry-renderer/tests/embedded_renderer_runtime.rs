// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Runtime integration test for `MasonryEmbeddedRenderer`.
//!
//! Exercises the full chain end-to-end against a real wgpu 29 device:
//!
//! 1. Boot headless wgpu via pollster.
//! 2. Construct a `MasonryEmbeddedRenderer` with a no-op `ModularWidget`
//!    factory.
//! 3. Call `ensure_producer` → `next_frame` → `release` and verify the
//!    returned wgpu texture has the expected dimensions + format.
//!
//! The widget doesn't paint anything visible — the test isn't checking
//! pixel content, only that the chain executes without panicking and
//! produces a texture of the right shape. Pixel-content verification
//! lands when a downstream consumer needs to assert visual output (e.g.
//! a snapshot test against a known widget tree).

use std::sync::Arc;

use kurbo::Size;
use masonry_core::core::{DefaultProperties, NewWidget};
use masonry_testing::ModularWidget;
use mere_masonry::{MasonryEmbeddedRenderer, RootWidgetFactory};
use mere_renderer_registry::{
    EmbeddedFrameRenderer, LodLevel, NodeContentKind, NodeIdentity, Placement, SceneNodeRef,
};

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
        // A no-op ModularWidget: default layout (100×100), no paint, no
        // input handling. We override layout to use whatever size the
        // tile gives us so it sizes correctly.
        let widget: ModularWidget<()> = ModularWidget::new(());
        NewWidget::new(widget).erased()
    })
}

#[test]
fn full_chain_produces_texture_with_requested_dimensions() {
    let Some((adapter, device, queue)) = boot_wgpu() else {
        // No GPU adapter available — likely headless CI without
        // accelerated graphics. Skip rather than fail.
        eprintln!("skipping: no wgpu adapter available");
        return;
    };

    let default_props = Arc::new(DefaultProperties::new());
    let factory = build_factory();
    let mut renderer = MasonryEmbeddedRenderer::new(
        "test.mere-masonry.panel",
        NodeContentKind::Panel,
        adapter,
        device,
        queue,
        default_props,
        factory,
    );

    let node = SceneNodeRef {
        identity: NodeIdentity::next(),
        placement: Placement::IDENTITY,
        lod: LodLevel::FullPane,
        size: Size::new(64.0, 32.0),
        content_kind: NodeContentKind::Panel,
        renderer_pin: None,
    };

    let handle = renderer.ensure_producer(&node);
    assert!(renderer.has_producer(handle));
    assert_eq!(renderer.producer_count(), 1);

    let texture = renderer
        .next_frame(handle)
        .expect("next_frame should return a texture");
    assert_eq!(texture.width(), 64, "texture width matches node size");
    assert_eq!(texture.height(), 32, "texture height matches node size");
    assert_eq!(
        texture.format(),
        wgpu::TextureFormat::Rgba8Unorm,
        "texture format matches what vello's register_texture expects"
    );

    // Second frame from the same producer should return another texture
    // (the producer reuses the underlying allocation; we get a clone of
    // the same handle).
    let _texture_2 = renderer
        .next_frame(handle)
        .expect("second frame should also produce a texture");

    renderer.release(handle);
    assert!(!renderer.has_producer(handle));
    assert_eq!(renderer.producer_count(), 0);
}

#[test]
fn multiple_producers_track_independently() {
    let Some((adapter, device, queue)) = boot_wgpu() else {
        eprintln!("skipping: no wgpu adapter available");
        return;
    };

    let default_props = Arc::new(DefaultProperties::new());
    let factory = build_factory();
    let mut renderer = MasonryEmbeddedRenderer::new(
        "test.mere-masonry.panel",
        NodeContentKind::Panel,
        adapter,
        device,
        queue,
        default_props,
        factory,
    );

    let node_a = SceneNodeRef {
        identity: NodeIdentity::next(),
        placement: Placement::IDENTITY,
        lod: LodLevel::FullPane,
        size: Size::new(48.0, 24.0),
        content_kind: NodeContentKind::Panel,
        renderer_pin: None,
    };
    let node_b = SceneNodeRef {
        identity: NodeIdentity::next(),
        placement: Placement::translate(100.0, 0.0),
        lod: LodLevel::FullPane,
        size: Size::new(80.0, 40.0),
        content_kind: NodeContentKind::Panel,
        renderer_pin: None,
    };

    let handle_a = renderer.ensure_producer(&node_a);
    let handle_b = renderer.ensure_producer(&node_b);
    assert_ne!(handle_a, handle_b);
    assert_eq!(renderer.producer_count(), 2);

    let tex_a = renderer.next_frame(handle_a).expect("frame A");
    let tex_b = renderer.next_frame(handle_b).expect("frame B");
    assert_eq!(tex_a.width(), 48);
    assert_eq!(tex_a.height(), 24);
    assert_eq!(tex_b.width(), 80);
    assert_eq!(tex_b.height(), 40);

    renderer.release(handle_a);
    assert_eq!(renderer.producer_count(), 1);
    assert!(!renderer.has_producer(handle_a));
    assert!(renderer.has_producer(handle_b));

    renderer.release(handle_b);
    assert_eq!(renderer.producer_count(), 0);
}
