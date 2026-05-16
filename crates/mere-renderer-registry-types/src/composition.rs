// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `CompositionMode` — the three ways a 2D/3D renderer can hand pixels to a
//! hosting compositor.
//!
//! Per the [contract brief §2.4]: every renderer Mere has, plans, or might
//! add fits into one of these three modes.
//!
//! [contract brief §2.4]: ../../../../design_docs/mere_docs/research/2026-05-15_renderer_registry_contract_brief.md

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CompositionMode {
    /// Renderer paints scene ops into the host's vello scene during the
    /// host's paint pass.
    InScenePaint,
    /// Renderer produces an independent wgpu texture / surface on its own
    /// schedule; host composites as external texture.
    EmbeddedFrame,
    /// Renderer renders into an out-of-band OS surface; OS composites.
    Overlay,
}
