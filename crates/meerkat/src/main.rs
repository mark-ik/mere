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
//! ## Two roots, one window
//!
//! The window composites **two document authorities**: the chrome root (the
//! reused toolbar / omnibar, diffed by the runner) in a top band, and the
//! content root ([`meerkat::content`], rebuilt from the navigated location) in
//! the rest. Each is run through serval into its own `Scene`, rasterized, and
//! composited at its band — neither root sees the other's tree. Clicks route by
//! region; the chrome band hit-tests the chrome root, the content band is inert
//! until the content root carries handlers.
//!
//! Next: a live content engine (fetch + parse) behind the content root, content
//! scrolling, a11y, and IME.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use layout_dom_api::LayoutDom;
use meerkat::content::build_content_dom;
use meerkat::{chrome_view, submit_omnibar, Chrome, ChromeLogic, ChromeView};
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions, Renderer, Scene};
use pelt_live::{fragments_from_scripted_dom, hit_test_node, scene_from_scripted_dom, TextCursor};
use serval_layout::ScrollOffsets;
use serval_scripted_dom::{NodeId, ScriptedDom};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};
use xilem_serval::{Key, KeyEvent, Modifiers, NamedKey, PointerClick, ServalAppRunner};

/// Author CSS for the **chrome** root. The toolbar is a flex row (back / forward
/// buttons + a growing omnibar); serval lays it out via taffy's flexbox.
const CHROME_SHEET: &[&str] = &[
    "div, button, input { display: block; }",
    ".toolbar { display: flex; background-color: rgb(236, 238, 243); padding: 8px; }",
    "button { font-size: 22px; color: rgb(30, 30, 40); \
        background-color: rgb(220, 224, 232); padding: 8px 14px; margin: 4px; }",
    ".disabled { color: rgb(170, 174, 184); background-color: rgb(228, 230, 236); }",
    "input { font-size: 22px; color: rgb(20, 20, 20); \
        background-color: rgb(255, 255, 255); padding: 8px; margin: 4px; flex-grow: 1; }",
];

/// Author CSS for the **content** root — a separate document authority, so it
/// carries its own sheet (no chrome rules leak in).
const CONTENT_SHEET: &[&str] = &[
    "div, h1, p { display: block; }",
    ".page { padding: 32px; background-color: rgb(255, 255, 255); }",
    "h1 { font-size: 40px; color: rgb(20, 22, 30); margin-bottom: 16px; }",
    ".loc { font-size: 30px; color: rgb(30, 34, 44); }",
    ".lead { font-size: 22px; color: rgb(60, 64, 76); margin-bottom: 12px; }",
    "p { font-size: 17px; color: rgb(70, 74, 84); margin-bottom: 8px; }",
    ".note { font-size: 16px; color: rgb(120, 124, 134); }",
];

/// Fallback chrome-band height (px) if the toolbar can't be measured.
const FALLBACK_TOOLBAR_H: u32 = 64;

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
    /// The content root — a separate document authority, rebuilt from the
    /// chrome's `content_location` whenever navigation changes it.
    content_dom: Rc<RefCell<ScriptedDom>>,
    /// The location currently built into `content_dom`; guards rebuilds.
    content_location: String,
    /// Cached measured height (px) of the chrome band; `0` until first measured.
    toolbar_h: u32,
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
        let content_location = runner.state().content_location().to_string();
        let content_dom = Rc::new(RefCell::new(build_content_dom(&content_location)));
        Self {
            dom,
            runner,
            content_dom,
            content_location,
            toolbar_h: 0,
            window: None,
            gpu: None,
            modifiers: Modifiers::default(),
            cursor: (0.0, 0.0),
            width: 1024,
            height: 600,
        }
    }

    /// Rebuild the content root if the chrome's `content_location` has changed
    /// since it was last built. Called after any input that can navigate.
    fn sync_content(&mut self) {
        let loc = self.runner.state().content_location().to_string();
        if loc != self.content_location {
            *self.content_dom.borrow_mut() = build_content_dom(&loc);
            self.content_location = loc;
        }
    }

    /// The chrome-band height, measuring + caching it on first use. The toolbar
    /// is a single flex row, so its border-box height is independent of the
    /// available width/height; measuring once suffices.
    fn toolbar_height(&mut self) -> u32 {
        if self.toolbar_h == 0 {
            self.toolbar_h = measure_toolbar_height(&self.dom.borrow(), self.width, self.height);
        }
        self.toolbar_h
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

    /// Render the two document authorities and present them. The chrome root
    /// fills a top band (its measured height); the content root fills the rest.
    /// Each is run through the serval engine into its own `netrender::Scene`,
    /// rasterized to an `Rgba8Unorm` texture, and composited onto the surface at
    /// its band — separate roots, separate scenes, one backbuffer.
    fn render(&mut self) {
        if self.gpu.is_none() {
            return;
        }
        let (w, h) = (self.width.max(1), self.height.max(1));
        let toolbar_h = self.toolbar_height().min(h);
        let content_h = h.saturating_sub(toolbar_h).max(1);

        // Chrome scene (top band). Paint the omnibar caret / selection when the
        // chrome omnibar is focused (byte offsets from the field's char model).
        let cursor = self.runner.focus().map(|node| {
            let field = &self.runner.state().omnibar;
            let byte_of = |i: usize| {
                field.text().char_indices().nth(i).map(|(b, _)| b).unwrap_or(field.text().len())
            };
            let selection = field.has_selection().then(|| {
                let (s, e) = field.selection();
                (byte_of(s), byte_of(e))
            });
            TextCursor { node, caret: field.caret_byte_in_render(), selection }
        });
        let scroll = ScrollOffsets::<NodeId>::default();
        let chrome_scene =
            scene_from_scripted_dom(&self.dom.borrow(), CHROME_SHEET, w, toolbar_h, cursor, &scroll);
        let content_scene = scene_from_scripted_dom(
            &self.content_dom.borrow(),
            CONTENT_SHEET,
            w,
            content_h,
            None,
            &scroll,
        );

        let gpu = self.gpu.as_ref().unwrap();
        let (_chrome_tex, chrome_view) = gpu.rasterize(&chrome_scene, w, toolbar_h);
        let (_content_tex, content_view) = gpu.rasterize(&content_scene, w, content_h);

        let device = &gpu.renderer.wgpu_device.core.device;
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
        let format = gpu.surface_config.format;
        // Stack the two authorities. dest_rect is [x0, y0, x1, y1] (min/max
        // corners); the viewport is the full surface for the NDC mapping.
        gpu.renderer.compose_external_texture(
            &chrome_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, 0.0, w as f32, toolbar_h as f32]),
        );
        gpu.renderer.compose_external_texture(
            &content_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, toolbar_h as f32, w as f32, h as f32]),
        );
        frame.present();
    }

    /// Request a redraw if a window exists.
    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl Gpu {
    /// Run `scene` into a fresh `(w, h)` `Rgba8Unorm` texture and return it with
    /// its view. The texture is returned (not just the view) so the caller keeps
    /// it alive until the composite pass has sampled it.
    fn rasterize(&self, scene: &Scene, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let device = &self.renderer.wgpu_device.core.device;
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("meerkat scene"),
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
            label: Some("meerkat scene view"),
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        self.renderer
            .render_vello(scene, &view, ColorLoad::Clear(wgpu::Color::WHITE));
        (tex, view)
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
                // Route by region: a click in the chrome band hit-tests the
                // chrome root and dispatches (the on_click handlers); a click in
                // the content band is inert until the content root carries
                // handlers. Each root is queried in its own coordinate space.
                let (x, y) = self.cursor;
                let toolbar_h = self.toolbar_height();
                if y < toolbar_h as f32 {
                    let offsets = ScrollOffsets::<NodeId>::default();
                    let hit = hit_test_node(
                        &self.dom.borrow(), CHROME_SHEET, self.width, toolbar_h, x, y, &offsets,
                    );
                    if let Some(node) = hit {
                        self.runner.dispatch_click(node, PointerClick::at((x, y)));
                        self.sync_content();
                        self.request_redraw();
                    }
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    let is_enter =
                        matches!(event.logical_key, WinitKey::Named(WinitNamedKey::Enter));
                    if is_enter && self.runner.focus().is_some() {
                        // Submit the focused omnibar: navigate (history + toolbar)
                        // then rebuild the content root for the new location.
                        self.runner.update(submit_omnibar);
                        tracing::info!(
                            location = %self.runner.state().toolbar.editable.location,
                            "omnibar submit"
                        );
                        self.sync_content();
                        self.request_redraw();
                    } else if let Some(key_event) =
                        key_event_from_winit(&event.logical_key, self.modifiers)
                    {
                        self.runner.dispatch_key(key_event);
                        self.request_redraw();
                    }
                }
            },
            WindowEvent::RedrawRequested => self.render(),
            _ => {},
        }
    }
}

/// Lay out the chrome root and return the toolbar div's border-box height (px),
/// rounded up. The toolbar is a single flex row, so this is stable across the
/// available width/height. Falls back to [`FALLBACK_TOOLBAR_H`] if the div is
/// absent (e.g. an empty chrome tree).
fn measure_toolbar_height(dom: &ScriptedDom, w: u32, h: u32) -> u32 {
    let frags = fragments_from_scripted_dom(dom, CHROME_SHEET, w, h);
    first_element(dom, dom.document(), "div")
        .and_then(|div| frags.rect_of(div))
        .map(|layout| (layout.location.y + layout.size.height).ceil() as u32)
        .filter(|&measured| measured > 0)
        .unwrap_or(FALLBACK_TOOLBAR_H)
}

/// The first element with local tag `local` in pre-order under `id`.
fn first_element(dom: &ScriptedDom, id: NodeId, local: &str) -> Option<NodeId> {
    if dom.element_name(id).is_some_and(|q| q.local.as_ref() == local) {
        return Some(id);
    }
    dom.dom_children(id).find_map(|c| first_element(dom, c, local))
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
