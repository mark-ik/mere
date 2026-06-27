/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Command-dispatch tests.

use super::*;

#[test]
fn empty_query_returns_all_in_order() {
    assert_eq!(filter(""), Command::ALL.to_vec());
    assert_eq!(filter("   "), Command::ALL.to_vec());
}

#[test]
fn query_filters_by_label_substring() {
    assert_eq!(filter("for"), vec![Command::Forward]);
    assert_eq!(filter("home"), vec![Command::Home]);
    // Case-insensitive; "back" also matches "...active in background".
    assert_eq!(filter("BACK"), vec![Command::Back, Command::BackgroundNode]);
}

#[test]
fn host_action_commands_filter_and_flag() {
    assert_eq!(filter("tile"), vec![Command::ToggleWorkbench]);
    assert_eq!(filter("delete"), vec![Command::DeleteNode]);
    assert!(Command::DeleteNode.is_host_action());
    assert!(Command::BackgroundNode.is_host_action());
    assert!(Command::RetryFocusedContent.is_host_action());
    assert!(Command::StopFocusedOperation.is_host_action());
    assert!(Command::PinFocusedOperation.is_host_action());
    assert!(
        !Command::Back.is_host_action(),
        "history verbs are not host actions"
    );
}

#[test]
fn unmatched_query_is_empty() {
    assert!(filter("zzz").is_empty());
}

#[test]
fn edge_commands_are_host_actions_with_verbs() {
    assert_eq!(Command::AssertEdge.verb(), "relate");
    assert_eq!(Command::RetractEdge.verb(), "unrelate");
    assert!(Command::AssertEdge.is_host_action());
    assert!(Command::RetractEdge.is_host_action());
    // The unambiguous label substring resolves; "relate" alone also matches
    // "Unrelate", so filter on the distinct token.
    assert_eq!(filter("unrelate"), vec![Command::RetractEdge]);
    assert!(filter("Relate selected").contains(&Command::AssertEdge));
}

#[test]
fn from_id_round_trips_every_verb() {
    // The registry seam: every command resolves from its own id, and an unknown id
    // resolves to nothing. This is what lets a script / agent / a11y route name a
    // command by id and get the typed command back.
    for cmd in Command::ALL {
        assert_eq!(Command::from_id(cmd.verb()), Some(cmd), "{cmd:?} must round-trip");
    }
    assert_eq!(Command::from_id("not_a_command"), None);
    assert_eq!(Command::from_id(""), None);
}

#[test]
fn command_entries_catalog_covers_all_with_unique_ids() {
    let entries = command_entries();
    assert_eq!(entries.len(), Command::ALL.len(), "the catalog lists every command");
    // ids are the verbs (already proven unique + identifier-safe), in ALL order.
    for (entry, cmd) in entries.iter().zip(Command::ALL) {
        assert_eq!(entry.id, cmd.verb());
        assert_eq!(entry.label, cmd.label());
        assert_eq!(entry.host_action, cmd.is_host_action());
        // every catalog id resolves back to its command (the registry round-trip).
        assert_eq!(Command::from_id(entry.id), Some(cmd));
    }
}

#[test]
fn palette_items_unify_commands_and_context_actions() {
    let all = palette_items("");
    assert_eq!(
        all.iter().filter(|i| matches!(i, PaletteItem::Command(_))).count(),
        Command::ALL.len(),
        "every command is a palette item",
    );
    assert_eq!(
        all.iter().filter(|i| matches!(i, PaletteItem::Context(_))).count(),
        PALETTE_CONTEXT_ACTIONS.len(),
        "every palette-exposed context action is a palette item",
    );
    // A query filters context-action labels too, and every returned item matches it.
    let isolate = palette_items("isolate");
    assert!(
        isolate
            .iter()
            .any(|i| matches!(i, PaletteItem::Context(crate::ContextAction::IsolateSelection))),
        "the Isolate context action filters in",
    );
    assert!(isolate.iter().all(|i| label_matches(i.label(), "isolate")));
    // The catalog round-trips a palette-exposed action and excludes a deferred one.
    assert_eq!(
        context_action_palette_label(crate::ContextAction::ShowAllNodes),
        Some("Show all nodes"),
    );
    assert_eq!(context_action_palette_label(crate::ContextAction::CloseGraphPane), None);
}

#[test]
fn registry_ids_are_unique_across_commands_and_context_actions() {
    // The registry is one id space: every command verb and every context-action id is
    // unique, and each context-action id round-trips. This is what lets an agent / script /
    // menu config name any action by one id namespace. (Command registry P3.)
    let mut seen = std::collections::HashSet::new();
    for cmd in Command::ALL {
        assert!(seen.insert(cmd.verb()), "duplicate registry id: {}", cmd.verb());
    }
    for &(action, id, _) in PALETTE_CONTEXT_ACTIONS {
        assert!(seen.insert(id), "context-action id collides with the registry: {id}");
        assert_eq!(context_action_from_id(id), Some(action), "{id} must round-trip");
        assert_eq!(context_action_id(action), Some(id));
    }
    assert_eq!(context_action_from_id("not_an_action"), None);
}

#[test]
fn default_menu_actions_are_all_known_registry_ids() {
    // Every default-menu id resolves to a scope + a label (no dead ids), so the config-driven
    // menu builder never silently drops a default row. (Command registry P4.)
    for &id in DEFAULT_MENU_ACTIONS {
        assert!(registry_scope(id).is_some(), "default menu id has no scope: {id}");
        assert!(registry_label(id).is_some(), "default menu id has no label: {id}");
    }
}

#[test]
fn menu_scope_filters_by_selection_shape() {
    assert!(MenuScope::Canvas.applies(0) && !MenuScope::Canvas.applies(1));
    assert!(MenuScope::SingleNode.applies(1) && !MenuScope::SingleNode.applies(2));
    assert!(MenuScope::Selection.applies(1) && MenuScope::Selection.applies(3));
    assert!(!MenuScope::Selection.applies(0));
    assert!(MenuScope::MultiNode.applies(2) && !MenuScope::MultiNode.applies(1));
    assert!(MenuScope::Always.applies(0));
    // The scopes that reproduce the hand-written menu's per-shape placement, plus a global
    // command (Always) and the AssertEdge ("relate") verb the menu's Relate row resolves to.
    assert_eq!(registry_scope("add_node"), Some(MenuScope::Canvas));
    assert_eq!(registry_scope("open_splits"), Some(MenuScope::Selection));
    assert_eq!(registry_scope("resize_node"), Some(MenuScope::SingleNode));
    assert_eq!(registry_scope("open_stack"), Some(MenuScope::MultiNode));
    assert_eq!(registry_scope("relate"), Some(MenuScope::MultiNode));
    assert_eq!(registry_scope("settings"), Some(MenuScope::Always));
    // Finer per-command scopes: a selection op, a single-focus op, a global toggle.
    assert_eq!(registry_scope("delete_node"), Some(MenuScope::Selection));
    assert_eq!(registry_scope("node_settings"), Some(MenuScope::SingleNode));
    assert_eq!(registry_scope("workbench"), Some(MenuScope::Always));
    assert_eq!(registry_scope("not_a_real_id"), None);
}

#[test]
fn every_verb_is_a_unique_valid_identifier() {
    // The command shell registers one rhai function per verb over `ALL`, so a
    // verb must be a non-empty, unique, identifier-safe token (lowercase /
    // digits / `_`, not starting with a digit). This guards a new command from
    // silently colliding with or shadowing another's binding.
    let mut seen = std::collections::HashSet::new();
    for cmd in Command::ALL {
        let v = cmd.verb();
        assert!(!v.is_empty(), "{cmd:?} has an empty verb");
        assert!(
            !v.starts_with(|c: char| c.is_ascii_digit()),
            "{cmd:?} verb starts with a digit: {v}"
        );
        assert!(
            v.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "{cmd:?} verb is not identifier-safe: {v}"
        );
        assert!(seen.insert(v), "duplicate verb: {v}");
    }
}
