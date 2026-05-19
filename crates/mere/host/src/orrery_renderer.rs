// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `OrreryRenderer` — InScenePaint renderer for `NodeContentKind::GraphView`
//! panes (the orrery, t1 root view, and any future graph-shaped surface).
//!
//! v0 ships **Path A** (per the cartography-projection options): the
//! renderer holds an `Arc<RwLock<HashMap<NodeIdentity, Projection>>>`
//! snapshot map that the host updates per frametree-sync. When a
//! `GraphView` substrate node is dispatched, the renderer looks up
//! its identity, paints the projection's nodes + edges. Falls back
//! to a static demo triangle when no snapshot is present (useful for
//! the bare windowed_app demo before the host has wired cartography).
//!
//! Path B (one substrate node per graph node — first-class spatial
//! entities) is a separate slice for streaming strategies (force-
//! directed and friends, where per-node hit-test + drag are the
//! real interactions).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use cartography::Projection;
use kurbo::{BezPath, Circle, Stroke};
use register_renderer::{
    CompositionMode, InScenePaintRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeIdentity, NodeRenderer, PaintCtx, PaintResult,
    ProfileBindingExpectation, RendererCapabilities, RendererId, SceneNodeRef,
};
use vello::peniko::{Brush, Color, Fill};

/// Default background color for orrery panes — slightly cooler than
/// the workbench's `rgb(0x141414)` so adjacent panes are visually
/// distinguishable.
pub const ORRERY_BG: Color = Color::from_rgba8(0x12, 0x14, 0x1a, 0xff);

/// Node fill color. Mid-bright so circles stand out against the
/// dark canvas; matches the gpui host's "active tile" highlight tone.
pub const ORRERY_NODE_FILL: Color = Color::from_rgba8(0x4a, 0x90, 0xff, 0xff);

/// Edge stroke color. Dimmer than nodes so the graph structure reads
/// as scaffolding.
pub const ORRERY_EDGE_STROKE: Color = Color::from_rgba8(0x40, 0x50, 0x60, 0xff);

/// Host-shared map of per-orrery-pane projection snapshots. Key is the
/// orrery's substrate `NodeIdentity`; value is the cartography
/// `Projection` whose positions paint into the pane.
///
/// The `Arc` is cloned at construction so the host can write while the
/// renderer reads. The renderer never mutates — only reads under a
/// shared lock during `paint()`.
pub type OrrerySnapshots = Arc<RwLock<HashMap<NodeIdentity, Projection>>>;

/// `InScenePaintRenderer` for `NodeContentKind::GraphView`. Reads
/// host-installed projections from a shared snapshot map; paints a
/// static demo triangle when no snapshot is bound (useful for
/// stand-alone tests / bare demos).
pub struct OrreryRenderer {
    id: RendererId,
    snapshots: OrrerySnapshots,
}

impl OrreryRenderer {
    pub fn new(id: &'static str) -> Self {
        Self {
            id: RendererId::from_static(id),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn id(&self) -> RendererId {
        self.id.clone()
    }

    /// Clone of the renderer's snapshot map. Hosts hold this clone
    /// to write projection results without going through the
    /// `NodeRenderer` trait. The renderer keeps its own clone for
    /// reads inside `paint()`.
    pub fn snapshots(&self) -> OrrerySnapshots {
        Arc::clone(&self.snapshots)
    }
}

impl Default for OrreryRenderer {
    fn default() -> Self {
        Self::new("mere.orrery.v0")
    }
}

impl NodeRenderer for OrreryRenderer {
    fn renderer_id(&self) -> RendererId {
        self.id.clone()
    }

    fn handles(&self) -> NodeContentKindSet {
        NodeContentKindSet::from_one(NodeContentKind::GraphView)
    }

    fn composition_mode(&self) -> CompositionMode {
        CompositionMode::InScenePaint
    }

    fn capabilities(&self) -> RendererCapabilities {
        // v0 is read-only. Input + a11y + scroll wait on the real
        // orrery integration; capture is `true` so the switcher
        // can thumbnail orrery panes.
        RendererCapabilities {
            accepts_input: false,
            handles_ime: false,
            handles_a11y: false,
            scrollable: false,
            hit_testable_subregions: false,
            profile_binding: ProfileBindingExpectation::None,
            supports_capture: true,
        }
    }

    fn as_in_scene_paint(&mut self) -> Option<&mut dyn InScenePaintRenderer> {
        Some(self)
    }
}

impl InScenePaintRenderer for OrreryRenderer {
    fn paint(&mut self, node: &SceneNodeRef, ctx: &mut PaintCtx<'_>) -> PaintResult {
        let w = node.size.width;
        let h = node.size.height;
        // Background fill — establishes the orrery canvas.
        let bg = kurbo::Rect::new(0.0, 0.0, w, h);
        ctx.scene.fill(
            Fill::NonZero,
            ctx.node_transform,
            &Brush::Solid(ORRERY_BG),
            None,
            &bg,
        );

        let snapshots = self
            .snapshots
            .read()
            .expect("orrery snapshots lock poisoned");
        if let Some(projection) = snapshots.get(&node.identity) {
            paint_projection(projection, node, ctx);
        } else {
            paint_demo_triangle(w, h, ctx);
        }
        Ok(())
    }

    fn input(&mut self, _node: &SceneNodeRef, _event: &InputEvent) -> InputDisposition {
        // v0 passive content — the host's pane-level hit-test still
        // resolves clicks to the orrery's substrate node; per-graph-
        // node interaction needs Path B (per-node substrate nodes).
        InputDisposition::Passthrough
    }
}

/// Paint a cartography `Projection` into the orrery pane's local
/// coordinate space. The projection's coordinates are already in
/// pane-local pixels (the host runs the strategy with
/// `ViewIntent::canvas_pixels(pane_width, pane_height)`).
fn paint_projection(projection: &Projection, _node: &SceneNodeRef, ctx: &mut PaintCtx<'_>) {
    // Edges first, so node circles sit on top of edge endpoints.
    let stroke = Stroke::new(2.0);
    let edge_brush = Brush::Solid(ORRERY_EDGE_STROKE);
    for edge in &projection.edges {
        if edge.path.is_empty() {
            continue;
        }
        let mut path = BezPath::new();
        let first = edge.path[0];
        path.move_to(kurbo::Point::new(first.x as f64, first.y as f64));
        for pt in &edge.path[1..] {
            path.line_to(kurbo::Point::new(pt.x as f64, pt.y as f64));
        }
        ctx.scene
            .stroke(&stroke, ctx.node_transform, &edge_brush, None, &path);
    }
    // Nodes.
    let node_brush = Brush::Solid(ORRERY_NODE_FILL);
    for positioned in &projection.nodes {
        let radius = if positioned.radius > 0.0 {
            positioned.radius as f64
        } else {
            8.0
        };
        let center = kurbo::Point::new(positioned.position.x as f64, positioned.position.y as f64);
        let circle = Circle::new(center, radius);
        ctx.scene.fill(
            Fill::NonZero,
            ctx.node_transform,
            &node_brush,
            None,
            &circle,
        );
    }
}

/// Static demo content — three nodes arranged in a triangle inside
/// the pane, connected by edges. Painted when no host-installed
/// snapshot covers the orrery's substrate identity. Same shape as
/// the pre-cartography v0 visual; conveys "graph view" at a glance.
fn paint_demo_triangle(w: f64, h: f64, ctx: &mut PaintCtx<'_>) {
    let margin = (w.min(h) * 0.18).clamp(8.0, 80.0);
    let radius = (w.min(h) * 0.04).clamp(4.0, 24.0);
    let nodes = [
        kurbo::Point::new(w * 0.5, margin),
        kurbo::Point::new(margin, h - margin),
        kurbo::Point::new(w - margin, h - margin),
    ];
    let stroke = Stroke::new(2.0);
    let edge_brush = Brush::Solid(ORRERY_EDGE_STROKE);
    for (i, a) in nodes.iter().enumerate() {
        for b in &nodes[i + 1..] {
            let mut path = BezPath::new();
            path.move_to(*a);
            path.line_to(*b);
            ctx.scene
                .stroke(&stroke, ctx.node_transform, &edge_brush, None, &path);
        }
    }
    let node_brush = Brush::Solid(ORRERY_NODE_FILL);
    for p in &nodes {
        let circle = Circle::new(*p, radius);
        ctx.scene.fill(
            Fill::NonZero,
            ctx.node_transform,
            &node_brush,
            None,
            &circle,
        );
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Size;
    use register_renderer::{Placement, RendererRegistry};
    use spatial_substrate::{SubstrateHost, SubstrateNode, SubstrateScene};

    use super::*;

    #[test]
    fn orrery_renderer_handles_graph_view_kind_only() {
        let r = OrreryRenderer::default();
        let kinds = r.handles();
        assert!(kinds.contains(NodeContentKind::GraphView));
        assert!(!kinds.contains(NodeContentKind::Panel));
    }

    #[test]
    fn orrery_renderer_declares_in_scene_paint_mode() {
        let r = OrreryRenderer::default();
        assert_eq!(r.composition_mode(), CompositionMode::InScenePaint);
    }

    #[test]
    fn orrery_renderer_dispatches_through_substrate() {
        let renderer = OrreryRenderer::default();
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(renderer)).unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::GraphView,
            Placement::translate(40.0, 40.0),
            Size::new(200.0, 150.0),
        ));

        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.painted, 1);
        assert!(report.is_clean());
    }

    #[test]
    fn snapshots_handle_is_cloneable_arc() {
        let r = OrreryRenderer::default();
        let h1 = r.snapshots();
        let h2 = r.snapshots();
        // Same underlying Arc.
        assert!(Arc::ptr_eq(&h1, &h2));
    }

    #[test]
    fn snapshots_handle_lets_external_writers_update_visible_state() {
        let r = OrreryRenderer::default();
        let handle = r.snapshots();
        let id = NodeIdentity::next();
        handle.write().unwrap().insert(id, Projection::empty());
        // Renderer's own borrow sees the write.
        assert!(r.snapshots.read().unwrap().contains_key(&id));
    }
}
