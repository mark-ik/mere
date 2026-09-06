// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A compact, read-only strip for cyclic normalized positions.
//!
//! The strip paints geometry only. Callers keep names, values, and any
//! interaction in DOM siblings or their wrapping view.

use paint_list_api::ColorF;

use crate::path::Path;
use crate::{Leaf, PaintCx, Size, SizeHint};

/// One colored position on an [`AngleStrip`]'s cyclic `0..1` track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AngleStripMark {
    /// Normalized cyclic position. [`AngleStrip::new`] and
    /// [`AngleStrip::set_marks`] canonicalize this with modulo `1.0`.
    pub position: f32,
    pub color: ColorF,
}

impl AngleStripMark {
    /// Construct a mark with its position normalized into `0.0..1.0`.
    pub fn new(position: f32, color: ColorF) -> Self {
        Self {
            position: normalize_position(position),
            color,
        }
    }

    /// Construct an opaque RGB mark without exposing `ColorF` to a consumer.
    pub fn rgb(position: f32, r: f32, g: f32, b: f32) -> Self {
        Self::new(position, ColorF { r, g, b, a: 1.0 })
    }
}

/// Read-only horizontal track for a group of cyclic normalized positions.
///
/// Multiple marks may share a position and paint in input order. The leaf is
/// deliberately inert: its wrapping DOM view owns semantics beyond the
/// graphics-object fallback, labels, values, and any interaction.
pub struct AngleStrip {
    marks: Vec<AngleStripMark>,
    /// Color of the horizontal track behind the marks.
    pub track_color: ColorF,
    /// Track thickness in device px.
    pub track_height: f32,
    /// Radius of each mark in device px.
    pub marker_radius: f32,
    intrinsic: Size,
    dirty: bool,
}

impl AngleStrip {
    pub fn new(marks: Vec<AngleStripMark>, intrinsic: Size) -> Self {
        Self {
            marks: normalize_marks(marks),
            track_color: ColorF {
                r: 0.20,
                g: 0.20,
                b: 0.24,
                a: 1.0,
            },
            track_height: 2.0,
            marker_radius: 3.0,
            intrinsic,
            dirty: true,
        }
    }

    /// Construct a strip from its marks and intrinsic device-pixel dimensions.
    pub fn with_size(marks: Vec<AngleStripMark>, width: f32, height: f32) -> Self {
        Self::new(marks, Size { width, height })
    }

    /// Replace all marks, canonicalizing every position into `0.0..1.0`.
    /// Paint is dirtied only when the normalized mark set changes.
    pub fn set_marks(&mut self, marks: Vec<AngleStripMark>) {
        let marks = normalize_marks(marks);
        if marks != self.marks {
            self.marks = marks;
            self.dirty = true;
        }
    }

    pub fn marks(&self) -> &[AngleStripMark] {
        &self.marks
    }
}

impl Leaf for AngleStrip {
    fn accessibility(&mut self, node: &mut accesskit::Node) {
        node.set_role(accesskit::Role::GraphicsObject);
    }

    fn measure(&mut self, _known: SizeHint, _available: SizeHint) -> Size {
        self.intrinsic
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let size = cx.size();
        let width = size.width.max(0.0);
        let height = size.height.max(0.0);
        let track_height = self.track_height.clamp(0.0, height);
        let center_y = height * 0.5;

        // Edge positions and large marker styling are both allowed, so the
        // leaf must establish its own clip before painting either primitive.
        cx.push_clip_rect(0.0, 0.0, width, height);
        if track_height > 0.0 {
            cx.fill_rect(
                0.0,
                center_y - track_height * 0.5,
                width,
                track_height,
                self.track_color,
            );
        }
        let marker_radius = self.marker_radius.max(0.0);
        let horizontal_inset = marker_radius.min(width * 0.5);
        let marker_width = width - 2.0 * horizontal_inset;
        for mark in &self.marks {
            cx.fill_path(
                Path::circle(
                    horizontal_inset + marker_width * mark.position,
                    center_y,
                    marker_radius,
                ),
                mark.color,
            );
        }
        cx.pop_clip();
        self.dirty = false;
    }

    fn paint_dirty(&self) -> bool {
        self.dirty
    }
}

fn normalize_marks(marks: Vec<AngleStripMark>) -> Vec<AngleStripMark> {
    marks
        .into_iter()
        .map(|mark| AngleStripMark::new(mark.position, mark.color))
        .collect()
}

fn normalize_position(position: f32) -> f32 {
    if position.is_finite() {
        position.rem_euclid(1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LeafRegistry, RenderedLeaves};
    use paint_list_api::PaintCmd;
    use paint_list_api::items::PathCommand;

    fn color(r: f32, g: f32, b: f32) -> ColorF {
        ColorF { r, g, b, a: 1.0 }
    }

    fn paint_of(strip: &mut AngleStrip, width: f32, height: f32) -> Vec<PaintCmd> {
        let mut commands = Vec::new();
        let mut cx = PaintCx::new(&mut commands, Size { width, height });
        strip.paint(&mut cx);
        commands
    }

    #[test]
    fn marks_normalize_modulo_one_and_non_finite_positions_to_zero() {
        let strip = AngleStrip::new(
            vec![
                AngleStripMark::new(-0.25, color(1.0, 0.0, 0.0)),
                AngleStripMark {
                    position: 1.25,
                    color: color(0.0, 1.0, 0.0),
                },
                AngleStripMark::new(f32::NAN, color(0.0, 0.0, 1.0)),
                AngleStripMark::new(f32::INFINITY, color(1.0, 1.0, 0.0)),
            ],
            Size {
                width: 96.0,
                height: 18.0,
            },
        );

        assert_eq!(
            strip
                .marks()
                .iter()
                .map(|mark| mark.position)
                .collect::<Vec<_>>(),
            vec![0.75, 0.25, 0.0, 0.0]
        );
    }

    #[test]
    fn paints_track_and_all_marks_inside_its_own_clip() {
        let mut strip = AngleStrip::new(
            vec![
                AngleStripMark::new(0.0, color(1.0, 0.0, 0.0)),
                AngleStripMark::new(0.5, color(0.0, 1.0, 0.0)),
                AngleStripMark::new(0.75, color(0.0, 0.0, 1.0)),
            ],
            Size {
                width: 96.0,
                height: 18.0,
            },
        );
        let commands = paint_of(&mut strip, 96.0, 18.0);

        assert!(matches!(commands.first(), Some(PaintCmd::PushClip(_))));
        assert!(matches!(commands.get(1), Some(PaintCmd::DrawRect(_))));
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, PaintCmd::DrawPath(_)))
                .count(),
            3
        );
        assert!(matches!(commands.last(), Some(PaintCmd::PopClip)));
        assert!(!strip.paint_dirty());
    }

    #[test]
    fn edge_marks_inset_their_centers_by_the_marker_radius() {
        let mut strip = AngleStrip::with_size(
            vec![
                AngleStripMark::rgb(0.0, 1.0, 0.0, 0.0),
                AngleStripMark::rgb(1.0, 0.0, 1.0, 0.0),
            ],
            96.0,
            18.0,
        );
        strip.marker_radius = 3.0;
        let commands = paint_of(&mut strip, 96.0, 18.0);
        let markers = commands
            .iter()
            .filter_map(|command| match command {
                PaintCmd::DrawPath(path) => match path.path.commands.first() {
                    Some(PathCommand::MoveTo(point)) => Some(point.x),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(markers, vec![6.0, 6.0]);
    }

    #[test]
    fn retained_strip_repaints_only_when_its_layout_size_changes() {
        let mut registry = LeafRegistry::new();
        registry.insert(
            7_u64,
            Box::new(AngleStrip::with_size(
                vec![AngleStripMark::rgb(0.5, 1.0, 1.0, 1.0)],
                96.0,
                18.0,
            )),
        );
        let mut rendered = RenderedLeaves::new();

        assert_eq!(
            registry.render_into(
                |_| {
                    Some(Size {
                        width: 96.0,
                        height: 18.0,
                    })
                },
                &mut rendered,
            ),
            1
        );
        assert_eq!(
            registry.render_into(
                |_| {
                    Some(Size {
                        width: 96.0,
                        height: 18.0,
                    })
                },
                &mut rendered,
            ),
            0
        );
        assert_eq!(
            registry.render_into(
                |_| {
                    Some(Size {
                        width: 144.0,
                        height: 18.0,
                    })
                },
                &mut rendered,
            ),
            1
        );
    }

    #[test]
    fn normalized_mark_updates_are_the_dirty_gate() {
        let mut strip = AngleStrip::new(
            vec![AngleStripMark::new(0.25, color(1.0, 0.0, 0.0))],
            Size {
                width: 96.0,
                height: 18.0,
            },
        );
        paint_of(&mut strip, 96.0, 18.0);
        strip.set_marks(vec![AngleStripMark::new(1.25, color(1.0, 0.0, 0.0))]);
        assert!(
            !strip.paint_dirty(),
            "equivalent normalized marks stay clean"
        );
        strip.set_marks(vec![AngleStripMark::new(0.5, color(1.0, 0.0, 0.0))]);
        assert!(strip.paint_dirty(), "changed marks dirty the leaf");
    }
}
