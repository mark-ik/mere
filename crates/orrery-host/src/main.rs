/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! orrery-host: the on-screen serval host for the orrery (the graph
//! field-canvas), build item 1D of the serval-as-host flip.
//!
//! ## Layers (the plan's three, built incrementally)
//!
//! 1. **scene-paint underlay** — [`platen::orrery`] turns the graph into one
//!    `CanvasPaintList` (edges + node rects + visual-coupling overlays) under a
//!    single camera transform. Composited to a `netrender::Scene` via
//!    [`paint_list_render::composite_paint_layers`] and presented through the
//!    shared [`SurfaceHost`]. (1D.1.)
//! 2. **live gyre positions** — a [`gyre::Simulation`] (seeded from the graph,
//!    forces = exclusion + edge-springs + a centering boundary) ticks each frame;
//!    [`orrery_paint_list_from_positions`] reprojects the underlay from the live
//!    bodies, so the layout settles on screen. **This slice (1D.2).**
//! 3. **abs-pos serval DOM node children** — one `<div>` per node, absolutely
//!    positioned inside a camera-transformed container `<div>`, moved to its
//!    world position by an inline transform; lowered to a `ServalPaintList` by
//!    [`paint_list_from_scripted_dom`] and composited *over* the underlay (so
//!    rich labelled nodes sit above the edges). **This slice (1D.3a)** — the full
//!    document built each frame; the cull/demote split (3b) and the
//!    pre-materialized pool + incremental layout (3c) refine it.
//!
//! Navigation (wheel=pan / ctrl+wheel=zoom / inertia) and the two-hit-test split
//! are 1E. Space re-seeds the layout (a tight central spiral) and re-runs the
//! settle, so the force-directed motion is replayable.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use euclid::default::Point2D;
use gyre::{Boundary, EdgeSpring, NodeExclusion, Simulation};
use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeAssertion, Graph, NodeKey, SemanticSubKind};
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions};
use paint_list_api::{DeviceIntSize, PaintList};
use paint_list_render::{composite_paint_layers, CompositeLayer};
use pelt_live::paint_list_from_scripted_dom;
use platen::orrery::orrery_paint_list_from_positions;
use platen::scene_paint::{Camera, ScenePaintStyle};
use serval_layout::ScrollOffsets;
use serval_scripted_dom::{NodeId as DomNodeId, ScriptedDom};
use serval_winit_host::SurfaceHost;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};

/// Force-directed settle length (frames) after a (re)seed, ~6s at 60fps.
const SETTLE_TICKS: u32 = 360;
/// Per-tick timestep handed to the gyre simulation.
const TICK_DT: f32 = 1.0 / 60.0;

/// Node box half-extent (px) — matches the underlay's default node rect, so each
/// DOM child sits centered on the same world position.
const NODE_HALF: f32 = 18.0;

/// Author CSS for the node-children document. `.stage` is the camera-transformed
/// container (also `position: relative`, so it is the containing block for the
/// abs-pos nodes); each `.gnode` is an absolutely-positioned labelled box moved
/// to its world position by an inline transform (serval propagates `.stage`'s
/// camera transform onto these abs-pos descendants — the 1A fix).
const NODE_SHEET: &[&str] = &[
    "div { display: block; }",
    ".stage { position: relative; }",
    ".gnode { position: absolute; left: 0; top: 0; width: 36px; height: 36px; \
        background-color: rgb(54, 92, 156); color: rgb(245, 247, 252); font-size: 15px; }",
];

/// The orrery host application: the graph, its physics, the camera, and the
/// present stack.
struct App {
    graph: Graph,
    /// The force-directed layout. The underlay reprojects from its live bodies.
    sim: Simulation,
    /// Remaining settle frames; while > 0 the sim ticks and redraws are chained.
    ticks_remaining: u32,
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
        let graph = sample_graph();
        let sim = build_simulation(&graph);
        Self {
            graph,
            sim,
            ticks_remaining: SETTLE_TICKS,
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

    /// Tick the layout (while settling), reproject the underlay from the live
    /// gyre positions, composite, and present. Chains another redraw while the
    /// settle is still running.
    fn render(&mut self) {
        if self.host.is_none() {
            return;
        }
        let (w, h) = (self.width.max(1), self.height.max(1));
        let viewport = DeviceIntSize::new(w as i32, h as i32);

        let animating = self.ticks_remaining > 0;
        if animating {
            self.sim.tick(TICK_DT);
            self.ticks_remaining -= 1;
            self.generation = self.generation.wrapping_add(1);
        }

        // Snapshot the live positions, then reproject the underlay from them
        // (a node with no body falls back to its committed position inside the
        // producer).
        let positions: HashMap<NodeKey, PortablePoint> = self
            .sim
            .positions()
            .map(|(k, p)| (k, PortablePoint::new(p.x, p.y)))
            .collect();
        let underlay = orrery_paint_list_from_positions(
            &self.graph,
            |k| positions.get(&k).copied(),
            viewport,
            self.camera,
            &self.style,
            self.generation,
        );

        // The node-children document: abs-pos labelled boxes under the camera
        // transform, lowered to a ServalPaintList and composited over the underlay.
        let nodes_dom = build_node_children_dom(&self.graph, &positions, self.camera);
        let nodes_plist = paint_list_from_scripted_dom(
            &nodes_dom,
            NODE_SHEET,
            w,
            h,
            None,
            &ScrollOffsets::<DomNodeId>::default(),
        );

        let layers = [
            CompositeLayer::commands_only(underlay.commands()),
            CompositeLayer {
                commands: nodes_plist.commands(),
                fonts: nodes_plist.fonts(),
                images: nodes_plist.images(),
            },
        ];
        let scene = composite_paint_layers(viewport, &layers).scene;

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

        if animating {
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
            WindowEvent::KeyboardInput { event, .. } => {
                // Space re-seeds the central spiral and replays the settle.
                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, WinitKey::Named(WinitNamedKey::Space))
                {
                    seed_cluster(&mut self.sim, &self.graph);
                    self.ticks_remaining = SETTLE_TICKS;
                    self.request_redraw();
                }
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

/// Build the force-directed simulation from `graph`: a body per node, the
/// undirected de-duplicated relation pairs as the spring topology, the standard
/// force trio (exclusion + edge-springs + a centering boundary), seeded into a
/// tight central spiral so the first settle is visible.
fn build_simulation(graph: &Graph) -> Simulation {
    let mut sim = Simulation::new();
    sim.sync_with_graph(graph);

    let mut seen = HashSet::new();
    let edges: Vec<(NodeKey, NodeKey)> = graph
        .relations()
        .filter_map(|r| {
            let pair = if r.from <= r.to { (r.from, r.to) } else { (r.to, r.from) };
            seen.insert(pair).then_some((r.from, r.to))
        })
        .collect();
    sim.sync_edges(edges);

    sim.add_force(NodeExclusion::default());
    sim.add_force(EdgeSpring::default());
    sim.add_force(Boundary::default());

    seed_cluster(&mut sim, graph);
    sim
}

/// Seed every node into a tight central spiral (golden-angle), so a ticked
/// settle visibly expands it into a readable layout (vs. starting from the
/// already-spread committed ring).
fn seed_cluster(sim: &mut Simulation, graph: &Graph) {
    let seeds: Vec<(NodeKey, Point2D<f32>)> = graph
        .nodes()
        .enumerate()
        .map(|(i, (key, _node))| {
            let r = 6.0 + i as f32 * 3.0;
            let theta = i as f32 * 2.399_963; // golden angle in radians
            (key, Point2D::new(r * theta.cos(), r * theta.sin()))
        })
        .collect();
    sim.seed_positions(seeds);
}

/// Build the node-children document: a camera-transformed `.stage` container with
/// one absolutely-positioned `.gnode` per graph node, each carrying its index as
/// a label and moved (via an inline transform) to its world position so it sits
/// centered on the underlay's node rect. Rebuilt each frame in this slice (1D.3a);
/// a pre-materialized pool replaces the rebuild in 1D.3c.
fn build_node_children_dom(
    graph: &Graph,
    positions: &HashMap<NodeKey, PortablePoint>,
    camera: Camera,
) -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();

    let stage = dom.create_element(qual("div"));
    dom.set_attribute(stage, qual("class"), "stage");
    dom.set_attribute(
        stage,
        qual("style"),
        &format!(
            "transform: translate({}px, {}px) scale({});",
            camera.offset.0, camera.offset.1, camera.zoom
        ),
    );
    dom.append_child(root, stage);

    for (i, (key, _node)) in graph.nodes().enumerate() {
        let pos = positions.get(&key).copied().unwrap_or_default();
        let gnode = dom.create_element(qual("div"));
        dom.set_attribute(gnode, qual("class"), "gnode");
        dom.set_attribute(
            gnode,
            qual("style"),
            &format!(
                "transform: translate({}px, {}px);",
                pos.x - NODE_HALF,
                pos.y - NODE_HALF
            ),
        );
        let label = dom.create_text(&i.to_string());
        dom.append_child(gnode, label);
        dom.append_child(stage, gnode);
    }
    dom
}

/// A `QualName` in the null namespace (the shape `ScriptedDom` element / attribute
/// builders take).
fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_graph_has_nodes_and_edges() {
        let g = sample_graph();
        assert_eq!(g.nodes().count(), 12, "the ring has twelve nodes");
        assert!(g.relations().count() >= 12, "at least the ring edges");
    }

    #[test]
    fn simulation_has_a_body_per_node_and_the_edge_topology() {
        let g = sample_graph();
        let sim = build_simulation(&g);
        assert_eq!(sim.body_count(), 12, "one physics body per node");
        assert!(sim.edge_count() >= 12, "the spring topology carries the edges");
    }

    #[test]
    fn ticking_moves_nodes_from_the_seed() {
        // From the tight central spiral, the force trio must spread the layout:
        // after a short settle at least one node has moved meaningfully.
        let g = sample_graph();
        let mut sim = build_simulation(&g);
        let before: Vec<(NodeKey, Point2D<f32>)> = sim.positions().collect();
        for _ in 0..60 {
            sim.tick(TICK_DT);
        }
        let after: HashMap<NodeKey, Point2D<f32>> = sim.positions().collect();
        let moved = before.iter().any(|(k, p0)| {
            after
                .get(k)
                .is_some_and(|p1| (p1.x - p0.x).hypot(p1.y - p0.y) > 1.0)
        });
        assert!(moved, "the force-directed settle moves nodes off the seed");
    }
}
