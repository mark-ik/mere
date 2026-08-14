// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Semantic input methods for [`Canvas`](crate::Canvas) — pointer events, wheel,
//! modifier state, and navigation. Factored from `lib.rs` to keep files under
//! the workspace 600-LOC ceiling.

use euclid::default::{Box2D, Point2D};
use kernel::geometry::PortablePoint;
use kernel::graph::apply::{self as graph_apply, GraphDelta, GraphDeltaResult, apply_graph_delta};
use kernel::graph::{
    Coupling, CouplingId, CouplingResponse, Falloff, Field, FieldDefinition, FieldExtent, FieldId,
    NodeKey, NodeSelector, ScalarField,
};

use super::build::hyperlink;
use super::edge_cells::{edge_cell_hit_test, edge_cells_in_rect};
use super::seiche_bridge::seed_cluster;
use super::{
    CLICK_SLOP, Canvas, Drag, EDGE_PICK_TOL, ORBIT_TILT_PER_PX, ORBIT_YAW_PER_PX, PointerButton,
    SETTLE_TICKS, WHEEL_PAN_SCALE, ZOOM_STEP,
};

/// World-space radius of a freshly placed field region — its `Disk` definition
/// radius and its enclosing square `Region` half-extent. A sensible default the
/// user can later resize. (Field regions P0.)
const DEFAULT_FIELD_RADIUS: f32 = 120.0;

/// Strength of a placed field's default gather coupling. The `Disk` scalar's
/// gradient is small (~falloff-slope / radius ≈ 0.01), so the strength is large to
/// make the inward pull visible — the first knob to tune if the gather is too weak
/// or too violent. (Field regions P1.)
const DEFAULT_FIELD_STRENGTH: f32 = 5000.0;

impl Canvas {
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

    /// Update whether Alt is held. A left-drag with Alt down orbits the camera (yaw + tilt)
    /// instead of picking / marqueeing. (Isometric camera — orbit gesture.)
    pub fn set_alt(&mut self, alt: bool) {
        self.alt = alt;
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
        if let Some(prev) = self.orbit_drag {
            // Alt+left orbit: horizontal drag yaws the view, vertical drag reclines the tilt
            // (toward the iso foreshorten). `set_tilt` clamps, so the camera never flips. (P2.)
            let (dx, dy) = (new.0 - prev.0, new.1 - prev.1);
            self.orbit_by(dx * ORBIT_YAW_PER_PX);
            self.set_tilt(self.camera.tilt - dy * ORBIT_TILT_PER_PX);
            self.orbit_drag = Some(new);
            self.cursor = new;
            return true;
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
                self.place_pinned_node(d.node, world);
                redraw = true;
            }
            self.drag = Some(d);
        }
        self.cursor = new;
        if self.dragging_field() {
            // A field move / resize drag follows the cursor (its box stays active);
            // the well re-aims live. (Field regions — move/resize.)
            self.drag_field_to(new);
            redraw = true;
        } else if self.update_active_field(new) {
            // Box-on-interaction: the field under the cursor shows its extent box (the
            // disk well always draws). (Field regions.)
            redraw = true;
        }
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

    /// The node under a screen-px cursor. Under the plain top-down projection this is
    /// the exact world-space collider pick (`screen_to_world` -> seiche `hit_test`, which
    /// respects each node's collider shape, including sprite hulls). Under the isometric
    /// / fake-height projection the gnodes are upright billboards drawn off their ground
    /// colliders, so instead test the cursor against each node's drawn billboard rect and
    /// take the front-most (largest projected ground depth). (Isometric camera — picking.)
    pub(crate) fn pick_at(&self, cursor: (f32, f32)) -> Option<NodeKey> {
        if !(self.is_isometric() || self.height_by_degree()) {
            return self
                .view
                .hit_test(self.screen_to_world(cursor))
                .filter(|&key| self.node_visible_in_canvas(key));
        }
        let z = self.camera.zoom;
        let (s, c) = self.camera.yaw.sin_cos();
        let mut best: Option<(NodeKey, f32)> = None;
        for (key, pos) in self
            .view
            .positions()
            .filter(|(key, _)| self.node_visible_in_canvas(*key))
        {
            let (ax, ay) = self.camera.to_screen(PortablePoint::new(pos.x, pos.y));
            let lift = self.node_height(key) * z;
            let half = self.node_size(key) * 0.5 * z;
            if (cursor.0 - ax).abs() <= half && (cursor.1 - (ay - lift)).abs() <= half {
                let depth = pos.x * s + pos.y * c;
                if best.map_or(true, |(_, d)| depth >= d) {
                    best = Some((key, depth));
                }
            }
        }
        best.map(|(k, _)| k)
    }

    /// A pointer button pressed at screen px `(x, y)`. Middle begins a pan; left
    /// grabs the node under the cursor (via [`pick_at`](Self::pick_at), projection-aware)
    /// or, on empty space, begins a marquee.
    pub fn pointer_down(&mut self, button: PointerButton, x: f32, y: f32) -> bool {
        self.cursor = (x, y);
        match button {
            PointerButton::Middle => {
                self.middle_drag = Some(self.cursor);
                self.pan_velocity = (0.0, 0.0);
            }
            PointerButton::Left => {
                if self.alt {
                    // Alt+left begins an orbit drag (yaw + tilt the camera); it owns the gesture,
                    // so no node pick / field grab / marquee starts. (Isometric camera — orbit.)
                    self.orbit_drag = Some(self.cursor);
                } else if let Some(fold) = self.fold_summary_at_screen(self.cursor) {
                    self.fold_press = Some((fold, self.cursor));
                } else if let Some(node) = self.pick_at(self.cursor) {
                    self.drag = Some(Drag {
                        node,
                        press: self.cursor,
                        moved: false,
                        was_pinned: self.pinned_nodes.contains(&node),
                    });
                } else if self.begin_field_drag(self.screen_to_world(self.cursor)) {
                    // Grabbed a field's box edge (move) or corner (resize) — the deep
                    // interior fell through, so no marquee starts. (Field regions.)
                } else {
                    self.marquee = Some(self.cursor);
                }
            }
            PointerButton::Right => {}
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
            }
            PointerButton::Left => {
                // End an orbit drag: the camera already moved live, nothing to settle. (P2.)
                if self.orbit_drag.take().is_some() {
                    return true;
                }
                if let Some((fold, press)) = self.fold_press.take() {
                    if (self.cursor.0 - press.0).hypot(self.cursor.1 - press.1) <= CLICK_SLOP {
                        return self.expand_fold(fold);
                    }
                    return true;
                }
                // End a field move / resize drag: re-aim already happened live; just
                // settle the layout into the new well and persist. (Field regions.)
                if self.end_field_drag() {
                    self.settle_physics(SETTLE_TICKS / 3);
                    return true;
                }
                if let Some(d) = self.drag.take() {
                    if d.moved {
                        self.physics.set_dragging(false);
                        if !d.was_pinned {
                            self.physics.unpin(d.node);
                            self.sync_anchor_force();
                            self.settle_physics(SETTLE_TICKS / 3);
                        }
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
                        self.selected = sel
                            .nodes
                            .into_iter()
                            .filter(|&key| self.node_visible_in_canvas(key))
                            .collect();
                        self.selected_edges =
                            edge_cells_in_rect(&self.graph, &self.view, &self.hidden_edges, region)
                                .into_iter()
                                .filter(|cell| {
                                    self.node_visible_in_canvas(cell.from)
                                        && self.node_visible_in_canvas(cell.to)
                                })
                                .collect();
                    } else if !self.shift {
                        // A bare empty click clears the selection (and may pick an
                        // edge under the cursor). With Shift held it is a no-op, so a
                        // multi-select in progress is preserved.
                        let world = self.screen_to_world(self.cursor);
                        let tol = EDGE_PICK_TOL / self.camera.zoom.max(f32::EPSILON);
                        self.selected.clear();
                        self.selected_edges.clear();
                        if let Some(cell) = edge_cell_hit_test(
                            &self.graph,
                            &self.view,
                            &self.hidden_edges,
                            world,
                            tol,
                        )
                        .filter(|cell| {
                            self.node_visible_in_canvas(cell.from)
                                && self.node_visible_in_canvas(cell.to)
                        }) {
                            self.selected_edges.insert(cell);
                        }
                    }
                    true
                } else {
                    false
                }
            }
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
        self.settle_physics(SETTLE_TICKS);
        true
    }

    /// Hold the single focused node at its current visual position. This is
    /// view-local curation: it does not write coordinates into the graph.
    pub fn pin_focused(&mut self) -> bool {
        let Some(key) = self.focused_key() else {
            return false;
        };
        let Some(position) = self.view.position_of(key) else {
            return false;
        };
        self.pinned_nodes.insert(key);
        self.place_pinned_node(key, position);
        true
    }

    /// Nudge the single focused node in world coordinates, holding it in its
    /// new position. Hosts map keyboard arrows into their chosen world step.
    /// The node stays held until [`release_focused`](Self::release_focused).
    pub fn nudge_focused(&mut self, dx: f32, dy: f32) -> bool {
        let Some(key) = self.focused_key() else {
            return false;
        };
        let Some(position) = self.view.position_of(key) else {
            return false;
        };
        let next = Point2D::new(position.x + dx, position.y + dy);
        self.pinned_nodes.insert(key);
        self.place_pinned_node(key, next);
        self.settle_physics(SETTLE_TICKS / 3);
        true
    }

    /// Screen-space twin of [`nudge_focused`](Self::nudge_focused). Keyboard
    /// hosts normally want a stable visual increment even while zoomed; this
    /// converts their pixel step into the canvas's world coordinates.
    pub fn nudge_focused_screen(&mut self, dx: f32, dy: f32) -> bool {
        let zoom = self.camera.zoom.max(f32::EPSILON);
        self.nudge_focused(dx / zoom, dy / zoom)
    }

    /// Return the single focused node from an explicit pin to dynamic layout.
    /// The solver gets a short settle budget so adjacent nodes visibly respond.
    pub fn release_focused(&mut self) -> bool {
        let Some(key) = self.focused_key() else {
            return false;
        };
        if !self.pinned_nodes.remove(&key) {
            return false;
        }
        self.physics.unpin(key);
        self.sync_anchor_force();
        self.settle_physics(SETTLE_TICKS / 3);
        true
    }

    /// Pin a node in the solver, update the local read model immediately, and
    /// preserve a paused analytic arrangement's slot. The graph remains
    /// position-free throughout.
    fn place_pinned_node(&mut self, node: NodeKey, world: Point2D<f32>) {
        self.physics.pin(node, world);
        self.view.set_position(node, world);
        if let Some(positions) = self.strategy_positions.as_mut()
            && let Some((_, position)) = positions.iter_mut().find(|(key, _)| *key == node)
        {
            *position = PortablePoint::new(world.x, world.y);
        }
    }

    /// Visit `url`: if a node with that URL already exists (URL identity), select
    /// it; otherwise add a new node — linked from the currently-selected node, if
    /// any, so the browse trail grows as graph structure — re-sync the physics and
    /// node-children pool, and re-settle. Returns the node either way. The host
    /// calls this on navigation (the graph-rooted browse loop, S2).
    pub fn visit(&mut self, url: &str) -> NodeKey {
        if let Some((key, _)) = self.graph.get_node_by_url(url) {
            // A revisit is graph truth, not merely a focus change: the P3
            // projection adapter turns this durable recency into score order
            // and optional size scaling. Route it through the graph delta so
            // replay/persistence see the same visit.
            let _ = apply_graph_delta(&mut self.graph, GraphDelta::TouchNodeLastVisited { key });
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
        let origin_key = origin
            .and_then(|m| self.graph.get_node_by_id(m))
            .map(|(k, _)| k);
        let key = self.mint_node(origin_key, url);
        self.graph
            .get_node(key)
            .map(|n| n.id)
            .expect("a freshly minted node has an id")
    }

    /// The node under a screen point (canvas-leaf-local px), if any — the host's hit-test
    /// for routing a drop / gesture onto a node (e.g. dropping an image file to set that
    /// node's sprite face). Mirrors the left-press hit-test; returns the member id.
    /// (Node representation P2 — sprite drop.)
    pub fn node_at_screen(&self, sx: f32, sy: f32) -> Option<uuid::Uuid> {
        self.pick_at((sx, sy))
            .and_then(|key| self.graph.get_node(key).map(|n| n.id))
    }

    /// Re-mint a deleted node from its bin record WITH ITS ORIGINAL member id —
    /// identity restored, so anything that keyed the node by uuid (facets,
    /// provenance, a host's sidecars) resolves to the recovered node again.
    /// Restores title and tags too (node truth that does not re-derive from a
    /// re-fetch). Edges are not restored (they left with the node); recovering
    /// a full subgraph is a later fidelity step. (Recover-deleted-node; the
    /// turnstone recycle-bin plan, 2026-07-20 — supersedes the fresh-mint Lane 0.)
    pub fn recover_node(
        &mut self,
        id: uuid::Uuid,
        url: &str,
        title: Option<&str>,
        tags: &[String],
    ) -> uuid::Uuid {
        // Idempotent: if the node already exists (an earlier recover, or the
        // url re-opened by hand under the same id), select it and stand down —
        // never mint a second node under one identity.
        if let Some(key) = self.graph.get_node_key_by_id(id) {
            self.select_only(key);
            return id;
        }
        let key = self.mint_node_as(None, url, id);
        if let Some(title) = title.filter(|t| !t.is_empty()) {
            let _ = apply_graph_delta(
                &mut self.graph,
                GraphDelta::SetNodeTitle {
                    key,
                    title: title.to_string(),
                },
            );
        }
        for tag in tags {
            let _ = apply_graph_delta(
                &mut self.graph,
                GraphDelta::InsertNodeTag {
                    key,
                    tag: tag.clone(),
                },
            );
        }
        self.graph
            .get_node(key)
            .map(|n| n.id)
            .expect("a freshly minted node has an id")
    }

    /// Mint a fresh unlinked node at the cursor's world position — the empty-space
    /// right-click "Add node" gesture. `content_band_xy` is the canvas-leaf-local
    /// cursor point (screen px); the camera inversion happens here, so the host
    /// needn't reach the crate-private [`screen_to_world`](Self::screen_to_world).
    /// Returns the new node's member id.
    pub fn add_node_at(&mut self, content_band_xy: (f32, f32), url: &str) -> uuid::Uuid {
        let world = self.screen_to_world(content_band_xy);
        let key = self.mint_node_at(world, url);
        self.graph
            .get_node(key)
            .map(|n| n.id)
            .expect("a freshly minted node has an id")
    }

    /// Place a fresh disk field region at the cursor's world position — the empty-space
    /// "Add field" gesture, the spatial-rule twin of [`add_node_at`](Self::add_node_at).
    /// `content_band_xy` is the canvas-leaf-local cursor point (screen px); the camera
    /// inversion happens here. The field is a `Disk` scalar definition inside a square
    /// `Region` extent centered on the point (the placed-extent the rules later evaluate
    /// over). Inert until coupled/projected; returns the new field's id. (Field regions P0.)
    pub fn add_field_at(&mut self, content_band_xy: (f32, f32)) -> uuid::Uuid {
        let world = self.screen_to_world(content_band_xy);
        let radius = DEFAULT_FIELD_RADIUS;
        let uuid = uuid::Uuid::new_v4();
        let id = FieldId::from_uuid(uuid);
        let definition = FieldDefinition::Scalar(ScalarField::disk_at(
            world.x,
            world.y,
            radius,
            Falloff::Smoothstep,
        ));
        let extent = FieldExtent::Region {
            min_x: world.x - radius,
            min_y: world.y - radius,
            max_x: world.x + radius,
            max_y: world.y + radius,
        };
        let _ = apply_graph_delta(
            &mut self.graph,
            GraphDelta::AddField {
                field: Field::new(id, definition).with_extent(extent),
            },
        );
        // The no-placebo gesture: a default coupling so the placed field immediately
        // *does* something — its nodes gather toward the disk's center. The disk is a
        // peak (1 at center → 0 at the radius), so `RepelFromMax` (force along +grad,
        // up the gradient toward the peak) pulls nodes inward; `NodeSelector::All` is
        // safe because the disk's bounded falloff zeroes the force outside the radius.
        // Add it to the graph, then push the resolved force into the live sim so it
        // pulls without a position-losing rebuild. (Field regions P1.)
        let coupling = Coupling::new(
            CouplingId::from_uuid(uuid::Uuid::new_v4()),
            id,
            NodeSelector::All,
            CouplingResponse::RepelFromMax,
            DEFAULT_FIELD_STRENGTH,
        );
        let _ = apply_graph_delta(&mut self.graph, GraphDelta::AddCoupling { coupling });
        // Rebuild the live coupling forces (re-resolving every coupling against the
        // current nodes) so the new field's well pulls immediately, then settle.
        // (Field regions — rebuild-on-mutation.)
        self.rebuild_coupling_forces();
        self.settle_physics(SETTLE_TICKS);
        uuid
    }

    /// Field `id`'s current coupling strength (the per-field force-well), for the
    /// roster's strength readout. `None` if the field has no coupling. The default a
    /// fresh field starts at is [`DEFAULT_FIELD_STRENGTH`]. (Field regions — strength.)
    pub fn field_strength(&self, id: FieldId) -> Option<f32> {
        self.graph.field_coupling_strength(id)
    }

    /// Set field `id`'s coupling strength (the per-field force), re-resolve its well
    /// into the live sim, and re-settle so the change pulls at once. Returns whether
    /// the field was found. Strength is graph truth, so the host persists on a
    /// change. (Field regions — strength tuning.)
    pub fn set_field_strength(&mut self, id: FieldId, strength: f32) -> bool {
        let changed = matches!(
            apply_graph_delta(
                &mut self.graph,
                GraphDelta::SetFieldCouplingStrength {
                    field: id,
                    strength
                },
            ),
            GraphDeltaResult::FieldChanged(true)
        );
        if changed {
            self.rebuild_coupling_forces();
            self.settle_physics(SETTLE_TICKS);
        }
        changed
    }

    /// Toggle the layout physics between paused (frozen) and running. Pausing halts
    /// the sim; resuming kicks a fresh settle. (Physics pause — Space / the button.)
    pub fn toggle_physics_paused(&mut self) {
        self.set_physics_paused(!self.physics_paused);
    }

    /// Pause or run the layout physics. **Global and orthogonal to the
    /// arrangement**: physics is a capability of the whole graph (like the size
    /// or shape channels), not a property of one layout mode. Every arrangement
    /// composes with either state — a paused Spiral holds its analytic
    /// placement exactly, a running Spiral seeds the sim and lets forces relax
    /// from there, and "force-directed" is simply *no* analytic arrangement
    /// with physics running. (Physics as a capability.)
    pub fn set_physics_paused(&mut self, paused: bool) {
        self.physics_paused = paused;
        // The arrangement's anchor springs only exist while playing (paused, the
        // buffered positions are asserted directly), so the pull follows the
        // pause. (Arrangement as attractor.)
        self.sync_anchor_force();
        if self.physics_paused {
            self.physics.halt();
        } else {
            // Resuming via the pause/play control means "run so I can watch": settle
            // for effectively forever (until the next pause) so a field's pull plays
            // out fully, instead of resting after the normal ~6s budget. The cold-
            // start auto-settle (node/field adds) keeps its short budget; only the
            // explicit play enters this continuous run. (Physics pause — continuous run.)
            self.settle_physics(u32::MAX);
        }
    }

    /// Whether the layout physics is currently paused (for the host's pause/play glyph).
    pub fn physics_paused(&self) -> bool {
        self.physics_paused
    }

    /// Set the linear damping (the "inertia" physics setting) on this canvas's
    /// bodies: lower preserves more drift after a settle, higher rests sooner. Takes
    /// effect immediately on the live bodies, then a short settle lets the new
    /// damping express itself. The host owns the setting value (persisted); this
    /// just applies it. (Physics settings.)
    pub fn set_physics_damping(&mut self, damping: f32) {
        self.physics.set_linear_damping(damping);
        self.settle_physics(SETTLE_TICKS / 3);
    }

    /// Request a settle of `ticks`, unless the physics is paused — a paused graph
    /// stays frozen through mutations until the user resumes. The single gate every
    /// settle trigger routes through (the only direct `physics.settle` caller).
    /// (Physics pause.)
    pub(crate) fn settle_physics(&mut self, ticks: u32) {
        if !self.physics_paused {
            self.physics.settle(ticks);
        }
    }

    /// Mint an unlinked node at an explicit world `seed`, selecting it. The
    /// origin-less twin of [`mint_node`](Self::mint_node): no navigated-from edge,
    /// no branched history — a fresh graphlet candidate placed exactly where asked.
    fn mint_node_at(&mut self, seed: Point2D<f32>, url: &str) -> NodeKey {
        let key = graph_apply::add_node(
            &mut self.graph,
            Some(uuid::Uuid::new_v4()),
            url.to_string(),
            PortablePoint::new(seed.x, seed.y),
        );
        let _ = apply_graph_delta(
            &mut self.graph,
            GraphDelta::NavigateNode {
                key,
                url: url.to_string(),
            },
        );
        // Containment before the topology hook, so a node minted under a
        // folder belongs to it from its first frame rather than from the next
        // load (see `Graph::derive_containment_for`).
        self.graph.derive_containment_for(key);
        self.reconcile_derived();
        self.view.set_position(key, seed);
        self.physics.seed(vec![(key, seed)]);
        self.select_only(key);
        self.settle_physics(SETTLE_TICKS);
        key
    }

    /// Mint a brand-new node at `url`, seeded just off `origin`, linked back to it
    /// with a navigated-from edge when `origin` is present, and with its own
    /// within-node history seeded so its back/forward work from birth. Selects it
    /// and restarts the settle. Shared by [`visit`](Self::visit)'s create branch
    /// and [`open_member_as_new_node`](Self::open_member_as_new_node).
    fn mint_node(&mut self, origin: Option<NodeKey>, url: &str) -> NodeKey {
        self.mint_node_as(origin, url, uuid::Uuid::new_v4())
    }

    /// [`Self::mint_node`] with the member id CALLER-CHOSEN: the recovery path
    /// re-mints a deleted node under its original id (identity restored); every
    /// fresh-surface path mints a new v4 through the wrapper above.
    fn mint_node_as(&mut self, origin: Option<NodeKey>, url: &str, id: uuid::Uuid) -> NodeKey {
        let anchor = origin
            .and_then(|k| self.view.position_of(k))
            .unwrap_or(Point2D::new(0.0, 0.0));
        let seed = Point2D::new(anchor.x + 12.0, anchor.y + 12.0);
        // `Graph::add_node` is native-only (it self-generates the UUID); on wasm the
        // host supplies it via `add_node_with_id`. A fresh surface mints a new random
        // id (node_namespace_id is for convergent linked-data ingest, not user-minted
        // nodes); new_v4 works on wasm via the unified uuid `js` backend.
        let key = graph_apply::add_node(
            &mut self.graph,
            Some(id),
            url.to_string(),
            PortablePoint::new(seed.x, seed.y),
        );
        // Anchor the new node's history under the origin's current visit (the
        // navigated-from point) BEFORE its first visit, so that first visit
        // attaches there in the shared lineage tree (the (b) cross-node anchor).
        if let Some(origin) = origin {
            let _ = apply_graph_delta(
                &mut self.graph,
                GraphDelta::BranchHistory {
                    child: key,
                    parent: origin,
                },
            );
        }
        // The new surface opens on `url`: seed its own history with that first
        // visit (the node is born with one page, not an empty history).
        let _ = apply_graph_delta(
            &mut self.graph,
            GraphDelta::NavigateNode {
                key,
                url: url.to_string(),
            },
        );
        if let Some(origin) = origin {
            let _ = graph_apply::assert_relation(&mut self.graph, origin, key, hyperlink());
        }
        // Re-sync derived state (bodies, edges, node pool), then seed the new node
        // near the anchor and re-settle.
        // Containment before the topology hook, so a node minted under a
        // folder belongs to it from its first frame rather than from the next
        // load (see `Graph::derive_containment_for`).
        self.graph.derive_containment_for(key);
        self.reconcile_derived();
        self.view.set_position(key, seed);
        self.physics.seed(vec![(key, seed)]);
        self.select_only(key);
        self.settle_physics(SETTLE_TICKS);
        key
    }
}
