// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `OrreryRenderer` — InScenePaint renderer for `NodeContentKind::GraphView`
//! panes (the orrery, t1 root view, and any future graph-shaped surface).
//!
//! v0 stub: paints a dark background + a small set of demo "nodes" as
//! filled circles with edges between them. Visually conveys "this is a
//! graph view" without yet plumbing real `mere_kernel::graph::Graph`
//! state through the renderer.
//!
//! Replace the static demo content with a host-installed snapshot when
//! the substrate's orrery integration grows up — see the
//! [renderer registry contract brief](
//! ../../../../design_docs/mere_docs/research/2026-05-15_renderer_registry_contract_brief.md
//! ) for the host-shared-state pattern (`Arc<Mutex<Snapshot>>` set per
//! frame from the host's graph registry).

use kurbo::{BezPath, Circle, Stroke};
use mere_renderer_registry::{
    CompositionMode, InScenePaintRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeRenderer, PaintCtx, PaintResult, ProfileBindingExpectation,
    RendererCapabilities, RendererId, SceneNodeRef,
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

/// `InScenePaintRenderer` for `NodeContentKind::GraphView`. v0 paints
/// a fixed demo graph layout so the renderer chain is visibly wired
/// end-to-end; a future revision plumbs real graph snapshots from
/// the host's `GraphRegistry`.
pub struct OrreryRenderer {
    id: RendererId,
}

impl OrreryRenderer {
    pub fn new(id: &'static str) -> Self {
        Self {
            id: RendererId::from_static(id),
        }
    }

    pub fn id(&self) -> RendererId {
        self.id.clone()
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

        // Demo content: three nodes arranged in a triangle inside
        // the pane, connected by edges. The triangle inscribes the
        // pane with a margin so it reads as "graph in a canvas".
        let margin = (w.min(h) * 0.18).clamp(8.0, 80.0);
        let radius = (w.min(h) * 0.04).clamp(4.0, 24.0);
        let nodes = [
            kurbo::Point::new(w * 0.5, margin),
            kurbo::Point::new(margin, h - margin),
            kurbo::Point::new(w - margin, h - margin),
        ];
        // Edges first, so node circles sit on top of edge endpoints.
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
        // Nodes.
        let node_brush = Brush::Solid(ORRERY_NODE_FILL);
        for p in &nodes {
            let circle = Circle::new(*p, radius);
            ctx.scene
                .fill(Fill::NonZero, ctx.node_transform, &node_brush, None, &circle);
        }
        Ok(())
    }

    fn input(&mut self, _node: &SceneNodeRef, _event: &InputEvent) -> InputDisposition {
        // v0 passive content — the host's pane-level hit-test still
        // resolves clicks to the orrery's substrate node; per-graph-
        // node interaction needs the real snapshot.
        InputDisposition::Passthrough
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Size;
    use mere_renderer_registry::{Placement, RendererRegistry};
    use mere_spatial_prototype::{SubstrateHost, SubstrateNode, SubstrateScene};

    use super::*;

    #[test]
    fn orrery_renderer_handles_graph_view_kind_only() {
        let r = OrreryRenderer::default();
        let kinds = r.handles();
        assert!(kinds.contains(NodeContentKind::GraphView));
        assert!(!kinds.contains(NodeContentKind::Panel));
        assert!(!kinds.contains(NodeContentKind::DocumentTile));
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
    fn orrery_renderer_capabilities_v0_are_passive_with_capture() {
        let r = OrreryRenderer::default();
        let caps = r.capabilities();
        assert!(!caps.accepts_input);
        assert!(caps.supports_capture);
    }
}
