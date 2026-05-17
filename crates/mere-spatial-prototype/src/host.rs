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
use crate::lod::{LodThresholds, compute_lod_for_node};
use crate::scene::SubstrateScene;

/// Substrate host: registry + per-frame dispatch + camera transform.
pub struct SubstrateHost {
    registry: RendererRegistry,
    /// Cached producer handles for embedded-frame renderers. Keyed by
    /// node identity; value pairs the renderer id (so we can detect
    /// renderer swap-out) with the producer handle the renderer minted.
    producers: HashMap<NodeIdentity, (RendererId, ProducerHandle)>,
    /// Scene-space → host-space transform. Default is identity (1:1
    /// pixel mapping). Pan / zoom mutate this; per-frame dispatch
    /// composes it onto each node's placement so renderers paint in
    /// host pixels.
    camera: Affine,
    /// Pixel thresholds for LOD promotion. Substrate computes each
    /// node's effective LOD per frame from `camera * placement` and
    /// rewrites `SceneNodeRef.lod` before passing to the renderer.
    lod_thresholds: LodThresholds,
}

impl SubstrateHost {
    pub fn new(registry: RendererRegistry) -> Self {
        Self {
            registry,
            producers: HashMap::new(),
            camera: Affine::IDENTITY,
            lod_thresholds: LodThresholds::DEFAULT,
        }
    }

    /// Current LOD-promotion thresholds. Substrate computes effective
    /// LOD before dispatch via these.
    pub fn lod_thresholds(&self) -> &LodThresholds {
        &self.lod_thresholds
    }

    /// Replace the LOD-promotion thresholds.
    pub fn set_lod_thresholds(&mut self, thresholds: LodThresholds) {
        self.lod_thresholds = thresholds;
    }

    pub fn with_default_registry() -> Self {
        Self::new(RendererRegistry::with_default_selector())
    }

    /// The current scene-space → host-space camera transform.
    pub fn camera(&self) -> Affine {
        self.camera
    }

    /// Replace the camera transform outright.
    pub fn set_camera(&mut self, camera: Affine) {
        self.camera = camera;
    }

    /// Reset to the identity camera (no zoom, no pan).
    pub fn reset_camera(&mut self) {
        self.camera = Affine::IDENTITY;
    }

    /// Translate the camera by `(dx, dy)` in host pixels. Composes
    /// onto the existing camera (multiple pans accumulate).
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.camera = Affine::translate((dx, dy)) * self.camera;
    }

    /// Apply a uniform zoom of `factor` around `pivot` (in host
    /// coordinates). `factor > 1.0` zooms in; `factor < 1.0` zooms
    /// out. Composes onto the existing camera.
    ///
    /// Geometric identity: `T(pivot) * S(factor) * T(-pivot) * camera`,
    /// which keeps the scene point currently under `pivot` stationary
    /// while everything else expands away from it.
    pub fn zoom_at(&mut self, pivot: Point, factor: f64) {
        let p = Affine::translate((pivot.x, pivot.y));
        let p_inv = Affine::translate((-pivot.x, -pivot.y));
        self.camera = p * Affine::scale(factor) * p_inv * self.camera;
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
        let camera = self.camera;
        let thresholds = self.lod_thresholds;
        for node in scene.iter() {
            let mut node_ref = node.as_ref();
            node_ref.lod = compute_lod_for_node(&node_ref, camera, &thresholds);
            let mut ctx = PaintCtx {
                scene: target,
                node_transform: camera * node.placement.transform,
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
        report.edges_painted = paint_edges(scene, target, scale_factor, camera);
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
        let camera = self.camera;
        let thresholds = self.lod_thresholds;
        for node in scene.iter() {
            let mut node_ref = node.as_ref();
            node_ref.lod = compute_lod_for_node(&node_ref, camera, &thresholds);
            self.dispatch_node(
                node_ref,
                target,
                compositor,
                vello_renderer,
                scale_factor,
                camera,
                &mut report,
            );
        }
        report.edges_painted = paint_edges(scene, target, scale_factor, camera);
        report
    }

    fn dispatch_node(
        &mut self,
        node_ref: mere_renderer_registry::SceneNodeRef,
        target: &mut vello::Scene,
        compositor: &mut ExternalTextureCompositor,
        vello_renderer: &mut vello::Renderer,
        scale_factor: f64,
        camera: Affine,
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
        let effective_transform = camera * node_ref.placement.transform;
        match mode {
            CompositionMode::InScenePaint => {
                let mut ctx = PaintCtx {
                    scene: target,
                    node_transform: effective_transform,
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
                    effective_transform,
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
        effective_transform: Affine,
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
            effective_transform,
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
        // wanting edge-click semantics call `host.scene_pos_from_host`
        // and `scene.hit_test` themselves.
        //
        // Pull the click back through the camera into scene space, then
        // hit-test. The pointer event's final position lands in
        // tile-local coordinates via the (camera * placement) composite.
        let scene_pos = self.scene_pos_from_host(host_pos);
        let Some(target) = scene.hit_test_node(scene_pos) else {
            return Ok(None);
        };
        let Some(node) = scene.get(target) else {
            return Ok(None);
        };
        let effective_transform = self.camera * node.placement.transform;
        let local_event = rewrite_pointer_position(event, host_pos, effective_transform);
        self.deliver_input(scene, target, &local_event)
    }

    /// Map a host-space point back to scene-space via the camera's
    /// inverse. Returns `host_pos` unchanged if the camera is
    /// degenerate (zero determinant — typically a pinned-to-singularity
    /// zoom-out edge case). Useful for callers that want to do their
    /// own hit-test against scene-space geometry.
    pub fn scene_pos_from_host(&self, host_pos: Point) -> Point {
        if self.camera.determinant() == 0.0 {
            return host_pos;
        }
        self.camera.inverse() * host_pos
    }
}

/// Paint substrate-owned relation edges into `target`. Edges paint
/// over nodes; this runs after the per-node dispatch loop.
///
/// `camera` is applied to the line geometry via the stroke transform
/// (so edges follow nodes under pan/zoom) and to the stroke width
/// (so the line maintains apparent thickness as the camera zooms).
///
/// v0a strokes a straight line between endpoint centers; label
/// rendering and per-`EdgeKind` styling are deferred to a follow-up
/// (needs parley wiring for text + a real consumer to drive the
/// per-kind color palette). Endpoints not in the scene cause the edge
/// to be silently skipped.
///
/// Returns the count of edges actually drawn (skipped-endpoint edges
/// don't count).
fn paint_edges(
    scene: &SubstrateScene,
    target: &mut vello::Scene,
    scale_factor: f64,
    camera: Affine,
) -> usize {
    // v0a styling: subtle stroke that reads clearly over both light
    // and dark backgrounds; width scaled to host DPR. Edge geometry
    // is in scene coords; `camera` placed via the stroke transform
    // moves edges with their endpoint nodes under pan/zoom.
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
        target.stroke(&stroke, camera, &brush, None, &line);
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
    fn diagnostics_fire_for_register_unregister_and_misroute() {
        use mere_renderer_registry::{DiagnosticEvent, RecordingSink, RouteDegradedReason};
        use std::sync::Arc;

        let sink = Arc::new(RecordingSink::new());

        // Sink stored once on the registry. We borrow through `sink_view`
        // for assertions; the registry holds its own Box<dyn DiagnosticSink>
        // pointing at the same RecordingSink via shared ownership.
        struct SharedSink(Arc<RecordingSink>);
        impl mere_renderer_registry::DiagnosticSink for SharedSink {
            fn record(&self, event: DiagnosticEvent) {
                self.0.record(event);
            }
        }

        let mut registry =
            RendererRegistry::with_default_selector().with_sink(Box::new(SharedSink(sink.clone())));
        let renderer_id = RendererId::from_static("test");
        registry
            .register(Box::new(RecordingRenderer::for_kind(
                "test",
                NodeContentKind::DocumentTile,
            )))
            .unwrap();

        // First event: RendererRegistered with the right kinds.
        let events = sink.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DiagnosticEvent::RendererRegistered { id, kinds } => {
                assert_eq!(*id, renderer_id);
                assert!(kinds.contains(&NodeContentKind::DocumentTile));
            }
            other => panic!("expected RendererRegistered, got {:?}", other),
        }

        // Now exercise a misroute: register a node with a content kind
        // no renderer handles.
        let mut host = SubstrateHost::new(registry);
        let mut scene = SubstrateScene::new();
        let unrouted = scene.insert(SubstrateNode::new(
            NodeContentKind::WebPage, // no renderer for this kind
            Placement::IDENTITY,
            Size::new(50.0, 50.0),
        ));
        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.missing_renderer, 1);

        // Second event: RouteDegraded { NoCandidates } for the unrouted node.
        let events = sink.events();
        assert!(
            events.iter().any(|e| matches!(
                e,
                DiagnosticEvent::RouteDegraded {
                    node,
                    reason: RouteDegradedReason::NoCandidates,
                } if *node == unrouted
            )),
            "expected RouteDegraded NoCandidates for {:?}; got {:?}",
            unrouted,
            events
        );

        // Unregister + verify the corresponding event.
        host.registry_mut().unregister(&renderer_id);
        let events = sink.events();
        assert!(
            events.iter().any(|e| matches!(
                e,
                DiagnosticEvent::RendererUnregistered { id } if *id == renderer_id
            )),
            "expected RendererUnregistered for {:?}; got {:?}",
            renderer_id,
            events
        );
    }

    #[test]
    fn renderer_pin_overrides_default_selector() {
        // Register two renderers for the same NodeContentKind; the
        // default selector would pick the first-registered one. A
        // pinned node should route to the second.
        let first = RecordingRenderer::for_kind("first", NodeContentKind::DocumentTile);
        let second = RecordingRenderer::for_kind("second", NodeContentKind::DocumentTile);
        let first_log = first.shared_paint_log();
        let second_log = second.shared_paint_log();
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(first)).unwrap();
        registry.register(Box::new(second)).unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        // Unpinned node: should go to "first".
        scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::translate(10.0, 10.0),
            Size::new(50.0, 50.0),
        ));
        // Pinned to "second".
        scene.insert(
            SubstrateNode::new(
                NodeContentKind::DocumentTile,
                Placement::translate(100.0, 10.0),
                Size::new(50.0, 50.0),
            )
            .with_renderer_pin(RendererId::from_static("second")),
        );

        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        assert_eq!(report.painted, 2);

        assert_eq!(first_log.borrow().len(), 1, "first received the unpinned node");
        assert_eq!(second_log.borrow().len(), 1, "second received the pinned node");
    }

    #[test]
    fn renderer_pin_falls_through_when_pin_not_registered() {
        let renderer = RecordingRenderer::for_kind("only", NodeContentKind::DocumentTile);
        let log = renderer.shared_paint_log();
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(renderer)).unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        scene.insert(
            SubstrateNode::new(
                NodeContentKind::DocumentTile,
                Placement::IDENTITY,
                Size::new(50.0, 50.0),
            )
            .with_renderer_pin(RendererId::from_static("ghost")),
        );

        let mut target = vello::Scene::new();
        let report = host.paint_scene(&scene, &mut target, 1.0);
        // Pin doesn't match any registered renderer → falls through to
        // first-candidate ("only").
        assert_eq!(report.painted, 1);
        assert_eq!(log.borrow().len(), 1);
    }

    #[test]
    fn camera_defaults_to_identity() {
        let host = SubstrateHost::with_default_registry();
        assert_eq!(host.camera(), Affine::IDENTITY);
    }

    #[test]
    fn set_and_reset_camera() {
        let mut host = SubstrateHost::with_default_registry();
        host.set_camera(Affine::translate((100.0, 50.0)));
        assert_eq!(host.camera(), Affine::translate((100.0, 50.0)));
        host.reset_camera();
        assert_eq!(host.camera(), Affine::IDENTITY);
    }

    #[test]
    fn pan_translates_camera() {
        let mut host = SubstrateHost::with_default_registry();
        host.pan(10.0, 20.0);
        host.pan(5.0, 5.0);
        // Pans compose; total is (15, 25).
        let mapped = host.camera() * kurbo::Point::new(0.0, 0.0);
        assert_eq!(mapped, kurbo::Point::new(15.0, 25.0));
    }

    #[test]
    fn zoom_at_keeps_pivot_stationary() {
        let mut host = SubstrateHost::with_default_registry();
        let pivot = kurbo::Point::new(200.0, 150.0);
        host.zoom_at(pivot, 2.0);
        // The pivot point should map to itself.
        let mapped = host.camera() * pivot;
        assert!((mapped.x - pivot.x).abs() < 1e-9);
        assert!((mapped.y - pivot.y).abs() < 1e-9);
        // A scene point 50 units left of the pivot expands to 100 units left.
        let scene_pt = kurbo::Point::new(150.0, 150.0);
        let mapped_pt = host.camera() * scene_pt;
        assert!((mapped_pt.x - 100.0).abs() < 1e-9);
    }

    #[test]
    fn scene_pos_from_host_inverts_camera() {
        let mut host = SubstrateHost::with_default_registry();
        host.set_camera(Affine::translate((100.0, 50.0)) * Affine::scale(2.0));
        // Host (200, 100) → through camera⁻¹ → scene (50, 25).
        let scene_pt = host.scene_pos_from_host(kurbo::Point::new(200.0, 100.0));
        assert!((scene_pt.x - 50.0).abs() < 1e-9);
        assert!((scene_pt.y - 25.0).abs() < 1e-9);
    }

    #[test]
    fn deliver_input_at_respects_camera() {
        use mere_renderer_registry::{ModifiersState, PointerButton, PointerEventKind};

        let renderer = RecordingRenderer::for_kind("rec", NodeContentKind::DocumentTile);
        let log = renderer.shared_input_log();
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(renderer)).unwrap();
        let mut host = SubstrateHost::new(registry);
        // Pan the camera so scene-space (0, 0) sits at host-space (200, 100).
        host.set_camera(Affine::translate((200.0, 100.0)));

        let mut scene = SubstrateScene::new();
        let id = scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::IDENTITY, // node at scene origin
            Size::new(80.0, 40.0),
        ));

        let event = InputEvent::Pointer {
            position: kurbo::Point::ZERO,
            kind: PointerEventKind::Down(PointerButton::Primary),
            modifiers: ModifiersState::default(),
        };
        // Click at host-space (220, 110), which maps to scene-space
        // (20, 10) — inside the panned node at tile-local (20, 10).
        let disposition = host
            .deliver_input_at(&scene, kurbo::Point::new(220.0, 110.0), &event)
            .expect("dispatch ok");
        assert_eq!(disposition, Some(InputDisposition::Consumed));
        let records = log.borrow();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, id);
        match &records[0].1 {
            InputEvent::Pointer { position, .. } => {
                assert!((position.x - 20.0).abs() < 1e-9, "x tile-local");
                assert!((position.y - 10.0).abs() < 1e-9, "y tile-local");
            }
            _ => panic!("expected pointer"),
        }
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
