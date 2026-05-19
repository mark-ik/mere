// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Render-to-PNG integration test: exercises the prototype end-to-end
//! through vello's GPU compute pass and saves the framebuffer to a
//! viewable PNG.
//!
//! Pipeline:
//!
//! 1. Boot headless wgpu 29 + vello 0.9 renderer.
//! 2. Build a `SubstrateScene` with three nodes (background, red panel,
//!    green doctile), each handled by a `SolidRectRenderer` registered
//!    against its `NodeContentKind`.
//! 3. `SubstrateHost::paint_scene` walks the scene and dispatches each
//!    node through the registry → renderer → `vello::Scene` ops.
//! 4. `vello::Renderer::render_to_texture` rasterizes those ops into a
//!    256×256 RGBA8 texture.
//! 5. `copy_texture_to_buffer` + `map_async` reads the pixels back to
//!    CPU memory.
//! 6. The PNG crate encodes the bytes to `target/spatial_prototype_render.png`
//!    (overwriting any previous run).
//! 7. Assertions sample known pixel positions for the expected colors.
//!
//! Skipped (with eprintln) if no wgpu adapter is available, so CI
//! environments without accelerated graphics don't fail. On Mark's
//! Windows workstation the test runs in ~1s and produces a visible PNG.

use std::sync::Arc;
use std::sync::mpsc;

use kurbo::Size;
use mere_renderer_registry::{NodeContentKind, Placement, RendererRegistry};
use mere_spatial_prototype::{
    RelationEdge, SolidRectRenderer, SubstrateHost, SubstrateNode, SubstrateScene,
};
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 256;

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
    // Backdrop: dark navy. Uses CustomCanvas content kind.
    registry
        .register(Box::new(SolidRectRenderer::for_kind(
            "test.background",
            NodeContentKind::CustomCanvas,
            Color::from_rgba8(20, 30, 60, 255),
        )))
        .unwrap();
    // Panels render red.
    registry
        .register(Box::new(SolidRectRenderer::for_kind(
            "test.panel",
            NodeContentKind::Panel,
            Color::from_rgba8(220, 80, 80, 255),
        )))
        .unwrap();
    // Document tiles render green.
    registry
        .register(Box::new(SolidRectRenderer::for_kind(
            "test.doctile",
            NodeContentKind::DocumentTile,
            Color::from_rgba8(80, 200, 100, 255),
        )))
        .unwrap();
    SubstrateHost::new(registry)
}

fn build_scene() -> SubstrateScene {
    let mut scene = SubstrateScene::new();
    // Backdrop fills the viewport.
    scene.insert(SubstrateNode::new(
        NodeContentKind::CustomCanvas,
        Placement::IDENTITY,
        Size::new(WIDTH as f64, HEIGHT as f64),
    ));
    // Red panel at (40, 40) sized 100×60.
    let panel_id = scene.insert(SubstrateNode::new(
        NodeContentKind::Panel,
        Placement::translate(40.0, 40.0),
        Size::new(100.0, 60.0),
    ));
    // Green doctile at (140, 130) sized 90×90.
    let doctile_id = scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::translate(140.0, 130.0),
        Size::new(90.0, 90.0),
    ));
    // Edge connecting the panel and doctile — exercises the
    // substrate-side edge paint pass (drawn over the node fill).
    scene.insert_edge(RelationEdge::new(panel_id, doctile_id).with_label("relates"));
    scene
}

fn allocate_target(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render-to-png target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        // STORAGE_BINDING: vello compute writes here.
        // TEXTURE_BINDING: vello samples it during its compute pass.
        // COPY_SRC: we read pixels back via copy_texture_to_buffer.
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn read_back_rgba8(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = WIDTH * 4;
    let buffer_size = (bytes_per_row * HEIGHT) as u64;
    let buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render-to-png readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    }));

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("render-to-png copy encoder"),
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
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).expect("map_async result channel");
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("device.poll");
    rx.recv()
        .expect("map_async callback")
        .expect("map_async ok");

    let mapped = slice.get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    bytes
}

fn write_png(path: &str, rgba: &[u8]) {
    let parent = std::path::Path::new(path).parent();
    if let Some(dir) = parent {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = std::fs::File::create(path).expect("create PNG file");
    let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png write");
}

fn pixel_at(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * WIDTH + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn substrate_renders_solid_rect_scene_to_png() {
    let Some((device, queue)) = boot_wgpu() else {
        eprintln!("skipping: no wgpu adapter available");
        return;
    };

    let mut host = build_host();
    let scene = build_scene();

    // 1. Substrate dispatch into a vello scene.
    let mut target_scene = vello::Scene::new();
    let report = host.paint_scene(&scene, &mut target_scene, 1.0);
    assert_eq!(report.painted, 3, "all three nodes painted");
    assert_eq!(report.edges_painted, 1, "the panel→doctile edge painted");
    assert!(report.is_clean(), "frame report clean: {:?}", report);

    // 2. Vello rasterizes the scene into a wgpu texture.
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
                width: WIDTH,
                height: HEIGHT,
                antialiasing_method: AaConfig::Area,
            },
        )
        .expect("vello render_to_texture");

    // 3. Read pixels back and write PNG for visual inspection.
    let rgba = read_back_rgba8(&device, &queue, &texture);
    write_png("target/spatial_prototype_render.png", &rgba);

    // 4. Assert known pixels match expected colors. Allow ±8 per channel
    //    tolerance to absorb vello's anti-aliasing along edges (we sample
    //    well inside each rect's interior).
    fn assert_close(actual: [u8; 4], expected: [u8; 4], tol: u8, where_: &str) {
        for c in 0..4 {
            let diff = (actual[c] as i16 - expected[c] as i16).unsigned_abs() as u8;
            assert!(
                diff <= tol,
                "{}: channel {} diff {} > tol {} (actual={:?}, expected={:?})",
                where_,
                c,
                diff,
                tol,
                actual,
                expected
            );
        }
    }

    // Backdrop pixel — well outside any panel.
    assert_close(pixel_at(&rgba, 10, 10), [20, 30, 60, 255], 8, "backdrop");
    // Inside red panel — sample near a corner, well off the diagonal edge
    // that goes from the panel's center (90, 70) to the doctile's center.
    assert_close(
        pixel_at(&rgba, 50, 50),
        [220, 80, 80, 255],
        8,
        "red panel corner",
    );
    // Inside green doctile — same corner-sample pattern.
    assert_close(
        pixel_at(&rgba, 220, 210),
        [80, 200, 100, 255],
        8,
        "green doctile corner",
    );
}
