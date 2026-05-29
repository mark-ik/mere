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

#[derive(Debug, Clone, PartialEq, Eq)]
enum BindingSlot {
    Routed(String),
    Conflict(Vec<String>),
}

fn toolbar_submit_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::NONE,
        keycode: Keycode::Named(NamedKey::Enter),
    }
}

fn graph_view_confirm_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::NONE,
        keycode: Keycode::Named(NamedKey::Enter),
    }
}

fn toolbar_nav_back_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::ALT,
        keycode: Keycode::Named(NamedKey::ArrowLeft),
    }
}

fn toolbar_nav_forward_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::ALT,
        keycode: Keycode::Named(NamedKey::ArrowRight),
    }
}

fn toolbar_nav_reload_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::NONE,
        keycode: Keycode::Named(NamedKey::F5),
    }
}

fn binding_label(binding: &InputBinding, context: InputContext) -> String {
    format!("{}@{}", binding.label(), context.label())
}

#[derive(Clone)]
struct DefaultBindingSpec {
    action_id: &'static str,
    display_name: &'static str,
    section: InputBindingSection,
    context: InputContext,
    binding: InputBinding,
}

fn default_binding_specs() -> Vec<DefaultBindingSpec> {
    vec![
        DefaultBindingSpec {
            action_id: action_id::graph::TOGGLE_OVERVIEW_PLANE,
            display_name: "Toggle Overview Plane",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::CTRL | ModifierMask::SHIFT,
                keycode: Keycode::Char('o'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::NODE_EDIT_TAGS,
            display_name: "Edit Node Tags",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::CTRL,
                keycode: Keycode::Char('t'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::TOGGLE_PHYSICS,
            display_name: "Toggle Physics Simulation",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('t'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::REHEAT_PHYSICS,
            display_name: "Reheat Physics Simulation",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('r'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::ZOOM_IN,
            display_name: "Zoom In",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::CTRL,
                keycode: Keycode::Named(NamedKey::Plus),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::ZOOM_OUT,
            display_name: "Zoom Out",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::CTRL,
                keycode: Keycode::Named(NamedKey::Minus),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::ZOOM_RESET,
            display_name: "Reset Zoom",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::CTRL,
                keycode: Keycode::Named(NamedKey::Num0),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::TOGGLE_POSITION_FIT_LOCK,
            display_name: "Toggle Position-Fit Lock",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('c'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::TOGGLE_ZOOM_FIT_LOCK,
            display_name: "Toggle Zoom-Fit Lock",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('z'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::NODE_NEW,
            display_name: "Create Node",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('n'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::EDGE_CONNECT_PAIR,
            display_name: "Connect Selected Pair",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('g'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::EDGE_CONNECT_BOTH,
            display_name: "Connect Both Directions",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::SHIFT,
                keycode: Keycode::Char('g'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::EDGE_REMOVE_USER,
            display_name: "Remove User Edge",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::ALT,
                keycode: Keycode::Char('g'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::NODE_PIN_SELECTED,
            display_name: "Pin Selected Node(s)",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('i'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::NODE_UNPIN_SELECTED,
            display_name: "Unpin Selected Node(s)",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('u'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::NODE_PIN_TOGGLE,
            display_name: "Toggle Primary Node Pin",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('l'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::NODE_DELETE,
            display_name: "Delete Selected Nodes",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Named(NamedKey::Delete),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::CLEAR,
            display_name: "Clear Graph",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask(ModifierMask::CTRL.0 | ModifierMask::SHIFT.0),
                keycode: Keycode::Named(NamedKey::Delete),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::SELECT_ALL,
            display_name: "Select All Nodes",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::CTRL,
                keycode: Keycode::Char('a'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::SELECT_VISIBLE,
            display_name: "Select Visible Nodes",
            section: InputBindingSection::Graph,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask(ModifierMask::CTRL.0 | ModifierMask::SHIFT.0),
                keycode: Keycode::Char('a'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::workbench::HELP_OPEN,
            display_name: "Toggle Help Panel",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Named(NamedKey::F1),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::COMMAND_PALETTE_OPEN,
            display_name: "Open Command Palette",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Named(NamedKey::F2),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::RADIAL_MENU_OPEN,
            display_name: "Toggle Radial Palette",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Named(NamedKey::F3),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::workbench::OPEN_PHYSICS_SETTINGS,
            display_name: "Open Physics Settings",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Char('p'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::workbench::OPEN_CAMERA_CONTROLS,
            display_name: "Open Camera Controls",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Named(NamedKey::F9),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::workbench::OPEN_HISTORY_MANAGER,
            display_name: "Open History Manager",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::CTRL,
                keycode: Keycode::Char('h'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::workbench::TOGGLE_SEMANTIC_TAB_GROUP,
            display_name: "Toggle Semantic Tab Group",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask(ModifierMask::CTRL.0 | ModifierMask::ALT.0),
                keycode: Keycode::Char('t'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::workbench::UNDO,
            display_name: "Undo",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::CTRL,
                keycode: Keycode::Char('z'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::workbench::REDO,
            display_name: "Redo",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::CTRL,
                keycode: Keycode::Char('y'),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::graph::CYCLE_FOCUS_REGION,
            display_name: "Cycle Focus Region",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Named(NamedKey::F6),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::workbench::TOGGLE_WORKBENCH_OVERLAY,
            display_name: "Toggle Workbench Overlay",
            section: InputBindingSection::Workbench,
            context: InputContext::GraphView,
            binding: InputBinding::Key {
                modifiers: ModifierMask::NONE,
                keycode: Keycode::Named(NamedKey::F7),
            },
        },
        DefaultBindingSpec {
            action_id: action_id::toolbar::NAV_BACK,
            display_name: "Navigate Back",
            section: InputBindingSection::Navigation,
            context: InputContext::DetailView,
            binding: toolbar_nav_back_binding(),
        },
        DefaultBindingSpec {
            action_id: action_id::toolbar::NAV_FORWARD,
            display_name: "Navigate Forward",
            section: InputBindingSection::Navigation,
            context: InputContext::DetailView,
            binding: toolbar_nav_forward_binding(),
        },
        DefaultBindingSpec {
            action_id: action_id::toolbar::NAV_RELOAD,
            display_name: "Reload",
            section: InputBindingSection::Navigation,
            context: InputContext::DetailView,
            binding: toolbar_nav_reload_binding(),
        },
    ]
}

fn legacy_binding(binding_id: &str) -> Option<(InputBinding, InputContext)> {
    match binding_id.to_ascii_lowercase().as_str() {
        binding_id::toolbar::SUBMIT => Some((toolbar_submit_binding(), InputContext::OmnibarOpen)),
        binding_id::toolbar::NAV_BACK => {
            Some((toolbar_nav_back_binding(), InputContext::DetailView))
        }
        binding_id::toolbar::NAV_FORWARD => {
            Some((toolbar_nav_forward_binding(), InputContext::DetailView))
        }
        binding_id::toolbar::NAV_RELOAD => {
            Some((toolbar_nav_reload_binding(), InputContext::DetailView))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputBindingResolution {
    pub binding_label: String,
    pub context: InputContext,
    pub action_id: Option<String>,
    pub matched: bool,
    pub conflicted: bool,
}

pub struct InputRegistry {
    bindings: HashMap<(InputContext, InputBinding), BindingSlot>,
}

impl InputRegistry {
    fn current_binding_for_action(
        &self,
        action_id: &str,
        context: InputContext,
    ) -> Option<InputBinding> {
        let normalized = action_id.to_ascii_lowercase();
        for ((entry_context, binding), slot) in &self.bindings {
            if *entry_context != context {
                continue;
            }
            if let BindingSlot::Routed(routed) = slot {
                if routed == &normalized {
                    return Some(binding.clone());
                }
            }
        }
        None
    }

    pub fn describe_bindable_actions(&self) -> Vec<InputActionBindingDescriptor> {
        default_binding_specs()
            .into_iter()
            .map(|spec| InputActionBindingDescriptor {
                action_id: spec.action_id.to_string(),
                display_name: spec.display_name,
                context: spec.context,
                section: spec.section,
                current_binding: self.current_binding_for_action(spec.action_id, spec.context),
                default_binding: Some(spec.binding),
            })
            .collect()
    }

    pub fn binding_display_labels_for_action(&self, action_id: &str) -> Vec<String> {
        self.describe_bindable_actions()
            .into_iter()
            .filter(|entry| entry.action_id == action_id)
            .filter_map(|entry| entry.current_binding.map(|binding| binding.display_label()))
            .collect()
    }

    pub fn register_binding(
        &mut self,
        binding: InputBinding,
        action_id: &str,
        context: InputContext,
    ) {
        let normalized_action_id = action_id.to_ascii_lowercase();
        match self.bindings.entry((context, binding)) {
            Entry::Vacant(entry) => {
                entry.insert(BindingSlot::Routed(normalized_action_id));
            }
            Entry::Occupied(mut entry) => match entry.get_mut() {
                BindingSlot::Routed(existing) if *existing == normalized_action_id => {}
                BindingSlot::Routed(existing) => {
                    let actions = vec![existing.clone(), normalized_action_id];
                    entry.insert(BindingSlot::Conflict(actions));
                }
                BindingSlot::Conflict(actions) => {
                    if !actions.contains(&normalized_action_id) {
                        actions.push(normalized_action_id);
                    }
                }
            },
        }
    }

    pub fn resolve(
        &self,
        binding: &InputBinding,
        context: InputContext,
    ) -> InputBindingResolution {
        let label = binding_label(binding, context);
        match self.bindings.get(&(context, binding.clone())) {
            Some(BindingSlot::Routed(action_id)) => InputBindingResolution {
                binding_label: label,
                context,
                matched: true,
                conflicted: false,
                action_id: Some(action_id.clone()),
            },
            Some(BindingSlot::Conflict(_)) => InputBindingResolution {
                binding_label: label,
                context,
                matched: false,
                conflicted: true,
                action_id: None,
            },
            None => InputBindingResolution {
                binding_label: label,
                context,
                matched: false,
                conflicted: false,
                action_id: None,
            },
        }
    }

    pub fn resolve_binding_id(&self, binding_id: &str) -> InputBindingResolution {
        if let Some((binding, context)) = legacy_binding(binding_id) {
            return self.resolve(&binding, context);
        }

        InputBindingResolution {
            binding_label: binding_id.to_ascii_lowercase(),
            context: InputContext::DialogOpen,
            action_id: None,
            matched: false,
            conflicted: false,
        }
    }

    pub fn remap_binding(
        &mut self,
        old: InputBinding,
        new: InputBinding,
        context: InputContext,
    ) -> Result<(), InputConflict> {
        if old == new {
            return match self.bindings.get(&(context, old.clone())) {
                Some(BindingSlot::Routed(_)) => Ok(()),
                Some(BindingSlot::Conflict(action_ids)) => Err(InputConflict::SourceConflict {
                    binding_label: binding_label(&old, context),
                    action_ids: action_ids.clone(),
                }),
                None => Err(InputConflict::MissingBinding {
                    binding_label: binding_label(&old, context),
                }),
            };
        }

        let old_key = (context, old.clone());
        let new_key = (context, new.clone());
        let old_slot =
            self.bindings
                .remove(&old_key)
                .ok_or_else(|| InputConflict::MissingBinding {
                    binding_label: binding_label(&old, context),
                })?;

        let action_id = match old_slot {
            BindingSlot::Routed(action_id) => action_id,
            BindingSlot::Conflict(action_ids) => {
                self.bindings
                    .insert(old_key, BindingSlot::Conflict(action_ids.clone()));
                return Err(InputConflict::SourceConflict {
                    binding_label: binding_label(&old, context),
                    action_ids,
                });
            }
        };

        match self.bindings.entry(new_key) {
            Entry::Vacant(entry) => {
                entry.insert(BindingSlot::Routed(action_id));
                Ok(())
            }
            Entry::Occupied(entry) => {
                let conflict = match entry.get() {
                    BindingSlot::Routed(existing) if *existing == action_id => None,
                    BindingSlot::Routed(existing) => Some(InputConflict::TargetConflict {
                        binding_label: binding_label(&new, context),
                        action_ids: vec![existing.clone(), action_id.clone()],
                    }),
                    BindingSlot::Conflict(action_ids) => Some(InputConflict::TargetConflict {
                        binding_label: binding_label(&new, context),
                        action_ids: action_ids.clone(),
                    }),
                };

                self.bindings
                    .insert(old_key, BindingSlot::Routed(action_id));
                match conflict {
                    Some(conflict) => Err(conflict),
                    None => Ok(()),
                }
            }
        }
    }

    pub fn with_remaps(remaps: &[InputBindingRemap]) -> Result<Self, InputConflict> {
        let mut registry = Self::default();
        for remap in remaps {
            registry.remap_binding(remap.old.clone(), remap.new.clone(), remap.context)?;
        }
        Ok(registry)
    }
}

impl Default for InputRegistry {
    fn default() -> Self {
        let mut registry = Self {
            bindings: HashMap::new(),
        };
        registry.register_binding(
            toolbar_submit_binding(),
            action_id::toolbar::SUBMIT,
            InputContext::OmnibarOpen,
        );
        registry.register_binding(
            graph_view_confirm_binding(),
            action_id::graph::VIEW_CONFIRM,
            InputContext::GraphView,
        );
        for spec in default_binding_specs() {
            registry.register_binding(spec.binding, spec.action_id, spec.context);
        }
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_namespaced_action_id(action_id: &str) -> bool {
        let mut segments = action_id.split(':');
        let Some(namespace) = segments.next() else {
            return false;
        };
        let Some(name) = segments.next() else {
            return false;
        };

        !namespace.is_empty() && !name.is_empty() && segments.next().is_none()
    }

    #[test]
    fn input_registry_resolves_toolbar_submit_binding() {
        let registry = InputRegistry::default();
        let resolution = registry.resolve(&toolbar_submit_binding(), InputContext::OmnibarOpen);

        assert!(resolution.matched);
        assert_eq!(
            resolution.action_id.as_deref(),
            Some(action_id::toolbar::SUBMIT)
        );
    }

    #[test]
    fn input_registry_reports_missing_binding() {
        let registry = InputRegistry::default();
        let resolution = registry.resolve_binding_id("input.unknown.binding");

        assert!(!resolution.matched);
        assert!(!resolution.conflicted);
        assert_eq!(resolution.action_id, None);
    }

    #[test]
    fn input_registry_resolves_toolbar_nav_bindings() {
        let registry = InputRegistry::default();

        let back = registry.resolve(&toolbar_nav_back_binding(), InputContext::DetailView);
        assert!(back.matched);
        assert_eq!(
            back.action_id.as_deref(),
            Some(action_id::toolbar::NAV_BACK)
        );

        let forward = registry.resolve(&toolbar_nav_forward_binding(), InputContext::DetailView);
        assert!(forward.matched);
        assert_eq!(
            forward.action_id.as_deref(),
            Some(action_id::toolbar::NAV_FORWARD)
        );

        let reload = registry.resolve(&toolbar_nav_reload_binding(), InputContext::DetailView);
        assert!(reload.matched);
        assert_eq!(
            reload.action_id.as_deref(),
            Some(action_id::toolbar::NAV_RELOAD)
        );
    }

    #[test]
    fn input_registry_resolves_enter_differently_by_context() {
        let registry = InputRegistry::default();

        let omnibar = registry.resolve(&toolbar_submit_binding(), InputContext::OmnibarOpen);
        assert_eq!(
            omnibar.action_id.as_deref(),
            Some(action_id::toolbar::SUBMIT)
        );

        let graph_view = registry.resolve(&graph_view_confirm_binding(), InputContext::GraphView);
        assert_eq!(
            graph_view.action_id.as_deref(),
            Some(action_id::graph::VIEW_CONFIRM)
        );
    }

    #[test]
    fn input_registry_detects_same_binding_conflict_within_context() {
        let mut registry = InputRegistry {
            bindings: HashMap::new(),
        };

        registry.register_binding(
            toolbar_submit_binding(),
            action_id::toolbar::SUBMIT,
            InputContext::OmnibarOpen,
        );
        registry.register_binding(
            toolbar_submit_binding(),
            action_id::graph::VIEW_CONFIRM,
            InputContext::OmnibarOpen,
        );

        let resolution = registry.resolve(&toolbar_submit_binding(), InputContext::OmnibarOpen);
        assert!(!resolution.matched);
        assert!(resolution.conflicted);
        assert_eq!(resolution.action_id, None);
    }

    #[test]
    fn input_registry_legacy_binding_ids_resolve_through_typed_map() {
        let registry = InputRegistry::default();

        let resolution = registry.resolve_binding_id(binding_id::toolbar::NAV_RELOAD);
        assert!(resolution.matched);
        assert_eq!(resolution.context, InputContext::DetailView);
        assert_eq!(
            resolution.action_id.as_deref(),
            Some(action_id::toolbar::NAV_RELOAD)
        );
    }

    #[test]
    fn input_binding_remap_round_trips_through_string_encoding() {
        let remap = InputBindingRemap {
            old: toolbar_nav_back_binding(),
            new: InputBinding::Key {
                modifiers: ModifierMask::ALT,
                keycode: Keycode::Char('b'),
            },
            context: InputContext::GraphView,
        };

        let decoded = InputBindingRemap::decode(&remap.encode()).expect("remap should decode");
        assert_eq!(decoded, remap);
    }

    #[test]
    fn input_registry_remap_binding_replaces_existing_binding() {
        let mut registry = InputRegistry::default();
        let old = toolbar_nav_back_binding();
        let new = InputBinding::Key {
            modifiers: ModifierMask::ALT,
            keycode: Keycode::Char('b'),
        };

        registry
            .remap_binding(old.clone(), new.clone(), InputContext::DetailView)
            .expect("remap should succeed");

        assert_eq!(
            registry.resolve(&old, InputContext::DetailView).action_id,
            None
        );
        assert_eq!(
            registry
                .resolve(&new, InputContext::DetailView)
                .action_id
                .as_deref(),
            Some(action_id::toolbar::NAV_BACK)
        );
    }

    #[test]
    fn input_registry_remap_binding_detects_target_conflicts() {
        let mut registry = InputRegistry::default();
        let result = registry.remap_binding(
            toolbar_nav_back_binding(),
            toolbar_nav_forward_binding(),
            InputContext::DetailView,
        );

        assert!(matches!(result, Err(InputConflict::TargetConflict { .. })));
        assert_eq!(
            registry
                .resolve(&toolbar_nav_back_binding(), InputContext::DetailView)
                .action_id
                .as_deref(),
            Some(action_id::toolbar::NAV_BACK)
        );
    }

    #[test]
    fn input_registry_with_remaps_replays_on_top_of_defaults() {
        let remaps = [InputBindingRemap {
            old: toolbar_nav_back_binding(),
            new: InputBinding::Key {
                modifiers: ModifierMask::ALT,
                keycode: Keycode::Char('b'),
            },
            context: InputContext::DetailView,
        }];
        let registry = InputRegistry::with_remaps(&remaps).expect("remaps should apply");

        assert_eq!(
            registry
                .resolve(&remaps[0].new, InputContext::DetailView)
                .action_id
                .as_deref(),
            Some(action_id::toolbar::NAV_BACK)
        );
    }

    #[test]
    fn input_binding_display_label_uses_human_shortcut_format() {
        let binding = InputBinding::Key {
            modifiers: ModifierMask(ModifierMask::CTRL.0 | ModifierMask::SHIFT.0),
            keycode: Keycode::Char('g'),
        };

        assert_eq!(binding.display_label(), "Ctrl+Shift+G");
    }

    #[test]
    fn input_registry_describes_bindable_actions_with_current_and_default_bindings() {
        let registry = InputRegistry::default();
        let descriptors = registry.describe_bindable_actions();
        let command_palette = descriptors
            .iter()
            .find(|entry| entry.action_id == action_id::graph::COMMAND_PALETTE_OPEN)
            .expect("command palette binding descriptor should exist");

        assert_eq!(command_palette.display_name, "Open Command Palette");
        assert_eq!(command_palette.context, InputContext::GraphView);
        assert_eq!(
            command_palette
                .current_binding
                .as_ref()
                .map(InputBinding::display_label)
                .as_deref(),
            Some("F2")
        );
        assert_eq!(
            command_palette
                .default_binding
                .as_ref()
                .map(InputBinding::display_label)
                .as_deref(),
            Some("F2")
        );
    }

    #[test]
    fn input_registry_uses_ctrl_modified_default_zoom_bindings() {
        let registry = InputRegistry::default();
        let descriptors = registry.describe_bindable_actions();
        let zoom_in = descriptors
            .iter()
            .find(|entry| entry.action_id == action_id::graph::ZOOM_IN)
            .expect("zoom-in binding descriptor should exist");
        let zoom_out = descriptors
            .iter()
            .find(|entry| entry.action_id == action_id::graph::ZOOM_OUT)
            .expect("zoom-out binding descriptor should exist");
        let zoom_reset = descriptors
            .iter()
            .find(|entry| entry.action_id == action_id::graph::ZOOM_RESET)
            .expect("zoom-reset binding descriptor should exist");

        assert_eq!(
            zoom_in
                .default_binding
                .as_ref()
                .map(InputBinding::display_label)
                .as_deref(),
            Some("Ctrl++")
        );
        assert_eq!(
            zoom_out
                .default_binding
                .as_ref()
                .map(InputBinding::display_label)
                .as_deref(),
            Some("Ctrl+-")
        );
        assert_eq!(
            zoom_reset
                .default_binding
                .as_ref()
                .map(InputBinding::display_label)
                .as_deref(),
            Some("Ctrl+0")
        );
    }

    #[test]
    fn input_registry_action_ids_follow_namespace_name_format() {
        for action_id in [
            action_id::toolbar::SUBMIT,
            action_id::toolbar::NAV_BACK,
            action_id::toolbar::NAV_FORWARD,
            action_id::toolbar::NAV_RELOAD,
            action_id::graph::VIEW_CONFIRM,
            action_id::graph::CYCLE_FOCUS_REGION,
            action_id::graph::COMMAND_PALETTE_OPEN,
            action_id::graph::RADIAL_MENU_OPEN,
            action_id::graph::NODE_EDIT_TAGS,
            action_id::graph::TOGGLE_PHYSICS,
            action_id::graph::REHEAT_PHYSICS,
            action_id::graph::ZOOM_IN,
            action_id::graph::ZOOM_OUT,
            action_id::graph::ZOOM_RESET,
            action_id::graph::TOGGLE_POSITION_FIT_LOCK,
            action_id::graph::TOGGLE_ZOOM_FIT_LOCK,
            action_id::graph::NODE_NEW,
            action_id::graph::EDGE_CONNECT_PAIR,
            action_id::graph::EDGE_CONNECT_BOTH,
            action_id::graph::EDGE_REMOVE_USER,
            action_id::graph::NODE_PIN_SELECTED,
            action_id::graph::NODE_UNPIN_SELECTED,
            action_id::graph::NODE_PIN_TOGGLE,
            action_id::graph::NODE_DELETE,
            action_id::graph::CLEAR,
            action_id::graph::SELECT_ALL,
            action_id::graph::SELECT_VISIBLE,
            action_id::graph::SELECT_VISIBLE,
            action_id::workbench::HELP_OPEN,
            action_id::workbench::TOGGLE_WORKBENCH_OVERLAY,
            action_id::workbench::OPEN_HISTORY_MANAGER,
            action_id::workbench::OPEN_PHYSICS_SETTINGS,
            action_id::workbench::OPEN_CAMERA_CONTROLS,
            action_id::workbench::TOGGLE_SEMANTIC_TAB_GROUP,
            action_id::workbench::UNDO,
            action_id::workbench::REDO,
            action_id::radial_menu::CATEGORY_PREVIOUS,
            action_id::radial_menu::CATEGORY_NEXT,
            action_id::radial_menu::SELECTION_PREVIOUS,
            action_id::radial_menu::SELECTION_NEXT,
            action_id::radial_menu::CONFIRM,
            action_id::radial_menu::CANCEL,
        ] {
            assert!(is_namespaced_action_id(action_id), "{action_id}");
        }
    }

    #[test]
    fn input_registry_exposes_ctrl_shift_a_for_select_visible() {
        let registry = InputRegistry::default();
        let descriptors = registry.describe_bindable_actions();
        let select_visible = descriptors
            .iter()
            .find(|entry| entry.action_id == action_id::graph::SELECT_VISIBLE)
            .expect("select-visible binding descriptor should exist");

        assert_eq!(
            select_visible
                .default_binding
                .as_ref()
                .map(InputBinding::display_label)
                .as_deref(),
            Some("Ctrl+Shift+A")
        );
    }
}
