// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `SolidRectRenderer` — minimal in-scene-paint renderer that fills the
//! node's bounds with a solid color.
//!
//! Two uses: a useful built-in for the prototype's PNG / windowed-demo
//! integration tests (lets a scene contain visibly distinct nodes
//! without depending on a full widget renderer), and a documented
//! reference impl of `InScenePaintRenderer` for downstream renderer
//! authors learning the contract.

use kurbo::Rect;
use mere_renderer_registry::{
    CompositionMode, InScenePaintRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeRenderer, PaintCtx, PaintResult, ProfileBindingExpectation,
    RendererCapabilities, RendererId, SceneNodeRef,
};
use vello::peniko::{Brush, Color, Fill};

/// An `InScenePaintRenderer` that fills the node's bounds with a solid
/// color. Useful built-in for tests and demos.
pub struct SolidRectRenderer {
    id: RendererId,
    handles: NodeContentKindSet,
    color: Color,
}

impl SolidRectRenderer {
    /// Construct a solid-rect renderer for a single content kind.
    pub fn for_kind(id: &'static str, kind: NodeContentKind, color: Color) -> Self {
        Self {
            id: RendererId::from_static(id),
            handles: NodeContentKindSet::from_one(kind),
            color,
        }
    }

    /// Construct a solid-rect renderer that handles a set of content kinds.
    pub fn for_kinds(
        id: &'static str,
        kinds: impl IntoIterator<Item = NodeContentKind>,
        color: Color,
    ) -> Self {
        Self {
            id: RendererId::from_static(id),
            handles: NodeContentKindSet::from_iter(kinds),
            color,
        }
    }

    pub fn id(&self) -> RendererId {
        self.id.clone()
    }
}

impl NodeRenderer for SolidRectRenderer {
    fn renderer_id(&self) -> RendererId {
        self.id.clone()
    }

    fn handles(&self) -> NodeContentKindSet {
        self.handles.clone()
    }

    fn composition_mode(&self) -> CompositionMode {
        CompositionMode::InScenePaint
    }

    fn capabilities(&self) -> RendererCapabilities {
        // Passive content: no input, no IME, no a11y semantics worth
        // mentioning, no scroll. Hosts treat this as a decorative layer.
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

impl InScenePaintRenderer for SolidRectRenderer {
    fn paint(&mut self, node: &SceneNodeRef, ctx: &mut PaintCtx<'_>) -> PaintResult {
        let rect = Rect::new(0.0, 0.0, node.size.width, node.size.height);
        ctx.scene.fill(
            Fill::NonZero,
            ctx.node_transform,
            &Brush::Solid(self.color),
            None,
            &rect,
        );
        Ok(())
    }

    fn input(&mut self, _node: &SceneNodeRef, _event: &InputEvent) -> InputDisposition {
        // Passive content; never claims input.
        InputDisposition::Passthrough
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Size;
    use mere_renderer_registry::{Placement, RendererRegistry};

    use super::*;
    use crate::host::SubstrateHost;
    use crate::scene::{SubstrateNode, SubstrateScene};

    #[test]
    fn solid_rect_dispatches_through_substrate() {
        let renderer = SolidRectRenderer::for_kind(
            "solid",
            NodeContentKind::DocumentTile,
            Color::from_rgba8(200, 80, 80, 255),
        );
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(renderer)).unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::translate(10.0, 20.0),
            Size::new(50.0, 50.0),
        ));

        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.painted, 1);
        assert!(report.is_clean());
    }
}
