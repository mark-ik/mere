// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `SubstrateHost` — drives one frame of registry-mediated paint dispatch.
//!
//! Owns the [`RendererRegistry`] and walks a [`SubstrateScene`] per frame,
//! handing each node to whichever renderer the selector resolves. The
//! per-frame reports surface what dispatched, what didn't, and why — the
//! diagnostic surface §8 of the contract brief expects.

use mere_renderer_registry::{
    DispatchError, InputDisposition, InputEvent, NodeIdentity, PaintCtx, PaintError,
    RendererId, RendererRegistry,
};

use crate::scene::SubstrateScene;

/// Substrate host: registry + per-frame dispatch.
pub struct SubstrateHost {
    registry: RendererRegistry,
}

impl SubstrateHost {
    pub fn new(registry: RendererRegistry) -> Self {
        Self { registry }
    }

    pub fn with_default_registry() -> Self {
        Self::new(RendererRegistry::with_default_selector())
    }

    pub fn registry(&self) -> &RendererRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut RendererRegistry {
        &mut self.registry
    }

    /// Paint every in-scene-paint node in `scene` into `target` at the
    /// node's placement transform. Embedded-frame and overlay renderers
    /// are skipped (counted in the returned report) — those paths go
    /// through different dispatch methods (not yet wired in v0a).
    pub fn paint_scene(
        &mut self,
        scene: &SubstrateScene,
        target: &mut vello::Scene,
        scale_factor: f64,
    ) -> FrameReport {
        let mut report = FrameReport::default();
        for node in scene.iter() {
            let node_ref = node.as_ref();
            let mut ctx = PaintCtx {
                scene: target,
                node_transform: node.placement.transform,
                scale_factor,
            };
            match self.registry.paint_node(&node_ref, &mut ctx) {
                Ok(None) => report.missing_renderer += 1,
                Ok(Some(Ok(()))) => report.painted += 1,
                Ok(Some(Err(PaintError::NotReady))) => report.not_ready += 1,
                Ok(Some(Err(PaintError::RendererFailed(msg)))) => {
                    report.renderer_failed.push(msg);
                }
                Err(DispatchError::WrongCompositionMode { id }) => {
                    report.wrong_mode.push(id);
                }
            }
        }
        report
    }

    /// Deliver an input event to a specific node by identity.
    ///
    /// The hit-test layer that resolves host coordinates to a node
    /// identity isn't part of this prototype — callers pass the
    /// identity directly. Returns `None` if the node isn't in `scene`
    /// or no renderer claims its content kind; returns
    /// `Some(InputDisposition::*)` otherwise.
    pub fn deliver_input(
        &mut self,
        scene: &SubstrateScene,
        target: NodeIdentity,
        event: &InputEvent,
    ) -> Result<Option<InputDisposition>, DispatchError> {
        let Some(node) = scene.get(target) else {
            return Ok(None);
        };
        self.registry.deliver_in_scene_input(&node.as_ref(), event)
    }
}

/// Summary of one frame's dispatch — what painted, what didn't, and why.
///
/// Phase-3 done conditions name diagnostic events
/// (`renderer.registered/unregistered/hot_swapped`,
/// `engine.route_chosen/degraded`, `surface.attach_failed`). This report
/// is the data those events would carry; emit-to-bus wiring lands when
/// the host action bus is the integration point.
#[derive(Debug, Default, Clone)]
pub struct FrameReport {
    /// Nodes whose renderer reported `Ok(())` paint.
    pub painted: usize,
    /// Nodes whose resolved renderer returned `PaintError::NotReady`.
    pub not_ready: usize,
    /// Nodes whose content kind matched no registered renderer.
    pub missing_renderer: usize,
    /// Renderers that returned `PaintError::RendererFailed(_)`; one
    /// entry per failure.
    pub renderer_failed: Vec<String>,
    /// Renderers resolved by the selector that don't implement
    /// `InScenePaintRenderer` — bug in the renderer registration or
    /// selector. One entry per occurrence.
    pub wrong_mode: Vec<RendererId>,
}

impl FrameReport {
    pub fn is_clean(&self) -> bool {
        self.missing_renderer == 0
            && self.renderer_failed.is_empty()
            && self.wrong_mode.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use kurbo::{Affine, Size};
    use mere_renderer_registry::{
        CompositionMode, NodeContentKind, NodeContentKindSet, NodeRenderer, Placement,
        RendererCapabilities, RendererId, RendererRegistry,
    };

    use super::*;
    use crate::recording_renderer::RecordingRenderer;
    use crate::scene::{SubstrateNode, SubstrateScene};

    fn build_host_with_recording(kind: NodeContentKind) -> (SubstrateHost, RendererId) {
        let renderer = RecordingRenderer::for_kind("recording", kind);
        let id = renderer.id();
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(renderer)).unwrap();
        (SubstrateHost::new(registry), id)
    }

    #[test]
    fn empty_scene_paints_nothing() {
        let mut host = SubstrateHost::with_default_registry();
        let scene = SubstrateScene::new();
        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.painted, 0);
        assert!(report.is_clean());
    }

    #[test]
    fn dispatch_calls_matching_renderer() {
        let (mut host, _) = build_host_with_recording(NodeContentKind::DocumentTile);
        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::translate(10.0, 20.0),
            Size::new(80.0, 40.0),
        ));
        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.painted, 1);
        assert!(report.is_clean());
    }

    #[test]
    fn dispatch_reports_missing_renderer_for_unknown_kind() {
        let (mut host, _) = build_host_with_recording(NodeContentKind::DocumentTile);
        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::GraphView, // unregistered
            Placement::IDENTITY,
            Size::new(10.0, 10.0),
        ));
        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.painted, 0);
        assert_eq!(report.missing_renderer, 1);
    }

    #[test]
    fn dispatch_passes_placement_to_renderer() {
        let renderer = RecordingRenderer::for_kind("recording", NodeContentKind::Panel);
        let log = renderer.shared_paint_log();
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(renderer)).unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(33.0, 77.0),
            Size::new(50.0, 50.0),
        ));
        let mut target = vello::Scene::new();
        let _ = host.paint_scene(&scene, &mut target, 1.5);

        let records = log.borrow();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].node_transform, Affine::translate((33.0, 77.0)));
        assert_eq!(records[0].scale_factor, 1.5);
    }

    #[test]
    fn dispatch_reports_wrong_composition_mode() {
        // A renderer that advertises InScenePaint via composition_mode()
        // but returns None from as_in_scene_paint(). Pathological — but
        // the dispatch helper has to report it cleanly.
        struct MislabelledRenderer {
            id: RendererId,
        }
        impl NodeRenderer for MislabelledRenderer {
            fn renderer_id(&self) -> RendererId {
                self.id.clone()
            }
            fn handles(&self) -> NodeContentKindSet {
                NodeContentKindSet::from_one(NodeContentKind::WebPage)
            }
            fn composition_mode(&self) -> CompositionMode {
                CompositionMode::InScenePaint
            }
            fn capabilities(&self) -> RendererCapabilities {
                RendererCapabilities::NONE
            }
            // Deliberately doesn't override as_in_scene_paint — default
            // None — which is the bug the dispatch helper must surface.
        }

        let mut registry = RendererRegistry::with_default_selector();
        let id = RendererId::from_static("mislabelled");
        registry
            .register(Box::new(MislabelledRenderer { id: id.clone() }))
            .unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::WebPage,
            Placement::IDENTITY,
            Size::new(10.0, 10.0),
        ));
        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.painted, 0);
        assert_eq!(report.wrong_mode, vec![id]);
    }
}
