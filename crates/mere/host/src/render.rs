// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Per-frame paint path: substrate dispatch → vello rasterise →
//! blit to the surface. Kept narrow and renderer-agnostic; each
//! `NodeRenderer` registered with the substrate decides what its
//! pane paints.

use vello::peniko::Color;
use vello::{AaConfig, RenderParams};

use crate::runtime::RuntimeState;

/// Lazy-allocate (or replace, after resize) the intermediate
/// `STORAGE_BINDING` texture vello writes into. Surface formats
/// must be `Rgba8Unorm` (or another non-srgb format) so the storage
/// binding actually validates — set during `setup` based on what
/// the adapter offers.
pub fn ensure_target_texture(state: &mut RuntimeState) {
    if state.gpu.target_texture.is_some() {
        return;
    }
    let width = state.gpu.surface_config.width;
    let height = state.gpu.surface_config.height;
    let texture = state.gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("host intermediate"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: state.gpu.surface_config.format,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    state.gpu.target_view = Some(view);
    state.gpu.target_texture = Some(texture);
}

/// Render one frame: substrate scene → vello scene → intermediate
/// texture → present surface. Skips paint on transient surface
/// states (Timeout / Occluded / Validation); reconfigures + drops
/// the cached texture on Outdated / Lost.
pub fn render_frame(state: &mut RuntimeState) {
    ensure_target_texture(state);

    // Publish live metrics for the reactive diagnostics panel before
    // the substrate paints — the panel's `tick` reads this snapshot
    // and rebuilds its widget tree during `next_frame`.
    state.frame_count += 1;
    if let Ok(mut snap) = state.diagnostics.lock() {
        snap.frame_count = state.frame_count;
        snap.scene_nodes = state.host_app.scene.len();
        snap.cursor = state.interaction.cursor.map(|p| (p.x, p.y));
    }

    use wgpu::CurrentSurfaceTexture;
    let surface_texture = match state.gpu.surface.get_current_texture() {
        CurrentSurfaceTexture::Success(t) | CurrentSurfaceTexture::Suboptimal(t) => t,
        CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
            state
                .gpu
                .surface
                .configure(&state.gpu.device, &state.gpu.surface_config);
            state.gpu.target_texture = None;
            return;
        }
        CurrentSurfaceTexture::Timeout
        | CurrentSurfaceTexture::Occluded
        | CurrentSurfaceTexture::Validation => return,
    };

    // Substrate dispatch — each registered renderer paints into the
    // vello scene at its content kind's slot.
    let mut vello_scene = vello::Scene::new();
    let _report = state.host_app.substrate.render_scene(
        &state.host_app.scene,
        &mut vello_scene,
        &mut state.host_app.compositor,
        &mut state.gpu.renderer,
        state.gpu.window.scale_factor(),
    );

    let target_view = state.gpu.target_view.as_ref().expect("ensured");
    let width = state.gpu.surface_config.width;
    let height = state.gpu.surface_config.height;
    state
        .gpu
        .renderer
        .render_to_texture(
            &state.gpu.device,
            &state.gpu.queue,
            &vello_scene,
            target_view,
            &RenderParams {
                base_color: Color::from_rgba8(15, 15, 25, 255),
                width,
                height,
                antialiasing_method: AaConfig::Area,
            },
        )
        .expect("vello render");

    // Blit → present.
    let target_texture = state.gpu.target_texture.as_ref().expect("ensured");
    let mut encoder = state
        .gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("host blit"),
        });
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: target_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: &surface_texture.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    state.gpu.queue.submit(Some(encoder.finish()));
    state.gpu.window.pre_present_notify();
    surface_texture.present();
}
