// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The node accent palette: the one table every representation of a node tints
//! from, so a node reads as the same node wherever it appears (canvas gnode,
//! workbench tab, gloss outline row, status-cluster glyph). The rule is uniform:
//! **selection wins over activation state**. (Representations carry node
//! identity.)
//!
//! Two consumer shapes, one source:
//!
//! - **DOM consumers** paste [`custom_property_declarations`] into whichever
//!   rule is the root of their document or panel, then reference the colors from
//!   descendant rules with `var(--node-open-bg)` and friends. The cascade does
//!   the rest, so a representation inherits node identity without threading a
//!   color through Rust.
//! - **Imperative painters** (chisel leaves, paint commands, the tab contract)
//!   read [`accent`] and convert with [`unit`], since they paint outside the
//!   cascade and cannot resolve a `var()`.

use crate::types::NodeState;
pub use tincture::{Seeds as ThemeSeeds, Srgb as ThemeColor};
use tincture::{derive_palette, mix};

/// Number of caller-controlled colors a derived face may address.
pub const DERIVED_FACE_COLOR_COUNT: usize = 8;
const _: () = assert!(DERIVED_FACE_COLOR_COUNT == pictograph::PALETTE_SPAN as usize);

/// The seed set behind Canvas's built-in derived-face palette. Hosts can replace it at runtime
/// with [`DerivedFacePalette::from_seeds`] and [`crate::Canvas::set_derived_face_palette`].
pub const DEFAULT_DERIVED_FACE_SEEDS: ThemeSeeds = ThemeSeeds {
    primary: ThemeColor::rgb(0x33, 0x66, 0xC8),
    secondary: ThemeColor::rgb(0x2E, 0x9D, 0xA6),
    tertiary: ThemeColor::rgb(0xE0, 0xA8, 0x46),
    neutral: ThemeColor::rgb(0x10, 0x14, 0x22),
    text_header: None,
    text_body: None,
    success: ThemeColor::rgb(0x4F, 0xB3, 0x6E),
    danger: ThemeColor::rgb(0xD5, 0x4E, 0x4E),
    dark: true,
};

/// Eight straight-alpha sRGB colors supplied to pictograph's palette-indexed fills.
///
/// This is live Canvas state, separate from deterministic face bytes. Replacing it recolors every
/// derived face on the next frame, including already-cached faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DerivedFacePalette {
    colors: [[u8; 4]; DERIVED_FACE_COLOR_COUNT],
}

impl DerivedFacePalette {
    /// A caller-authored palette. Colors are straight-alpha sRGB.
    pub const fn new(colors: [[u8; 4]; DERIVED_FACE_COLOR_COUNT]) -> Self {
        Self { colors }
    }

    /// Derive the face colors from the same seed vocabulary as Mere themes.
    pub fn from_seeds(seeds: &ThemeSeeds) -> Self {
        let palette = derive_palette(seeds);
        let companion = palette.text;
        Self::new([
            palette.primary.to_array(),
            palette.secondary.to_array(),
            palette.tertiary.to_array(),
            palette.success.to_array(),
            palette.danger.to_array(),
            mix(palette.primary, companion, 0.28).to_array(),
            mix(palette.secondary, companion, 0.28).to_array(),
            mix(palette.tertiary, companion, 0.28).to_array(),
        ])
    }

    /// The straight-alpha sRGB entries, in pictograph slot order.
    pub const fn colors(self) -> [[u8; 4]; DERIVED_FACE_COLOR_COUNT] {
        self.colors
    }
}

impl Default for DerivedFacePalette {
    fn default() -> Self {
        Self::from_seeds(&DEFAULT_DERIVED_FACE_SEEDS)
    }
}

/// A node's fill and label color for one activation state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeAccent {
    /// The tile / dot / tab fill.
    pub bg: [u8; 3],
    /// The label color that reads against `bg`.
    pub fg: [u8; 3],
}

/// Selected: amber, dark label. Wins over every activation state.
pub const SELECTED: NodeAccent = NodeAccent {
    bg: [232, 150, 40],
    fg: [28, 22, 10],
};
/// Open: a live actor is showing real fetched content.
pub const OPEN: NodeAccent = NodeAccent {
    bg: [58, 140, 94],
    fg: [238, 250, 243],
};
/// Closed: real fetched content, but no actor showing it right now.
pub const CLOSED: NodeAccent = NodeAccent {
    bg: [166, 72, 72],
    fg: [250, 240, 240],
};
/// Idle: local / settings / synthesized / blank / errored. The default fill.
pub const IDLE: NodeAccent = NodeAccent {
    bg: [54, 92, 156],
    fg: [245, 247, 252],
};

/// The canvas caption label riding beside a gnode (not a node fill, so it is not
/// a [`NodeAccent`]).
pub const CAPTION_FG: [u8; 3] = [216, 222, 234];

/// The accent for a node, applying the selection-wins rule.
pub fn accent(selected: bool, state: NodeState) -> NodeAccent {
    if selected {
        return SELECTED;
    }
    match state {
        NodeState::Open => OPEN,
        NodeState::Closed => CLOSED,
        NodeState::Idle => IDLE,
    }
}

/// The slug naming a node's accent, shared by the CSS custom-property names
/// (`--node-{slug}-bg`) and by consumers that build a modifier class from it
/// (`.gloss-outline-dot-{slug}`). Keeps the two vocabularies from drifting.
pub fn state_slug(selected: bool, state: NodeState) -> &'static str {
    if selected {
        return "selected";
    }
    match state {
        NodeState::Open => "open",
        NodeState::Closed => "closed",
        NodeState::Idle => "idle",
    }
}

/// A color as the CSS `rgb(r, g, b)` form.
pub fn rgb(c: [u8; 3]) -> String {
    format!("rgb({}, {}, {})", c[0], c[1], c[2])
}

/// A color as 0..1 components, for painters that take floats.
pub fn unit(c: [u8; 3]) -> [f32; 3] {
    [
        f32::from(c[0]) / 255.0,
        f32::from(c[1]) / 255.0,
        f32::from(c[2]) / 255.0,
    ]
}

/// The `--node-*` custom-property declarations, as a run of `name: value;` pairs
/// ready to paste inside a rule body. Put them on the root element of a document
/// (or the root of a panel) and every descendant rule can `var()` them.
pub fn custom_property_declarations() -> String {
    let mut out = String::new();
    for (slug, a) in [
        ("idle", IDLE),
        ("open", OPEN),
        ("closed", CLOSED),
        ("selected", SELECTED),
    ] {
        out.push_str(&format!(
            "--node-{slug}-bg: {}; --node-{slug}-fg: {}; ",
            rgb(a.bg),
            rgb(a.fg)
        ));
    }
    out.push_str(&format!("--node-caption-fg: {};", rgb(CAPTION_FG)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_wins_over_every_activation_state() {
        for state in [NodeState::Open, NodeState::Closed, NodeState::Idle] {
            assert_eq!(accent(true, state), SELECTED);
            assert_eq!(state_slug(true, state), "selected");
        }
    }

    #[test]
    fn each_state_maps_to_its_own_accent() {
        assert_eq!(accent(false, NodeState::Open), OPEN);
        assert_eq!(accent(false, NodeState::Closed), CLOSED);
        assert_eq!(accent(false, NodeState::Idle), IDLE);
    }

    #[test]
    fn declarations_define_every_var_the_sheets_reference() {
        let decls = custom_property_declarations();
        for slug in ["idle", "open", "closed", "selected"] {
            assert!(
                decls.contains(&format!("--node-{slug}-bg:")),
                "missing {slug} bg"
            );
            assert!(
                decls.contains(&format!("--node-{slug}-fg:")),
                "missing {slug} fg"
            );
        }
        assert!(decls.contains("--node-caption-fg:"));
    }

    #[test]
    fn declarations_carry_the_palette_values() {
        let decls = custom_property_declarations();
        assert!(decls.contains("--node-selected-bg: rgb(232, 150, 40);"));
        assert!(decls.contains("--node-open-bg: rgb(58, 140, 94);"));
        assert!(decls.contains("--node-closed-bg: rgb(166, 72, 72);"));
        assert!(decls.contains("--node-idle-bg: rgb(54, 92, 156);"));
    }

    #[test]
    fn unit_normalizes_to_zero_one() {
        assert_eq!(unit([255, 0, 255]), [1.0, 0.0, 1.0]);
        let [r, g, b] = unit(OPEN.bg);
        assert!((r - 58.0 / 255.0).abs() < 1e-6);
        assert!((g - 140.0 / 255.0).abs() < 1e-6);
        assert!((b - 94.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn derived_face_palette_comes_from_theme_seeds() {
        let palette = DerivedFacePalette::from_seeds(&DEFAULT_DERIVED_FACE_SEEDS);
        let colors = palette.colors();
        assert_eq!(colors.len(), pictograph::PALETTE_SPAN as usize);
        assert_eq!(colors[0], [0x33, 0x66, 0xC8, 255]);
        assert_eq!(colors[1], [0x2E, 0x9D, 0xA6, 255]);
        assert_eq!(colors[2], [0xE0, 0xA8, 0x46, 255]);
        let distinct = colors.into_iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(
            distinct.len(),
            colors.len(),
            "the default exposes eight visual choices"
        );
    }
}
