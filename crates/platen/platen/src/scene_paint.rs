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
}

/// World-to-view camera: a pan offset plus a uniform zoom, emitted as the
/// scene's `PushTransform`. Steal-the-shape target for `understory_view2d`.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub offset: (f32, f32),
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            offset: (0.0, 0.0),
            zoom: 1.0,
        }
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
    let mut commands = Vec::with_capacity(projection.edges.len() + projection.nodes.len() + 2);

    let transform = LayoutTransform::scale(camera.zoom, camera.zoom, 1.0)
        .then(&LayoutTransform::translation(camera.offset.0, camera.offset.1, 0.0));
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

    // Edges first (painted under the nodes).
    for edge in &projection.edges {
        let polyline: Vec<LayoutPoint> = if edge.path.len() >= 2 {
            edge.path.iter().map(|p| pt(*p)).collect()
        } else {
            match (node_pos.get(&edge.from), node_pos.get(&edge.to)) {
                (Some(&a), Some(&b)) => vec![pt(a), pt(b)],
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
            width: style.edge_width,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            dash: None,
        }));
    }

    // Nodes.
    for node in &projection.nodes {
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
}
