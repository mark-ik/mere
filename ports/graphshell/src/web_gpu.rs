// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Canvas presentation: the browser's surface target, and Graphshell's
//! content-over-chrome composition onto it.
//!
//! Booting wgpu, configuring the surface, and rasterizing a [`Scene`] are the
//! same on a canvas as on a desktop window, so they come from
//! `genet_render_host::RenderCore` rather than being rederived here. What stays
//! is the part that is actually Graphshell's: two layers, content blitted
//! opaque and chrome composited over it with alpha.

use std::cell::Cell;
use std::rc::Rc;

use genet_render_host::{RenderCore, WindowSurface};
use netrender::{ColorLoad, ExternalTexturePlacement, NetrenderOptions, Scene};
use web_sys::HtmlCanvasElement;

/// The scene keys `present` rasterizes under. A capture uses two more, so its
/// tiles never diff against the presented frame's (see `present`).
const CONTENT_KEY: u64 = 1;
const CHROME_KEY: u64 = 2;
const CAPTURE_CONTENT_KEY: u64 = 3;
const CAPTURE_CHROME_KEY: u64 = 4;

/// The shell colour content clears to, under the chrome.
const SHELL_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.027,
    g: 0.047,
    b: 0.059,
    a: 1.0,
};

/// A frame read back into host memory: what the scenario lane's `capture`
/// verb produces, composed exactly as `present` composes the swapchain frame
/// but into an owned `COPY_SRC` target, because swapchain textures are not
/// copyable (`genet_render_host::RenderCore::read_rgba8_texture` says the
/// same). Readback on WebGPU is asynchronous — there is no blocking poll on
/// the browser main thread — so this is a promise-shaped value: created in
/// one frame, `ready` some frames later, and taken once.
pub(crate) struct PendingCapture {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    done: Rc<Cell<bool>>,
}

impl PendingCapture {
    pub(crate) fn ready(&self) -> bool {
        self.done.get()
    }

    /// Tightly packed RGBA8 rows, top-down. Call only after `ready`.
    pub(crate) fn take(self) -> Result<(u32, u32, Vec<u8>), String> {
        let slice = self.buffer.slice(..);
        let data = slice
            .get_mapped_range()
            .map_err(|error| format!("capture map failed: {error}"))?;
        let unpadded = (self.width * 4) as usize;
        let mut rgba = Vec::with_capacity(unpadded * self.height as usize);
        for row in 0..self.height as usize {
            let start = row * self.padded_bytes_per_row as usize;
            rgba.extend_from_slice(&data[start..start + unpadded]);
        }
        drop(data);
        self.buffer.unmap();
        Ok((self.width, self.height, rgba))
    }
}

pub(crate) struct GpuPresenter {
    core: RenderCore,
    surface: WindowSurface,
    blitter: wgpu::util::TextureBlitter,
    /// A blitter for the capture target's format, made on first capture. The
    /// swapchain blitter cannot serve: a blitter is built for one format.
    capture_blitter: Option<wgpu::util::TextureBlitter>,
}

impl GpuPresenter {
    pub(crate) async fn boot(
        canvas: HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let core = RenderCore::boot_async(NetrenderOptions {
            tile_cache_size: Some(1024),
            enable_vello: true,
            ..Default::default()
        })
        .await?;
        // The one browser-specific line: wgpu takes an `HtmlCanvasElement` as a
        // surface target the same way it takes a winit window.
        let surface = core.create_surface(wgpu::SurfaceTarget::Canvas(canvas), width, height)?;
        let blitter = wgpu::util::TextureBlitter::new(core.device(), surface.format());
        Ok(Self {
            core,
            surface,
            blitter,
            capture_blitter: None,
        })
    }

    /// Compose `content` under `chrome` into an owned RGBA8 texture and start
    /// reading it back. The composition is `present`'s, line for line, on a
    /// different target: the same two-layer rule, the same clear colours, so a
    /// capture shows what the canvas showed.
    pub(crate) fn capture(
        &mut self,
        content: &Scene,
        chrome: &Scene,
        width: u32,
        height: u32,
    ) -> PendingCapture {
        let width = width.max(1);
        let height = height.max(1);
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let (_content_texture, content_view) = self.core.rasterize_scaled_for(
            CAPTURE_CONTENT_KEY,
            content,
            width,
            height,
            ColorLoad::Clear(SHELL_CLEAR),
            1.0,
        );
        let (_chrome_texture, chrome_view) = self.core.rasterize_scaled_for(
            CAPTURE_CHROME_KEY,
            chrome,
            width,
            height,
            ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            1.0,
        );
        let device = self.core.device();
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("graphshell capture target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let blitter = self
            .capture_blitter
            .get_or_insert_with(|| wgpu::util::TextureBlitter::new(device, format));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("graphshell capture content blit"),
        });
        blitter.copy(device, &mut encoder, &content_view, &target_view);
        self.core.queue().submit([encoder.finish()]);
        self.core.renderer().compose_external_texture(
            &chrome_view,
            &target_view,
            format,
            width,
            height,
            ExternalTexturePlacement::new([0.0, 0.0, width as f32, height as f32]),
        );
        let unpadded = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded.div_ceil(align) * align;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("graphshell capture readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("graphshell capture readback"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.core.queue().submit([encoder.finish()]);
        let done = Rc::new(Cell::new(false));
        let flag = done.clone();
        // On WebGPU the callback arrives from the browser's event loop once the
        // copy has executed; no poll is needed or possible here.
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |_| flag.set(true));
        PendingCapture {
            buffer,
            width,
            height,
            padded_bytes_per_row,
            done,
        }
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        self.surface.resize(&self.core, width, height);
    }

    /// Content underneath, chrome over it. Content clears to the shell colour
    /// and is blitted opaque; chrome clears transparent and composites, so the
    /// canvas below shows through wherever the chrome does not paint.
    ///
    /// The two scenes are keyed separately (`1` and `2`) because a shared tile
    /// cache entry would diff each scene against whichever rendered last and
    /// rebuild every tile each frame.
    pub(crate) fn present(
        &self,
        content: &Scene,
        chrome: &Scene,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let (_content_texture, content_view) = self.core.rasterize_scaled_for(
            CONTENT_KEY,
            content,
            width,
            height,
            ColorLoad::Clear(SHELL_CLEAR),
            1.0,
        );
        let (_chrome_texture, chrome_view) = self.core.rasterize_scaled_for(
            CHROME_KEY,
            chrome,
            width,
            height,
            ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            1.0,
        );
        let Some(frame) = self.surface.acquire(&self.core) else {
            return Ok(());
        };
        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.core
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("graphshell content blit"),
                });
        self.blitter
            .copy(self.core.device(), &mut encoder, &content_view, &frame_view);
        self.core.queue().submit([encoder.finish()]);
        self.core.renderer().compose_external_texture(
            &chrome_view,
            &frame_view,
            self.surface.format(),
            width,
            height,
            ExternalTexturePlacement::new([0.0, 0.0, width as f32, height as f32]),
        );
        self.core.queue().present(frame);
        Ok(())
    }
}
