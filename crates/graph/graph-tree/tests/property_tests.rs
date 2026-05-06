// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Property tests: tree invariants hold after any sequence of NavActions.

use graph_tree::*;
use proptest::prelude::*;

/// Member IDs are small integers so collisions (duplicate attaches, etc.) exercise
/// edge cases naturally.
const MAX_MEMBER_ID: u64 = 20;

/// Generate a random NavAction over small member IDs.
///
/// Weights are chosen so that shrink actions (Dismiss, Detach) fire
/// roughly as often as growth actions (Attach variants). This models
/// realistic usage — people close tabs, not just open them — and
/// prevents monotonic tree growth that produces pathological
/// serialization sizes.
fn arb_nav_action() -> impl Strategy<Value = NavAction<u64>> {
    prop_oneof![
        // Attach with various provenances (total weight: 3)
        1 => (1..=MAX_MEMBER_ID).prop_map(|m| NavAction::Attach {
            member: m,
            provenance: Provenance::Anchor,
        }),
        1 => (1..=MAX_MEMBER_ID, 1..=MAX_MEMBER_ID).prop_map(|(m, s)| NavAction::Attach {
            member: m,
            provenance: Provenance::Traversal {
                source: s,
                edge_kind: None,
            },
        }),
        1 => (1..=MAX_MEMBER_ID, 1..=MAX_MEMBER_ID).prop_map(|(m, s)| NavAction::Attach {
            member: m,
            provenance: Provenance::Manual {
                source: Some(s),
                context: None,
            },
        }),
        // Shrink actions — weighted to balance growth (total weight: 3)
        2 => (1..=MAX_MEMBER_ID).prop_map(NavAction::Dismiss),
        1 => (1..=MAX_MEMBER_ID, any::<bool>()).prop_map(|(m, r)| NavAction::Detach {
            member: m,
            recursive: r,
        }),
        // Select / Activate (total weight: 2)
        1 => (1..=MAX_MEMBER_ID).prop_map(NavAction::Select),
        1 => (1..=MAX_MEMBER_ID).prop_map(NavAction::Activate),
        // Expand / Reveal (total weight: 2)
        1 => (1..=MAX_MEMBER_ID).prop_map(NavAction::ToggleExpand),
        1 => (1..=MAX_MEMBER_ID).prop_map(NavAction::Reveal),
        // Reparent (weight: 1)
        1 => (1..=MAX_MEMBER_ID, 1..=MAX_MEMBER_ID).prop_map(|(m, p)| NavAction::Reparent {
            member: m,
            new_parent: p,
        }),
        // Lifecycle (weight: 1)
        1 => (1..=MAX_MEMBER_ID, prop_oneof![
            Just(Lifecycle::Active),
            Just(Lifecycle::Warm),
            Just(Lifecycle::Cold),
        ]).prop_map(|(m, l)| NavAction::SetLifecycle(m, l)),
        // Layout mode (weight: 1)
        1 => prop_oneof![
            Just(LayoutMode::TreeStyleTabs),
            Just(LayoutMode::FlatTabs),
            Just(LayoutMode::SplitPanes),
        ].prop_map(NavAction::SetLayoutMode),
        // Lens (weight: 1)
        1 => prop_oneof![
            Just(ProjectionLens::Traversal),
            Just(ProjectionLens::Arrangement),
            Just(ProjectionLens::Containment),
            Just(ProjectionLens::Semantic),
            Just(ProjectionLens::Recency),
            Just(ProjectionLens::All),
        ].prop_map(NavAction::SetLens),
        // Focus cycling (weight: 1)
        1 => prop_oneof![
            Just(FocusDirection::Next),
            Just(FocusDirection::Previous),
        ].prop_map(NavAction::CycleFocus),
        1 => prop_oneof![
            Just(FocusCycleRegion::Roots),
            Just(FocusCycleRegion::Branches),
            Just(FocusCycleRegion::Leaves),
        ].prop_map(NavAction::CycleFocusRegion),
        // Layout override (weight: 1)
        1 => (1..=MAX_MEMBER_ID, proptest::option::of(0.0f32..=1.0))
            .prop_map(|(m, ratio)| NavAction::SetLayoutOverride(m, graph_tree::LayoutOverride {
                min_width: None,
                min_height: None,
                flex_grow: None,
                flex_shrink: None,
                preferred_split: None,
                split_ratio: ratio,
            })),
    ]
}

/// Verify all tree invariants hold.
fn assert_tree_invariants(tree: &GraphTree<u64>) {
    // 1. Topology invariants (parent/child consistency, no cycles, no duplicates)
    tree.topology().assert_invariants();

    // 2. Every member in the members map MUST be in the topology (no orphans)
    for (id, _) in tree.members() {
        assert!(
            tree.topology().contains(id),
            "member {:?} exists in members map but not in topology — orphaned",
            id
        );
        // depth should not panic for any topology member
        let _ = tree.depth_of(id);
    }

    // 2b. Every topology node must have a members entry
    for root in tree.topology().roots() {
        assert!(
            tree.contains(root),
            "topology root {:?} has no members entry",
            root
        );
    }

    // 3. Active member (if any) must exist in members
    if let Some(active) = tree.active() {
        assert!(
            tree.contains(active),
            "active member {:?} not in members map",
            active
        );
    }

    // 4. Member count matches
    assert_eq!(
        tree.member_count(),
        tree.members().count(),
        "member_count disagrees with members iterator"
    );

    // 5. Lifecycle counts are consistent
    assert_eq!(
        tree.active_count() + tree.warm_count() + tree.cold_count(),
        tree.member_count(),
        "lifecycle counts don't sum to member_count"
    );

    // 6. visible_rows doesn't panic
    let _ = tree.visible_rows();

    // 7. compute_layout doesn't panic
    let _ = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn random_nav_actions_preserve_invariants(
        actions in proptest::collection::vec(arb_nav_action(), 1..40)
    ) {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        for action in actions {
            tree.apply(action);
            assert_tree_invariants(&tree);
        }
    }

    #[test]
    fn serialization_survives_random_state(
        actions in proptest::collection::vec(arb_nav_action(), 1..40)
    ) {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        for action in actions {
            tree.apply(action);
        }

        let json = serde_json::to_string(&tree).unwrap();
        let restored: GraphTree<u64> = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.member_count(), tree.member_count());
        assert_tree_invariants(&restored);
    }
}
