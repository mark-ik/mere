/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Framework-agnostic canvas input events.
//!
//! The host translates its framework-specific events (egui, iced, winit)
//! into these portable types before feeding them to the interaction engine.

use euclid::default::Point2D;
use serde::{Deserialize, Serialize};

/// A pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

/// Modifier keys held during an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// A canvas input event. The host converts from its own event model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CanvasInputEvent {
    /// Pointer moved to a screen-space position.
    PointerMoved { position: Point2D<f32> },
    /// Pointer button pressed.
    PointerPressed {
        position: Point2D<f32>,
        button: PointerButton,
        modifiers: Modifiers,
    },
    /// Pointer button released.
    PointerReleased {
        position: Point2D<f32>,
        button: PointerButton,
        modifiers: Modifiers,
    },
    /// Pointer double-clicked.
    PointerDoubleClick {
        position: Point2D<f32>,
        button: PointerButton,
        modifiers: Modifiers,
    },
    /// Scroll/wheel input for zooming.
    Scroll {
        delta: f32,
        position: Point2D<f32>,
        modifiers: Modifiers,
    },
    /// Pointer left the canvas area.
    PointerLeft,
}
