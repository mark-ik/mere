// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::action_id;
use super::binding_id;
use super::defaults::*;
use super::*;
use std::collections::HashMap;

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
