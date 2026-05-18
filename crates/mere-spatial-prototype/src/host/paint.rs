// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Paint + render dispatch for [`super::SubstrateHost`].
//!
//! Split out of `host.rs` to keep the parent module under the
//! workspace's 600-LOC ceiling. The dispatch helpers + edge painter
//! all live here; the struct, camera/registry accessors, and
//! [`FrameReport`](super::FrameReport) stay in the parent.

use kurbo::{Affine, Line, Stroke};
use mere_renderer_registry::{
    CompositionMode, DispatchError, PaintCtx, PaintError, ProducerHandle, RendererId,
};
use vello::peniko::Brush;

use crate::external_texture::{CompositorError, ExternalTextureCompositor};
use crate::lod::compute_lod_for_node;
use crate::scene::{EdgeKind, EdgeStyle, SubstrateScene};

use super::{FrameReport, SubstrateHost};

impl SubstrateHost {
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
        report.edges_painted =
            paint_edges(scene, target, scale_factor, camera, |k| self.edge_style(k));
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
        report.edges_painted =
            paint_edges(scene, target, scale_factor, camera, |k| self.edge_style(k));
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
        let handle: ProducerHandle = match self.producers.get(&node_ref.identity) {
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
    style_for: impl Fn(EdgeKind) -> EdgeStyle,
) -> usize {
    // Edge geometry is in scene coords; `camera` placed via the
    // stroke transform moves edges with their endpoint nodes under
    // pan/zoom. Stroke width scales with host DPR; per-kind styling
    // resolves through the host's `edge_style` palette (defaults from
    // `EdgeStyle::default_for_kind`).
    let mut drawn = 0;
    for edge in scene.iter_edges() {
        let (Some(a), Some(b)) = (
            scene.endpoint_center(edge.from),
            scene.endpoint_center(edge.to),
        ) else {
            continue;
        };
        let style = style_for(edge.kind);
        let stroke = Stroke::new(style.width_px * scale_factor.max(1.0));
        let brush = Brush::Solid(style.color);
        let line = Line::new(a, b);
        target.stroke(&stroke, camera, &brush, None, &line);
        drawn += 1;
    }
    drawn
}
