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
    /// Whether the graph collides with the backdrop scene bodies (toggled by `t`).
    nodes_tangible: bool,
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
            nodes_tangible: false,
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
        // A small living backdrop: a few drifting, intangible scene bodies behind the
        // graph. Added before offload so they ride onto the physics actor with the rest
        // of the world; they drift while the layout settles. (Physics scenes P1.)
        self.orrery.add_scene_body((-320.0, -140.0), 46.0, (16.0, 10.0));
        self.orrery.add_scene_body((280.0, 130.0), 64.0, (-13.0, 15.0));
        self.orrery.add_scene_body((-140.0, 220.0), 30.0, (11.0, -17.0));
        self.orrery.add_scene_body((200.0, -210.0), 38.0, (-9.0, -12.0));

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
                if event.state == ElementState::Pressed {
                    match &event.logical_key {
                        // Space re-seeds the central spiral and replays the settle.
                        WinitKey::Named(WinitNamedKey::Space) => {
                            if self.orrery.reseed() {
                                self.request_redraw();
                            }
                        },
                        // `i` toggles the isometric (2.5D foreshortened-ground) view.
                        WinitKey::Character(s) if s.as_str() == "i" => {
                            let on = !self.orrery.is_isometric();
                            self.orrery.set_isometric(on);
                            self.request_redraw();
                        },
                        // `q` / `e` orbit the view (yaw); pair with `i` for the 2.5D orbit.
                        WinitKey::Character(s) if s.as_str() == "q" => {
                            self.orrery.orbit_by(-0.15);
                            self.request_redraw();
                        },
                        WinitKey::Character(s) if s.as_str() == "e" => {
                            self.orrery.orbit_by(0.15);
                            self.request_redraw();
                        },
                        // `[` / `]` sweep the vertical foreshorten (tilt).
                        WinitKey::Character(s) if s.as_str() == "[" => {
                            self.orrery.set_tilt(self.orrery.tilt() - 0.05);
                            self.request_redraw();
                        },
                        WinitKey::Character(s) if s.as_str() == "]" => {
                            self.orrery.set_tilt(self.orrery.tilt() + 0.05);
                            self.request_redraw();
                        },
                        // `h` toggles height-by-degree: hubs float above the ground (P3).
                        WinitKey::Character(s) if s.as_str() == "h" => {
                            let on = !self.orrery.height_by_degree();
                            self.orrery.set_height_by_degree(on);
                            self.request_redraw();
                        },
                        // `t` toggles scene tangibility: the graph collides with the
                        // backdrop bodies vs passing through them (Physics scenes P2).
                        WinitKey::Character(s) if s.as_str() == "t" => {
                            self.nodes_tangible = !self.nodes_tangible;
                            self.orrery.set_nodes_tangible(self.nodes_tangible);
                            self.request_redraw();
                        },
                        // `1`-`7` load the declarative scene catalog (drop bowl, pyramid, dominoes,
                        // Galton board, funnel, drift, rope chain); `8` the liquid pool; `9` the
                        // whirlpool force-field; `f` the fountain emitter; `0` clears back to bare
                        // space. Press `t` to make the graph tangible and stir the scenes. (Physics
                        // scenes P3/P4a/b/c + fields + emitters.)
                        WinitKey::Character(s) if s.as_str() == "1" => {
                            self.orrery.load_demo_scene();
                            self.request_redraw();
                        },
                        WinitKey::Character(s) if s.as_str() == "2" => {
                            self.orrery.load_scene(orrery::pyramid_scene());
                            self.request_redraw();
                        },
                        WinitKey::Character(s) if s.as_str() == "3" => {
                            self.orrery.load_scene(orrery::domino_scene());
                            self.request_redraw();
                        },
                        WinitKey::Character(s) if s.as_str() == "4" => {
                            self.orrery.load_scene(orrery::galton_scene());
                            self.request_redraw();
                        },
                        WinitKey::Character(s) if s.as_str() == "5" => {
                            self.orrery.load_scene(orrery::funnel_scene());
                            self.request_redraw();
                        },
                        WinitKey::Character(s) if s.as_str() == "6" => {
                            self.orrery.load_scene(orrery::drift_scene());
                            self.request_redraw();
                        },
                        WinitKey::Character(s) if s.as_str() == "7" => {
                            self.orrery.load_scene(orrery::chain_scene());
                            self.request_redraw();
                        },
                        // `8` loads the demo liquid pool (PBF fluid). (Physics scenes P4c.)
                        WinitKey::Character(s) if s.as_str() == "8" => {
                            self.orrery.load_demo_fluid();
                            self.request_redraw();
                        },
                        // `9` loads the whirlpool: a vortex force-field swirling loose balls. (Fields.)
                        WinitKey::Character(s) if s.as_str() == "9" => {
                            self.orrery.load_whirlpool();
                            self.request_redraw();
                        },
                        // `f` loads the fountain: an emitter spraying droplets up into a basin. (Emitters.)
                        WinitKey::Character(s) if s.as_str() == "f" => {
                            self.orrery.load_fountain();
                            self.request_redraw();
                        },
                        // `c` loads Newton's cradle: elastic revolute pendulums clicking momentum across. (P4b.)
                        WinitKey::Character(s) if s.as_str() == "c" => {
                            self.orrery.load_scene(orrery::cradle_scene());
                            self.request_redraw();
                        },
                        // `b` loads the plank bridge: a revolute-hinge span sagging under dropped weights. (P4b.)
                        WinitKey::Character(s) if s.as_str() == "b" => {
                            self.orrery.load_scene(orrery::bridge_scene());
                            self.request_redraw();
                        },
                        // `k` loads the wrecking ball: a heavy rope chain swinging through a block tower. (P4b.)
                        WinitKey::Character(s) if s.as_str() == "k" => {
                            self.orrery.load_scene(orrery::ball_and_chain_scene());
                            self.request_redraw();
                        },
                        // `m` loads the mixer: a motorised revolute paddle stirring loose balls. (P4b.)
                        WinitKey::Character(s) if s.as_str() == "m" => {
                            self.orrery.load_scene(orrery::mixer_scene());
                            self.request_redraw();
                        },
                        // `x` demos textured scene props: register a procedural crate texture, then
                        // drop a stack of crates wearing it. (Scene-prop sprites.)
                        WinitKey::Character(s) if s.as_str() == "x" => {
                            let (rgba, cw, ch) = crate_texture();
                            self.orrery.register_scene_sprite("crate", rgba, cw, ch);
                            self.orrery.load_scene(crate_drop_scene());
                            self.request_redraw();
                        },
                        // `g` loads the Game of Life ambient backdrop (a non-rapier sim behind the
                        // graph). (Physics scenes P5.)
                        WinitKey::Character(s) if s.as_str() == "g" => {
                            self.orrery.load_game_of_life();
                            self.request_redraw();
                        },
                        // `n` loads the n-body drift ambient backdrop (an orbiting, clumping cloud). (P5.)
                        WinitKey::Character(s) if s.as_str() == "n" => {
                            self.orrery.load_nbody();
                            self.request_redraw();
                        },
                        // `p` loads the particle-life ambient backdrop (species self-organising). (P5.)
                        WinitKey::Character(s) if s.as_str() == "p" => {
                            self.orrery.load_particle_life();
                            self.request_redraw();
                        },
                        WinitKey::Character(s) if s.as_str() == "0" => {
                            self.orrery.clear_scene();
                            self.orrery.clear_fluid();
                            self.orrery.clear_ambient();
                            self.request_redraw();
                        },
                        _ => {},
                    }
                }
            },
            WindowEvent::RedrawRequested => self.render(),
            _ => {},
        }
    }
}

/// A small procedural crate texture (32x32 RGBA8): wood-brown fill with a darker border and an X of
/// slats, so a textured scene prop reads as a crate. The scene-prop sprite demo (`x` key). (Scene-
/// prop sprites.)
fn crate_texture() -> (Vec<u8>, u32, u32) {
    const N: u32 = 32;
    let wood = [165u8, 115, 60, 255];
    let dark = [95u8, 62, 30, 255];
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    for y in 0..N {
        for x in 0..N {
            let border = x < 2 || y < 2 || x >= N - 2 || y >= N - 2;
            let on_diag =
                (x as i32 - y as i32).abs() < 2 || ((x + y) as i32 - (N as i32 - 1)).abs() < 2;
            let px = if border || on_diag { dark } else { wood };
            let i = ((y * N + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&px);
        }
    }
    (rgba, N, N)
}

/// A demo scene of textured crates: a fixed floor with three rows of square crate props (each
/// wearing the registered `crate` sprite) dropped onto it. (Scene-prop sprites.)
fn crate_drop_scene() -> gyre::SceneSpec {
    use gyre::{NodeCollider, SceneBodySpec, SceneSpec};
    let mut bodies =
        vec![SceneBodySpec::fixed(NodeCollider::Square { half: 300.0 }, (0.0, 360.0)).restitution(0.0)];
    for row in 0..3 {
        for col in 0..5 {
            let x = (col as f32 - 2.0) * 70.0;
            let y = -120.0 - row as f32 * 70.0;
            bodies.push(
                SceneBodySpec::dynamic(NodeCollider::Square { half: 30.0 }, (x, y))
                    .restitution(0.1)
                    .sprite("crate"),
            );
        }
    }
    SceneSpec { bodies, gravity: (0.0, 520.0), default_tangible: false, perpetual: false, joints: Vec::new() }
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
