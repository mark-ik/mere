// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Renderer-facing input vocabulary.
//!
//! Substrate-side input router translates OS events into [`InputEvent`]
//! values; the registry's dispatch hands them to the resolved renderer's
//! `input(...)` method; renderers return [`InputDisposition`] to indicate
//! consumption.

use kurbo::{Point, Vec2};

/// An input event scoped to a single tile.
#[derive(Clone, Debug)]
pub enum InputEvent {
    /// Pointer (mouse, pen, touch) event.
    Pointer {
        /// Position in *tile-local logical coordinates*.
        position: Point,
        kind: PointerEventKind,
        modifiers: ModifiersState,
    },
    /// Keyboard event.
    Key {
        kind: KeyEventKind,
        code: KeyCode,
        modifiers: ModifiersState,
    },
    /// Committed text (post-IME).
    Text(String),
    /// IME composition lifecycle.
    Ime(ImeEvent),
    /// Tile gained or lost focus.
    Focus { focused: bool },
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PointerEventKind {
    Move,
    Down(PointerButton),
    Up(PointerButton),
    Wheel(Vec2),
    Enter,
    Leave,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Auxiliary(u8),
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct ModifiersState {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    /// Cmd on macOS, Win key on Windows, Super on Linux.
    pub meta: bool,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum KeyEventKind {
    Down,
    Up,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum KeyCode {
    Character(char),
    Named(NamedKey),
    Scancode(u32),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum NamedKey {
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    Insert,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

#[derive(Clone, Debug)]
pub enum ImeEvent {
    Start,
    Composing {
        text: String,
        cursor: std::ops::Range<usize>,
    },
    Commit(String),
    End,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum InputDisposition {
    Consumed,
    Passthrough,
}
