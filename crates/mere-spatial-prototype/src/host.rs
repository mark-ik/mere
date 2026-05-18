// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `SubstrateHost` — drives one frame of registry-mediated paint dispatch.
//!
//! Owns the [`RendererRegistry`] and walks a [`SubstrateScene`] per frame,
//! handing each node to whichever renderer the selector resolves. The
//! per-frame reports surface what dispatched, what didn't, and why — the
//! diagnostic surface §8 of the contract brief expects.
//!
//! The implementation is split across child modules to keep each file
//! under the workspace's 600-LOC ceiling:
//!
//! - [`paint`] — `paint_scene` / `render_scene` / private dispatch
//!   helpers + the substrate edge painter.
//! - [`embedded`] — embedded-frame producer lifecycle
//!   (`ensure_embedded_producers`, `take_subtree_for_node`,
//!   `release_node`).
//! - [`input`] — `deliver_input` / `deliver_input_at` + private
//!   pointer-position rewriter.

use std::collections::HashMap;

use kurbo::{Affine, Point};
use mere_renderer_registry::{NodeIdentity, ProducerHandle, RendererId, RendererRegistry};

use crate::lod::LodThresholds;
use crate::scene::{EdgeKind, EdgeStyle};

mod embedded;
mod input;
mod paint;

#[cfg(test)]
mod tests;

/// Substrate host: registry + per-frame dispatch + camera transform.
pub struct SubstrateHost {
    pub(super) registry: RendererRegistry,
    /// Cached producer handles for embedded-frame renderers. Keyed by
    /// node identity; value pairs the renderer id (so we can detect
    /// renderer swap-out) with the producer handle the renderer minted.
    pub(super) producers: HashMap<NodeIdentity, (RendererId, ProducerHandle)>,
    /// Scene-space → host-space transform. Default is identity (1:1
    /// pixel mapping). Pan / zoom mutate this; per-frame dispatch
    /// composes it onto each node's placement so renderers paint in
    /// host pixels.
    pub(super) camera: Affine,
    /// Pixel thresholds for LOD promotion. Substrate computes each
    /// node's effective LOD per frame from `camera * placement` and
    /// rewrites `SceneNodeRef.lod` before passing to the renderer.
    pub(super) lod_thresholds: LodThresholds,
    /// Per-EdgeKind stroke style overrides. Edges with no entry use
    /// `EdgeStyle::default_for_kind(kind)`. Hosts call
    /// [`Self::set_edge_style`] to install custom palettes — typical
    /// users are cartography, which owns the relation taxonomy.
    pub(super) edge_styles: HashMap<EdgeKind, EdgeStyle>,
}

impl SubstrateHost {
    pub fn new(registry: RendererRegistry) -> Self {
        Self {
            registry,
            producers: HashMap::new(),
            camera: Affine::IDENTITY,
            lod_thresholds: LodThresholds::DEFAULT,
            edge_styles: HashMap::new(),
        }
    }

    /// Install or replace the stroke style for a specific
    /// `EdgeKind`. If unset, the substrate paints with
    /// [`EdgeStyle::default_for_kind`].
    pub fn set_edge_style(&mut self, kind: EdgeKind, style: EdgeStyle) {
        self.edge_styles.insert(kind, style);
    }

    /// Lookup the effective style for `kind` — host-installed
    /// override if set, otherwise the v0a default.
    pub fn edge_style(&self, kind: EdgeKind) -> EdgeStyle {
        self.edge_styles
            .get(&kind)
            .copied()
            .unwrap_or_else(|| EdgeStyle::default_for_kind(kind))
    }

    /// Drop the override for `kind`; subsequent paints use the v0a
    /// default. Returns the previous override if any.
    pub fn clear_edge_style(&mut self, kind: EdgeKind) -> Option<EdgeStyle> {
        self.edge_styles.remove(&kind)
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
