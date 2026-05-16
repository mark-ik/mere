// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Translation from [`mere_renderer_registry::InputEvent`] to masonry's
//! [`TextEvent`] / [`PointerEvent`] vocabulary.
//!
//! masonry consumes [`ui-events`](https://crates.io/crates/ui-events). This
//! module wraps registry-shaped events into ui-events shapes that masonry's
//! `RenderRoot::handle_pointer_event` / `handle_text_event` accept.
//!
//! ## Mapping table
//!
//! | Registry `InputEvent`                  | masonry call                                |
//! | -------------------------------------- | ------------------------------------------- |
//! | `Pointer { Move, position, mods }`     | `PointerEvent::Move(PointerUpdate { ... })` |
//! | `Pointer { Down(button), pos, mods }`  | `PointerEvent::Down(PointerButtonEvent)`    |
//! | `Pointer { Up(button), pos, mods }`    | `PointerEvent::Up(PointerButtonEvent)`      |
//! | `Pointer { Wheel(delta), pos, mods }`  | `PointerEvent::Scroll(PointerScrollEvent)`  |
//! | `Pointer { Enter, pos, mods }`         | `PointerEvent::Enter(PointerInfo)`          |
//! | `Pointer { Leave, pos, mods }`         | `PointerEvent::Leave(PointerInfo)`          |
//! | `Key { Down, code, mods }`             | `TextEvent::Keyboard(KeyboardEvent)`        |
//! | `Key { Up, code, mods }`               | `TextEvent::Keyboard(KeyboardEvent)`        |
//! | `Text(s)`                              | `TextEvent::Ime(Ime::Commit(s))`            |
//! | `Ime(Start)`                           | `TextEvent::Ime(Ime::Enabled)`              |
//! | `Ime(Composing { text, cursor })`      | `TextEvent::Ime(Ime::Preedit(...))`         |
//! | `Ime(Commit(s))`                       | `TextEvent::Ime(Ime::Commit(s))`            |
//! | `Ime(End)`                             | `TextEvent::Ime(Ime::Disabled)`             |
//! | `Focus { focused }`                    | `TextEvent::WindowFocusChange(focused)`     |
//!
//! ## Field-shape notes
//!
//! `ui-events 0.3`'s `PointerState` carries platform-specific metadata
//! (`time`, `count`, `pressure`, `tangential_pressure`, `orientation`,
//! `contact_geometry`, `scale_factor`, etc.) the substrate's input router
//! doesn't have. v0 fills these with sensible defaults; masonry widgets
//! consuming a v0 substrate event won't see meaningful click counts /
//! pressure / pen orientation data. Wire those upstream when the substrate's
//! input router gains the corresponding fields.
//!
//! `KeyboardEvent::code` (the physical key position) is hard to recover from
//! a `KeyCode::Character(c)` — the substrate's input router will normally
//! produce both code and character together. v0 returns
//! `keyboard_types::Code::Unidentified` for `KeyCode::Character`; pin
//! `Named` arms to specific `Code` values where the named-key→physical-key
//! mapping is unambiguous.

use kurbo::Vec2;
use masonry_core::core::{Ime, PointerEvent, TextEvent};
use mere_renderer_registry::{
    ImeEvent, InputEvent, KeyCode, KeyEventKind, ModifiersState, NamedKey, PointerButton as RPB,
    PointerEventKind,
};
use ui_events::keyboard::{
    Code, Key, KeyState, KeyboardEvent, Location, Modifiers, NamedKey as UiNamedKey,
};
use ui_events::pointer::{
    PointerButton as UiPointerButton, PointerButtonEvent, PointerButtons, PointerId, PointerInfo,
    PointerOrientation, PointerScrollEvent, PointerState, PointerType, PointerUpdate,
};
use ui_events::ScrollDelta;

/// Translate a registry [`InputEvent`] into a masonry [`TextEvent`], if
/// applicable. Returns `None` for `Pointer` events.
pub fn translate_to_masonry_text(event: &InputEvent) -> Option<TextEvent> {
    match event {
        InputEvent::Pointer { .. } => None,

        InputEvent::Text(s) => Some(TextEvent::Ime(Ime::Commit(s.clone()))),

        InputEvent::Ime(ImeEvent::Start) => Some(TextEvent::Ime(Ime::Enabled)),

        InputEvent::Ime(ImeEvent::Composing { text, cursor }) => Some(TextEvent::Ime(
            Ime::Preedit(text.clone(), Some((cursor.start, cursor.end))),
        )),

        InputEvent::Ime(ImeEvent::Commit(s)) => Some(TextEvent::Ime(Ime::Commit(s.clone()))),

        InputEvent::Ime(ImeEvent::End) => Some(TextEvent::Ime(Ime::Disabled)),

        InputEvent::Focus { focused } => Some(TextEvent::WindowFocusChange(*focused)),

        InputEvent::Key { kind, code, modifiers } => {
            let (key, ui_code) = translate_key_code(code);
            Some(TextEvent::Keyboard(KeyboardEvent {
                state: match kind {
                    KeyEventKind::Down => KeyState::Down,
                    KeyEventKind::Up => KeyState::Up,
                },
                key,
                code: ui_code,
                location: Location::Standard,
                modifiers: translate_modifiers(*modifiers),
                is_composing: false,
                repeat: false,
            }))
        }
    }
}

/// Translate a registry [`InputEvent`] into a masonry [`PointerEvent`], if
/// applicable. Returns `None` for non-pointer events.
pub fn translate_to_masonry_pointer(event: &InputEvent) -> Option<PointerEvent> {
    let InputEvent::Pointer { position, kind, modifiers } = event else {
        return None;
    };

    let pointer = primary_pointer_info();
    let state = pointer_state(*position, *modifiers);

    Some(match kind {
        PointerEventKind::Move => PointerEvent::Move(PointerUpdate {
            pointer,
            current: state,
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }),
        PointerEventKind::Down(button) => PointerEvent::Down(PointerButtonEvent {
            button: Some(translate_pointer_button(*button)),
            pointer,
            state,
        }),
        PointerEventKind::Up(button) => PointerEvent::Up(PointerButtonEvent {
            button: Some(translate_pointer_button(*button)),
            pointer,
            state,
        }),
        PointerEventKind::Wheel(delta) => PointerEvent::Scroll(PointerScrollEvent {
            pointer,
            delta: scroll_delta_from_lines(*delta),
            state,
        }),
        PointerEventKind::Enter => PointerEvent::Enter(pointer),
        PointerEventKind::Leave => PointerEvent::Leave(pointer),
    })
}

// --- helpers ---------------------------------------------------------------

fn primary_pointer_info() -> PointerInfo {
    PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Mouse,
    }
}

/// Build a v0 `PointerState`. Position is preserved; modifiers are mapped;
/// every other field uses platform-neutral defaults.
fn pointer_state(position: kurbo::Point, modifiers: ModifiersState) -> PointerState {
    PointerState {
        position: dpi::PhysicalPosition::new(position.x, position.y),
        modifiers: translate_modifiers(modifiers),
        ..PointerState::default()
    }
}

fn translate_pointer_button(b: RPB) -> UiPointerButton {
    match b {
        RPB::Primary => UiPointerButton::Primary,
        RPB::Secondary => UiPointerButton::Secondary,
        RPB::Middle => UiPointerButton::Auxiliary,
        // ui-events numbers extra buttons B7..B32 (PointerButton::Auxiliary
        // is the middle button, then X1/X2 are mouse-back/forward, and B7+
        // are platform-numbered extras starting from 6 in winit's MouseButton::Other).
        RPB::Auxiliary(0) | RPB::Auxiliary(1) => UiPointerButton::X1,
        RPB::Auxiliary(2) => UiPointerButton::X2,
        RPB::Auxiliary(_) => UiPointerButton::B7,
    }
}

fn translate_modifiers(m: ModifiersState) -> Modifiers {
    let mut out = Modifiers::default();
    if m.shift {
        out.insert(Modifiers::SHIFT);
    }
    if m.control {
        out.insert(Modifiers::CONTROL);
    }
    if m.alt {
        out.insert(Modifiers::ALT);
    }
    if m.meta {
        out.insert(Modifiers::META);
    }
    out
}

/// Substrate carries scroll wheel deltas in *lines*; ui-events expects the
/// same for `LineDelta`.
fn scroll_delta_from_lines(delta: Vec2) -> ScrollDelta {
    ScrollDelta::LineDelta(delta.x as f32, delta.y as f32)
}

/// Translate a registry [`KeyCode`] into a `(Key, Code)` pair for
/// `KeyboardEvent`. v0 covers `Named` keys exhaustively (one arm per
/// registry `NamedKey` variant); `Character` produces a meaningful `Key`
/// but `Code::Unidentified` (the physical key position is platform-
/// specific and not recoverable from the character alone). `Scancode`
/// falls back to `Unidentified` for both.
fn translate_key_code(code: &KeyCode) -> (Key, Code) {
    match code {
        KeyCode::Character(c) => (Key::Character(c.to_string()), Code::Unidentified),
        KeyCode::Named(named) => translate_named_key(*named),
        KeyCode::Scancode(_) => (Key::Named(UiNamedKey::Unidentified), Code::Unidentified),
    }
}

fn translate_named_key(named: NamedKey) -> (Key, Code) {
    match named {
        NamedKey::Enter => (Key::Named(UiNamedKey::Enter), Code::Enter),
        NamedKey::Tab => (Key::Named(UiNamedKey::Tab), Code::Tab),
        NamedKey::Escape => (Key::Named(UiNamedKey::Escape), Code::Escape),
        NamedKey::Backspace => (Key::Named(UiNamedKey::Backspace), Code::Backspace),
        NamedKey::Delete => (Key::Named(UiNamedKey::Delete), Code::Delete),
        NamedKey::Insert => (Key::Named(UiNamedKey::Insert), Code::Insert),
        NamedKey::Space => (Key::Character(" ".to_string()), Code::Space),
        NamedKey::ArrowUp => (Key::Named(UiNamedKey::ArrowUp), Code::ArrowUp),
        NamedKey::ArrowDown => (Key::Named(UiNamedKey::ArrowDown), Code::ArrowDown),
        NamedKey::ArrowLeft => (Key::Named(UiNamedKey::ArrowLeft), Code::ArrowLeft),
        NamedKey::ArrowRight => (Key::Named(UiNamedKey::ArrowRight), Code::ArrowRight),
        NamedKey::Home => (Key::Named(UiNamedKey::Home), Code::Home),
        NamedKey::End => (Key::Named(UiNamedKey::End), Code::End),
        NamedKey::PageUp => (Key::Named(UiNamedKey::PageUp), Code::PageUp),
        NamedKey::PageDown => (Key::Named(UiNamedKey::PageDown), Code::PageDown),
        NamedKey::F1 => (Key::Named(UiNamedKey::F1), Code::F1),
        NamedKey::F2 => (Key::Named(UiNamedKey::F2), Code::F2),
        NamedKey::F3 => (Key::Named(UiNamedKey::F3), Code::F3),
        NamedKey::F4 => (Key::Named(UiNamedKey::F4), Code::F4),
        NamedKey::F5 => (Key::Named(UiNamedKey::F5), Code::F5),
        NamedKey::F6 => (Key::Named(UiNamedKey::F6), Code::F6),
        NamedKey::F7 => (Key::Named(UiNamedKey::F7), Code::F7),
        NamedKey::F8 => (Key::Named(UiNamedKey::F8), Code::F8),
        NamedKey::F9 => (Key::Named(UiNamedKey::F9), Code::F9),
        NamedKey::F10 => (Key::Named(UiNamedKey::F10), Code::F10),
        NamedKey::F11 => (Key::Named(UiNamedKey::F11), Code::F11),
        NamedKey::F12 => (Key::Named(UiNamedKey::F12), Code::F12),
        // `mere_renderer_registry::NamedKey` is `#[non_exhaustive]`; future
        // variants fall through to `Unidentified`.
        _ => (Key::Named(UiNamedKey::Unidentified), Code::Unidentified),
    }
}

// PointerOrientation is referenced via PointerState::default()'s expansion;
// re-export-suppression to silence unused-import warnings if the helper
// expansion ever drops the reference.
#[allow(dead_code)]
const _: Option<PointerOrientation> = None;
#[allow(dead_code)]
const _: Option<PointerButtons> = None;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ime_start_translates_to_enabled() {
        let translated = translate_to_masonry_text(&InputEvent::Ime(ImeEvent::Start));
        assert!(matches!(translated, Some(TextEvent::Ime(Ime::Enabled))));
    }

    #[test]
    fn ime_commit_round_trips_string() {
        let translated = translate_to_masonry_text(&InputEvent::Ime(ImeEvent::Commit(
            "hello".to_string(),
        )));
        match translated {
            Some(TextEvent::Ime(Ime::Commit(s))) => assert_eq!(s, "hello"),
            other => panic!("expected Ime::Commit, got {other:?}"),
        }
    }

    #[test]
    fn ime_composing_round_trips_text_and_cursor() {
        let translated = translate_to_masonry_text(&InputEvent::Ime(ImeEvent::Composing {
            text: "hi".to_string(),
            cursor: 0..2,
        }));
        match translated {
            Some(TextEvent::Ime(Ime::Preedit(text, Some((start, end))))) => {
                assert_eq!(text, "hi");
                assert_eq!((start, end), (0, 2));
            }
            other => panic!("expected Ime::Preedit, got {other:?}"),
        }
    }

    #[test]
    fn focus_translates_to_window_focus_change() {
        let translated = translate_to_masonry_text(&InputEvent::Focus { focused: true });
        assert!(matches!(translated, Some(TextEvent::WindowFocusChange(true))));
    }

    #[test]
    fn key_down_arrow_up_maps_to_named_arrow_up() {
        let translated = translate_to_masonry_text(&InputEvent::Key {
            kind: KeyEventKind::Down,
            code: KeyCode::Named(NamedKey::ArrowUp),
            modifiers: ModifiersState::default(),
        });
        match translated {
            Some(TextEvent::Keyboard(kb)) => {
                assert_eq!(kb.state, KeyState::Down);
                assert_eq!(kb.code, Code::ArrowUp);
                assert!(matches!(kb.key, Key::Named(UiNamedKey::ArrowUp)));
            }
            other => panic!("expected Keyboard event, got {other:?}"),
        }
    }

    #[test]
    fn key_with_shift_modifier() {
        let mut mods = ModifiersState::default();
        mods.shift = true;
        let translated = translate_to_masonry_text(&InputEvent::Key {
            kind: KeyEventKind::Down,
            code: KeyCode::Character('a'),
            modifiers: mods,
        });
        match translated {
            Some(TextEvent::Keyboard(kb)) => {
                assert!(kb.modifiers.contains(Modifiers::SHIFT));
                assert!(matches!(kb.key, Key::Character(ref s) if s == "a"));
            }
            other => panic!("expected Keyboard event, got {other:?}"),
        }
    }

    #[test]
    fn pointer_move_builds_move_event() {
        let translated = translate_to_masonry_pointer(&InputEvent::Pointer {
            position: kurbo::Point::new(10.0, 20.0),
            kind: PointerEventKind::Move,
            modifiers: ModifiersState::default(),
        });
        match translated {
            Some(PointerEvent::Move(update)) => {
                assert_eq!(update.current.position.x, 10.0);
                assert_eq!(update.current.position.y, 20.0);
            }
            other => panic!("expected PointerEvent::Move, got {other:?}"),
        }
    }

    #[test]
    fn pointer_down_primary_carries_button() {
        let translated = translate_to_masonry_pointer(&InputEvent::Pointer {
            position: kurbo::Point::ZERO,
            kind: PointerEventKind::Down(RPB::Primary),
            modifiers: ModifiersState::default(),
        });
        match translated {
            Some(PointerEvent::Down(button_event)) => {
                assert_eq!(button_event.button, Some(UiPointerButton::Primary));
            }
            other => panic!("expected PointerEvent::Down, got {other:?}"),
        }
    }

    #[test]
    fn pointer_wheel_builds_scroll_event() {
        let translated = translate_to_masonry_pointer(&InputEvent::Pointer {
            position: kurbo::Point::ZERO,
            kind: PointerEventKind::Wheel(Vec2::new(0.0, 1.5)),
            modifiers: ModifiersState::default(),
        });
        match translated {
            Some(PointerEvent::Scroll(scroll)) => match scroll.delta {
                ScrollDelta::LineDelta(x, y) => {
                    assert_eq!((x, y), (0.0, 1.5));
                }
                other => panic!("expected LineDelta, got {other:?}"),
            },
            other => panic!("expected PointerEvent::Scroll, got {other:?}"),
        }
    }

    #[test]
    fn pointer_event_returns_none_for_text_translator() {
        let translated = translate_to_masonry_text(&InputEvent::Pointer {
            position: kurbo::Point::ZERO,
            kind: PointerEventKind::Move,
            modifiers: ModifiersState::default(),
        });
        assert!(translated.is_none());
    }
}
