// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `SplitterRenderer` — InScenePaint renderer for
//! `NodeContentKind::Splitter` chrome nodes.
//!
//! Paints a flat-fill rectangle in the splitter's bounds — the
//! gpui host's `rgb(0x2a,0x2a,0x2a)` border tone, matching the pane
//! border so adjacent panes look like they share an edge that
//! happens to be draggable. Hover / active state coloring is the
//! host's responsibility (re-register the renderer with a brighter
//! color when a `SplitterDrag` is active) — v0 ships a single
//! resting color.
//!
//! Hit-test + drag state live host-side: the substrate routes
//! clicks via `SubstrateInputEvent::SplitterClicked { path }`,
//! the host snapshots a [`crate::SplitterDrag`], updates ratios on
//! cursor moves through `FrameLayout::set_split_ratio`, and clears
//! the snapshot on mouse-up.

use register_renderer::{
    CompositionMode, InScenePaintRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeRenderer, PaintCtx, PaintResult, ProfileBindingExpectation,
    RendererCapabilities, RendererId, SceneNodeRef,
};
use vello::peniko::{Brush, Color, Fill};

/// Default resting color for the splitter chrome — matches the
/// gpui host's pane-border tone.
pub const SPLITTER_REST_COLOR: Color = Color::from_rgba8(0x2a, 0x2a, 0x2a, 0xff);

/// Optional "active" color the host can use by registering a
/// second SplitterRenderer with a different id + color and
/// switching the registry's default policy per-kind during a drag.
/// Or, simpler, hot-swapping the registered renderer.
pub const SPLITTER_ACTIVE_COLOR: Color = Color::from_rgba8(0x4a, 0x90, 0xff, 0xff);

/// InScenePaint renderer for `NodeContentKind::Splitter`. Holds a
/// single color in v0 — hosts wanting hover / drag feedback hot-
/// swap the renderer with a different-colored instance via the
/// registry's hot-swap support.
pub struct SplitterRenderer {
    id: RendererId,
    color: Color,
}

impl SplitterRenderer {
    pub fn new(id: &'static str, color: Color) -> Self {
        Self {
            id: RendererId::from_static(id),
            color,
        }
    }

    /// Convenience: renderer with the default resting tone.
    pub fn resting(id: &'static str) -> Self {
        Self::new(id, SPLITTER_REST_COLOR)
    }

    pub fn id(&self) -> RendererId {
        self.id.clone()
    }
}

impl Default for SplitterRenderer {
    fn default() -> Self {
        Self::resting("mere.splitter.rest")
    }
}

impl NodeRenderer for SplitterRenderer {
    fn renderer_id(&self) -> RendererId {
        self.id.clone()
    }

    fn handles(&self) -> NodeContentKindSet {
        NodeContentKindSet::from_one(NodeContentKind::Splitter)
    }

    fn composition_mode(&self) -> CompositionMode {
        CompositionMode::InScenePaint
    }

    fn capabilities(&self) -> RendererCapabilities {
        // The splitter advertises `accepts_input = true` so hosts
        // can route clicks through the substrate's input pipeline
        // if they want; in practice the demo wires drag via
        // `SubstrateInputEvent::SplitterClicked` which arrives
        // from `handle_pointer_press` regardless of this flag.
        RendererCapabilities {
            accepts_input: true,
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

impl InScenePaintRenderer for SplitterRenderer {
    fn paint(&mut self, node: &SceneNodeRef, ctx: &mut PaintCtx<'_>) -> PaintResult {
        let rect = kurbo::Rect::new(0.0, 0.0, node.size.width, node.size.height);
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
        // Substrate-level click is consumed at the host layer (via
        // SplitterClicked); the renderer doesn't need to claim it.
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
    fn splitter_renderer_handles_splitter_kind_only() {
        let r = SplitterRenderer::default();
        let kinds = r.handles();
        assert!(kinds.contains(NodeContentKind::Splitter));
        assert!(!kinds.contains(NodeContentKind::Panel));
    }

    #[test]
    fn splitter_renderer_declares_in_scene_paint_mode() {
        let r = SplitterRenderer::default();
        assert_eq!(r.composition_mode(), CompositionMode::InScenePaint);
    }

    #[test]
    fn splitter_renderer_dispatches_through_substrate() {
        let mut registry = RendererRegistry::with_default_selector();
        registry
            .register(Box::new(SplitterRenderer::default()))
            .unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::Splitter,
            Placement::translate(100.0, 0.0),
            Size::new(4.0, 600.0),
        ));

        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.painted, 1);
        assert!(report.is_clean());
    }
}
