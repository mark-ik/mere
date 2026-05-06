// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Origin-aware memory policy for large trees.
//!
//! The tree has no capacity limit (thousands of members are normal), but
//! runtime resources (WebViews, GL contexts, DOM state) are finite. This
//! module provides a policy layer that queries the tree by origin/domain
//! grouping and produces lifecycle demotion actions to keep resource
//! consumption bounded.
//!
//! Key principles:
//!
//! - **The tree doesn't own origin data.** Origin is graph truth. The host
//!   supplies an `origin_of` classifier at policy evaluation time.
//! - **Policy produces `NavAction`s, not mutations.** The tree's normal
//!   `apply()` pipeline handles all transitions, preserving intents and
//!   consistency.
//! - **Cold is cheap.** Demoting to Cold preserves topology, provenance,
//!   and graphlet membership. Only runtime resources are freed.
//! - **No hard capacity limits.** Policy is advisory — it recommends
//!   demotions, the host decides whether to apply them.

use std::collections::HashMap;

use crate::MemberId;
use crate::member::Lifecycle;
use crate::nav::NavAction;
use crate::tree::GraphTree;

/// Origin identifier. Opaque string — could be a domain ("example.com"),
/// a protocol+domain ("gemini://station.smolweb"), or a synthetic group
/// ("local", "unresolved"). The host controls the taxonomy.
pub type Origin = String;

/// Memory pressure policy configuration.
///
/// All limits are soft — exceeding them produces demotion recommendations,
/// not errors. The host chooses which to apply.
#[derive(Clone, Debug)]
pub struct MemoryPolicy {
    /// Maximum number of Active + Warm members across all origins.
    /// When exceeded, the policy recommends Cold-demoting the least
    /// recently relevant members from the least recently touched origins.
    pub global_warm_budget: usize,

    /// Maximum Active + Warm members per origin. Prevents a single
    /// origin from monopolizing runtime resources.
    pub per_origin_warm_budget: usize,

    /// Origins on this list are never demoted by policy. The user's
    /// currently focused origin is typically exempt.
    pub exempt_origins: Vec<Origin>,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            global_warm_budget: 100,
            per_origin_warm_budget: 20,
            exempt_origins: Vec::new(),
        }
    }
}

/// Per-origin summary produced by [`evaluate`].
#[derive(Clone, Debug)]
pub struct OriginSummary<N: MemberId> {
    pub origin: Origin,
    pub active_members: Vec<N>,
    pub warm_members: Vec<N>,
    pub cold_members: Vec<N>,
}

impl<N: MemberId> OriginSummary<N> {
    pub fn warm_pressure(&self) -> usize {
        self.active_members.len() + self.warm_members.len()
    }

    pub fn total(&self) -> usize {
        self.active_members.len() + self.warm_members.len() + self.cold_members.len()
    }
}

/// Result of evaluating memory policy against the current tree state.
#[derive(Clone, Debug)]
pub struct PolicyEvaluation<N: MemberId> {
    /// Recommended lifecycle demotions. Apply these via `tree.apply()`.
    pub recommended_demotions: Vec<NavAction<N>>,

    /// Per-origin summaries for diagnostics / UI display.
    pub origin_summaries: Vec<OriginSummary<N>>,

    /// Whether the global warm budget is exceeded.
    pub global_budget_exceeded: bool,

    /// Origins that exceed their per-origin warm budget.
    pub over_budget_origins: Vec<Origin>,
}

/// Group tree members by origin and produce per-origin summaries.
pub fn group_by_origin<N: MemberId>(
    tree: &GraphTree<N>,
    origin_of: &dyn Fn(&N) -> Origin,
) -> Vec<OriginSummary<N>> {
    let mut groups: HashMap<Origin, OriginSummary<N>> = HashMap::new();

    for (member, entry) in tree.members() {
        let origin = origin_of(member);
        let summary = groups
            .entry(origin.clone())
            .or_insert_with(|| OriginSummary {
                origin,
                active_members: Vec::new(),
                warm_members: Vec::new(),
                cold_members: Vec::new(),
            });
        match entry.lifecycle {
            Lifecycle::Active => summary.active_members.push(member.clone()),
            Lifecycle::Warm => summary.warm_members.push(member.clone()),
            Lifecycle::Cold => summary.cold_members.push(member.clone()),
        }
    }

    let mut summaries: Vec<_> = groups.into_values().collect();
    // Sort by warm pressure descending — heaviest origins first.
    summaries.sort_by(|a, b| b.warm_pressure().cmp(&a.warm_pressure()));
    summaries
}

/// Evaluate memory policy against the current tree state.
///
/// Returns recommended `SetLifecycle(_, Cold)` actions for members that
/// should be demoted to free resources. The host decides which (if any)
/// to apply.
///
/// `last_touched` provides a recency ordering — members with higher
/// values were touched more recently and are preserved longer. If not
/// available for a member, it gets the lowest priority (demoted first).
pub fn evaluate<N: MemberId>(
    tree: &GraphTree<N>,
    policy: &MemoryPolicy,
    origin_of: &dyn Fn(&N) -> Origin,
    last_touched: &dyn Fn(&N) -> u64,
) -> PolicyEvaluation<N> {
    let summaries = group_by_origin(tree, origin_of);

    let mut demotions: Vec<NavAction<N>> = Vec::new();
    let mut over_budget_origins: Vec<Origin> = Vec::new();

    // Phase 1: Per-origin warm budget enforcement.
    // For each origin over budget, demote Warm members (oldest first).
    // Never demote Active — that's the user's current focus.
    for summary in &summaries {
        if policy.exempt_origins.contains(&summary.origin) {
            continue;
        }

        let warm_count = summary.warm_pressure();
        if warm_count > policy.per_origin_warm_budget {
            over_budget_origins.push(summary.origin.clone());

            let excess = warm_count - policy.per_origin_warm_budget;
            let mut candidates: Vec<_> = summary.warm_members.clone();
            // Sort by last_touched ascending — oldest first for demotion.
            candidates.sort_by_key(|m| last_touched(m));

            for member in candidates.into_iter().take(excess) {
                demotions.push(NavAction::SetLifecycle(member, Lifecycle::Cold));
            }
        }
    }

    // Phase 2: Global warm budget enforcement.
    // If still over budget after per-origin pass, demote from the
    // least recently touched origins (that aren't exempt).
    let current_warm: usize = summaries.iter().map(|s| s.warm_pressure()).sum();
    let global_exceeded = current_warm > policy.global_warm_budget;

    if global_exceeded {
        // How many more demotions do we need (accounting for phase 1)?
        let already_demoting = demotions.len();
        let still_warm = current_warm.saturating_sub(already_demoting);
        let global_excess = still_warm.saturating_sub(policy.global_warm_budget);

        if global_excess > 0 {
            // Collect all Warm members from non-exempt origins, sorted by recency.
            let mut global_candidates: Vec<(N, u64)> = Vec::new();
            for summary in &summaries {
                if policy.exempt_origins.contains(&summary.origin) {
                    continue;
                }
                for member in &summary.warm_members {
                    // Skip members already slated for demotion in phase 1.
                    let already_slated = demotions
                        .iter()
                        .any(|d| matches!(d, NavAction::SetLifecycle(m, _) if m == member));
                    if !already_slated {
                        global_candidates.push((member.clone(), last_touched(member)));
                    }
                }
            }
            // Sort ascending by last_touched — oldest first.
            global_candidates.sort_by_key(|(_, t)| *t);

            for (member, _) in global_candidates.into_iter().take(global_excess) {
                demotions.push(NavAction::SetLifecycle(member, Lifecycle::Cold));
            }
        }
    }

    PolicyEvaluation {
        recommended_demotions: demotions,
        origin_summaries: summaries,
        global_budget_exceeded: global_exceeded,
        over_budget_origins,
    }
}

/// Convenience: cold-sweep all members from a specific origin.
///
/// Demotes all Active + Warm members under the given origin to Cold.
/// Useful for "close all tabs from this site" or origin-level cleanup.
pub fn cold_sweep_origin<N: MemberId>(
    tree: &GraphTree<N>,
    origin_of: &dyn Fn(&N) -> Origin,
    target_origin: &str,
) -> Vec<NavAction<N>> {
    tree.members()
        .filter(|(member, entry)| {
            entry.lifecycle != Lifecycle::Cold && origin_of(member) == target_origin
        })
        .map(|(member, _)| NavAction::SetLifecycle(member.clone(), Lifecycle::Cold))
        .collect()
}

/// Convenience: count warm pressure per origin for diagnostics display.
pub fn warm_pressure_by_origin<N: MemberId>(
    tree: &GraphTree<N>,
    origin_of: &dyn Fn(&N) -> Origin,
) -> Vec<(Origin, usize)> {
    let summaries = group_by_origin(tree, origin_of);
    summaries
        .into_iter()
        .map(|s| {
            let pressure = s.warm_pressure();
            (s.origin, pressure)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutMode;
    use crate::lens::ProjectionLens;
    use crate::member::Provenance;

    fn origin_for_test(member: &u64) -> Origin {
        match member {
            1..=5 => "example.com".to_string(),
            6..=10 => "other.org".to_string(),
            _ => "unknown".to_string(),
        }
    }

    fn build_tree(members: &[(u64, Lifecycle)]) -> GraphTree<u64> {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);
        for &(id, lifecycle) in members {
            tree.apply(NavAction::Attach {
                member: id,
                provenance: Provenance::Anchor,
            });
            tree.apply(NavAction::SetLifecycle(id, lifecycle));
        }
        tree
    }

    #[test]
    fn group_by_origin_separates_correctly() {
        let tree = build_tree(&[
            (1, Lifecycle::Active),
            (2, Lifecycle::Warm),
            (6, Lifecycle::Cold),
        ]);
        let groups = group_by_origin(&tree, &origin_for_test);
        assert_eq!(groups.len(), 2);

        let example = groups.iter().find(|g| g.origin == "example.com").unwrap();
        assert_eq!(example.active_members.len(), 1);
        assert_eq!(example.warm_members.len(), 1);
        assert_eq!(example.cold_members.len(), 0);

        let other = groups.iter().find(|g| g.origin == "other.org").unwrap();
        assert_eq!(other.cold_members.len(), 1);
    }

    #[test]
    fn per_origin_budget_enforced() {
        // 5 warm members from example.com, budget is 3
        let tree = build_tree(&[
            (1, Lifecycle::Warm),
            (2, Lifecycle::Warm),
            (3, Lifecycle::Warm),
            (4, Lifecycle::Warm),
            (5, Lifecycle::Warm),
        ]);
        let policy = MemoryPolicy {
            global_warm_budget: 100,
            per_origin_warm_budget: 3,
            exempt_origins: vec![],
        };
        // Lower id = older (touched earlier)
        let last_touched = |m: &u64| *m;

        let eval = evaluate(&tree, &policy, &origin_for_test, &last_touched);

        assert_eq!(eval.recommended_demotions.len(), 2);
        assert!(
            eval.over_budget_origins
                .contains(&"example.com".to_string())
        );

        // Should demote members 1 and 2 (oldest)
        let demoted: Vec<u64> = eval
            .recommended_demotions
            .iter()
            .filter_map(|a| match a {
                NavAction::SetLifecycle(m, Lifecycle::Cold) => Some(*m),
                _ => None,
            })
            .collect();
        assert!(demoted.contains(&1));
        assert!(demoted.contains(&2));
    }

    #[test]
    fn exempt_origins_are_preserved() {
        let tree = build_tree(&[
            (1, Lifecycle::Warm),
            (2, Lifecycle::Warm),
            (3, Lifecycle::Warm),
            (4, Lifecycle::Warm),
            (5, Lifecycle::Warm),
        ]);
        let policy = MemoryPolicy {
            global_warm_budget: 100,
            per_origin_warm_budget: 2,
            exempt_origins: vec!["example.com".to_string()],
        };
        let last_touched = |m: &u64| *m;

        let eval = evaluate(&tree, &policy, &origin_for_test, &last_touched);
        assert!(eval.recommended_demotions.is_empty());
    }

    #[test]
    fn global_budget_triggers_cross_origin_demotion() {
        // 3 from example.com, 3 from other.org, global budget 4
        let tree = build_tree(&[
            (1, Lifecycle::Warm),
            (2, Lifecycle::Warm),
            (3, Lifecycle::Warm),
            (6, Lifecycle::Warm),
            (7, Lifecycle::Warm),
            (8, Lifecycle::Warm),
        ]);
        let policy = MemoryPolicy {
            global_warm_budget: 4,
            per_origin_warm_budget: 10,
            exempt_origins: vec![],
        };
        let last_touched = |m: &u64| *m;

        let eval = evaluate(&tree, &policy, &origin_for_test, &last_touched);
        assert!(eval.global_budget_exceeded);
        assert_eq!(eval.recommended_demotions.len(), 2);

        // Should demote 1 and 2 (oldest across all origins)
        let demoted: Vec<u64> = eval
            .recommended_demotions
            .iter()
            .filter_map(|a| match a {
                NavAction::SetLifecycle(m, Lifecycle::Cold) => Some(*m),
                _ => None,
            })
            .collect();
        assert!(demoted.contains(&1));
        assert!(demoted.contains(&2));
    }

    #[test]
    fn active_members_not_demoted_by_per_origin() {
        // 3 Active from example.com, budget is 2 — Active should not be touched
        let tree = build_tree(&[
            (1, Lifecycle::Active),
            (2, Lifecycle::Active),
            (3, Lifecycle::Active),
        ]);
        let policy = MemoryPolicy {
            global_warm_budget: 100,
            per_origin_warm_budget: 2,
            exempt_origins: vec![],
        };
        let last_touched = |m: &u64| *m;

        let eval = evaluate(&tree, &policy, &origin_for_test, &last_touched);
        // warm_pressure is 3 (Active counts), budget is 2, excess is 1.
        // But only Warm members are candidates for demotion, and there are none.
        assert!(eval.recommended_demotions.is_empty());
    }

    #[test]
    fn cold_sweep_origin_works() {
        let tree = build_tree(&[
            (1, Lifecycle::Active),
            (2, Lifecycle::Warm),
            (3, Lifecycle::Cold),
            (6, Lifecycle::Active),
        ]);
        let actions = cold_sweep_origin(&tree, &origin_for_test, "example.com");
        assert_eq!(actions.len(), 2); // members 1 and 2 (3 is already Cold)

        let swept: Vec<u64> = actions
            .iter()
            .filter_map(|a| match a {
                NavAction::SetLifecycle(m, Lifecycle::Cold) => Some(*m),
                _ => None,
            })
            .collect();
        assert!(swept.contains(&1));
        assert!(swept.contains(&2));
        assert!(!swept.contains(&6)); // different origin
    }
}
