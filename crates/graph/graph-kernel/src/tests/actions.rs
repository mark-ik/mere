/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tests for the portable action catalogue (actions.rs).

use crate::actions::*;
use std::collections::HashSet;

#[test]
fn action_category_labels_are_stable() {
    assert_eq!(ActionCategory::Node.label(), "Node");
    assert_eq!(ActionCategory::Edge.label(), "Edge");
    assert_eq!(ActionCategory::Graph.label(), "Graph");
    assert_eq!(ActionCategory::Persistence.label(), "Persistence");
}

#[test]
fn category_persisted_name_round_trips_all_variants() {
    for category in default_category_order() {
        let name = category_persisted_name(category);
        assert_eq!(
            category_from_persisted_name(name),
            Some(category),
            "round-trip for {category:?} (name={name})"
        );
    }
}

#[test]
fn category_from_persisted_name_rejects_unknown() {
    assert_eq!(category_from_persisted_name(""), None);
    assert_eq!(category_from_persisted_name("Node"), None); // case-sensitive
    assert_eq!(category_from_persisted_name("nodes"), None); // plural
}

#[test]
fn default_category_order_contains_each_category_exactly_once() {
    let order = default_category_order();
    let set: HashSet<_> = order.iter().copied().collect();
    assert_eq!(set.len(), order.len());
    assert_eq!(order.len(), 4);
}

#[test]
fn every_action_id_has_a_category() {
    // Pinned invariant: `ActionId::category()` covers every
    // variant. If a new variant is added without a category
    // arm, this test fails at the moment `all_action_ids` is
    // updated — catches the mistake in core's own suite before
    // it reaches the render-side resolver.
    for id in all_action_ids() {
        let _ = id.category();
    }
}

#[test]
fn every_action_id_key_is_namespace_format() {
    for id in all_action_ids() {
        let key = id.key();
        assert!(
            action_id_has_namespace_format(key),
            "action id {id:?} has key {key:?} which violates the `namespace:name` format"
        );
    }
}

#[test]
fn action_id_keys_are_unique() {
    // If two variants map to the same key, runtime dispatch
    // tables collapse and a user action routes to the wrong
    // handler. Pin the uniqueness invariant.
    let mut seen = HashSet::new();
    for id in all_action_ids() {
        let key = id.key();
        assert!(
            seen.insert(key),
            "duplicate action key {key:?} — two ActionId variants map to the same key"
        );
    }
}

#[test]
fn action_id_labels_are_non_empty() {
    for id in all_action_ids() {
        assert!(!id.label().is_empty(), "ActionId {id:?} has an empty label");
        assert!(
            !id.short_label().is_empty(),
            "ActionId {id:?} has an empty short_label"
        );
    }
}

#[test]
fn action_id_labels_differ_from_short_labels() {
    // Not always — a few ActionIds use the same text in both
    // labels. Just spot-check a couple that definitely differ so
    // a silent regression to "all short_labels == label" would
    // surface.
    assert_ne!(
        ActionId::NodeNewAsTab.label(),
        ActionId::NodeNewAsTab.short_label()
    );
    assert_ne!(
        ActionId::GraphToggleOverviewPlane.label(),
        ActionId::GraphToggleOverviewPlane.short_label()
    );
}

#[test]
fn action_id_has_namespace_format_rejects_malformed_inputs() {
    assert!(!action_id_has_namespace_format(""));
    assert!(!action_id_has_namespace_format("no_colon"));
    assert!(!action_id_has_namespace_format(":missing_namespace"));
    assert!(!action_id_has_namespace_format("missing_name:"));
    assert!(!action_id_has_namespace_format("too:many:colons"));
    assert!(!action_id_has_namespace_format("BadCase:name")); // uppercase namespace
    assert!(!action_id_has_namespace_format("ns:Name")); // uppercase name
    assert!(!action_id_has_namespace_format("ns:na-me")); // hyphen not allowed
}

#[test]
fn action_id_has_namespace_format_accepts_valid_inputs() {
    assert!(action_id_has_namespace_format("node:new"));
    assert!(action_id_has_namespace_format("graph:fit_graphlet"));
    assert!(action_id_has_namespace_format("ns2:name3"));
    assert!(action_id_has_namespace_format("a:b"));
}

#[test]
fn action_id_serde_json_round_trips_a_sample() {
    // ActionId variants are unit-like; serde_json emits them as
    // JSON strings matching the variant name. Pin a handful of
    // variants so a change to the serde derive is noticed.
    for id in [
        ActionId::NodeNew,
        ActionId::EdgeConnectPair,
        ActionId::GraphTogglePhysics,
        ActionId::PersistUndo,
    ] {
        let encoded = serde_json::to_string(&id).expect("serialize");
        let decoded: ActionId = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, id);
    }
}
