// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! canvas-host bin: a thin winit shell over [`canvas_host::Canvas`] — the
//! reusable, window-agnostic graph field-canvas content-root. It owns the window
//! and the shared [`SurfaceHost`] present stack, maps winit events onto the
//! canvas's semantic input methods, and on each redraw rasterizes + composites
//! the scene the canvas produces. meerkat hosts the same `Canvas` as its
//! content-root (S1 of the modular integration plan); this bin keeps the canvas
//! launchable + testable on its own.
//!
//! Navigation (per the graph-canvas directive): wheel = pan, Ctrl+wheel =
//! cursor-anchored zoom, middle-drag = pan, all with inertia. Left-drag grabs +
//! pins the node under the cursor; a click selects it; a left-drag on empty space
//! marquee-selects; a click near an edge picks it; a bare empty click clears.
//! Space re-seeds the layout and replays the settle. Arrow keys nudge the
//! focused node, `v` holds it, and `r` releases that explicit hold. The thin
//! source-time rail at the bottom selects read-only journal prefixes: click a
//! tick or use Home/End and Page Up/Page Down. The window title names the
//! selected source point, because the rail deliberately stays glyph-free and
//! leaves text rendering to its host chrome. `CANVAS_CAPTURE_DIR` writes the
//! presented frame to `canvas_capture.png`; `CANVAS_SOURCE_TIME_CURSOR` selects
//! an initial compact source tick for a repeatable native receipt.

use std::sync::Arc;

use canvas::{Canvas, PointerButton, SourceTimeCanvas, SourceTimeSelection, WHEEL_PAN_SCALE};
use genet_winit_host::SurfaceHost;
use kernel::graph::{CapturedDelta, EdgeAssertion, GraphJournal, SemanticSubKind, Seq};
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};

/// The canvas host application: the reusable [`Canvas`] content-root plus the
/// window + present stack that drives it.
struct App {
    /// The mutable graph presentation and the disposable historical one behind
    /// the native source-time rail.
    source_canvas: SourceTimeCanvas<Seq>,
    /// Standalone authority only. Production hosts pass their own journal to
    /// the same `SourceTimeCanvas` boundary.
    source_journal: GraphJournal,
    /// Wakes the loop when the physics actor has a fresh layout snapshot ready.
    proxy: EventLoopProxy<()>,
    /// Last cursor position in physical px. winit's `MouseInput` carries no
    /// position, so the shell tracks it from `CursorMoved` to drive press/release.
    cursor: (f32, f32),
    window: Option<Arc<Window>>,
    host: Option<SurfaceHost>,
    width: u32,
    height: u32,
    /// `CANVAS_CAPTURE_DIR`: write the app's rasterized frame rather than an
    /// OS screenshot, which can lose to overlapping windows.
    capture_dir: Option<std::path::PathBuf>,
    /// Whether the graph collides with the backdrop scene bodies (toggled by `t`).
    nodes_tangible: bool,
}

impl App {
    fn new(proxy: EventLoopProxy<()>) -> Self {
        let source_journal = demo_journal();
        let mut canvas = Canvas::with_graph(source_journal.replay());
        canvas.reseed();
        Self {
            // The standalone bin uses a real journal so its source-time rail
            // exercises the exact root-Canvas adapter. Meerkat supplies its
            // session graph and authority journal instead.
            source_canvas: SourceTimeCanvas::new(canvas),
            source_journal,
            proxy,
            cursor: (0.0, 0.0),
            window: None,
            host: None,
            width: 1024,
            height: 600,
            capture_dir: std::env::var_os("CANVAS_CAPTURE_DIR").map(Into::into),
            nodes_tangible: false,
        }
    }

    /// Produce the canvas's frame at the current size, rasterize + composite it
    /// through the present stack, and chain another redraw if the canvas is still
    /// animating (settling / gliding / dragging).
    fn render(&mut self) {
        if self.host.is_none() {
            return;
        }
        let (w, h) = (self.width.max(1), self.height.max(1));
        let (mut scene, needs_redraw) = self.source_canvas.frame(w, h);
        self.draw_source_timeline(&mut scene, w, h);

        let host = self.host.as_ref().unwrap();
        let (tex, view) = host.rasterize(&scene, w, h, ColorLoad::Clear(wgpu::Color::WHITE));
        if let Some(dir) = &self.capture_dir {
            let rgba = host
                .core()
                .renderer()
                .wgpu_device
                .read_rgba8_texture(&tex, w, h);
            let path = dir.join("canvas_capture.png");
            if let Err(error) = std::fs::create_dir_all(dir).and_then(|_| {
                let file = std::fs::File::create(&path)?;
                let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
                writer
                    .write_image_data(&rgba)
                    .map_err(std::io::Error::other)
            }) {
                eprintln!("[canvas-host] capture failed: {error}");
            }
        }
        let Some(frame) = host.acquire() else { return };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
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

    /// Stable compact source ticks. The control uses journal sequence because
    /// graph deltas do not promise wall-clock timestamps.
    fn source_ticks(&self) -> Vec<Seq> {
        self.source_journal.sequence_ticks(64)
    }

    fn selected_source_index(&self, ticks: &[Seq]) -> usize {
        let selected = match self.source_canvas.selection() {
            SourceTimeSelection::Live => self.source_journal.live_cursor(),
            SourceTimeSelection::Historical(cursor) => cursor,
        };
        ticks
            .iter()
            .position(|tick| *tick == selected)
            .unwrap_or_else(|| ticks.len().saturating_sub(1))
    }

    /// Move the rail to a stable journal prefix. Its live endpoint explicitly
    /// drops the disposable snapshot rather than replaying it into truth.
    fn select_source_index(&mut self, index: usize) -> bool {
        let ticks = self.source_ticks();
        let Some(cursor) = ticks.get(index.min(ticks.len().saturating_sub(1))).copied() else {
            return false;
        };
        let changed = self.source_canvas.select(&self.source_journal, cursor);
        if changed {
            self.update_source_title();
            self.request_redraw();
        }
        changed
    }

    fn step_source(&mut self, delta: isize) -> bool {
        let ticks = self.source_ticks();
        if ticks.is_empty() {
            return false;
        }
        let current = self.selected_source_index(&ticks) as isize;
        let next = (current + delta).clamp(0, ticks.len().saturating_sub(1) as isize) as usize;
        self.select_source_index(next)
    }

    fn return_source_to_live(&mut self) {
        if self.source_canvas.is_historical() {
            self.source_canvas.return_to_live();
            self.update_source_title();
            self.request_redraw();
        }
    }

    /// The rail intentionally has no text glyphs. The host window title gives
    /// it an accessible, always-current label without baking chrome typography
    /// into Canvas's portable scene.
    fn update_source_title(&self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let source = match self.source_canvas.selection() {
            SourceTimeSelection::Live => {
                format!("live: event {}", self.source_journal.live_cursor().index())
            }
            SourceTimeSelection::Historical(cursor) => format!(
                "history: event {} of {}",
                cursor.index(),
                self.source_journal.live_cursor().index()
            ),
        };
        window.set_title(&format!(
            "Canvas — {source} · Home/End or Page Up/Page Down · drag the source rail"
        ));
    }

    /// The footer's visual geometry. It is native Canvas paint rather than a
    /// Cambium control: the root canvas host does not own a Cambium DOM, while
    /// Swatches do. Both still select the same prefix-source contract.
    fn draw_source_timeline(&self, scene: &mut Scene, width: u32, height: u32) {
        let ticks = self.source_ticks();
        if ticks.is_empty() {
            return;
        }
        let left = 32.0;
        let right = (width as f32 - 32.0).max(left + 1.0);
        let top = height.saturating_sub(54) as f32;
        let bottom = height.saturating_sub(12) as f32;
        let track_y = top + 23.0;
        let selected = self.selected_source_index(&ticks);
        let historical = self.source_canvas.is_historical();
        scene.push_rect(
            left - 12.0,
            top,
            right + 12.0,
            bottom,
            [0.03, 0.05, 0.09, 0.82],
        );
        scene.push_rect(
            left,
            track_y - 1.0,
            right,
            track_y + 1.0,
            [0.42, 0.57, 0.84, 0.78],
        );
        let denominator = ticks.len().saturating_sub(1).max(1) as f32;
        for (index, _) in ticks.iter().enumerate() {
            let x = left + (right - left) * index as f32 / denominator;
            let active = index == selected;
            let color = if active {
                if historical {
                    [0.96, 0.67, 0.24, 1.0]
                } else {
                    [0.42, 0.88, 0.62, 1.0]
                }
            } else {
                [0.65, 0.74, 0.86, 0.72]
            };
            let half = if active { 5.0 } else { 2.0 };
            scene.push_rect(x - half, track_y - half, x + half, track_y + half, color);
        }
    }

    fn source_timeline_index_at(&self, x: f32, y: f32) -> Option<usize> {
        let ticks = self.source_ticks();
        if ticks.is_empty() || y < self.height.saturating_sub(62) as f32 {
            return None;
        }
        let left = 32.0;
        let right = (self.width as f32 - 32.0).max(left + 1.0);
        if !(left - 12.0..=right + 12.0).contains(&x) {
            return None;
        }
        let normalized = ((x - left) / (right - left)).clamp(0.0, 1.0);
        Some((normalized * ticks.len().saturating_sub(1) as f32).round() as usize)
    }

    fn live_canvas_mut(&mut self) -> Option<&mut Canvas> {
        (!self.source_canvas.is_historical()).then(|| self.source_canvas.live_canvas_mut())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Canvas — graph field-canvas on genet")
            .with_inner_size(PhysicalSize::new(self.width, self.height));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create canvas window"),
        );
        let size = window.inner_size();
        self.width = size.width.max(1);
        self.height = size.height.max(1);
        self.source_canvas.resize(self.width, self.height);
        self.source_canvas.live_canvas_mut().recenter();

        let options = NetrenderOptions {
            tile_cache_size: Some(64),
            enable_vello: true,
            ..Default::default()
        };
        match SurfaceHost::boot(window.clone(), self.width, self.height, options) {
            Ok(host) => self.host = Some(host),
            Err(err) => {
                eprintln!("[canvas-host] {err}");
                event_loop.exit();
                return;
            }
        }
        // Always-offload physics (P6): move the simulation onto an armillary actor
        // thread, waking this loop through the proxy when a layout snapshot lands.
        // Done here (once — `resumed` early-returns if a window already exists) so
        // the settle animates from the first visible frame.
        // A small living backdrop: a few drifting, intangible scene bodies behind the
        // graph. Added before offload so they ride onto the physics actor with the rest
        // of the world; they drift while the layout settles. (Physics scenes P1.)
        self.source_canvas
            .live_canvas_mut()
            .add_scene_body((-320.0, -140.0), 46.0, (16.0, 10.0));
        self.source_canvas
            .live_canvas_mut()
            .add_scene_body((280.0, 130.0), 64.0, (-13.0, 15.0));
        self.source_canvas
            .live_canvas_mut()
            .add_scene_body((-140.0, 220.0), 30.0, (11.0, -17.0));
        self.source_canvas
            .live_canvas_mut()
            .add_scene_body((200.0, -210.0), 38.0, (-9.0, -12.0));

        let proxy = self.proxy.clone();
        let physics_wake: armillary::Wake = Arc::new(move || {
            let _ = proxy.send_event(());
        });
        self.source_canvas
            .live_canvas_mut()
            .offload_physics(physics_wake);

        self.window = Some(window);
        self.update_source_title();
        if let Some(index) = std::env::var("CANVAS_SOURCE_TIME_CURSOR")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
        {
            self.select_source_index(index);
        }
        self.request_redraw();
    }

    /// The physics actor woke us through the proxy: a fresh layout snapshot is
    /// waiting. Redraw so `frame()` folds it in (and chains on while settling).
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
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
                self.source_canvas.resize(self.width, self.height);
                self.request_redraw();
            }
            WindowEvent::ModifiersChanged(mods) => {
                if let Some(canvas) = self.live_canvas_mut() {
                    canvas.set_ctrl(mods.state().control_key());
                    // Alt gates the orbit drag (Alt+left-drag yaws/tilts the camera). (Iso orbit.)
                    canvas.set_alt(mods.state().alt_key());
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                let (x, y) = self.cursor;
                if self
                    .live_canvas_mut()
                    .is_some_and(|canvas| canvas.cursor_moved(x, y))
                {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * WHEEL_PAN_SCALE, y * WHEEL_PAN_SCALE),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                if self
                    .live_canvas_mut()
                    .is_some_and(|canvas| canvas.wheel(dx, dy))
                {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => Some(PointerButton::Left),
                    MouseButton::Middle => Some(PointerButton::Middle),
                    MouseButton::Right => Some(PointerButton::Right),
                    _ => None,
                };
                if let Some(button) = button {
                    let (x, y) = self.cursor;
                    if button == PointerButton::Left
                        && state == ElementState::Pressed
                        && let Some(index) = self.source_timeline_index_at(x, y)
                    {
                        self.select_source_index(index);
                        return;
                    }
                    let redraw = match state {
                        ElementState::Pressed => self
                            .live_canvas_mut()
                            .is_some_and(|canvas| canvas.pointer_down(button, x, y)),
                        ElementState::Released => self
                            .live_canvas_mut()
                            .is_some_and(|canvas| canvas.pointer_up(button, x, y)),
                    };
                    if redraw {
                        self.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    match &event.logical_key {
                        WinitKey::Named(WinitNamedKey::Home) => {
                            self.select_source_index(0);
                            return;
                        }
                        WinitKey::Named(WinitNamedKey::End)
                        | WinitKey::Named(WinitNamedKey::Escape) => {
                            self.return_source_to_live();
                            return;
                        }
                        WinitKey::Named(WinitNamedKey::PageUp) => {
                            self.step_source(-1);
                            return;
                        }
                        WinitKey::Named(WinitNamedKey::PageDown) => {
                            self.step_source(1);
                            return;
                        }
                        // This toggles a live presentation setting. Handle it
                        // before borrowing Canvas for the ordinary key map.
                        WinitKey::Character(s) if s.as_str() == "t" => {
                            if !self.source_canvas.is_historical() {
                                self.nodes_tangible = !self.nodes_tangible;
                                let tangible = self.nodes_tangible;
                                self.source_canvas
                                    .live_canvas_mut()
                                    .set_nodes_tangible(tangible);
                                self.request_redraw();
                            }
                            return;
                        }
                        _ => {}
                    }
                    // A historical Canvas is a source preview, not a branch.
                    // Only source-time controls above are accepted until the
                    // user returns to live, so ordinary input cannot mutate a
                    // retained presentation or its source graph.
                    let Some(canvas) = self.live_canvas_mut() else {
                        return;
                    };
                    match &event.logical_key {
                        // Keyboard curation mirrors pointer pull without putting
                        // geometry into graph truth. Arrows are a stable 24px at
                        // every zoom; `v` holds a node and `r` returns it to the
                        // solver.
                        WinitKey::Named(WinitNamedKey::ArrowLeft) => {
                            if canvas.nudge_focused_screen(-24.0, 0.0) {
                                self.request_redraw();
                            }
                        }
                        WinitKey::Named(WinitNamedKey::ArrowRight) => {
                            if canvas.nudge_focused_screen(24.0, 0.0) {
                                self.request_redraw();
                            }
                        }
                        WinitKey::Named(WinitNamedKey::ArrowUp) => {
                            if canvas.nudge_focused_screen(0.0, -24.0) {
                                self.request_redraw();
                            }
                        }
                        WinitKey::Named(WinitNamedKey::ArrowDown) => {
                            if canvas.nudge_focused_screen(0.0, 24.0) {
                                self.request_redraw();
                            }
                        }
                        WinitKey::Character(s) if s.as_str() == "v" => {
                            if canvas.pin_focused() {
                                self.request_redraw();
                            }
                        }
                        WinitKey::Character(s) if s.as_str() == "r" => {
                            if canvas.release_focused() {
                                self.request_redraw();
                            }
                        }
                        // Space re-seeds the central spiral and replays the settle.
                        WinitKey::Named(WinitNamedKey::Space) => {
                            if canvas.reseed() {
                                self.request_redraw();
                            }
                        }
                        // `i` toggles the isometric (2.5D foreshortened-ground) view.
                        WinitKey::Character(s) if s.as_str() == "i" => {
                            let on = !canvas.is_isometric();
                            canvas.set_isometric(on);
                            self.request_redraw();
                        }
                        // `q` / `e` orbit the view (yaw); pair with `i` for the 2.5D orbit.
                        WinitKey::Character(s) if s.as_str() == "q" => {
                            canvas.orbit_by(-0.15);
                            self.request_redraw();
                        }
                        WinitKey::Character(s) if s.as_str() == "e" => {
                            canvas.orbit_by(0.15);
                            self.request_redraw();
                        }
                        // `[` / `]` sweep the vertical foreshorten (tilt).
                        WinitKey::Character(s) if s.as_str() == "[" => {
                            canvas.set_tilt(canvas.tilt() - 0.05);
                            self.request_redraw();
                        }
                        WinitKey::Character(s) if s.as_str() == "]" => {
                            canvas.set_tilt(canvas.tilt() + 0.05);
                            self.request_redraw();
                        }
                        // `h` toggles height-by-degree: hubs float above the ground (P3).
                        WinitKey::Character(s) if s.as_str() == "h" => {
                            let on = !canvas.height_by_degree();
                            canvas.set_height_by_degree(on);
                            self.request_redraw();
                        }
                        // `t` toggles scene tangibility: the graph collides with the
                        // backdrop bodies vs passing through them (Physics scenes P2).
                        // `1`-`7` load the declarative scene catalog (drop bowl, pyramid, dominoes,
                        // Galton board, funnel, drift, rope chain); `8` the liquid pool; `9` the
                        // whirlpool force-field; `f` the fountain emitter; `0` clears back to bare
                        // space. Press `t` to make the graph tangible and stir the scenes. (Physics
                        // scenes P3/P4a/b/c + fields + emitters.)
                        WinitKey::Character(s) if s.as_str() == "1" => {
                            canvas.load_demo_scene();
                            self.request_redraw();
                        }
                        WinitKey::Character(s) if s.as_str() == "2" => {
                            canvas.load_scene(canvas::pyramid_scene());
                            self.request_redraw();
                        }
                        WinitKey::Character(s) if s.as_str() == "3" => {
                            canvas.load_scene(canvas::domino_scene());
                            self.request_redraw();
                        }
                        WinitKey::Character(s) if s.as_str() == "4" => {
                            canvas.load_scene(canvas::galton_scene());
                            self.request_redraw();
                        }
                        WinitKey::Character(s) if s.as_str() == "5" => {
                            canvas.load_scene(canvas::funnel_scene());
                            self.request_redraw();
                        }
                        WinitKey::Character(s) if s.as_str() == "6" => {
                            canvas.load_scene(canvas::drift_scene());
                            self.request_redraw();
                        }
                        WinitKey::Character(s) if s.as_str() == "7" => {
                            canvas.load_scene(canvas::chain_scene());
                            self.request_redraw();
                        }
                        // `8` loads the demo liquid pool (PBF fluid). (Physics scenes P4c.)
                        WinitKey::Character(s) if s.as_str() == "8" => {
                            canvas.load_demo_fluid();
                            self.request_redraw();
                        }
                        // `9` loads the whirlpool: a vortex force-field swirling loose balls. (Fields.)
                        WinitKey::Character(s) if s.as_str() == "9" => {
                            canvas.load_whirlpool();
                            self.request_redraw();
                        }
                        // `f` loads the fountain: an emitter spraying droplets up into a basin. (Emitters.)
                        WinitKey::Character(s) if s.as_str() == "f" => {
                            canvas.load_fountain();
                            self.request_redraw();
                        }
                        // `c` loads Newton's cradle: elastic revolute pendulums clicking momentum across. (P4b.)
                        WinitKey::Character(s) if s.as_str() == "c" => {
                            canvas.load_scene(canvas::cradle_scene());
                            self.request_redraw();
                        }
                        // `b` loads the plank bridge: a revolute-hinge span sagging under dropped weights. (P4b.)
                        WinitKey::Character(s) if s.as_str() == "b" => {
                            canvas.load_scene(canvas::bridge_scene());
                            self.request_redraw();
                        }
                        // `k` loads the wrecking ball: a heavy rope chain swinging through a block tower. (P4b.)
                        WinitKey::Character(s) if s.as_str() == "k" => {
                            canvas.load_scene(canvas::ball_and_chain_scene());
                            self.request_redraw();
                        }
                        // `m` loads the mixer: a motorised revolute paddle stirring loose balls. (P4b.)
                        WinitKey::Character(s) if s.as_str() == "m" => {
                            canvas.load_scene(canvas::mixer_scene());
                            self.request_redraw();
                        }
                        // `x` demos textured scene props: register a procedural crate texture, then
                        // drop a stack of crates wearing it. (Scene-prop sprites.)
                        WinitKey::Character(s) if s.as_str() == "x" => {
                            let (rgba, cw, ch) = crate_texture();
                            canvas.register_scene_sprite("crate", rgba, cw, ch);
                            canvas.load_scene(crate_drop_scene());
                            self.request_redraw();
                        }
                        // `g` loads the Game of Life ambient backdrop (a non-rapier sim behind the
                        // graph). (Physics scenes P5.)
                        WinitKey::Character(s) if s.as_str() == "g" => {
                            canvas.load_game_of_life();
                            self.request_redraw();
                        }
                        // `n` loads the n-body drift ambient backdrop (an orbiting, clumping cloud). (P5.)
                        WinitKey::Character(s) if s.as_str() == "n" => {
                            canvas.load_nbody();
                            self.request_redraw();
                        }
                        // `p` loads the particle-life ambient backdrop (species self-organising). (P5.)
                        WinitKey::Character(s) if s.as_str() == "p" => {
                            canvas.load_particle_life();
                            self.request_redraw();
                        }
                        // `s` loads the falling-sand ambient backdrop (grains pour + pile + drain). (P5.)
                        WinitKey::Character(s) if s.as_str() == "s" => {
                            canvas.load_sand();
                            self.request_redraw();
                        }
                        WinitKey::Character(s) if s.as_str() == "0" => {
                            canvas.clear_scene();
                            canvas.clear_fluid();
                            canvas.clear_ambient();
                            self.request_redraw();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }
}

/// A chronological standalone authority: nodes first, then the relations that
/// make their field. The host is deliberately a consumer of `GraphJournal`,
/// rather than fabricating a list of display scenes, so every rail point is a
/// replayable source prefix.
fn demo_journal() -> GraphJournal {
    let mut journal = GraphJournal::new();
    let ids: Vec<_> = (0..12)
        .map(|index| uuid::Uuid::from_u128(index as u128 + 1))
        .collect();
    for (index, id) in ids.iter().enumerate() {
        journal.record(CapturedDelta::ReplayAddNodeWithIdIfMissing {
            id: id.to_string(),
            url: format!("mere://demo/{index}"),
            position: [0.0, 0.0],
        });
    }
    let hyperlink = || EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Hyperlink,
        label: None,
        decay_progress: None,
    };
    for index in 0..ids.len() {
        journal.record(CapturedDelta::ReplayAssertRelationByIds {
            from_id: ids[index].to_string(),
            to_id: ids[(index + 1) % ids.len()].to_string(),
            assertion: hyperlink(),
        });
    }
    for index in (2..ids.len()).step_by(3) {
        journal.record(CapturedDelta::ReplayAssertRelationByIds {
            from_id: ids[0].to_string(),
            to_id: ids[index].to_string(),
            assertion: hyperlink(),
        });
    }
    journal
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
fn crate_drop_scene() -> seiche::SceneSpec {
    use seiche::{NodeCollider, SceneBodySpec, SceneSpec};
    let mut bodies = vec![
        SceneBodySpec::fixed(NodeCollider::Square { half: 300.0 }, (0.0, 360.0)).restitution(0.0),
    ];
    for row in 0..3 {
        for col in 0..5 {
            // Stagger x + start each crate at a varied tilt so they topple into a messy pile - the
            // oriented sprite then shows each crate's true rotation. (Iso billboard demo.)
            let x = (col as f32 - 2.0) * 64.0 + ((row * 5 + col) as f32 * 1.3).sin() * 18.0;
            let y = -120.0 - row as f32 * 80.0;
            let tilt = ((row * 5 + col) as f32 * 0.9).sin() * 0.6;
            bodies.push(
                SceneBodySpec::dynamic(NodeCollider::Square { half: 30.0 }, (x, y))
                    .restitution(0.1)
                    .rotation(tilt)
                    .sprite("crate"),
            );
        }
    }
    SceneSpec {
        bodies,
        gravity: (0.0, 520.0),
        default_tangible: false,
        perpetual: false,
        joints: Vec::new(),
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("canvas_host=info")),
        )
        .init();
    tracing::info!("canvas-host starting");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    event_loop.run_app(&mut app).expect("event loop error");
}
