// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A read-only line for showing a measured normalized interval.
//!
//! The caller supplies all geometry. This leaf does not assign domain meaning
//! to its endpoints, traversal, or optional exact-target tick.

use paint_list_api::ColorF;

use crate::path::Path;
use crate::{Leaf, PaintCx, Size, SizeHint, round_stroke};

/// The caller-selected path between a [`DimensionLine`]'s ordered endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimensionLineTraversal {
    /// One span directly connecting the endpoints.
    Direct,
    /// Two spans from the start to the track end and from the track start to
    /// the endpoint.
    Wrapped,
}

/// Read-only rendering of a normalized measurement on a horizontal track.
pub struct DimensionLine {
    start: f32,
    end: f32,
    traversal: DimensionLineTraversal,
    target: Option<f32>,
    /// Full-track color behind the measured span.
    pub track_color: ColorF,
    /// Color of the measured direct or wrapped span.
    pub span_color: ColorF,
    /// Filled marker color for the ordered start endpoint.
    pub start_color: ColorF,
    /// Ring marker color for the ordered end endpoint.
    pub end_color: ColorF,
    /// Optional exact-target tick color.
    pub target_color: ColorF,
    /// Track and span thickness in device px.
    pub track_height: f32,
    /// Endpoint marker radius in device px.
    pub endpoint_radius: f32,
    /// Exact-target tick thickness in device px.
    pub target_tick_width: f32,
    intrinsic: Size,
    dirty: bool,
}

impl DimensionLine {
    pub fn new(
        start: f32,
        end: f32,
        traversal: DimensionLineTraversal,
        target: Option<f32>,
        intrinsic: Size,
    ) -> Self {
        Self {
            start: clamp_normalized(start),
            end: clamp_normalized(end),
            traversal,
            target: target.map(clamp_normalized),
            track_color: ColorF {
                r: 0.20,
                g: 0.20,
                b: 0.24,
                a: 1.0,
            },
            span_color: ColorF {
                r: 0.32,
                g: 0.58,
                b: 0.90,
                a: 1.0,
            },
            start_color: ColorF {
                r: 0.25,
                g: 0.68,
                b: 0.48,
                a: 1.0,
            },
            end_color: ColorF {
                r: 0.92,
                g: 0.52,
                b: 0.28,
                a: 1.0,
            },
            target_color: ColorF {
                r: 0.92,
                g: 0.82,
                b: 0.30,
                a: 1.0,
            },
            track_height: 2.0,
            endpoint_radius: 3.0,
            target_tick_width: 1.5,
            intrinsic,
            dirty: true,
        }
    }

    /// Construct a line without requiring callers to name [`Size`].
    pub fn with_size(
        start: f32,
        end: f32,
        traversal: DimensionLineTraversal,
        target: Option<f32>,
        width: f32,
        height: f32,
    ) -> Self {
        Self::new(start, end, traversal, target, Size { width, height })
    }

    /// Replace the caller-owned measurement. Every normalized coordinate is
    /// clamped to `0.0..=1.0`; non-finite input deterministically becomes
    /// `0.0`. Paint is dirtied only when that normalized measurement changes.
    pub fn set_measurement(
        &mut self,
        start: f32,
        end: f32,
        traversal: DimensionLineTraversal,
        target: Option<f32>,
    ) {
        let next = (
            clamp_normalized(start),
            clamp_normalized(end),
            traversal,
            target.map(clamp_normalized),
        );
        if self.measurement() != next {
            (self.start, self.end, self.traversal, self.target) = next;
            self.dirty = true;
        }
    }

    pub fn measurement(&self) -> (f32, f32, DimensionLineTraversal, Option<f32>) {
        (self.start, self.end, self.traversal, self.target)
    }
}

impl Leaf for DimensionLine {
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
        let endpoint_radius = self.endpoint_radius.max(0.0);
        let end_stroke_width = (endpoint_radius * 0.5).max(1.0);
        let horizontal_inset = (endpoint_radius + end_stroke_width * 0.5).min(width * 0.5);
        let marker_width = width - 2.0 * horizontal_inset;
        let start = horizontal_inset + self.start * marker_width;
        let end = horizontal_inset + self.end * marker_width;

        cx.push_clip_rect(0.0, 0.0, width, height);
        if track_height > 0.0 {
            let track_y = center_y - track_height * 0.5;
            cx.fill_rect(0.0, track_y, width, track_height, self.track_color);
            match self.traversal {
                DimensionLineTraversal::Direct => {
                    let left = start.min(end);
                    let span_width = (end - start).abs();
                    if span_width > 0.0 {
                        cx.fill_rect(left, track_y, span_width, track_height, self.span_color);
                    }
                },
                DimensionLineTraversal::Wrapped => {
                    if width > start {
                        cx.fill_rect(start, track_y, width - start, track_height, self.span_color);
                    }
                    if end > 0.0 {
                        cx.fill_rect(0.0, track_y, end, track_height, self.span_color);
                    }
                },
            }
        }

        // A filled start plus an outlined end keeps ordered endpoints legible
        // even when their normalized positions coincide.
        cx.fill_path(
            Path::circle(start, center_y, endpoint_radius),
            self.start_color,
        );
        cx.stroke_path(
            Path::circle(end, center_y, endpoint_radius),
            round_stroke(self.end_color, end_stroke_width),
        );
        if let Some(target) = self.target {
            let tick_width = self.target_tick_width.max(0.0).min(width);
            if tick_width > 0.0 {
                cx.fill_rect(
                    (horizontal_inset + target * marker_width - tick_width * 0.5)
                        .clamp(0.0, width - tick_width),
                    0.0,
                    tick_width,
                    height,
                    self.target_color,
                );
            }
        }
        cx.pop_clip();
        self.dirty = false;
    }

    fn paint_dirty(&self) -> bool {
        self.dirty
    }
}

fn clamp_normalized(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LeafRegistry, RenderedLeaves};
    use paint_list_api::PaintCmd;

    fn paint_of(line: &mut DimensionLine, width: f32, height: f32) -> Vec<PaintCmd> {
        let mut commands = Vec::new();
        let mut cx = PaintCx::new(&mut commands, Size { width, height });
        line.paint(&mut cx);
        commands
    }

    fn count_rects(commands: &[PaintCmd]) -> usize {
        commands
            .iter()
            .filter(|command| matches!(command, PaintCmd::DrawRect(_)))
            .count()
    }

    fn count_paths(commands: &[PaintCmd]) -> usize {
        commands
            .iter()
            .filter(|command| matches!(command, PaintCmd::DrawPath(_)))
            .count()
    }

    #[test]
    fn direct_line_paints_one_measured_span_and_target_tick() {
        let mut line = DimensionLine::with_size(
            0.2,
            0.6,
            DimensionLineTraversal::Direct,
            Some(0.4),
            100.0,
            20.0,
        );
        let commands = paint_of(&mut line, 100.0, 20.0);

        assert!(matches!(commands.first(), Some(PaintCmd::PushClip(_))));
        assert_eq!(count_rects(&commands), 3, "track + direct span + tick");
        assert_eq!(count_paths(&commands), 2, "ordered endpoint markers");
        assert!(matches!(commands.last(), Some(PaintCmd::PopClip)));
    }

    #[test]
    fn wrapped_line_paints_two_measured_segments() {
        let mut line =
            DimensionLine::with_size(0.8, 0.2, DimensionLineTraversal::Wrapped, None, 100.0, 20.0);
        let commands = paint_of(&mut line, 100.0, 20.0);

        assert_eq!(count_rects(&commands), 3, "track + two wrapped spans");
        assert_eq!(count_paths(&commands), 2);
    }

    #[test]
    fn coincident_direct_endpoints_remain_two_distinct_marker_shapes() {
        let mut line =
            DimensionLine::with_size(0.5, 0.5, DimensionLineTraversal::Direct, None, 100.0, 20.0);
        let commands = paint_of(&mut line, 100.0, 20.0);

        assert_eq!(count_rects(&commands), 1, "track only for a zero span");
        let (fills, strokes) =
            commands
                .iter()
                .fold((0, 0), |(fills, strokes), command| match command {
                    PaintCmd::DrawPath(path) if path.fill.is_some() => (fills + 1, strokes),
                    PaintCmd::DrawPath(path) if path.stroke.is_some() => (fills, strokes + 1),
                    _ => (fills, strokes),
                });
        assert_eq!((fills, strokes), (1, 1));
    }

    #[test]
    fn non_finite_and_out_of_range_measurements_clamp_deterministically() {
        let mut line = DimensionLine::with_size(
            f32::NAN,
            4.0,
            DimensionLineTraversal::Direct,
            Some(f32::NEG_INFINITY),
            100.0,
            20.0,
        );
        assert_eq!(
            line.measurement(),
            (0.0, 1.0, DimensionLineTraversal::Direct, Some(0.0))
        );
        paint_of(&mut line, 100.0, 20.0);
        line.set_measurement(
            f32::NEG_INFINITY,
            2.0,
            DimensionLineTraversal::Direct,
            Some(f32::NAN),
        );
        assert!(!line.paint_dirty(), "equivalent fallback remains clean");
    }

    #[test]
    fn changed_measurement_dirties_and_resize_renders_again() {
        let mut line =
            DimensionLine::with_size(0.25, 0.5, DimensionLineTraversal::Direct, None, 100.0, 20.0);
        paint_of(&mut line, 100.0, 20.0);
        line.set_measurement(0.25, 0.5, DimensionLineTraversal::Direct, None);
        assert!(!line.paint_dirty());
        line.set_measurement(0.25, 0.75, DimensionLineTraversal::Direct, None);
        assert!(line.paint_dirty());

        let mut registry = LeafRegistry::new();
        registry.insert(1, Box::new(line));
        let mut rendered = RenderedLeaves::new();
        assert_eq!(
            registry.render_into(
                |_| {
                    Some(Size {
                        width: 100.0,
                        height: 20.0,
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
                        width: 200.0,
                        height: 20.0,
                    })
                },
                &mut rendered,
            ),
            1,
            "a changed layout box repaints a retained leaf"
        );
    }
}
