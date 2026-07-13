/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The host **text-into-scene** path: shape a short line of text and push it
//! into a host-drawn [`netrender::Scene`] (the same scene the host draws rects
//! and strokes into for the gloss minimap, window controls, and the session
//! switcher). parley owns shaping; `netrender_text` lowers the laid-out
//! [`parley::Layout`] into `SceneGlyphRun`s. The contexts are held once (font
//! enumeration is one-time) rather than rebuilt per frame.
//!
//! This is deliberately single-line: the host-drawn surfaces label small chips
//! (switcher tiles), not paragraphs — full document text rides the genet /
//! document-canvas panes instead. (Multi-graph MG4 host text path.)

use netrender::Scene;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, GenericFamily, LayoutContext,
    StyleProperty,
};

/// Owns the parley shaping contexts for host-drawn text. One per host.
pub struct HostText {
    font_cx: FontContext,
    layout_cx: LayoutContext<[f32; 4]>,
}

impl HostText {
    pub fn new() -> Self {
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
        }
    }

    /// Shape one line of `text` at `size_px` in the system UI font, brushed with
    /// premultiplied `color`. No wrapping (a single visual line); the caller clips
    /// by the surface it pushes into.
    fn line(&mut self, text: &str, size_px: f32, color: [f32; 4]) -> parley::Layout<[f32; 4]> {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(FontFamily::from(GenericFamily::SystemUi));
        builder.push_default(StyleProperty::FontSize(size_px));
        builder.push_default(StyleProperty::Brush(color));
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }

    /// The `(width, height)` one line of `text` occupies at `size_px` (device px).
    /// Used to center / truncate a label within its chip before pushing it.
    pub fn measure(&mut self, text: &str, size_px: f32) -> (f32, f32) {
        let layout = self.line(text, size_px, [0.0; 4]);
        (layout.width(), layout.height())
    }

    /// Shape `text` and push its glyphs into `scene`, positioning the line's
    /// top-left at `origin` (device px, the scene's own frame).
    pub fn push_line(
        &mut self,
        scene: &mut Scene,
        text: &str,
        size_px: f32,
        color: [f32; 4],
        origin: [f32; 2],
    ) {
        let layout = self.line(text, size_px, color);
        netrender_text::push_layout(scene, &layout, origin);
    }
}

impl Default for HostText {
    fn default() -> Self {
        Self::new()
    }
}
