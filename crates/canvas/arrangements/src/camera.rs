// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The viewport a layout pass sizes itself against.
//!
//! Trimmed descendant of the retired `canvas-ir` `CanvasViewport`: the pane
//! rectangle + display scale. Layouts that fit their output to the pane
//! (grid, radial) read `rect`; analytic tilings ignore it.

use euclid::default::{Point2D, Rect, Size2D};
use serde::{Deserialize, Serialize};

/// Viewport rectangle and display scale factor for a canvas pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasViewport {
    /// The pane rectangle in logical (host-framework) coordinates.
    pub rect: Rect<f32>,
    /// Display scale factor (e.g. 2.0 for Retina / HiDPI).
    pub scale_factor: f32,
}

impl CanvasViewport {
    pub fn new(origin: Point2D<f32>, size: Size2D<f32>, scale_factor: f32) -> Self {
        Self {
            rect: Rect::new(origin, size),
            scale_factor,
        }
    }

    pub fn size(&self) -> Size2D<f32> {
        self.rect.size
    }

    pub fn center(&self) -> Point2D<f32> {
        self.rect.center()
    }
}

impl Default for CanvasViewport {
    fn default() -> Self {
        Self {
            rect: Rect::new(Point2D::origin(), Size2D::new(800.0, 600.0)),
            scale_factor: 1.0,
        }
    }
}
