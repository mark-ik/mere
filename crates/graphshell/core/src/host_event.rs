/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral input events flowing from platform adapters into the
//! runtime.
//!
//! [`HostEvent`] is a serialisable, platform-agnostic UI event that
//! gets fed into either the egui or iced hosts for parity verification
//! and golden replay. Pre-M4 slice 8 (2026-04-22) these types lived in
//! `shell/desktop/workbench/ux_replay.rs` next to the replay session
//! driver; the session itself (`UxReplaySession`) carries a
//! `UxTreeSnapshot` that stays shell-side, but the event vocabulary
//! is portable and moves here so `FrameHostInput.events: Vec<HostEvent>`
//! can live in graphshell-core.
//!
//! The shell crate re-exports these types from `ux_replay.rs` so
//! existing import paths resolve unchanged.

use serde::{Deserialize, Serialize};

/// Represents a platform-agnostic UI event that can be serialized,
/// deserialized, and fed into either the egui or expected iced hosts
/// for parity verification and golden testing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HostEvent {
    PointerMoved {
        x: f32,
        y: f32,
    },
    PointerDown {
        x: f32,
        y: f32,
        button: PointerButton,
    },
    PointerUp {
        x: f32,
        y: f32,
        button: PointerButton,
    },
    Scroll {
        dx: f32,
        dy: f32,
    },
    Zoom {
        delta: f32,
    },
    Text(String),
    Key {
        key: String,
        pressed: bool,
        modifiers: ModifiersState,
    },
    Focus(bool),
    WindowResized {
        width: f32,
        height: f32,
    },
    /// Synthesized command-surface events (e.g. from tests invoking
    /// Command Palette directly).
    CommandSurface {
        surface_id: String,
        action: String,
        payload: Option<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifiersState {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub mac_cmd: bool,
    pub command: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_default_is_all_false() {
        let m = ModifiersState::default();
        assert!(!m.alt && !m.ctrl && !m.shift && !m.mac_cmd && !m.command);
    }

    #[test]
    fn host_event_serde_round_trip() {
        // Replay golden tests depend on the serde wire shape being
        // stable across the runtime. Pin a representative sample.
        let event = HostEvent::Key {
            key: "Enter".to_string(),
            pressed: true,
            modifiers: ModifiersState {
                ctrl: true,
                ..ModifiersState::default()
            },
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: HostEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, event);
    }

    #[test]
    fn command_surface_event_round_trips_with_payload() {
        let payload = serde_json::json!({"arg": 42, "label": "submit"});
        let event = HostEvent::CommandSurface {
            surface_id: "command_palette".into(),
            action: "invoke".into(),
            payload: Some(payload.clone()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: HostEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn pointer_button_other_preserves_value() {
        let b = PointerButton::Other(42);
        let json = serde_json::to_string(&b).unwrap();
        let back: PointerButton = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }
}
