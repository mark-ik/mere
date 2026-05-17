// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `SubstrateHost` — drives one frame of registry-mediated paint dispatch.
//!
//! Owns the [`RendererRegistry`] and walks a [`SubstrateScene`] per frame,
//! handing each node to whichever renderer the selector resolves. The
//! per-frame reports surface what dispatched, what didn't, and why — the
//! diagnostic surface §8 of the contract brief expects.

use std::collections::HashMap;

use kurbo::{Affine, Line, Point, Stroke};
use mere_renderer_registry::{
    CompositionMode, DispatchError, InputDisposition, InputEvent, NodeIdentity, PaintCtx,
    PaintError, ProducerHandle, RendererId, RendererRegistry,
};
use vello::peniko::{Brush, Color};

use crate::external_texture::{CompositorError, ExternalTextureCompositor};
use crate::scene::SubstrateScene;

/// Substrate host: registry + per-frame dispatch.
pub struct SubstrateHost {
    registry: RendererRegistry,
    /// Cached producer handles for embedded-frame renderers. Keyed by
    /// node identity; value pairs the renderer id (so we can detect
    /// renderer swap-out) with the producer handle the renderer minted.
    producers: HashMap<NodeIdentity, (RendererId, ProducerHandle)>,
}

impl SubstrateHost {
    pub fn new(registry: RendererRegistry) -> Self {
        Self {
            registry,
            producers: HashMap::new(),
        }
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
        report.edges_painted = paint_edges(scene, target, scale_factor);
        report
    }

    /// Render `scene` into `target`, dispatching each node through the
    /// composition-mode path its registered renderer claims.
    ///
    /// `compositor` + `vello_renderer` are required for the EmbeddedFrame
    /// branch — the compositor calls `vello_renderer.register_texture` on
    /// new producer textures. InScene nodes paint directly into `target`
    /// and don't touch the compositor; pure-InScene scenes can use
    /// [`Self::paint_scene`] instead and avoid the vello::Renderer
    /// dependency.
    ///
    /// Overlay-mode nodes are skipped — out-of-band OS surfaces are owned
    /// by a different dispatch path (not yet wired in v0a).
    pub fn render_scene(
        &mut self,
        scene: &SubstrateScene,
        target: &mut vello::Scene,
        compositor: &mut ExternalTextureCompositor,
        vello_renderer: &mut vello::Renderer,
        scale_factor: f64,
    ) -> FrameReport {
        let mut report = FrameReport::default();
        for node in scene.iter() {
            self.dispatch_node(node.as_ref(), target, compositor, vello_renderer, scale_factor, &mut report);
        }
        report.edges_painted = paint_edges(scene, target, scale_factor);
        report
    }

    fn dispatch_node(
        &mut self,
        node_ref: mere_renderer_registry::SceneNodeRef,
        target: &mut vello::Scene,
        compositor: &mut ExternalTextureCompositor,
        vello_renderer: &mut vello::Renderer,
        scale_factor: f64,
        report: &mut FrameReport,
    ) {
        let Some(id) = self.registry.select(&node_ref) else {
            report.missing_renderer += 1;
            return;
        };
        // composition_mode() is the renderer's *self-declared* mode; the
        // sub-trait downcast (`as_in_scene_paint` / `as_embedded_frame`)
        // is what actually proves it. A mismatch counts as wrong_mode.
        let mode = self
            .registry
            .get(&id)
            .expect("selector returned id not in registry")
            .composition_mode();
        match mode {
            CompositionMode::InScenePaint => {
                let mut ctx = PaintCtx {
                    scene: target,
                    node_transform: node_ref.placement.transform,
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
            CompositionMode::EmbeddedFrame => {
                self.dispatch_embedded(
                    &node_ref,
                    id,
                    target,
                    compositor,
                    vello_renderer,
                    report,
                );
            }
            CompositionMode::Overlay => {
                report.overlay_skipped += 1;
            }
        }
    }

    fn dispatch_embedded(
        &mut self,
        node_ref: &mere_renderer_registry::SceneNodeRef,
        id: RendererId,
        target: &mut vello::Scene,
        compositor: &mut ExternalTextureCompositor,
        vello_renderer: &mut vello::Renderer,
        report: &mut FrameReport,
    ) {
        let renderer = self
            .registry
            .get_mut(&id)
            .expect("selector returned id not in registry");
        let Some(producer) = renderer.as_embedded_frame() else {
            report.wrong_mode.push(id);
            return;
        };
        // Ensure a ProducerHandle for this node. If the selector swapped
        // renderers under us mid-session, drop the cached handle and ask
        // the new renderer for a fresh one.
        let handle = match self.producers.get(&node_ref.identity) {
            Some(&(ref cached_id, h)) if cached_id == &id => h,
            _ => {
                let h = producer.ensure_producer(node_ref);
                self.producers.insert(node_ref.identity, (id.clone(), h));
                h
            }
        };
        // Pull this frame's texture (None = no new frame; we composite
        // last-known content via whatever's already registered).
        let new_texture = producer.next_frame(handle);
        if let Some(texture) = new_texture {
            compositor.register(vello_renderer, node_ref.identity, texture);
        }
        match compositor.compose(
            target,
            node_ref.identity,
            node_ref.placement.transform,
            node_ref.size,
        ) {
            Ok(()) => report.painted += 1,
            Err(CompositorError::NotRegistered(_)) => report.not_ready += 1,
            Err(CompositorError::ZeroSizedTexture(_) | CompositorError::ZeroSizedNode(_)) => {
                report
                    .renderer_failed
                    .push(format!("compositor rejected node {}", node_ref.identity.as_u64()));
            }
        }
    }

    /// Release any per-node state the host holds for an embedded-frame
    /// node — the renderer's producer handle and the compositor's
    /// texture registration. Call before dropping a node from the scene
    /// to keep vello's image-override table tidy.
    pub fn release_node(
        &mut self,
        node_identity: NodeIdentity,
        compositor: &mut ExternalTextureCompositor,
        vello_renderer: &mut vello::Renderer,
    ) {
        if let Some((id, handle)) = self.producers.remove(&node_identity) {
            if let Some(renderer) = self.registry.get_mut(&id) {
                if let Some(producer) = renderer.as_embedded_frame() {
                    producer.release(handle);
                }
            }
        }
        compositor.unregister(vello_renderer, node_identity);
    }

    /// Deliver an input event to a specific node by identity.
    ///
    /// Resolves the node's renderer, dispatches based on
    /// `composition_mode()`. For `InScenePaint` renderers this calls
    /// the registry's in-scene input helper; for `EmbeddedFrame` the
    /// host's cached `ProducerHandle` is needed (returns
    /// `Ok(Some(Passthrough))` if no producer has been ensured yet —
    /// hosts typically render before delivering input). Overlay-mode
    /// nodes pass through (not yet wired).
    ///
    /// The event's coordinates are assumed to already be in tile-local
    /// space; use [`Self::deliver_input_at`] when the caller has
    /// host-space coordinates and wants hit-test + translation in one
    /// step.
    pub fn deliver_input(
        &mut self,
        scene: &SubstrateScene,
        target: NodeIdentity,
        event: &InputEvent,
    ) -> Result<Option<InputDisposition>, DispatchError> {
        let Some(node) = scene.get(target) else {
            return Ok(None);
        };
        let node_ref = node.as_ref();
        let Some(id) = self.registry.select(&node_ref) else {
            return Ok(None);
        };
        let mode = self
            .registry
            .get(&id)
            .expect("selector returned id not in registry")
            .composition_mode();
        match mode {
            CompositionMode::InScenePaint => {
                self.registry.deliver_in_scene_input(&node_ref, event)
            }
            CompositionMode::EmbeddedFrame => {
                let Some(&(ref stored_id, handle)) = self.producers.get(&target) else {
                    return Ok(Some(InputDisposition::Passthrough));
                };
                if stored_id != &id {
                    return Ok(Some(InputDisposition::Passthrough));
                }
                let renderer = self
                    .registry
                    .get_mut(&id)
                    .expect("selector returned id not in registry");
                let Some(producer) = renderer.as_embedded_frame() else {
                    return Err(DispatchError::WrongCompositionMode { id });
                };
                Ok(Some(producer.deliver_input(handle, event)))
            }
            CompositionMode::Overlay => Ok(Some(InputDisposition::Passthrough)),
        }
    }

    /// Spatial input router: hit-test `host_pos` against `scene`,
    /// translate the position to the hit node's tile-local coordinates,
    /// and dispatch via [`Self::deliver_input`].
    ///
    /// `host_pos` is the source of truth for the dispatched event's
    /// position — any `position` field on `event` is overwritten. The
    /// caller passes `event` to carry the non-positional payload (kind,
    /// modifiers, etc.); fields like `InputEvent::Pointer { position:
    /// Point::ZERO, ... }` are typical.
    ///
    /// Returns:
    /// - `Ok(None)` if no node was hit at `host_pos`, or if a hit node
    ///   has no registered renderer.
    /// - `Ok(Some(disposition))` on successful dispatch.
    /// - `Err(DispatchError)` for renderer composition-mode bugs.
    pub fn deliver_input_at(
        &mut self,
        scene: &SubstrateScene,
        host_pos: Point,
        event: &InputEvent,
    ) -> Result<Option<InputDisposition>, DispatchError> {
        // Pointer routing targets nodes specifically — edges don't have
        // registry-resolved renderers to deliver_input through. Hosts
        // wanting edge-click semantics call `scene.hit_test` (unified)
        // and handle the `SceneHit::Edge` branch themselves.
        let Some(target) = scene.hit_test_node(host_pos) else {
            return Ok(None);
        };
        let Some(node) = scene.get(target) else {
            return Ok(None);
        };
        let local_event = rewrite_pointer_position(event, host_pos, node.placement.transform);
        self.deliver_input(scene, target, &local_event)
    }
}

/// Paint substrate-owned relation edges into `target`. Edges paint
/// over nodes; this runs after the per-node dispatch loop.
///
/// v0a strokes a straight line between endpoint centers; label
/// rendering and per-`EdgeKind` styling are deferred to a follow-up
/// (needs parley wiring for text + a real consumer to drive the
/// per-kind color palette). Endpoints not in the scene cause the edge
/// to be silently skipped.
///
/// Returns the count of edges actually drawn (skipped-endpoint edges
/// don't count).
fn paint_edges(scene: &SubstrateScene, target: &mut vello::Scene, scale_factor: f64) -> usize {
    // v0a styling: subtle stroke that reads clearly over both light
    // and dark backgrounds; width scaled to the host's DPR.
    let edge_color = Color::from_rgba8(200, 200, 210, 200);
    let stroke = Stroke::new(2.0 * scale_factor.max(1.0));
    let brush = Brush::Solid(edge_color);
    let mut drawn = 0;
    for edge in scene.iter_edges() {
        let (Some(a), Some(b)) = (
            scene.endpoint_center(edge.from),
            scene.endpoint_center(edge.to),
        ) else {
            continue;
        };
        let line = Line::new(a, b);
        target.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
        drawn += 1;
    }
    drawn
}

/// For `InputEvent::Pointer`, replace the position with
/// `node_transform.inverse() * host_pos`. Non-pointer events pass
/// through unchanged. Degenerate transforms leave `host_pos`
/// unchanged in the dispatched event.
fn rewrite_pointer_position(
    event: &InputEvent,
    host_pos: Point,
    node_transform: Affine,
) -> InputEvent {
    if let InputEvent::Pointer {
        kind, modifiers, ..
    } = event
    {
        let local = if node_transform.determinant() == 0.0 {
            host_pos
        } else {
            node_transform.inverse() * host_pos
        };
        return InputEvent::Pointer {
            position: local,
            kind: *kind,
            modifiers: *modifiers,
        };
    }
    event.clone()
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
    /// Nodes that painted successfully — InScene `paint() = Ok` *or*
    /// EmbeddedFrame compositor `compose() = Ok`.
    pub painted: usize,
    /// Nodes whose renderer returned `PaintError::NotReady`, or whose
    /// EmbeddedFrame producer hasn't yet produced a first texture
    /// (compositor `NotRegistered`).
    pub not_ready: usize,
    /// Nodes whose content kind matched no registered renderer.
    pub missing_renderer: usize,
    /// Renderer-or-compositor failures with a human-readable reason.
    /// One entry per failure.
    pub renderer_failed: Vec<String>,
    /// Renderers resolved by the selector whose claimed composition
    /// mode doesn't match what `as_in_scene_paint` / `as_embedded_frame`
    /// returns. Bug in the renderer impl.
    pub wrong_mode: Vec<RendererId>,
    /// Overlay-mode nodes skipped by `render_scene` — overlays are
    /// out-of-band OS surfaces dispatched through a different path
    /// (not yet wired in v0a).
    pub overlay_skipped: usize,
    /// Edges drawn by the substrate's paint pass. Edges paint over
    /// nodes; this count is informational (no per-edge failure modes
    /// today — edges with missing endpoints silently skip).
    pub edges_painted: usize,
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

    #[test]
    fn deliver_input_at_routes_via_hit_test() {
        use mere_renderer_registry::{ModifiersState, PointerButton, PointerEventKind};

        let renderer = RecordingRenderer::for_kind("recording", NodeContentKind::DocumentTile);
        let log = renderer.shared_input_log();
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(renderer)).unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        let id = scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::translate(100.0, 200.0),
            Size::new(80.0, 40.0),
        ));

        let event = InputEvent::Pointer {
            position: kurbo::Point::new(0.0, 0.0), // overwritten by deliver_input_at
            kind: PointerEventKind::Down(PointerButton::Primary),
            modifiers: ModifiersState::default(),
        };
        // Click at (120, 220) — inside the rect at tile-local (20, 20).
        let disposition = host
            .deliver_input_at(&scene, kurbo::Point::new(120.0, 220.0), &event)
            .expect("dispatch ok");
        assert_eq!(disposition, Some(InputDisposition::Consumed));

        // Recording renderer captured the dispatched event with the
        // translated position.
        let records = log.borrow();
        assert_eq!(records.len(), 1);
        let (recorded_id, recorded_event) = &records[0];
        assert_eq!(*recorded_id, id);
        match recorded_event {
            InputEvent::Pointer { position, .. } => {
                assert_eq!(position.x, 20.0, "x translated to tile-local");
                assert_eq!(position.y, 20.0, "y translated to tile-local");
            }
            _ => panic!("expected Pointer event"),
        }
    }

    #[test]
    fn deliver_input_at_returns_none_when_no_hit() {
        use mere_renderer_registry::{ModifiersState, PointerButton, PointerEventKind};

        let (mut host, _) = build_host_with_recording(NodeContentKind::DocumentTile);
        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::translate(100.0, 200.0),
            Size::new(80.0, 40.0),
        ));

        let event = InputEvent::Pointer {
            position: kurbo::Point::new(0.0, 0.0),
            kind: PointerEventKind::Down(PointerButton::Primary),
            modifiers: ModifiersState::default(),
        };
        // Click far from the rect.
        let disposition = host
            .deliver_input_at(&scene, kurbo::Point::new(5.0, 5.0), &event)
            .expect("dispatch ok");
        assert_eq!(disposition, None);
    }

    #[test]
    fn deliver_input_at_picks_topmost_on_overlap() {
        use mere_renderer_registry::{ModifiersState, PointerButton, PointerEventKind};

        // Two renderers, two content kinds. The CustomCanvas background
        // and the Panel both cover (50, 50). hit-test should route to
        // the Panel (topmost).
        let bg_renderer = RecordingRenderer::for_kind("bg", NodeContentKind::CustomCanvas);
        let panel_renderer = RecordingRenderer::for_kind("panel", NodeContentKind::Panel);
        let bg_log = bg_renderer.shared_input_log();
        let panel_log = panel_renderer.shared_input_log();
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(bg_renderer)).unwrap();
        registry.register(Box::new(panel_renderer)).unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::CustomCanvas,
            Placement::IDENTITY,
            Size::new(200.0, 200.0),
        ));
        let panel_id = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(40.0, 40.0),
            Size::new(100.0, 100.0),
        ));

        let event = InputEvent::Pointer {
            position: kurbo::Point::new(0.0, 0.0),
            kind: PointerEventKind::Down(PointerButton::Primary),
            modifiers: ModifiersState::default(),
        };
        host.deliver_input_at(&scene, kurbo::Point::new(60.0, 60.0), &event)
            .expect("dispatch ok");

        assert_eq!(bg_log.borrow().len(), 0, "background not hit");
        assert_eq!(panel_log.borrow().len(), 1, "panel hit");
        assert_eq!(panel_log.borrow()[0].0, panel_id);
    }
}
