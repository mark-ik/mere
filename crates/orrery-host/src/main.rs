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
//! 3. **abs-pos serval DOM node children** — one `<div>` per **on-screen** node,
//!    absolutely positioned inside a camera-transformed container `<div>`, moved
//!    to its world position by an inline transform; lowered to a `ServalPaintList`
//!    by [`paint_list_from_scripted_dom`] and composited *over* the underlay.
//!    Off-screen nodes (gyre `cull_aabb` against the world viewport) **demote** to
//!    the underlay as plain rects via [`orrery_paint_list_demoted`], so the two
//!    layers never double-draw a node (3a + 3b). The pre-materialized pool +
//!    incremental layout (3c) replaces the per-frame rebuild next.
//!
//! Navigation is wired (1E.1): wheel = pan, Ctrl+wheel = cursor-anchored zoom,
//! middle-drag = pan, all with inertia — an infinite-canvas document, per the
//! graph-canvas navigation directive. Zooming out makes the 3b demote visible
//! (nodes leaving the viewport reappear as underlay rects). Left-drag grabs the
//! node under the cursor and pins it to the pointer (gyre pin/unpin) while the
//! sim ticks, so neighbors follow through the springs; a press that stays within
//! the slop is a click (node activation). The node/edge/marquee hit-test split
//! is 1E.3. Space re-seeds the layout (a tight central spiral) and replays the
//! settle.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use euclid::default::{Box2D, Point2D};
use gyre::{Boundary, EdgeSpring, NodeExclusion, Simulation};
use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeAssertion, Graph, NodeKey, SemanticSubKind};
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions};
use paint_list_api::{DeviceIntSize, PaintList};
use paint_list_render::{composite_paint_layers, CompositeLayer};
use pelt_live::paint_list_from_scripted_dom;
use platen::orrery::orrery_paint_list_demoted;
use platen::scene_paint::{Camera, ScenePaintStyle};
use serval_layout::ScrollOffsets;
use serval_scripted_dom::{NodeId as DomNodeId, ScriptedDom};
use serval_winit_host::SurfaceHost;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};

/// Force-directed settle length (frames) after a (re)seed, ~6s at 60fps.
const SETTLE_TICKS: u32 = 360;
/// Per-tick timestep handed to the gyre simulation.
const TICK_DT: f32 = 1.0 / 60.0;
/// Pixels per wheel line-notch (LineDelta → device px).
const WHEEL_PAN_SCALE: f32 = 40.0;
/// Zoom multiplier per wheel notch under Ctrl.
const ZOOM_STEP: f32 = 1.15;
/// Pan-inertia decay per frame (lower = stops sooner).
const PAN_DECAY: f32 = 0.85;
/// Clamp for the camera zoom.
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
/// Screen-px the pointer may move before a press counts as a drag, not a click.
const CLICK_SLOP: f32 = 4.0;

/// An in-progress left-button interaction on a node: a click until the pointer
/// passes [`CLICK_SLOP`], then a drag that pins the node to the cursor.
#[derive(Clone, Copy)]
struct Drag {
    node: NodeKey,
    /// Press position in screen px (the click/drag-slop origin).
    press: (f32, f32),
    /// Set once the pointer has moved past the slop — a real drag.
    moved: bool,
}

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
    /// Producer generation, bumped each rendered frame (positions / camera move).
    generation: u64,
    /// Last cursor position in physical px (zoom anchor + drag origin).
    cursor: (f32, f32),
    /// Inertial pan velocity (px/frame); decays each frame when not dragging.
    pan_velocity: (f32, f32),
    /// `Some(last_cursor)` while a middle-button pan drag is in progress.
    middle_drag: Option<(f32, f32)>,
    /// An in-progress left-button node click/drag, if any.
    drag: Option<Drag>,
    /// Whether Ctrl is held (gates wheel-zoom vs wheel-pan).
    ctrl: bool,
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
            cursor: (0.0, 0.0),
            pan_velocity: (0.0, 0.0),
            middle_drag: None,
            drag: None,
            ctrl: false,
            window: None,
            host: None,
            width: 1024,
            height: 600,
        }
    }

    /// Zoom by `factor`, keeping the world point under `anchor` (screen px) fixed.
    fn zoom_at(&mut self, anchor: (f32, f32), factor: f32) {
        let new_zoom = (self.camera.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let applied = new_zoom / self.camera.zoom;
        self.camera.offset.0 = anchor.0 - (anchor.0 - self.camera.offset.0) * applied;
        self.camera.offset.1 = anchor.1 - (anchor.1 - self.camera.offset.1) * applied;
        self.camera.zoom = new_zoom;
    }

    /// Put the world origin at the viewport center — the sample graph is laid out
    /// around `(0, 0)`, so this frames it at zoom 1. (A fit-to-`content_bounds`
    /// camera replaces this when navigation lands in 1E.)
    fn recenter(&mut self) {
        self.camera.offset = (self.width as f32 / 2.0, self.height as f32 / 2.0);
    }

    /// Map a screen-px point back to world space through the camera
    /// (`world = (screen - offset) / zoom`).
    fn screen_to_world(&self, (sx, sy): (f32, f32)) -> Point2D<f32> {
        let (ox, oy) = self.camera.offset;
        let z = self.camera.zoom.max(f32::EPSILON);
        Point2D::new((sx - ox) / z, (sy - oy) / z)
    }

    /// The screen viewport mapped to world space — the region gyre culls against
    /// to decide which nodes are on screen.
    fn world_viewport(&self) -> Box2D<f32> {
        Box2D::new(
            self.screen_to_world((0.0, 0.0)),
            self.screen_to_world((self.width as f32, self.height as f32)),
        )
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

        // Tick while settling or while a node is being dragged (so neighbors
        // react to the pinned node through the springs).
        let settling = self.ticks_remaining > 0;
        let dragging = self.drag.is_some_and(|d| d.moved);
        if settling || dragging {
            self.sim.tick(TICK_DT);
            if settling {
                self.ticks_remaining -= 1;
            }
        }
        // Pan inertia: glide + decay when not actively middle-dragging.
        let gliding = self.middle_drag.is_none()
            && (self.pan_velocity.0.abs() > 0.05 || self.pan_velocity.1.abs() > 0.05);
        if gliding {
            self.camera.offset.0 += self.pan_velocity.0;
            self.camera.offset.1 += self.pan_velocity.1;
            self.pan_velocity.0 *= PAN_DECAY;
            self.pan_velocity.1 *= PAN_DECAY;
        } else if self.middle_drag.is_none() {
            self.pan_velocity = (0.0, 0.0);
        }
        self.generation = self.generation.wrapping_add(1);

        // Snapshot the live positions, then reproject the underlay from them
        // (a node with no body falls back to its committed position inside the
        // producer).
        let positions: HashMap<NodeKey, PortablePoint> = self
            .sim
            .positions()
            .map(|(k, p)| (k, PortablePoint::new(p.x, p.y)))
            .collect();
        // On-screen nodes (gyre cull against the world-space viewport) become DOM
        // children; the rest demote to underlay rects, so no node double-draws.
        let on_screen: HashSet<NodeKey> =
            self.sim.cull_aabb(self.world_viewport()).into_iter().collect();

        let underlay = orrery_paint_list_demoted(
            &self.graph,
            |k| positions.get(&k).copied(),
            |k| !on_screen.contains(&k),
            viewport,
            self.camera,
            &self.style,
            self.generation,
        );

        // The node-children document: abs-pos labelled boxes for the on-screen
        // nodes under the camera transform, composited over the underlay.
        let nodes_dom = build_node_children_dom(&self.graph, &positions, &on_screen, self.camera);
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

        if settling || gliding || dragging {
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
                self.request_redraw();
            },
            WindowEvent::ModifiersChanged(mods) => {
                self.ctrl = mods.state().control_key();
            },
            WindowEvent::CursorMoved { position, .. } => {
                let new = (position.x as f32, position.y as f32);
                // A middle-button drag pans; the last move's delta seeds inertia.
                if let Some(prev) = self.middle_drag {
                    let d = (new.0 - prev.0, new.1 - prev.1);
                    self.camera.offset.0 += d.0;
                    self.camera.offset.1 += d.1;
                    self.pan_velocity = d;
                    self.middle_drag = Some(new);
                    self.request_redraw();
                }
                // A left-button drag past the slop pins its node to the cursor.
                if let Some(mut d) = self.drag {
                    if !d.moved && (new.0 - d.press.0).hypot(new.1 - d.press.1) > CLICK_SLOP {
                        d.moved = true;
                    }
                    if d.moved {
                        let world = self.screen_to_world(new);
                        self.sim.pin(d.node, world);
                        self.request_redraw();
                    }
                    self.drag = Some(d);
                }
                self.cursor = new;
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (x * WHEEL_PAN_SCALE, y * WHEEL_PAN_SCALE)
                    },
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                if self.ctrl {
                    // Ctrl+wheel = zoom, anchored at the cursor.
                    let factor = ZOOM_STEP.powf(dy / WHEEL_PAN_SCALE);
                    self.zoom_at(self.cursor, factor);
                } else {
                    // Wheel = pan (infinite-canvas), as an impulse into inertia.
                    self.pan_velocity.0 += dx;
                    self.pan_velocity.1 += dy;
                }
                self.request_redraw();
            },
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Middle, ElementState::Pressed) => {
                    self.middle_drag = Some(self.cursor);
                    self.pan_velocity = (0.0, 0.0);
                },
                (MouseButton::Middle, ElementState::Released) => self.middle_drag = None,
                (MouseButton::Left, ElementState::Pressed) => {
                    // Grab the node under the cursor (gyre node-pick in world space).
                    let world = self.screen_to_world(self.cursor);
                    if let Some(node) = self.sim.hit_test(world) {
                        self.drag = Some(Drag { node, press: self.cursor, moved: false });
                    }
                },
                (MouseButton::Left, ElementState::Released) => {
                    if let Some(d) = self.drag.take() {
                        if d.moved {
                            // Drop: release the pin so the node rejoins the
                            // physics, and re-settle the neighborhood.
                            self.sim.unpin(d.node);
                            self.ticks_remaining = self.ticks_remaining.max(SETTLE_TICKS / 3);
                            self.request_redraw();
                        } else {
                            // A click, not a drag: node activation (selection
                            // rendering is 1E.3).
                            tracing::info!(node = ?d.node, "node activated");
                        }
                    }
                },
                _ => {},
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
    on_screen: &HashSet<NodeKey>,
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
        if !on_screen.contains(&key) {
            continue; // off-screen nodes are demoted to the underlay
        }
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
    fn zoom_at_keeps_the_anchor_world_point_fixed() {
        // Cursor-anchored zoom: the world point under the anchor must not move on
        // screen across a zoom change.
        let mut app = App::new();
        app.camera.offset = (100.0, 50.0);
        app.camera.zoom = 1.0;
        let anchor = (200.0, 80.0);
        let world = |a: &App| {
            (
                (anchor.0 - a.camera.offset.0) / a.camera.zoom,
                (anchor.1 - a.camera.offset.1) / a.camera.zoom,
            )
        };
        let before = world(&app);
        app.zoom_at(anchor, 2.0);
        let after = world(&app);
        assert!((after.0 - before.0).abs() < 0.01, "anchor world x fixed");
        assert!((after.1 - before.1).abs() < 0.01, "anchor world y fixed");
        assert_eq!(app.camera.zoom, 2.0, "zoom applied");
    }

    #[test]
    fn screen_to_world_inverts_the_camera() {
        // Hit-testing + cull map the cursor to world space through this; pin the
        // inverse of `screen = world * zoom + offset`.
        let mut app = App::new();
        app.camera.offset = (100.0, 50.0);
        app.camera.zoom = 2.0;
        let w = app.screen_to_world((300.0, 150.0));
        assert!((w.x - 100.0).abs() < 0.01, "world x = (300-100)/2");
        assert!((w.y - 50.0).abs() < 0.01, "world y = (150-50)/2");
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
