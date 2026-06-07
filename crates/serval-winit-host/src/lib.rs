/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared serval-on-winit host plumbing.
//!
//! The meerkat chrome shell and the orrery host are both "a serval surface
//! presented on a winit window via netrender". The present mechanics — booting
//! wgpu + a netrender [`Renderer`], configuring the surface, rasterizing a
//! [`Scene`] into an offscreen texture, acquiring + compositing onto the
//! backbuffer — and the winit→serval key / modifier mapping are identical
//! between them, so they live here. Each host keeps only its own scene
//! composition and input routing.
//!
//! Per-frame shape a host follows:
//!
//! ```text
//! let (_tex, view) = host.rasterize(&scene, w, h, clear);   // one per layer
//! let Some(frame)  = host.acquire() else { return };         // skip if outdated
//! let target = frame.texture.create_view(&Default::default());
//! host.renderer().compose_external_texture(&view, &target, host.format(), w, h, placement);
//! frame.present();
//! ```

use std::sync::Arc;

use netrender::{ColorLoad, NetrenderOptions, Renderer, Scene};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey};
use winit::window::Window;
use xilem_serval::{Key, KeyEvent, Modifiers, NamedKey};

/// A wgpu surface + netrender [`Renderer`] bound to one winit window: the present
/// stack a serval host draws into. Build with [`SurfaceHost::boot`], then per
/// frame [`rasterize`](Self::rasterize) each scene and [`acquire`](Self::acquire)
/// the backbuffer to composite onto.
pub struct SurfaceHost {
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

impl SurfaceHost {
    /// Boot wgpu + a netrender [`Renderer`] and configure a surface on `window`
    /// at `(width, height)`. Returns a diagnostic string on any boot / surface
    /// failure (the caller decides whether to exit the event loop). Prefers an
    /// sRGB surface format, falling back to the first advertised format.
    /// Native blocking boot via netrender's synchronous wgpu boot. On wasm the
    /// WebGPU device request is async, so use [`boot_async`](Self::boot_async).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn boot(
        window: Arc<Window>,
        width: u32,
        height: u32,
        options: NetrenderOptions,
    ) -> Result<Self, String> {
        let handles = netrender::boot().map_err(|e| format!("netrender wgpu boot failed: {e}"))?;
        Self::from_handles(handles, window, width, height, options)
    }

    /// Async boot: awaits netrender's `boot_async`. This is the only boot path on
    /// wasm (WebGPU device acquisition is asynchronous); it works on every target,
    /// so a native caller can `block_on` it instead of [`boot`](Self::boot).
    pub async fn boot_async(
        window: Arc<Window>,
        width: u32,
        height: u32,
        options: NetrenderOptions,
    ) -> Result<Self, String> {
        let handles = netrender::boot_async()
            .await
            .map_err(|e| format!("netrender wgpu boot failed: {e}"))?;
        Self::from_handles(handles, window, width, height, options)
    }

    /// Build + configure the surface and create the renderer from already-booted
    /// wgpu handles. Shared by [`boot`](Self::boot) and [`boot_async`](Self::boot_async).
    fn from_handles(
        handles: netrender::WgpuHandles,
        window: Arc<Window>,
        width: u32,
        height: u32,
        options: NetrenderOptions,
    ) -> Result<Self, String> {
        let surface = handles
            .instance
            .create_surface(window)
            .map_err(|e| format!("create_surface failed: {e}"))?;
        let caps = surface.get_capabilities(&handles.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        let renderer = netrender::create_netrender_instance(handles, options)
            .map_err(|e| format!("netrender init failed: {e:?}"))?;
        surface.configure(&renderer.wgpu_device.core.device, &surface_config);
        Ok(Self { surface, surface_config, renderer })
    }

    /// The netrender renderer — call `compose_external_texture` (and friends) on
    /// it to composite rasterized layers onto the backbuffer.
    pub fn renderer(&self) -> &Renderer {
        &self.renderer
    }

    /// The surface's texture format (pass to `compose_external_texture`).
    pub fn format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    /// The wgpu device backing the renderer.
    pub fn device(&self) -> &wgpu::Device {
        &self.renderer.wgpu_device.core.device
    }

    /// Reconfigure the surface for a new size (clamped to ≥ 1).
    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface_config.width = width.max(1);
        self.surface_config.height = height.max(1);
        self.surface.configure(self.device(), &self.surface_config);
    }

    /// Rasterize `scene` into a fresh `(w, h)` `Rgba8Unorm` texture, cleared to
    /// `clear`. Returns the texture with its view; keep the texture alive until
    /// the composite pass has sampled the view.
    pub fn rasterize(
        &self,
        scene: &Scene,
        w: u32,
        h: u32,
        clear: ColorLoad,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let device = self.device();
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("serval-winit-host scene"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("serval-winit-host scene view"),
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        self.renderer.render_vello(scene, &view, clear);
        (tex, view)
    }

    /// Acquire the surface backbuffer for this frame. Returns `None` (and
    /// reconfigures) when the surface is outdated / lost or otherwise
    /// unavailable, so the caller simply skips the frame.
    pub fn acquire(&self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(self.device(), &self.surface_config);
                None
            },
            other => {
                eprintln!("[serval-winit-host] surface acquire skipped: {other:?}");
                None
            },
        }
    }
}

/// Map a winit logical key + modifiers to a serval [`KeyEvent`]; `None` for dead
/// / unidentified keys with no routable mapping.
pub fn key_event_from_winit(key: &WinitKey, mods: Modifiers) -> Option<KeyEvent> {
    let mapped = match key {
        WinitKey::Character(s) => Key::Character(s.to_string()),
        WinitKey::Named(named) => Key::Named(match named {
            WinitNamedKey::Backspace => NamedKey::Backspace,
            WinitNamedKey::Enter => NamedKey::Enter,
            WinitNamedKey::Tab => NamedKey::Tab,
            WinitNamedKey::Escape => NamedKey::Escape,
            WinitNamedKey::Space => NamedKey::Space,
            WinitNamedKey::ArrowLeft => NamedKey::ArrowLeft,
            WinitNamedKey::ArrowRight => NamedKey::ArrowRight,
            WinitNamedKey::ArrowUp => NamedKey::ArrowUp,
            WinitNamedKey::ArrowDown => NamedKey::ArrowDown,
            WinitNamedKey::Delete => NamedKey::Delete,
            WinitNamedKey::Home => NamedKey::Home,
            WinitNamedKey::End => NamedKey::End,
            _ => NamedKey::Other,
        }),
        WinitKey::Dead(_) | WinitKey::Unidentified(_) => return None,
    };
    Some(KeyEvent::with_mods(mapped, mods))
}

/// Map winit's modifier state to serval's [`Modifiers`].
pub fn modifiers_from_winit(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        meta: state.super_key(),
    }
}
