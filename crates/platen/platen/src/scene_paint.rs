/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Render a cartography [`Projection`] to a `paint_list_api` paint list — the
//! orrery's host-agnostic scene underlay (the serval-as-host eval's "Layer 1").
//!
//! Platen is "the press": it already dispatches strategies to a `Projection`
//! (see [`crate::cartography_scene`]); this turns that projection into the
//! engine-agnostic [`PaintCmd`] stream `netrender` renders, the *same* output
//! whether the host is Masonry today or serval later. Nodes render as rects and
//! edges as straight strokes under a camera transform; richer visuals (glyphs,
//! routed edges, external-texture node content) layer on later.
//!
//! This is deliberately minimal and does *not* reach into the 9.6k-LOC
//! `graph-canvas` crate (whose own physics is superseded by `gyre` and whose
//! projection overlaps cartography). It works off the cartography `Projection`
//! abstraction, so any strategy's output — analytic or an gyre layout rebuilt
//! into a projection — paints through one path.

use std::collections::HashMap;

use cartography::Projection;
use kernel::geometry::PortablePoint;
use kernel::graph::NodeKey;
use paint_list_api::{
    ColorF, CommonPlacement, DeviceIntSize, EngineId, LayoutPoint, LayoutRect, LayoutTransform,
    PaintCmd, PaintList, PathCommand, PathData, RectItem, StrokeCap, StrokeItem, StrokeJoin,
    TransformKind, TransformSpec,
};
use serde::{Deserialize, Serialize};

/// A concrete [`PaintList`] for canvas chrome (the orrery). Chrome is not a
/// content engine, so it carries [`EngineId::UNASSIGNED`]; a dedicated canvas
/// engine id is a small `paint_list_api` follow-up if chrome paint ever needs
/// to be distinguished downstream.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CanvasPaintList {
    commands: Vec<PaintCmd>,
    viewport: DeviceIntSize,
    generation: u64,
}

impl PaintList for CanvasPaintList {
    fn engine_id(&self) -> EngineId {
        EngineId::UNASSIGNED
    }
    fn viewport(&self) -> DeviceIntSize {
        self.viewport
    }
    fn generation_id(&self) -> u64 {
        self.generation
    }
    fn commands(&self) -> &[PaintCmd] {
        &self.commands
    }
}

impl CanvasPaintList {
    /// Splice world-space overlay commands in just inside the camera transform
    /// (immediately before the trailing `PopTransform`), so they share the
    /// scene's world→view mapping. Lists from [`paint_projection`] always end in
    /// that `PopTransform`; on the (unreachable) empty list this is a no-op tail
    /// insert. Used by the visual-coupling pass ([`crate::coupling_paint`]).
    pub fn splice_world_overlays(&mut self, overlays: impl IntoIterator<Item = PaintCmd>) {
        let before_pop = self.commands.len().saturating_sub(1);
        self.commands.splice(before_pop..before_pop, overlays);
    }

    /// Splice world-space overlay commands in at the **bottom** of the camera
    /// transform (immediately after the leading `PushTransform`), so they share the
    /// scene's world→view mapping but paint *beneath* the projection's nodes + edges.
    /// The under-content twin of [`splice_world_overlays`] — for background regions
    /// like field extents, which the graph should appear to sit *within*. On the
    /// (unreachable) empty list this is a head insert.
    pub fn splice_world_underlay(&mut self, overlays: impl IntoIterator<Item = PaintCmd>) {
        let after_push = self.commands.len().min(1);
        self.commands.splice(after_push..after_push, overlays);
    }
}

/// World-to-view camera: pan, uniform zoom, plus an isometric orbit (`yaw`) and
/// vertical foreshorten (`tilt`). The forward map is [`Camera::to_screen`], the
/// inverse [`Camera::to_world`], and [`Camera::ground_transform`] is the affine the
/// scene `PushTransform` carries for ground-plane paint. Plain top-down is the
/// degenerate isometric at `yaw == 0.0`, `tilt == 1.0`, where the map reduces
/// exactly to `screen = world * zoom + offset`. (Isometric camera P0.)
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub offset: (f32, f32),
    pub zoom: f32,
    /// Orbit angle about the vertical, radians. `0.0` is top-down.
    pub yaw: f32,
    /// Vertical foreshorten in `(0, 1]`. `1.0` is no foreshorten (top-down 2D); a
    /// classic isometric squash is ~`0.55`.
    pub tilt: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            offset: (0.0, 0.0),
            zoom: 1.0,
            yaw: 0.0,
            tilt: 1.0,
        }
    }
}

impl Camera {
    /// Project a world point to screen px. At `(yaw 0, tilt 1)` this is exactly
    /// `world * zoom + offset`; otherwise it rotates the ground plane by `yaw`,
    /// foreshortens the vertical by `tilt`, scales by `zoom`, and translates by
    /// `offset`. (Isometric camera P0.)
    pub fn to_screen(&self, world: PortablePoint) -> (f32, f32) {
        let (s, c) = self.yaw.sin_cos();
        let rx = world.x * c - world.y * s;
        let ry = world.x * s + world.y * c;
        (
            rx * self.zoom + self.offset.0,
            ry * self.tilt * self.zoom + self.offset.1,
        )
    }

    /// The inverse of [`Camera::to_screen`]: a screen-px point back to world space.
    /// At `(yaw 0, tilt 1)` this is `(screen - offset) / zoom`. (Isometric camera P0.)
    pub fn to_world(&self, screen: (f32, f32)) -> PortablePoint {
        let z = self.zoom.max(f32::EPSILON);
        let t = self.tilt.max(f32::EPSILON);
        let px = (screen.0 - self.offset.0) / z;
        let py = (screen.1 - self.offset.1) / (z * t);
        let (s, c) = self.yaw.sin_cos();
        // Inverse rotation (rotate by -yaw).
        PortablePoint::new(px * c + py * s, -px * s + py * c)
    }

    /// The world->screen affine the scene `PushTransform` carries for **ground-plane**
    /// paint (edges, field regions, demoted dots), so they recline onto the floor while
    /// node cards stay upright billboards via [`Camera::to_screen`]. Built to match
    /// `to_screen` exactly: rotate the ground by `yaw` (Z), foreshorten the vertical by
    /// `tilt`, scale by `zoom`, translate by `offset`. Row-major, per euclid's
    /// `transform_point2d` (`x' = x*m11 + y*m21 + m41`, `y' = x*m12 + y*m22 + m42`); at
    /// `yaw 0` it reduces to `scale(zoom, tilt*zoom).then(translate(offset))`, and at
    /// `(yaw 0, tilt 1)` to the plain top-down map. (Isometric camera P2 — yaw orbit.)
    pub fn ground_transform(&self) -> LayoutTransform {
        let (s, c) = self.yaw.sin_cos();
        let z = self.zoom;
        let tz = self.tilt * self.zoom;
        LayoutTransform::new(
            c * z, s * tz, 0.0, 0.0,
            -s * z, c * tz, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            self.offset.0, self.offset.1, 0.0, 1.0,
        )
    }
}

/// Visual knobs for the scene paint. Public so the host can theme it (the
/// `register-theme` wiring lands later).
#[derive(Clone, Copy, Debug)]
pub struct ScenePaintStyle {
    pub node_color: ColorF,
    /// Node half-extent used when a `PositionedNode` reports `radius == 0`.
    pub default_node_radius: f32,
    pub edge_color: ColorF,
    pub edge_width: f32,
}

impl Default for ScenePaintStyle {
    fn default() -> Self {
        Self {
            node_color: ColorF::new(0.55, 0.72, 0.92, 1.0),
            default_node_radius: 18.0,
            edge_color: ColorF::new(0.5, 0.5, 0.55, 0.7),
            edge_width: 1.5,
        }
    }
}

fn pt(p: PortablePoint) -> LayoutPoint {
    LayoutPoint::new(p.x, p.y)
}

/// Render a [`Projection`] into a [`CanvasPaintList`]: a camera transform, then
/// an edge stroke per edge (its routed `path` if present, else a straight line
/// between the endpoint node positions) and a node rect per node. Edges paint
/// under nodes.
pub fn paint_projection(
    projection: &Projection,
    viewport: DeviceIntSize,
    camera: Camera,
    style: &ScenePaintStyle,
    generation: u64,
) -> CanvasPaintList {
    paint_projection_filtered(projection, viewport, camera, style, generation, |_| true)
}

/// [`paint_projection`], but a node's rect is painted only when `draw_node`
/// returns `true` for it. Edges always use every node's position (so an edge to
/// an undrawn node still renders), so the orrery can **demote** off-screen nodes
/// to the underlay (drawn as rects here) while on-screen nodes are drawn as
/// richer host DOM and excluded from this pass — see
/// [`crate::orrery::orrery_paint_list_demoted`].
pub fn paint_projection_filtered(
    projection: &Projection,
    viewport: DeviceIntSize,
    camera: Camera,
    style: &ScenePaintStyle,
    generation: u64,
    draw_node: impl Fn(NodeKey) -> bool,
) -> CanvasPaintList {
    let mut commands = Vec::with_capacity(projection.edges.len() + projection.nodes.len() + 2);

    let transform = camera.ground_transform();
    commands.push(PaintCmd::PushTransform(TransformSpec {
        origin: LayoutPoint::zero(),
        transform,
        kind: TransformKind::Standard,
    }));

    let node_pos: HashMap<NodeKey, PortablePoint> = projection
        .nodes
        .iter()
        .map(|n| (n.node, n.position))
        .collect();
    // Each node's drawn radius (its own, or the style default for an un-sized node) —
    // used to trim a straight edge so it meets the face, not the center.
    let node_radius: HashMap<NodeKey, f32> = projection
        .nodes
        .iter()
        .map(|n| (n.node, if n.radius > 0.0 { n.radius } else { style.default_node_radius }))
        .collect();
    let radius_of = |key: &NodeKey| node_radius.get(key).copied().unwrap_or(style.default_node_radius);

    // Edges first (painted under the nodes).
    for edge in &projection.edges {
        let polyline: Vec<LayoutPoint> = if edge.path.len() >= 2 {
            // A routed path is explicit — paint it as given, no face trim.
            edge.path.iter().map(|p| pt(*p)).collect()
        } else {
            match (node_pos.get(&edge.from), node_pos.get(&edge.to)) {
                // A straight edge stops at each endpoint's face (radius) rather than
                // plunging to its center, so a sized node looks connected at its
                // visual edge. (Node-rep Decision 5 — face geometry.)
                (Some(&a), Some(&b)) => {
                    trim_to_faces(a, b, radius_of(&edge.from), radius_of(&edge.to))
                },
                _ => continue,
            }
        };
        let mut path_cmds = Vec::with_capacity(polyline.len());
        path_cmds.push(PathCommand::MoveTo(polyline[0]));
        for p in &polyline[1..] {
            path_cmds.push(PathCommand::LineTo(*p));
        }
        commands.push(PaintCmd::DrawStroke(StrokeItem {
            placement: CommonPlacement::new(bounds_of(&polyline)),
            path: PathData { commands: path_cmds },
            color: style.edge_color,
            // Edge weight (the multigraph multiplicity) thickens the stroke: weight 1 paints the
            // normal width, heavier pairs scale up, capped so a busy pair stays legible. (Graph
            // signals — edge-weight encoding.)
            width: (style.edge_width * edge.weight.max(1.0)).min(style.edge_width * 4.0),
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            dash: None,
        }));
    }

    // Nodes (only those the filter draws; edges above used every position).
    for node in &projection.nodes {
        if !draw_node(node.node) {
            continue;
        }
        let r = if node.radius > 0.0 {
            node.radius
        } else {
            style.default_node_radius
        };
        let c = pt(node.position);
        let bounds = LayoutRect::new(
            LayoutPoint::new(c.x - r, c.y - r),
            LayoutPoint::new(c.x + r, c.y + r),
        );
        commands.push(PaintCmd::DrawRect(RectItem {
            placement: CommonPlacement::new(bounds),
            color: style.node_color,
        }));
    }

    commands.push(PaintCmd::PopTransform);

    CanvasPaintList {
        commands,
        viewport,
        generation,
    }
}

/// A straight segment `a`–`b` shortened so each end stops at the node's face — `a`
/// pulled in by `ra`, `b` by `rb` — instead of meeting at the centers. When the
/// centers are closer than the two radii (overlapping / touching faces, so there is
/// no clean gap to draw into) the segment falls back to center-to-center, which is no
/// worse than the un-trimmed line and avoids a reversed stroke. (Node-rep Decision 5.)
fn trim_to_faces(a: PortablePoint, b: PortablePoint, ra: f32, rb: f32) -> Vec<LayoutPoint> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist <= ra + rb + 1.0 {
        return vec![pt(a), pt(b)];
    }
    let (ux, uy) = (dx / dist, dy / dist);
    vec![
        LayoutPoint::new(a.x + ux * ra, a.y + uy * ra),
        LayoutPoint::new(b.x - ux * rb, b.y - uy * rb),
    ]
}

/// Axis-aligned bounds of a non-empty polyline.
fn bounds_of(points: &[LayoutPoint]) -> LayoutRect {
    let mut min = points[0];
    let mut max = points[0];
    for p in &points[1..] {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    LayoutRect::new(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::projection::{PositionedEdge, PositionedNode};

    fn node(key: NodeKey, x: f32, y: f32, radius: f32) -> PositionedNode {
        PositionedNode {
            node: key,
            position: PortablePoint::new(x, y),
            radius,
        }
    }

    #[test]
    fn paints_camera_then_edges_then_nodes() {
        let a = NodeKey::new(0);
        let b = NodeKey::new(1);
        let projection = Projection {
            nodes: vec![node(a, 0.0, 0.0, 10.0), node(b, 100.0, 0.0, 0.0)],
            edges: vec![PositionedEdge {
                edge: None,
                from: a,
                to: b,
                path: Vec::new(),
                weight: 1.0,
            }],
            ..Projection::empty()
        };

        let list = paint_projection(
            &projection,
            DeviceIntSize::new(800, 600),
            Camera::default(),
            &ScenePaintStyle::default(),
            7,
        );
        let cmds = list.commands();

        // PushTransform + 1 edge stroke + 2 node rects + PopTransform.
        assert_eq!(cmds.len(), 5);
        assert!(matches!(cmds.first(), Some(PaintCmd::PushTransform(_))));
        assert!(matches!(cmds.last(), Some(PaintCmd::PopTransform)));
        assert_eq!(
            cmds.iter()
                .filter(|c| matches!(c, PaintCmd::DrawStroke(_)))
                .count(),
            1
        );
        assert_eq!(
            cmds.iter()
                .filter(|c| matches!(c, PaintCmd::DrawRect(_)))
                .count(),
            2
        );
        assert_eq!(list.engine_id(), EngineId::UNASSIGNED);
        assert_eq!(list.viewport(), DeviceIntSize::new(800, 600));
        assert_eq!(list.generation_id(), 7);
    }

    #[test]
    fn edge_weight_scales_the_stroke_width() {
        // a-b at unit weight, c-d at the multigraph multiplicity of 2: the heavier pair paints a
        // proportionally thicker stroke. This is the paint half of the multiplicity encoding; since
        // the orrery's universal underlay path (project_keys -> paint_projection_filtered) feeds
        // weighted edges in EVERY layout mode, edge thickness shows under analytic layouts too, not
        // only force-directed. (Graph signals — edge-weight encoding.)
        let (a, b, c, d) = (NodeKey::new(0), NodeKey::new(1), NodeKey::new(2), NodeKey::new(3));
        let projection = Projection {
            nodes: vec![
                node(a, 0.0, 0.0, 0.0),
                node(b, 100.0, 0.0, 0.0),
                node(c, 0.0, 100.0, 0.0),
                node(d, 100.0, 100.0, 0.0),
            ],
            edges: vec![
                PositionedEdge { edge: None, from: a, to: b, path: Vec::new(), weight: 1.0 },
                PositionedEdge { edge: None, from: c, to: d, path: Vec::new(), weight: 2.0 },
            ],
            ..Projection::empty()
        };
        let style = ScenePaintStyle::default();
        let list = paint_projection(
            &projection,
            DeviceIntSize::new(800, 600),
            Camera::default(),
            &style,
            0,
        );
        let mut widths: Vec<f32> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                PaintCmd::DrawStroke(s) => Some(s.width),
                _ => None,
            })
            .collect();
        widths.sort_by(|x, y| x.partial_cmp(y).unwrap());
        assert_eq!(widths.len(), 2, "one stroke per edge");
        assert!((widths[0] - style.edge_width).abs() < 1e-4, "the unit pair paints the default width");
        assert!(
            (widths[1] - style.edge_width * 2.0).abs() < 1e-4,
            "the weight-2 pair paints twice as thick: {}",
            widths[1]
        );
    }

    #[test]
    fn edge_with_missing_endpoint_is_skipped() {
        let a = NodeKey::new(0);
        let ghost = NodeKey::new(99);
        let projection = Projection {
            nodes: vec![node(a, 0.0, 0.0, 10.0)],
            edges: vec![PositionedEdge {
                edge: None,
                from: a,
                to: ghost,
                path: Vec::new(),
                weight: 1.0,
            }],
            ..Projection::empty()
        };
        let list = paint_projection(
            &projection,
            DeviceIntSize::new(10, 10),
            Camera::default(),
            &ScenePaintStyle::default(),
            0,
        );
        // PushTransform + 1 node rect + PopTransform; the dangling edge is skipped.
        assert_eq!(list.commands().len(), 3);
    }

    #[test]
    fn empty_projection_is_just_the_camera_frame() {
        let list = paint_projection(
            &Projection::empty(),
            DeviceIntSize::new(10, 10),
            Camera::default(),
            &ScenePaintStyle::default(),
            0,
        );
        assert_eq!(list.commands().len(), 2);
    }

    #[test]
    fn trim_to_faces_pulls_endpoints_to_the_radii() {
        let a = PortablePoint::new(0.0, 0.0);
        let b = PortablePoint::new(100.0, 0.0);
        // a's face at r=10, b's at r=18 → the stroke runs 10..82 along the x-axis.
        let seg = trim_to_faces(a, b, 10.0, 18.0);
        assert_eq!(seg.len(), 2);
        assert!((seg[0].x - 10.0).abs() < 1e-4 && seg[0].y.abs() < 1e-4, "near end at a's face");
        assert!((seg[1].x - 82.0).abs() < 1e-4 && seg[1].y.abs() < 1e-4, "far end at b's face");

        // Faces that overlap (radii exceed the gap) fall back to center-to-center so
        // the stroke never reverses.
        let seg = trim_to_faces(a, b, 60.0, 60.0);
        assert!((seg[0].x - 0.0).abs() < 1e-4, "fallback keeps a's center");
        assert!((seg[1].x - 100.0).abs() < 1e-4, "fallback keeps b's center");
    }

    #[test]
    fn camera_top_down_is_plain_scale_translate() {
        let cam = Camera { offset: (10.0, 20.0), zoom: 2.0, yaw: 0.0, tilt: 1.0 };
        assert_eq!(
            cam.to_screen(PortablePoint::new(5.0, 7.0)),
            (5.0 * 2.0 + 10.0, 7.0 * 2.0 + 20.0)
        );
        let w = cam.to_world((30.0, 40.0));
        assert!((w.x - (30.0 - 10.0) / 2.0).abs() < 1e-4);
        assert!((w.y - (40.0 - 20.0) / 2.0).abs() < 1e-4);
    }

    #[test]
    fn camera_isometric_foreshortens_y_and_round_trips() {
        let cam = Camera { offset: (0.0, 0.0), zoom: 1.0, yaw: 0.0, tilt: 0.5 };
        let (sx, sy) = cam.to_screen(PortablePoint::new(10.0, 10.0));
        assert!((sx - 10.0).abs() < 1e-4, "x is unaffected by tilt");
        assert!((sy - 5.0).abs() < 1e-4, "y is foreshortened by tilt");
        let w = cam.to_world((sx, sy));
        assert!(
            (w.x - 10.0).abs() < 1e-4 && (w.y - 10.0).abs() < 1e-4,
            "to_world inverts to_screen under foreshorten"
        );
    }

    #[test]
    fn ground_transform_matches_to_screen_under_yaw() {
        // The ground affine must agree with `to_screen` (which positions the upright
        // billboards) so edges meet the cards under an orbit. Verified at yaw != 0.
        let cam = Camera { offset: (3.0, 5.0), zoom: 1.5, yaw: 0.6, tilt: 0.55 };
        let t = cam.ground_transform();
        for (x, y) in [(0.0_f32, 0.0_f32), (40.0, -20.0), (-15.0, 33.0)] {
            let (sx, sy) = cam.to_screen(PortablePoint::new(x, y));
            let p = t.transform_point2d(LayoutPoint::new(x, y)).expect("affine maps");
            assert!(
                (p.x - sx).abs() < 1e-3 && (p.y - sy).abs() < 1e-3,
                "ground transform must match to_screen at ({x},{y}): ({},{}) vs ({sx},{sy})",
                p.x,
                p.y
            );
        }
    }
}
