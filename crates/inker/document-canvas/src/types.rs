/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Render-packet types.
//!
//! All coordinates are in *logical pixels* relative to the viewport's
//! top-left origin. Y grows downward (screen convention).

use serde::{Deserialize, Serialize};

/// Viewport into which the document is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    /// Device-pixel ratio. v1 layout doesn't use this directly (it lays out
    /// in logical pixels), but downstream rasterisers need it.
    pub scale_factor: f32,
}

impl Viewport {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            scale_factor: 1.0,
        }
    }

    pub fn with_scale_factor(mut self, scale_factor: f32) -> Self {
        self.scale_factor = scale_factor;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(w, h),
        }
    }

    pub fn max_x(&self) -> f32 {
        self.origin.x + self.size.width
    }

    pub fn max_y(&self) -> f32 {
        self.origin.y + self.size.height
    }
}

/// One positioned glyph in a glyph run. Position is relative to the run's
/// `origin`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PositionedGlyph {
    /// Glyph index in the font (parley uses `u32`; downstream renderers
    /// typically downcast as needed).
    pub glyph_id: u32,
    /// X offset from the run's origin.
    pub x: f32,
    /// Y offset from the run's origin (typically 0; non-zero for rare
    /// scripts).
    pub y: f32,
    pub advance: f32,
}

/// Identifies the concrete font face a [`GlyphRun`] was *shaped against*,
/// as parley actually chose it (after any fallback). Index into the
/// out-of-band [`crate::FontTable`] sidecar that rides alongside the
/// packet — the bytes live there, not on the (serializable) packet.
///
/// This is the face the glyph **ids** index into. Shipping any other
/// face (e.g. one re-resolved from `font_family`/`weight`/`style`, which
/// are the *requested* attributes, not necessarily the chosen face) makes
/// the renderer index the wrong outlines on fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FontFaceId(pub u32);

/// A run of glyphs that share a font + style.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GlyphRun {
    /// Origin of the run, relative to the packet's content origin.
    pub origin: Point,
    pub font_size: f32,
    /// The face the glyph ids in `glyphs` were shaped against (parley's
    /// actual choice). Resolves to bytes via the [`crate::FontTable`]
    /// sidecar.
    pub font_face: FontFaceId,
    /// Requested family label, kept for a11y / debug. May differ from the
    /// face actually shaped against (`font_face`) on fallback — do **not**
    /// use it to resolve render bytes.
    pub font_family: String,
    /// CSS-style weight: 100..900. Requested, not necessarily the chosen
    /// face's own weight. For a11y / debug.
    pub font_weight: u16,
    /// Requested style. For a11y / debug (see `font_weight`).
    pub font_style: TextStyle,
    pub glyphs: Vec<PositionedGlyph>,
    /// Y of the baseline relative to `origin.y`.
    pub baseline_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextStyle {
    Normal,
    Italic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderedBlock {
    /// Index into [`inker::EngineDocument::blocks`] this rendered block
    /// came from. Lets consumers correlate rendered geometry with source
    /// content (for selection, citation, debug overlays, etc.).
    pub source_block_index: usize,
    /// Total bounds of the rendered block, in packet-local coordinates.
    pub bounds: Rect,
    pub kind: RenderedBlockKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RenderedBlockKind {
    /// Block whose visual content is laid-out text (heading, paragraph,
    /// code block, list-item content, metadata row, etc.).
    Text { glyph_runs: Vec<GlyphRun> },
    /// Image — v1 reserves space; downstream renderer fetches + paints
    /// the bytes.
    Image { url: String, alt: String },
    /// Horizontal rule. The bounds is the rule's strip; renderer paints a
    /// hairline at the vertical center.
    Rule,
    /// Container for nested blocks (Quote, List, FeedHeader/Entry, etc.).
    /// Children's bounds are in packet-local coordinates.
    Group { children: Vec<RenderedBlock> },
}

/// Hit-testable region the host translates into a navigation / interaction
/// event when the user clicks / hovers / focuses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InteractionRegion {
    pub bounds: Rect,
    pub kind: InteractionKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InteractionKind {
    /// Inline link — clicking navigates the URL.
    Link { url: String },
}

/// The output of [`crate::layout_document`]. A pure-data record describing
/// what to render where; no GPU resources, no host types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentRenderPacket {
    pub viewport: Viewport,
    /// Total content extent. May exceed `viewport.height` (scrolling) but
    /// width matches the viewport's available width.
    pub content_bounds: Rect,
    pub blocks: Vec<RenderedBlock>,
    pub interactions: Vec<InteractionRegion>,
}
