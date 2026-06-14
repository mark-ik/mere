/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Semantic input methods for [`Orrery`](crate::Orrery) — pointer events, wheel,
//! modifier state, and navigation. Factored from `lib.rs` to keep files under
//! the workspace 600-LOC ceiling.

use euclid::default::{Box2D, Point2D};
use kernel::geometry::PortablePoint;
use kernel::graph::{Falloff, Field, FieldDefinition, FieldExtent, FieldId, NodeKey, ScalarField};

use super::build::{hyperlink, seed_cluster};
use super::{Drag, Orrery, PointerButton, CLICK_SLOP, EDGE_PICK_TOL, SETTLE_TICKS, WHEEL_PAN_SCALE, ZOOM_STEP};

/// World-space radius of a freshly placed field region — its `Disk` definition
/// radius and its enclosing square `Region` half-extent. A sensible default the
/// user can later resize. (Field regions P0.)
const DEFAULT_FIELD_RADIUS: f32 = 120.0;

impl Orrery {
    // ----- Input (semantic; each returns whether the host should redraw) --------

    /// Update whether Ctrl is held (gates wheel-zoom vs wheel-pan).
    pub fn set_ctrl(&mut self, ctrl: bool) {
        self.ctrl = ctrl;
    }

    /// Update whether Shift is held. A node click with Shift down toggles that
    /// node in the selection (multi-select) rather than replacing it.
    pub fn set_shift(&mut self, shift: bool) {
        self.shift = shift;
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
            let was_moved = d.moved;
            if !d.moved && (new.0 - d.press.0).hypot(new.1 - d.press.1) > CLICK_SLOP {
                d.moved = true;
            }
            if d.moved {
                // On the click→drag transition, tell the backend to keep ticking
                // so the pinned node's neighbors react through the springs.
                if !was_moved {
                    self.physics.set_dragging(true);
                }
                let world = self.screen_to_world(new);
                self.physics.pin(d.node, world);
                // Track the cursor in the view immediately (the actor lags a frame).
                self.view.set_position(d.node, world);
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
                if let Some(node) = self.view.hit_test(world) {
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
                        self.physics.unpin(d.node);
                        self.physics.set_dragging(false);
                        self.physics.settle(SETTLE_TICKS / 3);
                    } else if self.shift {
                        // Shift-click toggles the node in the selection (multi-select).
                        if !self.selected.remove(&d.node) {
                            self.selected.insert(d.node);
                        }
                        self.selected_edges.clear();
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
                        let sel = self.view.rect_select(region);
                        self.selected = sel.nodes.into_iter().collect();
                        self.selected_edges = sel.edges.into_iter().collect();
                    } else if !self.shift {
                        // A bare empty click clears the selection (and may pick an
                        // edge under the cursor). With Shift held it is a no-op, so a
                        // multi-select in progress is preserved.
                        let world = self.screen_to_world(self.cursor);
                        let tol = EDGE_PICK_TOL / self.camera.zoom.max(f32::EPSILON);
                        self.selected.clear();
                        self.selected_edges.clear();
                        if let Some(edge) = self.view.edge_hit_test(world, tol) {
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
        let seeds = seed_cluster(&self.graph);
        for &(key, pos) in &seeds {
            self.view.set_position(key, pos);
        }
        self.physics.seed(seeds);
        self.physics.settle(SETTLE_TICKS);
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
        // A fresh URL mints a node off the current selection, so the browse trail
        // spreads outward; the first node lands at the origin and settles.
        let from = self.selected.iter().copied().next();
        self.mint_node(from, url)
    }

    /// Open `url` as a **brand-new node** (a new browsing surface) linked back to
    /// `origin` (the source node, by member id) with a navigated-from
    /// [`Semantic::Hyperlink`](kernel::graph::EdgeAssertion) edge. Unlike in-place
    /// navigation this always mints a node (no URL dedup — duplicates are welcome
    /// in the node-identity model); unlike [`visit`](Self::visit) the origin is
    /// explicit (the focused tile in Tree, the selection in Cartography). An
    /// `origin` of `None` or an unknown member mints an unlinked node — a graphlet
    /// candidate. Returns the new node's member id. This backs the "open in new
    /// node/tile" gesture (Ctrl/Cmd-Enter, middle-click, context menu) — P2 of the
    /// node-navigation-lineage plan.
    pub fn open_member_as_new_node(&mut self, origin: Option<uuid::Uuid>, url: &str) -> uuid::Uuid {
        let origin_key = origin.and_then(|m| self.graph.get_node_by_id(m)).map(|(k, _)| k);
        let key = self.mint_node(origin_key, url);
        self.graph.get_node(key).map(|n| n.id).expect("a freshly minted node has an id")
    }

    /// Mint a fresh unlinked node at the cursor's world position — the empty-space
    /// right-click "Add node" gesture. `content_band_xy` is the orrery-leaf-local
    /// cursor point (screen px); the camera inversion happens here, so the host
    /// needn't reach the crate-private [`screen_to_world`](Self::screen_to_world).
    /// Returns the new node's member id.
    pub fn add_node_at(&mut self, content_band_xy: (f32, f32), url: &str) -> uuid::Uuid {
        let world = self.screen_to_world(content_band_xy);
        let key = self.mint_node_at(world, url);
        self.graph.get_node(key).map(|n| n.id).expect("a freshly minted node has an id")
    }

    /// Place a fresh disk field region at the cursor's world position — the empty-space
    /// "Add field" gesture, the spatial-rule twin of [`add_node_at`](Self::add_node_at).
    /// `content_band_xy` is the orrery-leaf-local cursor point (screen px); the camera
    /// inversion happens here. The field is a `Disk` scalar definition inside a square
    /// `Region` extent centered on the point (the placed-extent the rules later evaluate
    /// over). Inert until coupled/projected; returns the new field's id. (Field regions P0.)
    pub fn add_field_at(&mut self, content_band_xy: (f32, f32)) -> uuid::Uuid {
        let world = self.screen_to_world(content_band_xy);
        let radius = DEFAULT_FIELD_RADIUS;
        let uuid = uuid::Uuid::new_v4();
        let id = FieldId::from_uuid(uuid);
        let definition = FieldDefinition::Scalar(ScalarField::disk_at(
            world.x, world.y, radius, Falloff::Smoothstep,
        ));
        let extent = FieldExtent::Region {
            min_x: world.x - radius,
            min_y: world.y - radius,
            max_x: world.x + radius,
            max_y: world.y + radius,
        };
        self.graph.add_field(Field::new(id, definition).with_extent(extent));
        uuid
    }

    /// Mint an unlinked node at an explicit world `seed`, selecting it. The
    /// origin-less twin of [`mint_node`](Self::mint_node): no navigated-from edge,
    /// no branched history — a fresh graphlet candidate placed exactly where asked.
    fn mint_node_at(&mut self, seed: Point2D<f32>, url: &str) -> NodeKey {
        let key = self.graph.add_node_with_id(
            uuid::Uuid::new_v4(),
            url.to_string(),
            PortablePoint::new(seed.x, seed.y),
        );
        self.graph.navigate_node(key, url);
        self.reconcile_derived();
        self.view.set_position(key, seed);
        self.physics.seed(vec![(key, seed)]);
        self.select_only(key);
        self.physics.settle(SETTLE_TICKS);
        key
    }

    /// Mint a brand-new node at `url`, seeded just off `origin`, linked back to it
    /// with a navigated-from edge when `origin` is present, and with its own
    /// within-node history seeded so its back/forward work from birth. Selects it
    /// and restarts the settle. Shared by [`visit`](Self::visit)'s create branch
    /// and [`open_member_as_new_node`](Self::open_member_as_new_node).
    fn mint_node(&mut self, origin: Option<NodeKey>, url: &str) -> NodeKey {
        let anchor = origin
            .and_then(|k| self.view.position_of(k))
            .unwrap_or(Point2D::new(0.0, 0.0));
        let seed = Point2D::new(anchor.x + 12.0, anchor.y + 12.0);
        // `Graph::add_node` is native-only (it self-generates the UUID); on wasm the
        // host supplies it via `add_node_with_id`. mint_node opens a fresh surface, so
        // a new random id is the right one here (node_namespace_id is for convergent
        // linked-data ingest, not user-minted nodes). new_v4 works on wasm via the
        // unified uuid `js` backend.
        let key = self.graph.add_node_with_id(
            uuid::Uuid::new_v4(),
            url.to_string(),
            PortablePoint::new(seed.x, seed.y),
        );
        // Anchor the new node's history under the origin's current visit (the
        // navigated-from point) BEFORE its first visit, so that first visit
        // attaches there in the shared lineage tree (the (b) cross-node anchor).
        if let Some(origin) = origin {
            self.graph.branch_history(key, origin);
        }
        // The new surface opens on `url`: seed its own history with that first
        // visit (the node is born with one page, not an empty history).
        self.graph.navigate_node(key, url);
        if let Some(origin) = origin {
            let _ = self.graph.assert_relation(origin, key, hyperlink());
        }
        // Re-sync derived state (bodies, edges, node pool), then seed the new node
        // near the anchor and re-settle.
        self.reconcile_derived();
        self.view.set_position(key, seed);
        self.physics.seed(vec![(key, seed)]);
        self.select_only(key);
        self.physics.settle(SETTLE_TICKS);
        key
    }
}
