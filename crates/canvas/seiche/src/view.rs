/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The **read model**: a position-only view of the layout the host renders and
//! hit-tests from, decoupled from rapier.
//!
//! The live [`Simulation`](crate::Simulation) owns the rapier world and is the
//! only thing that *integrates* motion. But the host's per-frame reads — render
//! positions, frustum cull, node/edge picking, marquee select — need none of
//! rapier: every node is a uniform circle ([`crate::NODE_BODY_RADIUS`]), so each
//! read reduces to plain geometry over a `{node → position}` map. [`LayoutView`]
//! is that map plus the geometry, and [`LayoutSnapshot`] is the `Send` payload a
//! physics actor emits each tick to refresh it.
//!
//! This is the seam that lets the simulation run off the UI thread: the actor
//! owns the `Simulation` and ticks it; the host keeps a `LayoutView` it rebuilds
//! from snapshots and reads synchronously, with zero round-trips on the input or
//! render path. (See the always-offload physics-actor work, P6.)

use std::collections::HashMap;

use euclid::default::{Box2D, Point2D};
use crate::NodeKey;

use crate::{NODE_BODY_RADIUS, NodeCollider, SceneBodyId};

/// Result of [`LayoutView::rect_select`] (and [`Simulation::rect_select`]): the
/// nodes and edges a marquee covers.
///
/// [`Simulation::rect_select`]: crate::Simulation::rect_select
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RectSelection {
    /// Nodes whose center lies inside the region.
    pub nodes: Vec<NodeKey>,
    /// Edges whose segment intersects the region (either endpoint inside, or the
    /// segment crosses the rect).
    pub edges: Vec<(NodeKey, NodeKey)>,
}

/// A scene-decoration body as the host paints it: its id, world position, rotation (radians),
/// and collider shape — so the paint can match the true face (a rotated square, a hull polygon)
/// rather than a uniform orb. (Physics scenes P4b — shape-aware paint.)
#[derive(Clone, Debug, PartialEq)]
pub struct SceneBodyView {
    pub id: SceneBodyId,
    pub position: Point2D<f32>,
    /// Orientation in radians (the body's live rotation).
    pub rotation: f32,
    pub collider: NodeCollider,
    /// The prop's optional sprite handle (an opaque texture key the host resolves), carried through
    /// from its [`SceneBodySpec`](crate::SceneBodySpec). `None` paints the abstract orb / polygon.
    /// (Physics scenes — scene-prop sprites.)
    pub sprite: Option<String>,
}

/// The `Send` payload a physics actor emits each tick: every node's current
/// position plus the producer generation it was computed at (so the host can
/// drop a snapshot from a layout it has already moved past). Carries positions
/// only — edges live with the host's graph, not the simulation, so they need
/// not cross the boundary every frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutSnapshot {
    /// `(node, position)` for every body in the simulation.
    pub positions: Vec<(NodeKey, Point2D<f32>)>,
    /// Every scene-decoration body (the living backdrop / loaded scene) as a paintable
    /// [`SceneBodyView`] (id, position, rotation, shape), riding the same snapshot so it
    /// renders + animates off-thread. (Physics scenes P1 / P4b.)
    pub scene: Vec<SceneBodyView>,
    /// Fluid particle positions (the liquid pool), riding the snapshot so the pool renders + flows
    /// off-thread; `fluid_radius` is the uniform paint radius. Empty when no fluid is loaded. (P4c.)
    pub fluid: Vec<Point2D<f32>>,
    pub fluid_radius: f32,
    /// The generation this layout was produced at (monotonic on the actor).
    pub generation: u64,
}

/// A position-only, rapier-free view of the layout: a `{node → position}` map,
/// the edge topology (for edge geometry / picks), and the uniform node radius.
///
/// The host holds one and reads it synchronously each frame. Positions are
/// refreshed wholesale from a [`LayoutSnapshot`] ([`apply_snapshot`]); edges are
/// set on a structural change ([`set_edges`]); a single dragged node can be
/// pinned locally ([`set_position`]) so it tracks the cursor with no round-trip
/// while the actor catches up for its neighbors.
///
/// [`apply_snapshot`]: LayoutView::apply_snapshot
/// [`set_edges`]: LayoutView::set_edges
/// [`set_position`]: LayoutView::set_position
#[derive(Clone, Debug)]
pub struct LayoutView {
    positions: HashMap<NodeKey, Point2D<f32>>,
    edges: Vec<(NodeKey, NodeKey)>,
    radius: f32,
    /// Per-node radius overrides (a node sized away from the uniform default by
    /// size-by-degree or a per-node footprint). A node absent here picks/culls at
    /// `radius`. Set wholesale by the host from each node's `node_size / 2`, so the
    /// grab and the picture stay in sync. (Decision 5 — size drives the collider.)
    radii: HashMap<NodeKey, f32>,
    /// Scene-decoration bodies as paintable [`SceneBodyView`]s, refreshed from each snapshot —
    /// the living backdrop the host paints behind the graph. (Physics scenes P1 / P4b.)
    scene: Vec<SceneBodyView>,
    /// Fluid particle positions + uniform paint radius, refreshed from each snapshot. (P4c.)
    fluid: Vec<Point2D<f32>>,
    fluid_radius: f32,
}

impl Default for LayoutView {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutView {
    /// An empty view (no nodes, no edges), using the default node radius.
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            edges: Vec::new(),
            radius: NODE_BODY_RADIUS,
            radii: HashMap::new(),
            scene: Vec::new(),
            fluid: Vec::new(),
            fluid_radius: 0.0,
        }
    }

    /// Build a view directly from parts — positions, edge topology, and the node
    /// radius the picks/cull use. No per-node radius overrides (every node picks at
    /// `radius`); the host sets those via [`set_radii`](LayoutView::set_radii).
    pub fn from_parts(
        positions: impl IntoIterator<Item = (NodeKey, Point2D<f32>)>,
        edges: impl IntoIterator<Item = (NodeKey, NodeKey)>,
        radius: f32,
    ) -> Self {
        Self {
            positions: positions.into_iter().collect(),
            edges: edges.into_iter().collect(),
            radius,
            radii: HashMap::new(),
            scene: Vec::new(),
            fluid: Vec::new(),
            fluid_radius: 0.0,
        }
    }

    /// The pick/cull radius of a node: its per-node override if set, else the
    /// uniform default. (Decision 5 — size drives the collider.)
    fn radius_of(&self, node: NodeKey) -> f32 {
        self.radii.get(&node).copied().unwrap_or(self.radius)
    }

    /// Replace the per-node radius overrides wholesale (the host pushes each node's
    /// `node_size / 2` after a size knob moves or the graph changes structurally).
    /// Nodes absent from `radii` revert to the uniform default, so passing the
    /// current node set prunes stale entries for free.
    pub fn set_radii(&mut self, radii: impl IntoIterator<Item = (NodeKey, f32)>) {
        self.radii.clear();
        self.radii.extend(radii);
    }

    /// Replace the positions from a snapshot (the actor's per-tick output).
    /// Leaves the edge topology untouched.
    pub fn apply_snapshot(&mut self, snapshot: &LayoutSnapshot) {
        self.positions.clear();
        self.positions.extend(snapshot.positions.iter().copied());
        self.scene.clear();
        self.scene.extend(snapshot.scene.iter().cloned());
        self.fluid.clear();
        self.fluid.extend(snapshot.fluid.iter().copied());
        self.fluid_radius = snapshot.fluid_radius;
    }

    /// Replace the edge topology (after a structural graph change).
    pub fn set_edges(&mut self, edges: impl IntoIterator<Item = (NodeKey, NodeKey)>) {
        self.edges.clear();
        self.edges.extend(edges);
    }

    /// Locally override one node's position (a dragged node tracking the cursor),
    /// so the host renders it under the pointer before the actor's next snapshot.
    pub fn set_position(&mut self, node: NodeKey, position: Point2D<f32>) {
        self.positions.insert(node, position);
    }

    /// Number of nodes the view knows a position for.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Whether the view holds no node positions.
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// The current position of a node, if the view has one.
    pub fn position_of(&self, node: NodeKey) -> Option<Point2D<f32>> {
        self.positions.get(&node).copied()
    }

    /// Iterate every `(node, position)` in the view. Order is unspecified.
    pub fn positions(&self) -> impl Iterator<Item = (NodeKey, Point2D<f32>)> + '_ {
        self.positions.iter().map(|(&k, &p)| (k, p))
    }

    /// Iterate every scene-decoration body as a paintable [`SceneBodyView`] — the host's read
    /// for painting the living backdrop / loaded scene. (Physics scenes P1 / P4b.)
    pub fn scene_bodies(&self) -> impl Iterator<Item = SceneBodyView> + '_ {
        self.scene.iter().cloned()
    }

    /// Iterate every fluid particle's position — the host's read for painting the liquid pool as
    /// metaballs. (Physics scenes P4c.)
    pub fn fluid_particles(&self) -> impl Iterator<Item = Point2D<f32>> + '_ {
        self.fluid.iter().copied()
    }

    /// The uniform paint radius for fluid particles (0 when no fluid is loaded). (Physics scenes P4c.)
    pub fn fluid_radius(&self) -> f32 {
        self.fluid_radius
    }

    /// Hit-test a world-space point: the node whose circle (center, [`radius`])
    /// contains it, nearest center first; `None` if the point hits no node.
    /// Matches [`Simulation::hit_test`] for the uniform-radius world, but
    /// deterministic on overlap (nearest wins, not rapier's arbitrary first).
    ///
    /// [`radius`]: LayoutView::radius
    /// [`Simulation::hit_test`]: crate::Simulation::hit_test
    pub fn hit_test(&self, point: Point2D<f32>) -> Option<NodeKey> {
        let mut best: Option<(NodeKey, f32)> = None;
        for (&node, &center) in &self.positions {
            let r = self.radius_of(node);
            let d2 = (point - center).square_length();
            if d2 <= r * r && best.map_or(true, |(_, bd)| d2 < bd) {
                best = Some((node, d2));
            }
        }
        best.map(|(node, _)| node)
    }

    /// Frustum cull: every node whose circle's bounding box intersects `region`
    /// (world space) — i.e. whose center lies within `region` grown by the node
    /// radius. Matches [`Simulation::cull_aabb`] for the uniform-radius world.
    ///
    /// [`Simulation::cull_aabb`]: crate::Simulation::cull_aabb
    pub fn cull_aabb(&self, region: Box2D<f32>) -> Vec<NodeKey> {
        self.positions()
            .filter(|(node, center)| {
                let r = self.radius_of(*node);
                region.inflate(r, r).contains(*center)
            })
            .map(|(node, _)| node)
            .collect()
    }

    /// The live geometry of every edge: each `(a, b)` pair with both endpoints'
    /// current positions. Edges with a missing endpoint are skipped.
    pub fn edge_segments(
        &self,
    ) -> impl Iterator<Item = (NodeKey, NodeKey, Point2D<f32>, Point2D<f32>)> + '_ {
        self.edges.iter().filter_map(move |&(a, b)| {
            let pa = self.position_of(a)?;
            let pb = self.position_of(b)?;
            Some((a, b, pa, pb))
        })
    }

    /// Pick the edge whose segment passes within `tolerance` (world units) of
    /// `point`, nearest first; `None` if no edge is within tolerance. On a tie
    /// the earliest edge in topology order wins.
    pub fn edge_hit_test(&self, point: Point2D<f32>, tolerance: f32) -> Option<(NodeKey, NodeKey)> {
        let mut best: Option<((NodeKey, NodeKey), f32)> = None;
        for (a, b, pa, pb) in self.edge_segments() {
            let d = point_segment_distance(point, pa, pb);
            if d <= tolerance && best.map_or(true, |(_, bd)| d < bd) {
                best = Some(((a, b), d));
            }
        }
        best.map(|(edge, _)| edge)
    }

    /// Marquee select over a world-space region: nodes whose center lies inside
    /// `region`, and edges whose segment intersects it. Order is unspecified.
    pub fn rect_select(&self, region: Box2D<f32>) -> RectSelection {
        let nodes = self
            .positions()
            .filter(|(_, p)| region.contains(*p))
            .map(|(node, _)| node)
            .collect();
        let edges = self
            .edge_segments()
            .filter(|&(_, _, pa, pb)| segment_intersects_box(pa, pb, region))
            .map(|(a, b, _, _)| (a, b))
            .collect();
        RectSelection { nodes, edges }
    }
}

/// Shortest distance from `p` to the segment `a`–`b`. Degenerate (zero-length)
/// segments fall back to the point distance. Shared by [`LayoutView`] and the
/// live [`Simulation`](crate::Simulation)'s edge picks.
pub(crate) fn point_segment_distance(p: Point2D<f32>, a: Point2D<f32>, b: Point2D<f32>) -> f32 {
    let ab = b - a;
    let len2 = ab.square_length();
    if len2 <= f32::EPSILON {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    let proj = a + ab * t;
    (p - proj).length()
}

/// Whether the segment `a`–`b` intersects (or lies inside) the axis-aligned box
/// `r`. Liang–Barsky parametric clip: the segment touches the rect iff the
/// clipped parameter interval `[u1, u2]` is non-empty.
pub(crate) fn segment_intersects_box(a: Point2D<f32>, b: Point2D<f32>, r: Box2D<f32>) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    // Boundary "p" (direction) and "q" (distance to edge) per Liang–Barsky.
    let p = [-dx, dx, -dy, dy];
    let q = [a.x - r.min.x, r.max.x - a.x, a.y - r.min.y, r.max.y - a.y];
    let mut u1 = 0.0_f32;
    let mut u2 = 1.0_f32;
    for i in 0..4 {
        if p[i].abs() < f32::EPSILON {
            // Parallel to this boundary: wholly outside if it starts outside.
            if q[i] < 0.0 {
                return false;
            }
        } else {
            let t = q[i] / p[i];
            if p[i] < 0.0 {
                if t > u2 {
                    return false;
                }
                if t > u1 {
                    u1 = t;
                }
            } else {
                if t < u1 {
                    return false;
                }
                if t < u2 {
                    u2 = t;
                }
            }
        }
    }
    u1 <= u2
}

#[cfg(test)]
mod tests {
    use euclid::default::{Box2D, Point2D};

    use crate::{NodeKey, Simulation};

    /// a@(0,0), b@(100,0), one edge a–b — built through a real `Simulation` so
    /// the view-vs-sim parity assertions share one source of truth.
    fn sim_with_edge() -> (Simulation, NodeKey, NodeKey) {
        let a = NodeKey::new(0);
        let b = NodeKey::new(1);
        let mut sim = Simulation::new();
        sim.sync_nodes([(a, Point2D::new(0.0, 0.0)), (b, Point2D::new(100.0, 0.0))]);
        sim.sync_edges([(a, b)]);
        sim.refresh_spatial_index();
        (sim, a, b)
    }

    #[test]
    fn view_node_picks_and_cull_match_the_simulation() {
        let (sim, a, b) = sim_with_edge();
        let view = sim.view();

        // Node point-pick parity (well-separated, so rapier and the view agree).
        assert_eq!(
            view.hit_test(Point2D::new(0.0, 0.0)),
            sim.hit_test(Point2D::new(0.0, 0.0))
        );
        assert_eq!(view.hit_test(Point2D::new(0.0, 0.0)), Some(a));
        assert_eq!(view.hit_test(Point2D::new(100.0, 0.0)), Some(b));
        assert!(view.hit_test(Point2D::new(5000.0, 5000.0)).is_none());

        // Cull parity over a window around the origin (catches a, not b).
        let region = Box2D::new(Point2D::new(-30.0, -30.0), Point2D::new(30.0, 30.0));
        let mut from_view = view.cull_aabb(region);
        let mut from_sim = sim.cull_aabb(region);
        from_view.sort();
        from_sim.sort();
        assert_eq!(from_view, from_sim);
        assert_eq!(from_view, vec![a]);
    }

    #[test]
    fn view_edge_picks_match_the_simulation() {
        let (sim, a, b) = sim_with_edge();
        let view = sim.view();

        assert_eq!(
            view.edge_hit_test(Point2D::new(50.0, 0.0), 5.0),
            Some((a, b))
        );
        assert_eq!(
            view.edge_hit_test(Point2D::new(50.0, 0.0), 5.0),
            sim.edge_hit_test(Point2D::new(50.0, 0.0), 5.0)
        );
        assert!(view.edge_hit_test(Point2D::new(50.0, 50.0), 5.0).is_none());

        let near_origin = view.rect_select(Box2D::new(
            Point2D::new(-10.0, -10.0),
            Point2D::new(10.0, 10.0),
        ));
        assert_eq!(near_origin.nodes, vec![a]);
        assert_eq!(near_origin.edges, vec![(a, b)]);
    }

    #[test]
    fn snapshot_round_trips_and_drag_override_sticks() {
        let (sim, a, _b) = sim_with_edge();
        let snapshot = sim.snapshot(7);
        assert_eq!(snapshot.generation, 7);
        assert_eq!(snapshot.positions.len(), 2);

        let mut view = crate::LayoutView::new();
        view.apply_snapshot(&snapshot);
        assert_eq!(view.len(), 2);
        assert!((view.position_of(a).unwrap() - Point2D::new(0.0, 0.0)).length() < 0.5);

        // A local drag override moves the node without a new snapshot.
        view.set_position(a, Point2D::new(250.0, 250.0));
        assert_eq!(view.position_of(a), Some(Point2D::new(250.0, 250.0)));
        // ...and a later snapshot replaces it (the actor caught up).
        view.apply_snapshot(&snapshot);
        assert!((view.position_of(a).unwrap() - Point2D::new(0.0, 0.0)).length() < 0.5);
    }

    #[test]
    fn per_node_radius_grows_the_pick_and_cull() {
        let (sim, a, b) = sim_with_edge(); // a@(0,0), b@(100,0)
        let mut view = sim.view();

        // A point 40px from a's center misses at the default radius (18)...
        let near_a = Point2D::new(40.0, 0.0);
        assert!(view.hit_test(near_a).is_none());

        // ...but grow a to radius 60 and the same point now grabs it.
        view.set_radii([(a, 60.0)]);
        assert_eq!(view.hit_test(near_a), Some(a));
        // b, left at the default, is unaffected — a point 30px off b (and well clear of
        // grown a) still misses.
        assert!(view.hit_test(Point2D::new(100.0, 30.0)).is_none());

        // Cull grows with the radius too: a tight window that excludes a's center's
        // default circle still catches it once a is large enough to reach in.
        let window = Box2D::new(Point2D::new(45.0, -10.0), Point2D::new(55.0, 10.0));
        assert!(view.cull_aabb(window).contains(&a));

        // Clearing a's override (the new set names only b) reverts a to the default:
        // a point 40px straight up off a misses again (and grown b, off on the x-axis,
        // can't reach it).
        view.set_radii([(b, 60.0)]);
        assert!(view.hit_test(Point2D::new(0.0, 40.0)).is_none());
    }
}
