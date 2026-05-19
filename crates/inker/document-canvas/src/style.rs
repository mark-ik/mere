/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Style configuration for document layout.
//!
//! [`StyleConfig`] is the user-tunable knobs for typography + spacing.
//! Default values are sensible for body documents at 14px base size.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StyleConfig {
    /// Body text font size (px).
    pub body_font_size: f32,
    /// Heading font sizes for levels 1..6 (index 0 = h1, …, index 5 = h6).
    pub heading_sizes: [f32; 6],
    /// Line-height multiplier (e.g. 1.4 = 140% of font size).
    pub line_height_ratio: f32,
    /// Vertical space (px) between paragraphs and other body blocks.
    pub paragraph_spacing: f32,
    /// Vertical space (px) above each heading.
    pub heading_spacing_above: f32,
    /// Vertical space (px) below each heading.
    pub heading_spacing_below: f32,
    /// Horizontal indent (px) per nesting level for quotes / lists.
    pub indent_per_level: f32,
    /// Horizontal padding (px) inside the viewport.
    pub horizontal_padding: f32,
    /// Vertical padding (px) at the top + bottom of the viewport.
    pub vertical_padding: f32,
    /// Body font family name (resolved by the host's font system).
    pub body_font_family: String,
    /// Monospace font family name for code blocks + inline code.
    pub mono_font_family: String,
    /// Color vocabulary for downstream renderers. v1 of the netrender
    /// backend reads these for placeholder + rule colors; future glyph
    /// emission reads `body_text` / `heading_text` / `link_text` /
    /// `code_text` / `badge_text` for per-block-kind text color.
    pub colors: ColorVocabulary,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            body_font_size: 14.0,
            heading_sizes: [28.0, 22.0, 18.0, 16.0, 14.0, 12.0],
            line_height_ratio: 1.4,
            paragraph_spacing: 12.0,
            heading_spacing_above: 16.0,
            heading_spacing_below: 8.0,
            indent_per_level: 24.0,
            horizontal_padding: 16.0,
            vertical_padding: 16.0,
            body_font_family: "system-ui".to_string(),
            mono_font_family: "monospace".to_string(),
            colors: ColorVocabulary::default(),
        }
    }
}

/// Theme primitives used by downstream renderers. All colors are
/// premultiplied RGBA in 0..=1 space (the format `netrender::Scene`
/// expects directly).
///
/// The defaults are a "near-black on transparent" palette suitable for
/// light themes; callers tune for their host. Per-style brushes
/// (link-text, code-text, etc.) are stored separately so a single field
/// change re-themes one block kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorVocabulary {
    pub body_text: [f32; 4],
    pub heading_text: [f32; 4],
    pub link_text: [f32; 4],
    pub code_text: [f32; 4],
    pub badge_text: [f32; 4],
    pub rule: [f32; 4],
    /// Tint for placeholder rects emitted when a renderer can't (yet)
    /// emit real glyphs — e.g. when no font resolver is registered for a
    /// given family.
    pub placeholder_text: [f32; 4],
    /// Tint for image placeholder strips (before the host fetches the
    /// real image bytes).
    pub placeholder_image: [f32; 4],
}

impl Default for ColorVocabulary {
    fn default() -> Self {
        Self {
            body_text: [0.05, 0.05, 0.08, 1.0],
            heading_text: [0.02, 0.02, 0.05, 1.0],
            link_text: [0.10, 0.30, 0.70, 1.0],
            code_text: [0.20, 0.05, 0.10, 1.0],
            badge_text: [0.30, 0.30, 0.40, 1.0],
            rule: [0.40, 0.40, 0.40, 1.0],
            placeholder_text: [0.05, 0.05, 0.08, 0.10],
            placeholder_image: [0.50, 0.50, 0.60, 0.20],
        }
    }
}

impl StyleConfig {
    /// Font size for a heading at `level` (1..=6). Levels above 6 clamp
    /// to h6.
    pub fn heading_size(&self, level: u8) -> f32 {
        let idx = level.clamp(1, 6) as usize - 1;
        self.heading_sizes[idx]
    }

    pub fn line_height(&self, font_size: f32) -> f32 {
        font_size * self.line_height_ratio
    }
}

/// Per-inline-span style attributes. Computed during inline-flattening and
/// applied to parley as ranged style properties. Used as parley's `Brush`
/// type — must satisfy `Clone + PartialEq + Default + Debug`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InlineStyle {
    pub italic: bool,
    pub bold: bool,
    pub monospace: bool,
    pub link: bool,
}

impl InlineStyle {
    pub const NORMAL: Self = Self {
        italic: false,
        bold: false,
        monospace: false,
        link: false,
    };

    pub fn with_italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn with_bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn with_monospace(mut self) -> Self {
        self.monospace = true;
        self
    }

    pub fn with_link(mut self) -> Self {
        self.link = true;
        self
    }
}
