/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable overlay stroke descriptors.
//!
//! These are the pure-data shapes that the workbench compositor emits
//! for host painters to render on top of graph panes — focus rings,
//! selection borders, dashed drag-preview outlines, lens-glyph badges.
//! Descriptors flow through the `FrameViewModel.overlays` field in
//! the host-neutral vocabulary; egui / iced hosts convert the
//! portable types back to their native painting primitives at the
//! render boundary.
//!
//! Pre-M4 slice 10 (2026-04-22) these types lived in
//! `shell/desktop/workbench/compositor_adapter.rs` alongside the
//! egui-specific painter, and the glyph-overlay types were in
//! `registries/atomic/lens/registry.rs`. Consolidating them here
//! removes the last data-shape barrier between `FrameViewModel` and
//! graphshell-core.

use serde::{Deserialize, Serialize};

use crate::geometry::PortableRect;
use crate::graph::NodeKey;
use crate::pane::TileRenderMode;

/// One overlay stroke pass the host painter renders over a pane.
///
/// Fields use portable types throughout (`PortableRect`,
/// `graph_canvas::packet::Stroke`) so the descriptor flows across the
/// host boundary without egui leakage. Egui painters convert at the
/// draw-call boundary via `egui_rect_from_portable` /
/// `egui_stroke_from_portable` in `compositor_adapter`; iced painters
/// consume the portable types directly.
#[derive(Clone)]
pub struct OverlayStrokePass {
    pub node_key: NodeKey,
    pub tile_rect: PortableRect,
    pub rounding: f32,
    pub stroke: graph_canvas::packet::Stroke,
    pub glyph_overlays: Vec<GlyphOverlay>,
    pub style: OverlayAffordanceStyle,
    pub render_mode: TileRenderMode,
}

/// Affordance classification — hosts use this to pick the visual
/// treatment. Pure data with no host-specific payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayAffordanceStyle {
    /// Plain rectangular stroke (focus ring, selection border).
    RectStroke,
    /// Dashed rectangular stroke (drag-preview target indicator).
    DashedRectStroke,
    /// Stroke applied to just the pane chrome area, not the full
    /// tile rect.
    AreaStroke,
    /// Render chrome only — no stroke overlay.
    ChromeOnly,
}

/// Lens-registered glyph overlay attached to a pane corner or center.
///
/// The glyph itself is identified by a host-resolved string id; the
/// host supplies the glyph-to-pixel rendering (usually via an icon
/// font or SVG atlas). Pre-M4 slice 10 (2026-04-22) this lived in
/// `registries/atomic/lens/registry.rs`; moved here alongside
/// `OverlayStrokePass` because they travel together through the
/// view-model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlyphOverlay {
    pub glyph_id: String,
    pub anchor: GlyphAnchor,
}

/// Anchor classification for [`GlyphOverlay`]. Hosts position the
/// glyph based on this: corner or center of the pane rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlyphAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_overlay_serde_round_trip() {
        // Serialize → deserialize — pins the wire shape across
        // persistence / network emission boundaries.
        let original = GlyphOverlay {
            glyph_id: "warning".to_string(),
            anchor: GlyphAnchor::TopRight,
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: GlyphOverlay = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn glyph_anchor_serde_uses_variant_names() {
        // Pin wire shape — persisted sessions mustn't break when the
        // enum order changes.
        let cases = [
            (GlyphAnchor::TopLeft, "\"TopLeft\""),
            (GlyphAnchor::TopRight, "\"TopRight\""),
            (GlyphAnchor::BottomLeft, "\"BottomLeft\""),
            (GlyphAnchor::BottomRight, "\"BottomRight\""),
            (GlyphAnchor::Center, "\"Center\""),
        ];
        for (variant, expected) in cases {
            assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
        }
    }

    #[test]
    fn overlay_affordance_style_variants_distinct() {
        assert_ne!(
            OverlayAffordanceStyle::RectStroke,
            OverlayAffordanceStyle::DashedRectStroke
        );
        assert_ne!(
            OverlayAffordanceStyle::AreaStroke,
            OverlayAffordanceStyle::ChromeOnly
        );
    }
}
