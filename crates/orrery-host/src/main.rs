/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! orrery-host bin: a thin winit shell over [`orrery_host::Orrery`] — the
//! reusable, window-agnostic graph field-canvas content-root. It owns the window
//! and the shared [`SurfaceHost`] present stack, maps winit events onto the
//! orrery's semantic input methods, and on each redraw rasterizes + composites
//! the scene the orrery produces. meerkat hosts the same `Orrery` as its
//! content-root (S1 of the modular integration plan); this bin keeps the orrery
//! launchable + testable on its own.
//!
//! Navigation (per the graph-canvas directive): wheel = pan, Ctrl+wheel =
//! cursor-anchored zoom, middle-drag = pan, all with inertia. Left-drag grabs +
//! pins the node under the cursor; a click selects it; a left-drag on empty space
//! marquee-selects; a click near an edge picks it; a bare empty click clears.
//! Space re-seeds the layout and replays the settle.

use std::sync::Arc;

use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions};
use orrery::{Orrery, PointerButton, WHEEL_PAN_SCALE};
use serval_winit_host::SurfaceHost;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};

/// The orrery host application: the reusable [`Orrery`] content-root plus the
/// window + present stack that drives it.
struct App {
    orrery: Orrery,
    /// Wakes the loop when the physics actor has a fresh layout snapshot ready.
    proxy: EventLoopProxy<()>,
    /// Last cursor position in physical px. winit's `MouseInput` carries no
    /// position, so the shell tracks it from `CursorMoved` to drive press/release.
    cursor: (f32, f32),
    window: Option<Arc<Window>>,
    host: Option<SurfaceHost>,
    width: u32,
    height: u32,
}

impl App {
    fn new(proxy: EventLoopProxy<()>) -> Self {
        Self {
            // The standalone bin shows the built-in sample graph; meerkat drives a
            // live session graph through `Orrery::visit`.
            orrery: Orrery::with_sample_graph(),
            proxy,
            cursor: (0.0, 0.0),
            window: None,
            host: None,
            width: 1024,
            height: 600,
        }
    }

    /// Produce the orrery's frame at the current size, rasterize + composite it
    /// through the present stack, and chain another redraw if the orrery is still
    /// animating (settling / gliding / dragging).
    fn render(&mut self) {
        if self.host.is_none() {
            return;
        }
        let (w, h) = (self.width.max(1), self.height.max(1));
        let (scene, needs_redraw) = self.orrery.frame(w, h);

        let host = self.host.as_ref().unwrap();
        let (_tex, view) = host.rasterize(&scene, w, h, ColorLoad::Clear(wgpu::Color::WHITE));
        let Some(frame) = host.acquire() else { return };
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        host.renderer().compose_external_texture(
            &view,
            &target,
            host.format(),
            w,
            h,
            ExternalTexturePlacement::new([0.0, 0.0, w as f32, h as f32]),
        );
        frame.present();

        if needs_redraw {
            self.request_redraw();
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Orrery — graph field-canvas on serval")
            .with_inner_size(PhysicalSize::new(self.width, self.height));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create orrery window"),
        );
        let size = window.inner_size();
        self.width = size.width.max(1);
        self.height = size.height.max(1);
        self.orrery.resize(self.width, self.height);
        self.orrery.recenter();

        let options =
            NetrenderOptions { tile_cache_size: Some(64), enable_vello: true, ..Default::default() };
        match SurfaceHost::boot(window.clone(), self.width, self.height, options) {
            Ok(host) => self.host = Some(host),
            Err(err) => {
                eprintln!("[orrery-host] {err}");
                event_loop.exit();
                return;
            },
        }
        // Always-offload physics (P6): move the simulation onto an armillary actor
        // thread, waking this loop through the proxy when a layout snapshot lands.
        // Done here (once — `resumed` early-returns if a window already exists) so
        // the settle animates from the first visible frame.
        let proxy = self.proxy.clone();
        let physics_wake: armillary::Wake = Arc::new(move || {
            let _ = proxy.send_event(());
        });
        self.orrery.offload_physics(physics_wake);

        window.request_redraw();
        self.window = Some(window);
    }

    /// The physics actor woke us through the proxy: a fresh layout snapshot is
    /// waiting. Redraw so `frame()` folds it in (and chains on while settling).
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if self.window.as_ref().map(|w| w.id()) != Some(window_id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.width = size.width.max(1);
                self.height = size.height.max(1);
                if let Some(host) = self.host.as_mut() {
                    host.resize(self.width, self.height);
                }
                self.orrery.resize(self.width, self.height);
                self.request_redraw();
            },
            WindowEvent::ModifiersChanged(mods) => {
                self.orrery.set_ctrl(mods.state().control_key());
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                if self.orrery.cursor_moved(self.cursor.0, self.cursor.1) {
                    self.request_redraw();
                }
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * WHEEL_PAN_SCALE, y * WHEEL_PAN_SCALE),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                if self.orrery.wheel(dx, dy) {
                    self.request_redraw();
                }
            },
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => Some(PointerButton::Left),
                    MouseButton::Middle => Some(PointerButton::Middle),
                    MouseButton::Right => Some(PointerButton::Right),
                    _ => None,
                };
                if let Some(button) = button {
                    let (x, y) = self.cursor;
                    let redraw = match state {
                        ElementState::Pressed => self.orrery.pointer_down(button, x, y),
                        ElementState::Released => self.orrery.pointer_up(button, x, y),
                    };
                    if redraw {
                        self.request_redraw();
                    }
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                // Space re-seeds the central spiral and replays the settle.
                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, WinitKey::Named(WinitNamedKey::Space))
                    && self.orrery.reseed()
                {
                    self.request_redraw();
                }
            },
            WindowEvent::RedrawRequested => self.render(),
            _ => {},
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("orrery_host=info")),
        )
        .init();
    tracing::info!("orrery-host starting");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    event_loop.run_app(&mut app).expect("event loop error");
}
