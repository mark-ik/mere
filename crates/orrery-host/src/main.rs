/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! orrery-host: the on-screen serval host for the orrery (the graph
//! field-canvas), build item 1D of the serval-as-host flip.
//!
//! ## Layers (the plan's three, built incrementally)
//!
//! 1. **scene-paint underlay** — [`platen::orrery::orrery_paint_list`] turns the
//!    graph into one `CanvasPaintList` (edges + node rects + visual-coupling
//!    overlays) under a single camera transform. Composited to a `netrender::Scene`
//!    via [`paint_list_render::composite_paint_layers`] and presented through the
//!    shared [`SurfaceHost`]. **This slice (1D.1).**
//! 2. live gyre positions driving the underlay (1D.2).
//! 3. abs-pos serval DOM node children under the camera transform, pooled +
//!    culled, moved by per-frame inline transforms (1D.3).
//!
//! Navigation (wheel=pan / ctrl+wheel=zoom / inertia) and the two-hit-test split
//! are 1E. This slice renders the graph statically, centered in the viewport.

use std::sync::Arc;

use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeAssertion, Graph, SemanticSubKind};
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions};
use paint_list_api::{DeviceIntSize, PaintList};
use paint_list_render::{composite_paint_layers, CompositeLayer};
use platen::orrery::orrery_paint_list;
use platen::scene_paint::{Camera, ScenePaintStyle};
use serval_winit_host::SurfaceHost;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// The orrery host application: the graph, the camera, and the present stack.
struct App {
    graph: Graph,
    camera: Camera,
    style: ScenePaintStyle,
    /// Producer generation, bumped when the scene's semantic content changes.
    generation: u64,
    window: Option<Arc<Window>>,
    host: Option<SurfaceHost>,
    width: u32,
    height: u32,
}

impl App {
    fn new() -> Self {
        Self {
            graph: sample_graph(),
            camera: Camera::default(),
            style: ScenePaintStyle::default(),
            generation: 0,
            window: None,
            host: None,
            width: 1024,
            height: 600,
        }
    }

    /// Put the world origin at the viewport center — the sample graph is laid out
    /// around `(0, 0)`, so this frames it at zoom 1. (A fit-to-`content_bounds`
    /// camera replaces this when navigation lands in 1E.)
    fn recenter(&mut self) {
        self.camera.offset = (self.width as f32 / 2.0, self.height as f32 / 2.0);
    }

    /// Build the underlay, composite it into one scene, and present it.
    fn render(&mut self) {
        let Some(host) = self.host.as_ref() else { return };
        let (w, h) = (self.width.max(1), self.height.max(1));
        let viewport = DeviceIntSize::new(w as i32, h as i32);

        let underlay =
            orrery_paint_list(&self.graph, viewport, self.camera, &self.style, self.generation);
        let layers = [CompositeLayer::commands_only(underlay.commands())];
        let scene = composite_paint_layers(viewport, &layers).scene;

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
        self.recenter();

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
        window.request_redraw();
        self.window = Some(window);
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
                self.recenter();
                self.request_redraw();
            },
            WindowEvent::RedrawRequested => self.render(),
            _ => {},
        }
    }
}

/// A small sample graph: a ring of nodes around the origin with ring edges plus a
/// few hub spokes, so the underlay has both edges and nodes to draw. (Live gyre
/// positions replace the static ring in 1D.2.)
fn sample_graph() -> Graph {
    let mut graph = Graph::new();
    let count = 12usize;
    let radius = 220.0_f32;
    let mut keys = Vec::with_capacity(count);
    for i in 0..count {
        let theta = (i as f32) / (count as f32) * std::f32::consts::TAU;
        let pos = PortablePoint::new(radius * theta.cos(), radius * theta.sin());
        let key =
            graph.add_node_with_id(uuid::Uuid::from_u128(i as u128 + 1), format!("mere://node/{i}"), pos);
        graph.set_node_position(key, pos);
        keys.push(key);
    }
    // Ring edges around the cycle.
    for i in 0..count {
        let _ = graph.assert_relation(keys[i], keys[(i + 1) % count], hyperlink());
    }
    // A few spokes from node 0 across the ring.
    for i in (2..count).step_by(3) {
        let _ = graph.assert_relation(keys[0], keys[i], hyperlink());
    }
    graph
}

/// A plain hyperlink relation (the orrery draws one undirected line per pair).
fn hyperlink() -> EdgeAssertion {
    EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Hyperlink,
        label: None,
        decay_progress: None,
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
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("event loop error");
}
