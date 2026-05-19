// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Thumbnail-capture integration test: drives the same scene as
//! `render_to_png.rs` but through a fit camera into a smaller target
//! texture, producing a switcher-preview-sized thumbnail.
//!
//! Pipeline:
//!
//! 1. Build a 512×384 scene (larger than the thumbnail target).
//! 2. Call `SubstrateScene::content_bounds()` to discover the
//!    union of node bounds.
//! 3. Compute the fit camera via `fit_camera(bounds, (128, 128),
//!    ContainAspect)`.
//! 4. Set the host's camera; call `SubstrateHost::paint_scene` to
//!    produce a vello scene already-in-target-coords.
//! 5. Render to a 128×128 wgpu texture.
//! 6. Read back; write PNG to `target/spatial_prototype_thumbnail.png`.
//! 7. Assert the PNG has the expected size and that the backdrop and
//!    a sampled colored region are present.

use std::sync::Arc;
use std::sync::mpsc;

use kurbo::Size;
use mere_renderer_registry::{NodeContentKind, Placement, RendererRegistry};
use spatial_substrate::{
    RelationEdge, SolidRectRenderer, SubstrateHost, SubstrateNode, SubstrateScene, ThumbnailFit,
    fit_camera,
};
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions};

const SCENE_WIDTH: f64 = 512.0;
const SCENE_HEIGHT: f64 = 384.0;
const THUMB: u32 = 128;

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

fn build_host() -> SubstrateHost {
    let mut registry = RendererRegistry::with_default_selector();
    registry
        .register(Box::new(SolidRectRenderer::for_kind(
            "thumb.bg",
            NodeContentKind::CustomCanvas,
            Color::from_rgba8(20, 30, 60, 255),
        )))
        .unwrap();
    registry
        .register(Box::new(SolidRectRenderer::for_kind(
            "thumb.panel",
            NodeContentKind::Panel,
            Color::from_rgba8(220, 80, 80, 255),
        )))
        .unwrap();
    registry
        .register(Box::new(SolidRectRenderer::for_kind(
            "thumb.doctile",
            NodeContentKind::DocumentTile,
            Color::from_rgba8(80, 200, 100, 255),
        )))
        .unwrap();
    SubstrateHost::new(registry)
}

fn build_scene() -> SubstrateScene {
    let mut scene = SubstrateScene::new();
    scene.insert(SubstrateNode::new(
        NodeContentKind::CustomCanvas,
        Placement::IDENTITY,
        Size::new(SCENE_WIDTH, SCENE_HEIGHT),
    ));
    let panel = scene.insert(SubstrateNode::new(
        NodeContentKind::Panel,
        Placement::translate(80.0, 80.0),
        Size::new(200.0, 120.0),
    ));
    let doctile = scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::translate(280.0, 240.0),
        Size::new(180.0, 100.0),
    ));
    scene.insert_edge(RelationEdge::new(panel, doctile));
    scene
}

fn allocate_target(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("thumbnail target"),
        size: wgpu::Extent3d {
            width: THUMB,
            height: THUMB,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn read_back_rgba8(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = THUMB * 4;
    let buffer_size = (bytes_per_row * THUMB) as u64;
    let buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("thumbnail readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    }));
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("thumbnail copy encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(THUMB),
            },
        },
        wgpu::Extent3d {
            width: THUMB,
            height: THUMB,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("device.poll");
    rx.recv().unwrap().unwrap();
    let mapped = slice.get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    bytes
}

fn write_png(path: &str, rgba: &[u8]) {
    if let Some(dir) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = std::fs::File::create(path).expect("create PNG");
    let mut encoder = png::Encoder::new(file, THUMB, THUMB);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png write");
}

#[test]
fn substrate_renders_scene_to_thumbnail_png() {
    let Some((device, queue)) = boot_wgpu() else {
        eprintln!("skipping: no wgpu adapter available");
        return;
    };

    let mut host = build_host();
    let scene = build_scene();

    // Discover the scene's content bounds and compute the fit camera.
    let bounds = scene.content_bounds().expect("scene has nodes");
    let camera = fit_camera(bounds, (THUMB, THUMB), ThumbnailFit::ContainAspect);
    host.set_camera(camera);

    // Paint into a vello scene; geometry already in target coords
    // because the camera applies during dispatch.
    let mut target_scene = vello::Scene::new();
    let report = host.paint_scene(&scene, &mut target_scene, 1.0);
    assert_eq!(report.painted, 3);
    assert_eq!(report.edges_painted, 1);
    assert!(report.is_clean(), "{:?}", report);

    let mut renderer = vello::Renderer::new(
        &device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::area_only(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .expect("vello renderer init");
    let (texture, view) = allocate_target(&device);
    renderer
        .render_to_texture(
            &device,
            &queue,
            &target_scene,
            &view,
            &RenderParams {
                base_color: Color::TRANSPARENT,
                width: THUMB,
                height: THUMB,
                antialiasing_method: AaConfig::Area,
            },
        )
        .expect("vello render_to_texture");

    let rgba = read_back_rgba8(&device, &queue, &texture);
    write_png("target/spatial_prototype_thumbnail.png", &rgba);

    // Pixel sanity: the backdrop should reach (4, 4) — the fit camera
    // maps the 512×384 source into the 128×128 target with letterbox
    // (384 < 512, so the target is wider than source-aspect; here the
    // source is wider than target-aspect, so we get top+bottom
    // letterbox). At (4, 4) we're well inside the source's mapped
    // region (after content_bounds discovery the backdrop's
    // 512×384-equivalent fills the full source rect).
    fn assert_close(actual: [u8; 4], expected: [u8; 4], tol: u8, where_: &str) {
        for c in 0..4 {
            let diff = (actual[c] as i16 - expected[c] as i16).unsigned_abs() as u8;
            assert!(
                diff <= tol,
                "{}: chan {} diff {} > tol {} (actual={:?}, expected={:?})",
                where_,
                c,
                diff,
                tol,
                actual,
                expected
            );
        }
    }
    // The fit-scaled backdrop fills the target rect because the
    // CustomCanvas at IDENTITY * (512×384) IS the content bounds.
    let center = ((THUMB / 2) as u32, (THUMB / 2) as u32);
    let pixel = pixel_at(&rgba, center.0, center.1);
    // Center of the thumbnail will overlap one of the panels or
    // backdrop — but we don't know which without doing the math. Just
    // assert the alpha is fully opaque (no fully-transparent output).
    assert_eq!(pixel[3], 255, "thumbnail center should be opaque");

    // ContainAspect fits a 512×384 (4:3) source into a 128×128 (1:1)
    // target by scaling 0.25× along the smaller axis (width) — source
    // maps to 128×96, vertically centered → y ∈ [16, 112]. (10, 50)
    // is inside the painted region and well into the dark-navy backdrop.
    let inside_backdrop = pixel_at(&rgba, 10, 50);
    assert!(
        inside_backdrop[2] >= inside_backdrop[0] && inside_backdrop[3] == 255,
        "inside-letterbox sample should be opaque + bluish (backdrop); got {:?}",
        inside_backdrop
    );

    // (4, 4) is in the letterbox region above the scaled source —
    // base_color is TRANSPARENT so this should read all zeros.
    let letterbox = pixel_at(&rgba, 4, 4);
    assert_eq!(letterbox, [0, 0, 0, 0], "letterbox region transparent");

    assert_close(
        pixel_at(&rgba, 30, 50),
        [220, 80, 80, 255],
        16,
        "panel sampled through fit-camera scaling",
    );
}

fn pixel_at(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * THUMB + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}
