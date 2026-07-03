/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::{
    InputBinding, InputBindingSection, InputContext, Keycode, ModifierMask, NamedKey, action_id,
    binding_id,
};

pub(super) fn toolbar_submit_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::NONE,
        keycode: Keycode::Named(NamedKey::Enter),
    }
}

pub(super) fn graph_view_confirm_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::NONE,
        keycode: Keycode::Named(NamedKey::Enter),
    }
}

pub(super) fn toolbar_nav_back_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::ALT,
        keycode: Keycode::Named(NamedKey::ArrowLeft),
    }
}

pub(super) fn toolbar_nav_forward_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::ALT,
        keycode: Keycode::Named(NamedKey::ArrowRight),
    }
}

pub(super) fn toolbar_nav_reload_binding() -> InputBinding {
    InputBinding::Key {
        modifiers: ModifierMask::NONE,
        keycode: Keycode::Named(NamedKey::F5),
    }
}

pub(super) fn binding_label(binding: &InputBinding, context: InputContext) -> String {
    format!("{}@{}", binding.label(), context.label())
}

#[derive(Clone)]
pub(super) struct DefaultBindingSpec {
    pub(super) action_id: &'static str,
    pub(super) display_name: &'static str,
    pub(super) section: InputBindingSection,
    pub(super) context: InputContext,
    pub(super) binding: InputBinding,
}

pub(super) fn default_binding_specs() -> Vec<DefaultBindingSpec> {
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

pub(super) fn legacy_binding(binding_id: &str) -> Option<(InputBinding, InputContext)> {
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
