/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! meerkat-shell: the on-screen serval host for Mere's chrome.
//!
//! A winit window that runs the reused chrome ([`meerkat::chrome_view`] over a
//! [`meerkat::Chrome`] wrapping the graphshell `ToolbarState`) through serval and
//! presents via netrender — pelt-live-shaped. It reuses pelt-live's lib for the
//! cascade → layout → paint → `Scene` builder ([`scene_from_scripted_dom`]) and
//! the point→node hit-test ([`hit_test_node`]), so this file is just the window +
//! present + input-dispatch harness, not a second engine.
//!
//! First on-screen slice: render + left-click dispatch + keyboard dispatch.
//! a11y, IME, scrolling, and the editable-omnibar `TextInput` land next.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use meerkat::{chrome_view, Chrome, ChromeLogic, ChromeView};
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions, Renderer};
use pelt_live::{hit_test_node, scene_from_scripted_dom};
use serval_layout::ScrollOffsets;
use serval_scripted_dom::{NodeId, ScriptedDom};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};
use xilem_serval::{Key, KeyEvent, Modifiers, NamedKey, PointerClick, ServalAppRunner};

/// Author CSS for the chrome. The toolbar is a flex row (back / forward buttons
/// + a growing omnibar); serval lays it out via taffy's flexbox.
const SHEET: &[&str] = &[
    "div, button, input { display: block; }",
    ".toolbar { display: flex; background-color: rgb(236, 238, 243); padding: 8px; }",
    "button { font-size: 22px; color: rgb(30, 30, 40); \
        background-color: rgb(220, 224, 232); padding: 8px 14px; margin: 4px; }",
    ".omnibar { font-size: 22px; color: rgb(20, 20, 20); \
        background-color: rgb(255, 255, 255); padding: 8px; margin: 4px; flex-grow: 1; }",
];

/// wgpu/netrender state, built once a window exists.
struct Gpu {
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

/// The meerkat shell application: the shared chrome DOM, the runner that diffs
/// the chrome view tree into it, the window + GPU, and input bookkeeping.
struct App {
    /// The chrome DOM the runner mutates and the render path reads.
    dom: Rc<RefCell<ScriptedDom>>,
    runner: ServalAppRunner<Chrome, ChromeLogic, ChromeView>,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    /// Tracked keyboard modifiers, folded into each dispatched `KeyEvent`.
    modifiers: Modifiers,
    /// Last cursor position in physical pixels (window space == content space).
    cursor: (f32, f32),
    width: u32,
    height: u32,
}

impl App {
    fn new() -> Self {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        let runner = ServalAppRunner::new(
            dom.clone(),
            chrome_view as ChromeLogic,
            Chrome::new("mere://welcome"),
        );
        Self {
            dom,
            runner,
            window: None,
            gpu: None,
            modifiers: Modifiers::default(),
            cursor: (0.0, 0.0),
            width: 1024,
            height: 600,
        }
    }

    /// Reconfigure the surface for `(width, height)` and request a redraw.
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.surface_config.width = self.width;
            gpu.surface_config.height = self.height;
            gpu.surface
                .configure(&gpu.renderer.wgpu_device.core.device, &gpu.surface_config);
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Render the chrome DOM and present it. Mirrors pelt-live's present path:
    /// `scene_from_scripted_dom` runs the serval engine into a `netrender::Scene`,
    /// `render_vello` rasterizes it into an `Rgba8Unorm` texture, and
    /// `compose_external_texture` blits that onto the surface backbuffer.
    fn render(&mut self) {
        let Some(gpu) = self.gpu.as_ref() else { return };
        let (w, h) = (self.width.max(1), self.height.max(1));

        // 1. Engine pipeline → Scene. No focused-field cursor and no scrolling in
        //    this slice (the editable omnibar + scrollable chrome land later).
        let scroll_offsets = ScrollOffsets::<NodeId>::default();
        let scene = scene_from_scripted_dom(&self.dom.borrow(), SHEET, w, h, None, &scroll_offsets);

        // 2. Rasterize the scene into a fresh Rgba8Unorm target.
        let device = &gpu.renderer.wgpu_device.core.device;
        let content = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("meerkat chrome content"),
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
        let content_view = content.create_view(&wgpu::TextureViewDescriptor {
            label: Some("meerkat chrome content view"),
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        gpu.renderer
            .render_vello(&scene, &content_view, ColorLoad::Clear(wgpu::Color::WHITE));

        // 3. Acquire the surface backbuffer and blit the content onto it.
        let frame = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                gpu.surface.configure(device, &gpu.surface_config);
                return;
            },
            other => {
                eprintln!("[meerkat] surface acquire skipped: {other:?}");
                return;
            },
        };
        let target_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        gpu.renderer.compose_external_texture(
            &content_view,
            &target_view,
            gpu.surface_config.format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, 0.0, w as f32, h as f32]),
        );
        frame.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Meerkat — Mere chrome on serval")
            .with_inner_size(PhysicalSize::new(self.width, self.height));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create meerkat window"),
        );
        let size = window.inner_size();
        self.width = size.width.max(1);
        self.height = size.height.max(1);

        // wgpu handles via netrender::boot, then the netrender renderer.
        let handles = match netrender::boot() {
            Ok(handles) => handles,
            Err(err) => {
                eprintln!("[meerkat] netrender wgpu boot failed: {err}");
                event_loop.exit();
                return;
            },
        };
        let surface = match handles.instance.create_surface(window.clone()) {
            Ok(surface) => surface,
            Err(err) => {
                eprintln!("[meerkat] create_surface failed: {err}");
                event_loop.exit();
                return;
            },
        };
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
            width: self.width,
            height: self.height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        let renderer = match netrender::create_netrender_instance(
            handles,
            NetrenderOptions { tile_cache_size: Some(64), enable_vello: true, ..Default::default() },
        ) {
            Ok(renderer) => renderer,
            Err(err) => {
                eprintln!("[meerkat] netrender init failed: {err:?}");
                event_loop.exit();
                return;
            },
        };
        surface.configure(&renderer.wgpu_device.core.device, &surface_config);
        self.gpu = Some(Gpu { surface, surface_config, renderer });
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if self.window.as_ref().map(|w| w.id()) != Some(window_id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
            },
            WindowEvent::ModifiersChanged(mods) => {
                let s = mods.state();
                self.modifiers = Modifiers {
                    shift: s.shift_key(),
                    ctrl: s.control_key(),
                    alt: s.alt_key(),
                    meta: s.super_key(),
                };
            },
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                // Hit-test the cursor through serval's query, then dispatch a
                // click to the hit node (routes to the chrome's on_click handlers).
                let (x, y) = self.cursor;
                let offsets = ScrollOffsets::<NodeId>::default();
                let hit = hit_test_node(&self.dom.borrow(), SHEET, self.width, self.height, x, y, &offsets);
                if let Some(node) = hit {
                    self.runner.dispatch_click(node, PointerClick::at((x, y)));
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let Some(key_event) = key_event_from_winit(&event.logical_key, self.modifiers) {
                        self.runner.dispatch_key(key_event);
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                    }
                }
            },
            WindowEvent::RedrawRequested => self.render(),
            _ => {},
        }
    }
}

/// Map a winit logical key + modifiers to a serval [`KeyEvent`]; `None` for dead
/// / unidentified keys with no routable mapping.
fn key_event_from_winit(key: &WinitKey, mods: Modifiers) -> Option<KeyEvent> {
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

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("meerkat=info")),
        )
        .init();
    tracing::info!("meerkat-shell starting");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("event loop error");
}
