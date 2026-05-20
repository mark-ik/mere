// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `GraphNodeRenderer` — InScenePaint renderer for
//! `NodeContentKind::GraphNode` (Path B of the cartography
//! projection).
//!
//! Path A ([`crate::orrery_renderer::OrreryRenderer`]) paints an entire
//! projection inside one `GraphView` pane. Path B explodes that
//! projection into one substrate node per graph node — each a
//! first-class spatial entity with its own placement, hit-test target,
//! and (future) drag handling. This renderer paints a single such
//! node: a filled circle that fills the substrate node's bounds.
//!
//! Per-node color is uniform in v0a. When selection / hover / per-node
//! semantic coloring lands, a shared per-identity style map (mirroring
//! [`crate::orrery_renderer::OrrerySnapshots`]) plugs in here.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use kurbo::{Circle, Stroke};
use register_renderer::{
    CompositionMode, InScenePaintRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeIdentity, NodeRenderer, PaintCtx, PaintResult,
    ProfileBindingExpectation, RendererCapabilities, RendererId, SceneNodeRef,
};
use vello::peniko::{Brush, Color, Fill};

/// Fill color for exploded graph nodes. Matches the orrery renderer's
/// node fill so Path A and Path B read identically at a glance.
pub const GRAPH_NODE_FILL: Color = Color::from_rgba8(0x4a, 0x90, 0xff, 0xff);

/// Stroke color for a selected node's highlight ring. Warm amber so it
/// reads clearly against the cool node fill.
pub const GRAPH_NODE_SELECTED_STROKE: Color = Color::from_rgba8(0xff, 0xc8, 0x5a, 0xff);

/// Host-shared set of selected graph-node substrate identities. The
/// host writes it on click; the renderer reads it under a shared lock
/// during `paint()` to draw a highlight ring on selected nodes.
///
/// Cloned at construction (Arc) so the host writes while the renderer
/// reads — same pattern as [`crate::orrery_renderer::OrrerySnapshots`].
pub type GraphNodeSelection = Arc<RwLock<HashSet<NodeIdentity>>>;

/// `InScenePaintRenderer` for `NodeContentKind::GraphNode`. Paints a
/// filled circle inscribed in the node's bounds; selected nodes get a
/// highlight ring.
pub struct GraphNodeRenderer {
    id: RendererId,
    selection: GraphNodeSelection,
}

impl GraphNodeRenderer {
    pub fn new(id: &'static str) -> Self {
        Self {
            id: RendererId::from_static(id),
            selection: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Clone of the renderer's selection set. The host holds this to
    /// mark nodes selected without going through the `NodeRenderer`
    /// trait; the renderer keeps its own clone for reads in `paint`.
    pub fn selection(&self) -> GraphNodeSelection {
        Arc::clone(&self.selection)
    }
}

impl Default for GraphNodeRenderer {
    fn default() -> Self {
        Self::new("mere.graph_node.v0")
    }
}

impl NodeRenderer for GraphNodeRenderer {
    fn renderer_id(&self) -> RendererId {
        self.id.clone()
    }

    fn handles(&self) -> NodeContentKindSet {
        NodeContentKindSet::from_one(NodeContentKind::GraphNode)
    }

    fn composition_mode(&self) -> CompositionMode {
        CompositionMode::InScenePaint
    }

    fn capabilities(&self) -> RendererCapabilities {
        // v0a is read-only paint. `accepts_input` flips on when the
        // drag slice lands — the substrate already hit-tests each node
        // individually, so per-node selection / drag is the next step.
        RendererCapabilities {
            accepts_input: false,
            handles_ime: false,
            handles_a11y: false,
            scrollable: false,
            hit_testable_subregions: false,
            profile_binding: ProfileBindingExpectation::None,
            supports_capture: false,
        }
    }

    fn as_in_scene_paint(&mut self) -> Option<&mut dyn InScenePaintRenderer> {
        Some(self)
    }
}

impl InScenePaintRenderer for GraphNodeRenderer {
    fn paint(&mut self, node: &SceneNodeRef, ctx: &mut PaintCtx<'_>) -> PaintResult {
        let w = node.size.width;
        let h = node.size.height;
        let radius = w.min(h) / 2.0;
        let center = kurbo::Point::new(w / 2.0, h / 2.0);
        let circle = Circle::new(center, radius);
        ctx.scene.fill(
            Fill::NonZero,
            ctx.node_transform,
            &Brush::Solid(GRAPH_NODE_FILL),
            None,
            &circle,
        );
        let selected = self
            .selection
            .read()
            .map(|set| set.contains(&node.identity))
            .unwrap_or(false);
        if selected {
            // Highlight ring just outside the node fill.
            let ring = Circle::new(center, radius + 3.0);
            ctx.scene.stroke(
                &Stroke::new(2.5),
                ctx.node_transform,
                &Brush::Solid(GRAPH_NODE_SELECTED_STROKE),
                None,
                &ring,
            );
        }
        Ok(())
    }

    fn input(&mut self, _node: &SceneNodeRef, _event: &InputEvent) -> InputDisposition {
        // v0a passive. The substrate hit-test already resolves clicks
        // to this node's identity; per-node selection / drag dispatch
        // is the next Path B slice.
        InputDisposition::Passthrough
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Size;
    use register_renderer::{Placement, RendererRegistry};
    use spatial_substrate::{SubstrateHost, SubstrateNode, SubstrateScene};

    use super::*;

    #[test]
    fn handles_graph_node_kind_only() {
        let r = GraphNodeRenderer::default();
        let kinds = r.handles();
        assert!(kinds.contains(NodeContentKind::GraphNode));
        assert!(!kinds.contains(NodeContentKind::GraphView));
    }

    #[test]
    fn declares_in_scene_paint_mode() {
        let r = GraphNodeRenderer::default();
        assert_eq!(r.composition_mode(), CompositionMode::InScenePaint);
    }

    #[test]
    fn dispatches_through_substrate() {
        let mut registry = RendererRegistry::with_default_selector();
        registry
            .register(Box::new(GraphNodeRenderer::default()))
            .unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::GraphNode,
            Placement::translate(120.0, 80.0),
            Size::new(16.0, 16.0),
        ));

        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.painted, 1);
        assert!(report.is_clean());
    }
}
