// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Paint contract for in-scene-paint renderers.
//!
//! [`PaintCtx`] is what the host hands to a renderer's `paint(...)` call —
//! the host-owned `&mut vello::Scene` to draw into, plus the node's transform
//! and the HiDPI scale factor.

use kurbo::Affine;
use vello::Scene;

/// Context passed to [`crate::InScenePaintRenderer::paint`] each frame.
pub struct PaintCtx<'a> {
    /// Host-owned target scene. Renderers either:
    ///
    /// - Build their own `Scene` and call
    ///   `target.append(&renderer_scene, Some(node_transform))` — Option A
    ///   from the [renderer-registry brief §3.1] (recommended for v0).
    /// - Paint operations directly into `target` after pushing a transform —
    ///   Option B (lower allocation overhead, slightly more work in the
    ///   renderer to keep transforms balanced).
    ///
    /// [renderer-registry brief §3.1]: ../../../../design_docs/mere_docs/research/2026-05-15_renderer_registry_contract_brief.md
    pub scene: &'a mut Scene,

    /// Affine transform that places this node within the host scene's
    /// coordinate space. Renderers using Option A pass this directly to
    /// `Scene::append`; renderers using Option B use it to position their
    /// drawing.
    pub node_transform: Affine,

    /// HiDPI scale factor (`1.0`, `1.5`, `2.0`, ...). Renderers needing
    /// pixel-perfect rendering scale their stroke widths / hairline cutoffs
    /// by this; most renderers can ignore (vello handles transform-aware
    /// rasterization).
    pub scale_factor: f64,
}

/// Why a paint pass failed.
///
/// v0 is intentionally narrow; expand as real failure modes surface.
#[derive(Debug)]
pub enum PaintError {
    /// Renderer's per-node lifecycle isn't ready to paint this frame
    /// (e.g. embedded-frame producer hasn't produced a first frame; the
    /// renderer crate just elects not to paint).
    NotReady,
    /// Renderer encountered an internal failure; the host should paint a
    /// placeholder for this node.
    RendererFailed(String),
}

pub type PaintResult = Result<(), PaintError>;
