// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable paint primitives — `Color` and `Stroke`.
//!
//! These types are the kernel's framework-agnostic color + stroke
//! vocabulary. Host painters convert to backend-specific types
//! (egui `Color32`, iced `Color`, vello / wgpu colors) at the draw-
//! call boundary; portable descriptors flow through `kernel`'s
//! overlay surface (`OverlayStrokePass`), through `host-contract`
//! ports, and through `graph-canvas`'s `packet::SceneDrawItem` /
//! `node_style` / `derive` surfaces — all without coupling the kernel
//! to a renderer crate.
//!
//! **Why these live in kernel, not graph-canvas**: previously
//! they lived in `graph_canvas::packet`, with kernel importing
//! them — that made `kernel → graph-canvas` an upward dependency
//! that blocked the
//! [cartography layer](../../../../design_docs/mere_docs/research/2026-05-10_cartography_layer_brief.md)
//! brief's revised §9 step 4 (the `graph-layout` extraction).
//! Moving these into the kernel and having `graph-canvas` re-export
//! them reverses the edge to its natural direction (kernel is
//! foundational; everything else depends on it). The graph-canvas
//! public API stays stable through the re-export.
//!
//! ## Coexistence with `color::Color32`
//!
//! The kernel also has [`crate::color::Color32`] — an u8-channel
//! color type used for egui-shaped host palettes (drag-preview
//! tinting, etc.). [`Color`] in this module is the f32-channel form
//! used for renderer-style descriptors. The two coexist by design:
//! `Color32` is the kernel's host-palette type; `Color` is the
//! kernel's renderer-descriptor type. A future cleanup may unify
//! them; for now both ship.

use serde::{Deserialize, Serialize};

/// A color in linear RGBA, 0.0–1.0 per channel.
///
/// Framework-agnostic color type. Conversion to/from backend-specific
/// colors (egui `Color32`, iced `Color`, renderer-specific colors)
/// happens at the host bridge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

/// Stroke style for lines and shape outlines.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_default_is_white() {
        assert_eq!(Color::default(), Color::WHITE);
    }

    #[test]
    fn color_constants_have_expected_channel_values() {
        assert_eq!(Color::WHITE.r, 1.0);
        assert_eq!(Color::BLACK.r, 0.0);
        assert_eq!(Color::TRANSPARENT.a, 0.0);
    }

    #[test]
    fn color_serde_round_trip() {
        let original = Color::new(0.25, 0.5, 0.75, 1.0);
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn stroke_carries_color_and_width() {
        let stroke = Stroke {
            color: Color::BLACK,
            width: 2.0,
        };
        assert_eq!(stroke.color, Color::BLACK);
        assert_eq!(stroke.width, 2.0);
    }
}
