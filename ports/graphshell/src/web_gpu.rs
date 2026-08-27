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

use genet_render_host::{RenderCore, WindowSurface};
use netrender::{ColorLoad, ExternalTexturePlacement, NetrenderOptions, Scene};
use web_sys::HtmlCanvasElement;

pub(crate) struct GpuPresenter {
    core: RenderCore,
    surface: WindowSurface,
    blitter: wgpu::util::TextureBlitter,
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
        })
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
            1,
            content,
            width,
            height,
            ColorLoad::Clear(wgpu::Color {
                r: 0.027,
                g: 0.047,
                b: 0.059,
                a: 1.0,
            }),
            1.0,
        );
        let (_chrome_texture, chrome_view) = self.core.rasterize_scaled_for(
            2,
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
