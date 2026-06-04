/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The orrery as a reusable, **window-agnostic content-root** — the graph's
//! spatial presentation (build item 1D of the serval-as-host flip; S1 of the
//! modular integration plan).
//!
//! [`Orrery`] owns the graph, its [`gyre::Simulation`], the camera, and the
//! pre-materialized abs-pos node-children pool. It exposes:
//!
//! - [`Orrery::frame`] — advance one frame at a given viewport and return the
//!   composited `netrender::Scene` plus whether another frame is needed (the sim
//!   still settling, pan still gliding, or a node being dragged). It does **not**
//!   present — the host (a winit bin, or meerkat's content-root) rasterizes +
//!   composites the returned scene.
//! - Semantic input methods ([`pointer_down`](Orrery::pointer_down) /
//!   [`pointer_up`](Orrery::pointer_up) / [`cursor_moved`](Orrery::cursor_moved) /
//!   [`wheel`](Orrery::wheel) / [`set_ctrl`](Orrery::set_ctrl) /
//!   [`reseed`](Orrery::reseed)), each returning whether a redraw is needed. The
//!   host maps its raw events (winit, serval input, …) onto these; the orrery
//!   never sees a window.
//!
//! The three composited layers (per the §6 plan): the [`platen::orrery`]
//! scene-paint underlay (edges + demoted off-screen node rects + coupling
//! overlays) under one camera transform; the on-screen nodes as abs-pos serval
//! DOM children (laid out incrementally, moved per-frame by inline transform on
//! the `RepaintOnly` path); and a screen-space marquee rubber-band when active.
//!
//! The sample graph, simulation, node-children pool, and the small paint/DOM
//! helpers live in [`mod@build`].

use std::collections::{HashMap, HashSet};

use euclid::default::{Box2D, Point2D};
use gyre::Simulation;
use kernel::geometry::PortablePoint;
use kernel::graph::{Graph, NodeKey};
use layout_dom_api::LayoutDomMut;
use netrender::Scene;
use paint_list_api::{DeviceIntSize, PaintList};
use paint_list_render::{composite_paint_layers, CompositeLayer};
use platen::orrery::orrery_paint_list_demoted;
use platen::scene_paint::{Camera, ScenePaintStyle};
use serval_layout::{Applied, IncrementalLayout, ScrollOffsets};
use serval_scripted_dom::{NodeId as DomNodeId, ScriptedDom};

mod build;
use build::{
    background_cmds, build_pool_dom, build_simulation, dark_scene_style, dedup_edges, hyperlink,
    marquee_rect_cmds, sample_graph, seed_cluster, set_class, set_style, selected_edge_overlay,
    surface_bg, NODE_SHEET,
};

/// Force-directed settle length (frames) after a (re)seed, ~6s at 60fps.
const SETTLE_TICKS: u32 = 360;
/// Per-tick timestep handed to the gyre simulation.
const TICK_DT: f32 = 1.0 / 60.0;
/// Pixels per wheel line-notch (the host scales `LineDelta` by this before
/// calling [`Orrery::wheel`]; `wheel` divides back out to recover notches for zoom).
pub const WHEEL_PAN_SCALE: f32 = 40.0;
/// Zoom multiplier per wheel notch under Ctrl.
const ZOOM_STEP: f32 = 1.15;
/// Pan-inertia decay per frame (lower = stops sooner).
const PAN_DECAY: f32 = 0.85;
/// Clamp for the camera zoom.
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
/// Screen-px the pointer may move before a press counts as a drag, not a click.
const CLICK_SLOP: f32 = 4.0;
/// Screen-px pick radius around an edge segment for `edge_hit_test`.
const EDGE_PICK_TOL: f32 = 6.0;
/// Node box half-extent (px) — matches the underlay's default node rect, so each
/// DOM child sits centered on the same world position.
const NODE_HALF: f32 = 18.0;

/// A pointer button the host reports to the orrery (winit / serval / … map onto it).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
}

/// The orrery's camera as plain pan + zoom, for the host to persist and restore
/// (the host maps it to/from its own serialized form). `offset` is the screen-px
/// translation, `zoom` the uniform scale: `screen = world * zoom + offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraView {
    pub offset: (f32, f32),
    pub zoom: f32,
}

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

/// The orrery content-root: the graph, its physics, the camera, and the abs-pos
/// node-children pool. Window-agnostic — see the module docs.
pub struct Orrery {
    graph: Graph,
    /// The force-directed layout. The underlay reprojects from its live bodies.
    sim: Simulation,
    /// The pre-materialized node-children pool: a persistent serval DOM with one
    /// `.gnode` per node under a `.stage` container (built once, mutated per
    /// frame — never rebuilt structurally).
    node_dom: ScriptedDom,
    /// Incremental layout over `node_dom`; per-frame transform / class mutations
    /// go through `apply` (paint-tier → `RepaintOnly`) and paint via
    /// `emit_paint_list`. `None` until first built (or after a viewport change).
    node_layout: Option<IncrementalLayout<DomNodeId>>,
    /// node → its `.gnode` element in `node_dom`.
    gnode_of: HashMap<NodeKey, DomNodeId>,
    /// The `.stage` container element (carries the camera transform).
    stage_node: DomNodeId,
    /// Viewport `node_layout` was built at; a change rebuilds it.
    pool_w: u32,
    pool_h: u32,
    /// Remaining settle frames; while > 0 the sim ticks and redraws are chained.
    ticks_remaining: u32,
    camera: Camera,
    style: ScenePaintStyle,
    /// Producer generation, bumped each rendered frame (positions / camera move).
    generation: u64,
    /// Last cursor position in screen px (zoom anchor + drag origin).
    cursor: (f32, f32),
    /// Inertial pan velocity (px/frame); decays each frame when not dragging.
    pan_velocity: (f32, f32),
    /// `Some(last_cursor)` while a middle-button pan drag is in progress.
    middle_drag: Option<(f32, f32)>,
    /// An in-progress left-button node click/drag, if any.
    drag: Option<Drag>,
    /// Currently-selected nodes (click selects one; marquee selects many).
    selected: HashSet<NodeKey>,
    /// Currently-selected edges (edge-pick, or covered by a marquee), as the
    /// `(from, to)` pairs gyre reports.
    selected_edges: HashSet<(NodeKey, NodeKey)>,
    /// Edges the user has hidden, as undirected `(from, to)` pairs (`from <= to`).
    /// The edge pass skips any relation whose pair is here; the relation and its
    /// physics spring persist (hiding is display-only). In-session for now;
    /// persistence rides view-intent's `hidden_relations`.
    hidden_edges: HashSet<(NodeKey, NodeKey)>,
    /// `Some(press_origin)` (screen px) while a left-drag marquee on empty space
    /// is in progress.
    marquee: Option<(f32, f32)>,
    /// Whether Ctrl is held (gates wheel-zoom vs wheel-pan).
    ctrl: bool,
    /// The current viewport (px), updated by [`frame`](Orrery::frame) /
    /// [`resize`](Orrery::resize); used by `world_viewport` (cull) + `recenter`.
    view_w: u32,
    view_h: u32,
}

impl Default for Orrery {
    fn default() -> Self {
        Self::new()
    }
}

impl Orrery {
    /// A new orrery over an **empty** session graph. The host grows it with
    /// [`visit`](Orrery::visit) as the user navigates (the graph-rooted browse
    /// loop). For an isolated demo / the standalone bin, use
    /// [`with_sample_graph`](Orrery::with_sample_graph).
    pub fn new() -> Self {
        Self::from_graph(Graph::new())
    }

    /// A new orrery over a small built-in sample graph (a ring + spokes), seeded
    /// into a tight central spiral so the first settle is visible. The standalone
    /// `orrery-host` bin and the orrery tests use this; meerkat uses
    /// [`new`](Orrery::new) and drives the graph through [`visit`](Orrery::visit).
    pub fn with_sample_graph() -> Self {
        Self::from_graph(sample_graph())
    }

    /// Build an orrery over a **restored** session `graph`: keep each node at its
    /// saved (committed) position and do not auto-settle, so a reloaded session
    /// looks as it was left rather than re-scrambling into a fresh spiral.
    /// (Persistence host seam, S3.)
    pub fn with_graph(graph: Graph) -> Self {
        let mut orrery = Self::from_graph(graph);
        let positions: Vec<(NodeKey, Point2D<f32>)> = orrery
            .graph
            .nodes()
            .map(|(key, node)| {
                let p = node.projected_position();
                (key, Point2D::new(p.x, p.y))
            })
            .collect();
        orrery.sim.seed_positions(positions);
        orrery.ticks_remaining = 0;
        orrery
    }

    /// Build an orrery over `graph`: its [`build_simulation`], the node-children
    /// pool, and a default camera. Shared by [`new`](Orrery::new) (empty),
    /// [`with_sample_graph`](Orrery::with_sample_graph), and
    /// [`with_graph`](Orrery::with_graph).
    fn from_graph(graph: Graph) -> Self {
        let sim = build_simulation(&graph);
        let (node_dom, gnode_of, stage_node) = build_pool_dom(&graph);
        Self {
            graph,
            sim,
            node_dom,
            node_layout: None,
            gnode_of,
            stage_node,
            pool_w: 0,
            pool_h: 0,
            ticks_remaining: SETTLE_TICKS,
            camera: Camera::default(),
            style: dark_scene_style(),
            generation: 0,
            cursor: (0.0, 0.0),
            pan_velocity: (0.0, 0.0),
            middle_drag: None,
            drag: None,
            selected: HashSet::new(),
            selected_edges: HashSet::new(),
            hidden_edges: HashSet::new(),
            marquee: None,
            ctrl: false,
            view_w: 1024,
            view_h: 600,
        }
    }

    /// Set the viewport the orrery culls + centers against. The host calls this on
    /// a surface resize; the next [`frame`](Orrery::frame) rebuilds the node-pool
    /// layout at the new size.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.view_w = width.max(1);
        self.view_h = height.max(1);
    }

    /// Put the world origin at the viewport center — the sample graph is laid out
    /// around `(0, 0)`, so this frames it at zoom 1. (A fit-to-`content_bounds`
    /// camera replaces this when a real graph is hosted.)
    pub fn recenter(&mut self) {
        self.camera.offset = (self.view_w as f32 / 2.0, self.view_h as f32 / 2.0);
    }

    /// Advance one frame at viewport `(w, h)` and return the composited content
    /// scene plus whether the host should request another frame (sim still
    /// settling, pan still gliding, or a node being dragged). Does not present.
    pub fn frame(&mut self, w: u32, h: u32) -> (Scene, bool) {
        let (w, h) = (w.max(1), h.max(1));
        self.view_w = w;
        self.view_h = h;
        let viewport = DeviceIntSize::new(w as i32, h as i32);

        // Tick while settling or while a node is being dragged (so neighbors react
        // to the pinned node through the springs).
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

        // Snapshot the live positions, then reproject the underlay from them (a
        // node with no body falls back to its committed position in the producer).
        let positions: HashMap<NodeKey, PortablePoint> = self
            .sim
            .positions()
            .map(|(k, p)| (k, PortablePoint::new(p.x, p.y)))
            .collect();
        // On-screen nodes (gyre cull against the world-space viewport) become DOM
        // children; the rest demote to underlay rects, so no node double-draws.
        let on_screen: HashSet<NodeKey> =
            self.sim.cull_aabb(self.world_viewport()).into_iter().collect();

        let mut underlay = orrery_paint_list_demoted(
            &self.graph,
            |k| positions.get(&k).copied(),
            |k| !on_screen.contains(&k),
            // Skip relations whose undirected pair the user has hidden.
            |rel| {
                let pair =
                    if rel.from <= rel.to { (rel.from, rel.to) } else { (rel.to, rel.from) };
                !self.hidden_edges.contains(&pair)
            },
            viewport,
            self.camera,
            &self.style,
            self.generation,
        );
        // Highlight selected edges by splicing thicker strokes inside the
        // underlay's camera transform (world space — no transform replication).
        if !self.selected_edges.is_empty() {
            underlay.splice_world_overlays(selected_edge_overlay(&self.sim, &self.selected_edges));
        }

        // The node-children layer — the pre-materialized pool. Ensure the
        // incremental layout exists at this viewport, then mutate the `.stage`
        // camera transform and each gnode's transform + selection class. These are
        // attribute-only (paint-tier), so `apply` stays on the RepaintOnly path —
        // no per-frame relayout or DOM rebuild.
        if self.node_layout.is_none() || self.pool_w != w || self.pool_h != h {
            let mut discard = Vec::new();
            self.node_dom.drain_mutations(&mut discard);
            self.node_layout =
                Some(IncrementalLayout::new(&self.node_dom, NODE_SHEET, w as f32, h as f32));
            self.pool_w = w;
            self.pool_h = h;
        }
        set_style(
            &mut self.node_dom,
            self.stage_node,
            &format!(
                "transform: translate({}px, {}px) scale({});",
                self.camera.offset.0, self.camera.offset.1, self.camera.zoom
            ),
        );
        let gnodes: Vec<(NodeKey, DomNodeId)> = self.gnode_of.iter().map(|(&k, &g)| (k, g)).collect();
        for (key, gnode) in gnodes {
            let pos = positions.get(&key).copied().unwrap_or_default();
            set_style(
                &mut self.node_dom,
                gnode,
                &format!("transform: translate({}px, {}px);", pos.x - NODE_HALF, pos.y - NODE_HALF),
            );
            let class = if self.selected.contains(&key) { "gnode-selected" } else { "gnode" };
            set_class(&mut self.node_dom, gnode, class);
        }
        let mut muts = Vec::new();
        self.node_dom.drain_mutations(&mut muts);
        let applied = self.node_layout.as_mut().unwrap().apply(&self.node_dom, NODE_SHEET, &muts);
        if !matches!(applied, Applied::RepaintOnly | Applied::Unchanged) {
            tracing::warn!(?applied, "orrery pool: node layout left the RepaintOnly path");
        }
        let scroll = ScrollOffsets::<DomNodeId>::default();
        let nodes_plist =
            self.node_layout.as_ref().unwrap().emit_paint_list(&self.node_dom, &scroll, viewport);

        // A third (screen-space) layer for the marquee rubber-band, when active.
        let marquee_cmds = self.marquee.map(|origin| marquee_rect_cmds(origin, self.cursor));
        // The orrery's own opaque backdrop is the bottom layer (so the surface is
        // dark without depending on the host clear color); then the underlay edges
        // + demoted rects, then the on-screen node DOM, then any marquee on top.
        let bg_cmds = background_cmds(w, h, surface_bg());
        let mut layers = vec![
            CompositeLayer::commands_only(&bg_cmds),
            CompositeLayer::commands_only(underlay.commands()),
            CompositeLayer {
                commands: nodes_plist.commands(),
                fonts: nodes_plist.fonts(),
                images: nodes_plist.images(),
            },
        ];
        if let Some(cmds) = marquee_cmds.as_ref() {
            layers.push(CompositeLayer::commands_only(cmds));
        }
        let scene = composite_paint_layers(viewport, &layers).scene;

        let needs_redraw = settling || gliding || dragging;
        (scene, needs_redraw)
    }

    // ----- Input (semantic; each returns whether the host should redraw) --------

    /// Update whether Ctrl is held (gates wheel-zoom vs wheel-pan).
    pub fn set_ctrl(&mut self, ctrl: bool) {
        self.ctrl = ctrl;
    }

    /// The pointer moved to screen px `(x, y)`: pans on a middle-drag, pins the
    /// grabbed node on a left-drag past the slop, grows an active marquee.
    pub fn cursor_moved(&mut self, x: f32, y: f32) -> bool {
        let new = (x, y);
        let mut redraw = false;
        if let Some(prev) = self.middle_drag {
            let d = (new.0 - prev.0, new.1 - prev.1);
            self.camera.offset.0 += d.0;
            self.camera.offset.1 += d.1;
            self.pan_velocity = d;
            self.middle_drag = Some(new);
            redraw = true;
        }
        if let Some(mut d) = self.drag {
            if !d.moved && (new.0 - d.press.0).hypot(new.1 - d.press.1) > CLICK_SLOP {
                d.moved = true;
            }
            if d.moved {
                self.sim.pin(d.node, self.screen_to_world(new));
                redraw = true;
            }
            self.drag = Some(d);
        }
        self.cursor = new;
        if self.marquee.is_some() {
            redraw = true;
        }
        redraw
    }

    /// A wheel/scroll event, `(dx, dy)` already in device px (the host maps
    /// `LineDelta` by [`WHEEL_PAN_SCALE`] / `PixelDelta` straight through). Ctrl =
    /// cursor-anchored zoom; otherwise an infinite-canvas pan impulse into inertia.
    pub fn wheel(&mut self, dx: f32, dy: f32) -> bool {
        if self.ctrl {
            let factor = ZOOM_STEP.powf(dy / WHEEL_PAN_SCALE);
            self.zoom_at(self.cursor, factor);
        } else {
            self.pan_velocity.0 += dx;
            self.pan_velocity.1 += dy;
        }
        true
    }

    /// A pointer button pressed at screen px `(x, y)`. Middle begins a pan; left
    /// grabs the node under the cursor (gyre node-pick) or, on empty space, begins
    /// a marquee.
    pub fn pointer_down(&mut self, button: PointerButton, x: f32, y: f32) -> bool {
        self.cursor = (x, y);
        match button {
            PointerButton::Middle => {
                self.middle_drag = Some(self.cursor);
                self.pan_velocity = (0.0, 0.0);
            },
            PointerButton::Left => {
                let world = self.screen_to_world(self.cursor);
                if let Some(node) = self.sim.hit_test(world) {
                    self.drag = Some(Drag { node, press: self.cursor, moved: false });
                } else {
                    self.marquee = Some(self.cursor);
                }
            },
            PointerButton::Right => {},
        }
        false
    }

    /// A pointer button released at screen px `(x, y)`. Ends a middle-pan; drops a
    /// dragged node (re-settling its neighborhood) or selects a clicked node; ends
    /// a marquee (rect-select) or, on a bare empty click, picks the nearest edge
    /// within tolerance, else clears the selection.
    pub fn pointer_up(&mut self, button: PointerButton, x: f32, y: f32) -> bool {
        self.cursor = (x, y);
        match button {
            PointerButton::Middle => {
                self.middle_drag = None;
                false
            },
            PointerButton::Left => {
                if let Some(d) = self.drag.take() {
                    if d.moved {
                        self.sim.unpin(d.node);
                        self.ticks_remaining = self.ticks_remaining.max(SETTLE_TICKS / 3);
                    } else {
                        self.select_only(d.node);
                    }
                    true
                } else if let Some(origin) = self.marquee.take() {
                    let dragged =
                        (self.cursor.0 - origin.0).hypot(self.cursor.1 - origin.1) > CLICK_SLOP;
                    if dragged {
                        let region = Box2D::from_points([
                            self.screen_to_world(origin),
                            self.screen_to_world(self.cursor),
                        ]);
                        let sel = self.sim.rect_select(region);
                        self.selected = sel.nodes.into_iter().collect();
                        self.selected_edges = sel.edges.into_iter().collect();
                    } else {
                        let world = self.screen_to_world(self.cursor);
                        let tol = EDGE_PICK_TOL / self.camera.zoom.max(f32::EPSILON);
                        self.selected.clear();
                        self.selected_edges.clear();
                        if let Some(edge) = self.sim.edge_hit_test(world, tol) {
                            self.selected_edges.insert(edge);
                        }
                    }
                    true
                } else {
                    false
                }
            },
            PointerButton::Right => false,
        }
    }

    /// Re-seed the central spiral and replay the settle (the `Space` gesture).
    pub fn reseed(&mut self) -> bool {
        seed_cluster(&mut self.sim, &self.graph);
        self.ticks_remaining = SETTLE_TICKS;
        true
    }

    /// Visit `url`: if a node with that URL already exists (URL identity), select
    /// it; otherwise add a new node — linked from the currently-selected node, if
    /// any, so the browse trail grows as graph structure — re-sync the physics and
    /// node-children pool, and re-settle. Returns the node either way. The host
    /// calls this on navigation (the graph-rooted browse loop, S2).
    pub fn visit(&mut self, url: &str) -> NodeKey {
        if let Some((key, _)) = self.graph.get_node_by_url(url) {
            self.select_only(key);
            return key;
        }
        // Place the new node just off the one we came from (the current
        // selection), so the trail spreads outward; the first node lands at the
        // origin and the settle takes over.
        let from = self.selected.iter().copied().next();
        let anchor = from.and_then(|k| self.sim.position_of(k)).unwrap_or(Point2D::new(0.0, 0.0));
        let seed = Point2D::new(anchor.x + 12.0, anchor.y + 12.0);
        let key = self.graph.add_node(url.to_string(), PortablePoint::new(seed.x, seed.y));
        if let Some(from) = from {
            let _ = self.graph.assert_relation(from, key, hyperlink());
        }
        // Re-sync derived state (bodies, edges, node pool), then seed the new node
        // near the anchor and re-settle.
        self.reconcile_derived();
        self.sim.seed_positions([(key, seed)]);
        self.select_only(key);
        self.ticks_remaining = SETTLE_TICKS;
        key
    }

    /// Re-sync the physics bodies, edge topology, and node-children pool to the
    /// current graph after a structural change. Does not seed positions, alter the
    /// selection, or restart the settle; callers do that as they need. The pool
    /// is structural, so it is rebuilt, not grown incrementally.
    fn reconcile_derived(&mut self) {
        self.sim.sync_with_graph(&self.graph);
        self.sim.sync_edges(dedup_edges(&self.graph));
        let (node_dom, gnode_of, stage_node) = build_pool_dom(&self.graph);
        self.node_dom = node_dom;
        self.gnode_of = gnode_of;
        self.stage_node = stage_node;
        self.node_layout = None;
    }

    /// Apply a structural mutation to the session graph and reconcile every derived
    /// view, so externally-ingested nodes/edges (e.g. a linked-data merge) join the
    /// spatial field. `mutate` returns whether it changed the graph; on a change,
    /// the newly-added nodes are fanned out around the current selection (they are
    /// minted at the origin, which the force sim must not stack), and the settle
    /// restarts. Returns whether the graph changed.
    ///
    /// orrery-host stays free of the linked-data bridge: the host passes the merge
    /// in as a closure over [`Graph`].
    pub fn ingest_graph<F: FnOnce(&mut Graph) -> bool>(&mut self, mutate: F) -> bool {
        let before: HashSet<NodeKey> = self.graph.nodes().map(|(k, _)| k).collect();
        if !mutate(&mut self.graph) {
            return false;
        }
        self.reconcile_derived();
        let anchor = self
            .selected
            .iter()
            .copied()
            .next()
            .and_then(|k| self.sim.position_of(k))
            .unwrap_or(Point2D::new(0.0, 0.0));
        let seeds: Vec<_> = self
            .graph
            .nodes()
            .map(|(k, _)| k)
            .filter(|k| !before.contains(k))
            .enumerate()
            .map(|(i, k)| {
                let col = (i % 6) as f32;
                let row = (i / 6) as f32;
                (k, Point2D::new(anchor.x + 12.0 + col * 16.0, anchor.y + 12.0 + row * 16.0))
            })
            .collect();
        self.sim.seed_positions(seeds);
        self.ticks_remaining = SETTLE_TICKS;
        true
    }

    /// Replace the selection with just `key` (clearing any selected nodes/edges).
    fn select_only(&mut self, key: NodeKey) {
        self.selected.clear();
        self.selected_edges.clear();
        self.selected.insert(key);
    }

    /// Select the existing node with `url` (URL identity), if present, without
    /// adding one. Returns whether a node was found and focused. The host calls
    /// this to restore the focused node from persisted view-intent.
    pub fn select_by_url(&mut self, url: &str) -> bool {
        if let Some((key, _)) = self.graph.get_node_by_url(url) {
            self.select_only(key);
            true
        } else {
            false
        }
    }

    /// Remove the single focused node from the session graph (if exactly one is
    /// selected), returning its member id (the kernel node UUID) so the host can
    /// reap any activation for it. Clears the selection and reconciles the physics
    /// + node pool to the smaller graph. Returns `None` when zero or many nodes
    /// are selected, leaving the graph untouched. The host calls this on a
    /// delete-node gesture; deactivation (reaping the actor) is the host's job.
    pub fn remove_focused(&mut self) -> Option<uuid::Uuid> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        let id = self.graph.get_node(key)?.id;
        self.graph.remove_node(key);
        self.selected.clear();
        self.selected_edges.clear();
        self.reconcile_derived();
        Some(id)
    }

    /// Hide the currently-selected edges: move them into the hidden set (as
    /// undirected pairs) so the edge pass skips them, and clear the selection.
    /// Returns how many were hidden. The relations and their physics springs
    /// persist — this hides the drawn lines, it does not delete the relations.
    pub fn hide_selected_edges(&mut self) -> usize {
        let count = self.selected_edges.len();
        for (a, b) in self.selected_edges.drain() {
            let pair = if a <= b { (a, b) } else { (b, a) };
            self.hidden_edges.insert(pair);
        }
        count
    }

    /// Reveal every hidden edge. Returns how many were shown.
    pub fn show_all_edges(&mut self) -> usize {
        let count = self.hidden_edges.len();
        self.hidden_edges.clear();
        count
    }

    /// The URL of the single focused (selected) node, if exactly one node is
    /// selected. The host reads this to project the focused node's media — e.g.
    /// meerkat's floating content card. `None` when zero or many are selected.
    pub fn focused_url(&self) -> Option<&str> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        self.graph.get_node(key).map(|n| n.url())
    }

    /// The session graph, for the host to persist (`to_snapshot` → `graph.json`).
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The current camera (pan + zoom), for the host to persist as view-intent.
    pub fn camera(&self) -> CameraView {
        CameraView { offset: self.camera.offset, zoom: self.camera.zoom }
    }

    /// Restore the camera from persisted view-intent. A non-finite or
    /// non-positive zoom falls back to `1.0`; the zoom is clamped to the orrery's
    /// range. The host suppresses its own first-frame recenter when it restores a
    /// camera, so this value is not immediately overwritten.
    pub fn set_camera(&mut self, view: CameraView) {
        self.camera.zoom = if view.zoom.is_finite() && view.zoom > 0.0 {
            view.zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.0
        };
        self.camera.offset = view.offset;
    }

    /// Zoom by `factor`, keeping the world point under `anchor` (screen px) fixed.
    fn zoom_at(&mut self, anchor: (f32, f32), factor: f32) {
        let new_zoom = (self.camera.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let applied = new_zoom / self.camera.zoom;
        self.camera.offset.0 = anchor.0 - (anchor.0 - self.camera.offset.0) * applied;
        self.camera.offset.1 = anchor.1 - (anchor.1 - self.camera.offset.1) * applied;
        self.camera.zoom = new_zoom;
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
            self.screen_to_world((self.view_w as f32, self.view_h as f32)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_at_keeps_the_anchor_world_point_fixed() {
        let mut orrery = Orrery::new();
        orrery.camera.offset = (100.0, 50.0);
        orrery.camera.zoom = 1.0;
        let anchor = (200.0, 80.0);
        let world = |o: &Orrery| {
            (
                (anchor.0 - o.camera.offset.0) / o.camera.zoom,
                (anchor.1 - o.camera.offset.1) / o.camera.zoom,
            )
        };
        let before = world(&orrery);
        orrery.zoom_at(anchor, 2.0);
        let after = world(&orrery);
        assert!((after.0 - before.0).abs() < 0.01, "anchor world x fixed");
        assert!((after.1 - before.1).abs() < 0.01, "anchor world y fixed");
        assert_eq!(orrery.camera.zoom, 2.0, "zoom applied");
    }

    #[test]
    fn screen_to_world_inverts_the_camera() {
        let mut orrery = Orrery::new();
        orrery.camera.offset = (100.0, 50.0);
        orrery.camera.zoom = 2.0;
        let w = orrery.screen_to_world((300.0, 150.0));
        assert!((w.x - 100.0).abs() < 0.01, "world x = (300-100)/2");
        assert!((w.y - 50.0).abs() < 0.01, "world y = (150-50)/2");
    }

    #[test]
    fn visit_adds_a_linked_node_and_selects_it() {
        let mut orrery = Orrery::new(); // empty session
        let a = orrery.visit("mere://welcome");
        assert_eq!(orrery.graph.nodes().count(), 1, "the first visit seeds the root node");
        assert!(orrery.selected.contains(&a), "the visited node is selected");

        let b = orrery.visit("https://example.com");
        assert_eq!(orrery.graph.nodes().count(), 2, "a fresh URL adds a node");
        assert_ne!(a, b);
        assert!(
            orrery.selected.contains(&b) && !orrery.selected.contains(&a),
            "selection moves to the newly visited node",
        );
        assert!(orrery.graph.relations().count() >= 1, "an edge links the browse trail");
        assert_eq!(orrery.sim.body_count(), 2, "the simulation grew a body for the new node");
    }

    #[test]
    fn ingest_graph_merges_nodes_and_grows_the_sim() {
        let mut orrery = Orrery::new();
        orrery.visit("mere://seed");
        let before = orrery.graph.nodes().count();
        let changed = orrery.ingest_graph(|g| {
            let a = g.add_node("mere://x".to_string(), Default::default());
            let b = g.add_node("mere://y".to_string(), Default::default());
            let _ = g.assert_relation(a, b, hyperlink());
            true
        });
        assert!(changed, "a mutating closure reports a change");
        assert_eq!(orrery.graph.nodes().count(), before + 2, "both ingested nodes joined");
        assert!(orrery.graph.get_node_by_url("mere://x").is_some());
        assert_eq!(orrery.sim.body_count(), before + 2, "the sim grew a body per node");
        // The origin-minted nodes are seeded apart, so the settle runs clean (no
        // coincident-point blow-up / NaN).
        let _ = orrery.frame(800, 600);
        // A closure that reports no change is a no-op.
        assert!(!orrery.ingest_graph(|_| false), "no-op mutation reports no change");
    }

    #[test]
    fn visit_dedups_by_url() {
        let mut orrery = Orrery::new();
        let a = orrery.visit("https://example.com");
        let _ = orrery.visit("https://other.com");
        let again = orrery.visit("https://example.com");
        assert_eq!(a, again, "revisiting a URL selects the existing node, not a duplicate");
        assert_eq!(orrery.graph.nodes().count(), 2, "no duplicate node is added");
        assert!(orrery.selected.contains(&a), "selection returns to the revisited node");
    }

    #[test]
    fn focused_url_is_the_single_selected_nodes_url() {
        let mut orrery = Orrery::new();
        assert_eq!(orrery.focused_url(), None, "nothing selected yet → no focus");
        orrery.visit("https://example.com");
        assert_eq!(orrery.focused_url(), Some("https://example.com"), "visit focuses the node");
        orrery.visit("https://second.com");
        assert_eq!(orrery.focused_url(), Some("https://second.com"), "focus follows the new node");
        orrery.visit("https://example.com");
        assert_eq!(orrery.focused_url(), Some("https://example.com"), "revisit re-focuses");
    }

    #[test]
    fn with_graph_restores_nodes_and_positions() {
        let mut graph = Graph::new();
        graph.add_node("https://one.example".to_string(), PortablePoint::new(100.0, 50.0));
        graph.add_node("https://two.example".to_string(), PortablePoint::new(-30.0, 80.0));
        let orrery = Orrery::with_graph(graph);
        assert_eq!(orrery.graph().nodes().count(), 2, "the restored graph keeps its nodes");
        assert_eq!(orrery.sim.body_count(), 2, "a body per restored node");
        assert_eq!(orrery.ticks_remaining, 0, "a restored session does not auto-resettle");
        let key = orrery.graph().get_node_by_url("https://one.example").unwrap().0;
        let pos = orrery.sim.position_of(key).expect("a body position");
        assert!(
            (pos.x - 100.0).abs() < 1.0 && (pos.y - 50.0).abs() < 1.0,
            "the saved position is preserved (no spiral re-seed), got {pos:?}",
        );
    }

    #[test]
    fn camera_round_trips_and_guards_bad_zoom() {
        let mut orrery = Orrery::new();
        orrery.set_camera(CameraView { offset: (123.0, -45.0), zoom: 2.5 });
        let cv = orrery.camera();
        assert_eq!(cv.offset, (123.0, -45.0));
        assert_eq!(cv.zoom, 2.5);
        // A zero / non-finite zoom falls back to 1.0 rather than collapsing.
        orrery.set_camera(CameraView { offset: (0.0, 0.0), zoom: 0.0 });
        assert_eq!(orrery.camera().zoom, 1.0);
    }

    #[test]
    fn select_by_url_selects_existing_or_reports_missing() {
        let mut orrery = Orrery::new();
        orrery.visit("https://one.example");
        orrery.visit("https://two.example"); // focus moves to two
        assert!(orrery.select_by_url("https://one.example"), "an existing url is found + focused");
        assert_eq!(orrery.focused_url(), Some("https://one.example"));
        assert!(!orrery.select_by_url("https://absent.example"), "a missing url reports false");
        assert_eq!(orrery.focused_url(), Some("https://one.example"), "a miss leaves selection intact");
    }

    #[test]
    fn remove_focused_drops_the_node_and_reports_its_id() {
        let mut orrery = Orrery::new();
        orrery.visit("https://a.example");
        orrery.visit("https://b.example"); // focus on b
        let before = orrery.graph().nodes().count();
        let removed = orrery.remove_focused();
        assert!(removed.is_some(), "the focused node is removed and its id returned");
        assert_eq!(orrery.graph().nodes().count(), before - 1, "the graph shrinks by one");
        assert!(orrery.graph().get_node_by_url("https://b.example").is_none(), "the node is gone");
        assert_eq!(orrery.focused_url(), None, "the selection clears");
        assert_eq!(orrery.sim.body_count(), before - 1, "the physics body is reconciled away");
        // Nothing focused → a second remove is a no-op.
        assert!(orrery.remove_focused().is_none(), "no focus → no removal");
    }

    #[test]
    fn hide_selected_edges_then_show_all_round_trips() {
        let mut orrery = Orrery::new();
        orrery.visit("https://a.example");
        orrery.visit("https://b.example"); // the browse trail links a — b
        let a = orrery.graph().get_node_by_url("https://a.example").unwrap().0;
        let b = orrery.graph().get_node_by_url("https://b.example").unwrap().0;
        orrery.selected_edges.insert((a, b)); // the host edge-picks this normally

        assert_eq!(orrery.hide_selected_edges(), 1, "the selected edge is hidden");
        assert!(orrery.selected_edges.is_empty(), "hiding clears the selection");
        assert_eq!(orrery.hidden_edges.len(), 1);
        // The relation itself survives (hiding is display-only).
        assert!(orrery.graph().relations().count() >= 1, "the relation is not deleted");

        assert_eq!(orrery.show_all_edges(), 1, "show-all reveals the hidden edge");
        assert!(orrery.hidden_edges.is_empty());
    }
}
