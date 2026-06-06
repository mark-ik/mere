/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */


//! Input binding registry — keyboard / mouse / pad bindings keyed by
//! `action_id` strings, late-bound via `register_binding` and resolved
//! via `resolve_binding_id`.
//!
//! Extracted from `shell/desktop/runtime/registries/input.rs` per
//! Slice 54 of the workspace architecture proposal. The original
//! file had zero `crate::*` dependencies (pure std + serde), making
//! it the cleanest shell-side registry to extract once the keystone
//! `register-diagnostics` (Slice 53) had landed.

use std::collections::{HashMap, hash_map::Entry};
use std::str::FromStr;

pub mod binding_id {
    pub mod toolbar {
        pub const SUBMIT: &str = "input.toolbar.submit";
        pub const NAV_BACK: &str = "input.toolbar.nav.back";
        pub const NAV_FORWARD: &str = "input.toolbar.nav.forward";
        pub const NAV_RELOAD: &str = "input.toolbar.nav.reload";
    }
}

pub mod action_id {
    pub mod toolbar {
        pub const SUBMIT: &str = "toolbar:submit";
        pub const NAV_BACK: &str = "toolbar:navigate_back";
        pub const NAV_FORWARD: &str = "toolbar:navigate_forward";
        pub const NAV_RELOAD: &str = "toolbar:navigate_reload";
    }

    pub mod graph {
        pub const VIEW_CONFIRM: &str = "graph:view_confirm";
        pub const CYCLE_FOCUS_REGION: &str = "graph:cycle_focus_region";
        pub const TOGGLE_OVERVIEW_PLANE: &str = "graph:toggle_overview_plane";
        pub const COMMAND_PALETTE_OPEN: &str = "workbench:command_palette_open";
        pub const RADIAL_MENU_OPEN: &str = "workbench:radial_menu_open";
        pub const NODE_EDIT_TAGS: &str = "graph:node_edit_tags";
        pub const TOGGLE_PHYSICS: &str = "graph:toggle_physics";
        pub const REHEAT_PHYSICS: &str = "graph:reheat_physics";
        pub const ZOOM_IN: &str = "graph:zoom_in";
        pub const ZOOM_OUT: &str = "graph:zoom_out";
        pub const ZOOM_RESET: &str = "graph:zoom_reset";
        pub const TOGGLE_POSITION_FIT_LOCK: &str = "graph:toggle_position_fit_lock";
        pub const TOGGLE_ZOOM_FIT_LOCK: &str = "graph:toggle_zoom_fit_lock";
        pub const NODE_NEW: &str = "graph:node_new";
        pub const EDGE_CONNECT_PAIR: &str = "graph:edge_connect_pair";
        pub const EDGE_CONNECT_BOTH: &str = "graph:edge_connect_both";
        pub const EDGE_REMOVE_USER: &str = "graph:edge_remove_user";
        pub const NODE_PIN_SELECTED: &str = "graph:node_pin_selected";
        pub const NODE_UNPIN_SELECTED: &str = "graph:node_unpin_selected";
        pub const NODE_PIN_TOGGLE: &str = "graph:node_pin_toggle";
        pub const NODE_DELETE: &str = "graph:node_delete";
        pub const CLEAR: &str = "graph:clear";
        pub const SELECT_ALL: &str = "graph:select_all";
        pub const SELECT_VISIBLE: &str = "graph:select_visible";
    }

    pub mod workbench {
        pub const HELP_OPEN: &str = "workbench:help_open";
        pub const TOGGLE_WORKBENCH_OVERLAY: &str = "workbench:toggle_workbench_overlay";
        pub const OPEN_HISTORY_MANAGER: &str = "workbench:open_history_manager";
        pub const OPEN_PHYSICS_SETTINGS: &str = "workbench:open_physics_settings";
        pub const OPEN_CAMERA_CONTROLS: &str = "workbench:open_camera_controls";
        pub const TOGGLE_SEMANTIC_TAB_GROUP: &str = "workbench:toggle_semantic_tab_group";
        pub const UNDO: &str = "workbench:undo";
        pub const REDO: &str = "workbench:redo";
    }

    pub mod radial_menu {
        pub const CATEGORY_PREVIOUS: &str = "radial_menu:category_previous";
        pub const CATEGORY_NEXT: &str = "radial_menu:category_next";
        pub const SELECTION_PREVIOUS: &str = "radial_menu:selection_previous";
        pub const SELECTION_NEXT: &str = "radial_menu:selection_next";
        pub const CONFIRM: &str = "radial_menu:confirm";
        pub const CANCEL: &str = "radial_menu:cancel";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModifierMask(u8);

impl ModifierMask {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1 << 0);
    pub const SHIFT: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);

    fn label(self) -> &'static str {
        match self.0 {
            0 => "none",
            value if value == Self::CTRL.0 => "ctrl",
            value if value == Self::SHIFT.0 => "shift",
            value if value == Self::ALT.0 => "alt",
            value if value == (Self::CTRL.0 | Self::SHIFT.0) => "ctrl_shift",
            value if value == (Self::CTRL.0 | Self::ALT.0) => "ctrl_alt",
            value if value == (Self::SHIFT.0 | Self::ALT.0) => "shift_alt",
            value if value == (Self::CTRL.0 | Self::SHIFT.0 | Self::ALT.0) => "ctrl_shift_alt",
            _ => "custom",
        }
    }

    #[cfg(feature = "egui-host")]
    pub fn from_egui(modifiers: &egui::Modifiers) -> Self {
        let mut mask = Self::NONE;
        if modifiers.ctrl || modifiers.command {
            mask.0 |= Self::CTRL.0;
        }
        if modifiers.shift {
            mask.0 |= Self::SHIFT.0;
        }
        if modifiers.alt {
            mask.0 |= Self::ALT.0;
        }
        mask
    }

    fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

impl std::ops::BitOr for ModifierMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl FromStr for ModifierMask {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Self::NONE),
            "ctrl" => Ok(Self::CTRL),
            "shift" => Ok(Self::SHIFT),
            "alt" => Ok(Self::ALT),
            "ctrl_shift" => Ok(Self(Self::CTRL.0 | Self::SHIFT.0)),
            "ctrl_alt" => Ok(Self(Self::CTRL.0 | Self::ALT.0)),
            "shift_alt" => Ok(Self(Self::SHIFT.0 | Self::ALT.0)),
            "ctrl_shift_alt" => Ok(Self(Self::CTRL.0 | Self::SHIFT.0 | Self::ALT.0)),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    Enter,
    ArrowLeft,
    ArrowRight,
    F5,
    F1,
    F2,
    F3,
    F6,
    F7,
    F9,
    Home,
    Escape,
    Delete,
    Backspace,
    Plus,
    Minus,
    Num0,
}

impl NamedKey {
    fn label(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::ArrowLeft => "arrow_left",
            Self::ArrowRight => "arrow_right",
            Self::F5 => "f5",
            Self::F1 => "f1",
            Self::F2 => "f2",
            Self::F3 => "f3",
            Self::F6 => "f6",
            Self::F7 => "f7",
            Self::F9 => "f9",
            Self::Home => "home",
            Self::Escape => "escape",
            Self::Delete => "delete",
            Self::Backspace => "backspace",
            Self::Plus => "plus",
            Self::Minus => "minus",
            Self::Num0 => "num0",
        }
    }
}

impl FromStr for NamedKey {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "enter" => Ok(Self::Enter),
            "arrow_left" => Ok(Self::ArrowLeft),
            "arrow_right" => Ok(Self::ArrowRight),
            "f5" => Ok(Self::F5),
            "f1" => Ok(Self::F1),
            "f2" => Ok(Self::F2),
            "f3" => Ok(Self::F3),
            "f6" => Ok(Self::F6),
            "f7" => Ok(Self::F7),
            "f9" => Ok(Self::F9),
            "home" => Ok(Self::Home),
            "escape" => Ok(Self::Escape),
            "delete" => Ok(Self::Delete),
            "backspace" => Ok(Self::Backspace),
            "plus" => Ok(Self::Plus),
            "minus" => Ok(Self::Minus),
            "num0" => Ok(Self::Num0),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Keycode {
    Named(NamedKey),
    Char(char),
}

impl Keycode {
    fn label(self) -> String {
        match self {
            Self::Named(named) => named.label().to_string(),
            Self::Char(ch) => format!("char:{}", ch.to_ascii_lowercase()),
        }
    }
}

impl FromStr for Keycode {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let normalized = raw.trim().to_ascii_lowercase();
        if let Some(ch) = normalized.strip_prefix("char:") {
            let mut chars = ch.chars();
            if let (Some(value), None) = (chars.next(), chars.next()) {
                return Ok(Self::Char(value));
            }
            return Err(());
        }

        Ok(Self::Named(normalized.parse()?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputBinding {
    Key {
        modifiers: ModifierMask,
        keycode: Keycode,
    },
    Chord(Vec<InputBinding>),
}

impl InputBinding {
    pub fn label(&self) -> String {
        match self {
            Self::Key { modifiers, keycode } => {
                format!("key:{}:{}", modifiers.label(), keycode.label())
            }
            Self::Chord(sequence) => {
                let parts = sequence.iter().map(Self::label).collect::<Vec<_>>();
                format!("chord:{}", parts.join(">"))
            }
        }
    }

    pub fn display_label(&self) -> String {
        match self {
            Self::Key { modifiers, keycode } => {
                let mut parts = Vec::new();
                if modifiers.contains(ModifierMask::CTRL) {
                    parts.push("Ctrl".to_string());
                }
                if modifiers.contains(ModifierMask::SHIFT) {
                    parts.push("Shift".to_string());
                }
                if modifiers.contains(ModifierMask::ALT) {
                    parts.push("Alt".to_string());
                }
                parts.push(match keycode {
                    Keycode::Named(named) => match named {
                        NamedKey::Enter => "Enter".to_string(),
                        NamedKey::ArrowLeft => "Left".to_string(),
                        NamedKey::ArrowRight => "Right".to_string(),
                        NamedKey::F5 => "F5".to_string(),
                        NamedKey::F1 => "F1".to_string(),
                        NamedKey::F2 => "F2".to_string(),
                        NamedKey::F3 => "F3".to_string(),
                        NamedKey::F6 => "F6".to_string(),
                        NamedKey::F7 => "F7".to_string(),
                        NamedKey::F9 => "F9".to_string(),
                        NamedKey::Home => "Home".to_string(),
                        NamedKey::Escape => "Esc".to_string(),
                        NamedKey::Delete => "Delete".to_string(),
                        NamedKey::Backspace => "Backspace".to_string(),
                        NamedKey::Plus => "+".to_string(),
                        NamedKey::Minus => "-".to_string(),
                        NamedKey::Num0 => "0".to_string(),
                    },
                    Keycode::Char(ch) => ch.to_ascii_uppercase().to_string(),
                });
                parts.join("+")
            }
            Self::Chord(sequence) => sequence
                .iter()
                .map(Self::display_label)
                .collect::<Vec<_>>()
                .join(" then "),
        }
    }

    #[cfg(feature = "egui-host")]
    pub fn from_egui_key(key: egui::Key, modifiers: &egui::Modifiers) -> Option<Self> {
        let keycode = match key {
            egui::Key::Enter => Keycode::Named(NamedKey::Enter),
            egui::Key::ArrowLeft => Keycode::Named(NamedKey::ArrowLeft),
            egui::Key::ArrowRight => Keycode::Named(NamedKey::ArrowRight),
            egui::Key::F1 => Keycode::Named(NamedKey::F1),
            egui::Key::F2 => Keycode::Named(NamedKey::F2),
            egui::Key::F3 => Keycode::Named(NamedKey::F3),
            egui::Key::F5 => Keycode::Named(NamedKey::F5),
            egui::Key::F6 => Keycode::Named(NamedKey::F6),
            egui::Key::F7 => Keycode::Named(NamedKey::F7),
            egui::Key::F9 => Keycode::Named(NamedKey::F9),
            egui::Key::Home => Keycode::Named(NamedKey::Home),
            egui::Key::Escape => Keycode::Named(NamedKey::Escape),
            egui::Key::Delete => Keycode::Named(NamedKey::Delete),
            egui::Key::Backspace => Keycode::Named(NamedKey::Backspace),
            egui::Key::Plus | egui::Key::Equals => Keycode::Named(NamedKey::Plus),
            egui::Key::Minus => Keycode::Named(NamedKey::Minus),
            egui::Key::Num0 => Keycode::Named(NamedKey::Num0),
            egui::Key::A => Keycode::Char('a'),
            egui::Key::C => Keycode::Char('c'),
            egui::Key::F => Keycode::Char('f'),
            egui::Key::G => Keycode::Char('g'),
            egui::Key::H => Keycode::Char('h'),
            egui::Key::I => Keycode::Char('i'),
            egui::Key::K => Keycode::Char('k'),
            egui::Key::L => Keycode::Char('l'),
            egui::Key::N => Keycode::Char('n'),
            egui::Key::O => Keycode::Char('o'),
            egui::Key::P => Keycode::Char('p'),
            egui::Key::Questionmark => Keycode::Char('?'),
            egui::Key::R => Keycode::Char('r'),
            egui::Key::T => Keycode::Char('t'),
            egui::Key::U => Keycode::Char('u'),
            egui::Key::Y => Keycode::Char('y'),
            egui::Key::Z => Keycode::Char('z'),
            _ => return None,
        };

        Some(Self::Key {
            modifiers: ModifierMask::from_egui(modifiers),
            keycode,
        })
    }
}

impl FromStr for InputBinding {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let normalized = raw.trim().to_ascii_lowercase();
        if let Some(rest) = normalized.strip_prefix("key:") {
            let mut parts = rest.splitn(2, ':');
            let modifiers = parts.next().ok_or(())?.parse()?;
            let keycode = parts.next().ok_or(())?.parse()?;
            return Ok(Self::Key { modifiers, keycode });
        }

        if let Some(rest) = normalized.strip_prefix("chord:") {
            let sequence = rest
                .split('>')
                .map(str::parse)
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Self::Chord(sequence));
        }

        Err(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputContext {
    GraphView,
    DetailView,
    OmnibarOpen,
    RadialMenuOpen,
    DialogOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputBindingSection {
    Graph,
    Workbench,
    Navigation,
}

impl InputBindingSection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Graph => "Graph",
            Self::Workbench => "Workbench",
            Self::Navigation => "Navigation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputActionBindingDescriptor {
    pub action_id: String,
    pub display_name: &'static str,
    pub context: InputContext,
    pub section: InputBindingSection,
    pub current_binding: Option<InputBinding>,
    pub default_binding: Option<InputBinding>,
}

impl InputContext {
    pub fn label(self) -> &'static str {
        match self {
            Self::GraphView => "graph_view",
            Self::DetailView => "detail_view",
            Self::OmnibarOpen => "omnibar_open",
            Self::RadialMenuOpen => "radial_menu_open",
            Self::DialogOpen => "dialog_open",
        }
    }
}

impl FromStr for InputContext {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "graph_view" => Ok(Self::GraphView),
            "detail_view" => Ok(Self::DetailView),
            "omnibar_open" => Ok(Self::OmnibarOpen),
            "radial_menu_open" => Ok(Self::RadialMenuOpen),
            "dialog_open" => Ok(Self::DialogOpen),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputBindingRemap {
    pub old: InputBinding,
    pub new: InputBinding,
    pub context: InputContext,
}

impl InputBindingRemap {
    pub fn encode(&self) -> String {
        format!(
            "{}|{}|{}",
            self.context.label(),
            self.old.label(),
            self.new.label()
        )
    }

    pub fn decode(raw: &str) -> Result<Self, ()> {
        let mut parts = raw.splitn(3, '|');
        let context = parts.next().ok_or(())?.parse()?;
        let old = parts.next().ok_or(())?.parse()?;
        let new = parts.next().ok_or(())?.parse()?;
        Ok(Self { old, new, context })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputConflict {
    MissingBinding {
        binding_label: String,
    },
    SourceConflict {
        binding_label: String,
        action_ids: Vec<String>,
    },
    TargetConflict {
        binding_label: String,
        action_ids: Vec<String>,
    },
}

mod defaults;
mod registry;
pub use registry::*;
#[cfg(test)]
mod tests;
