/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Semantic input methods for [`Orrery`](crate::Orrery) — pointer events, wheel,
//! modifier state, and navigation. Factored from `lib.rs` to keep files under
//! the workspace 600-LOC ceiling.

use euclid::default::{Box2D, Point2D};
use kernel::geometry::PortablePoint;
use kernel::graph::NodeKey;

use super::build::{hyperlink, seed_cluster};
use super::{Drag, Orrery, PointerButton, CLICK_SLOP, EDGE_PICK_TOL, SETTLE_TICKS, WHEEL_PAN_SCALE, ZOOM_STEP};

impl Orrery {
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
}
