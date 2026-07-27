// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The three memory levels' read-model: classifying a node as short-term or
//! long-term, and computing which short-term nodes an eviction policy would drop.
//!
//! This is the spine of Alembic slice C (the memory model). It is pure logic over
//! a graph's persisted nodes plus per-node last-visit timing the host supplies, so
//! it is fully testable without a live graph, store, or clock. The host wiring (the
//! Alembic Recent / Saved sections rendering over this, the policy as a setting, the
//! actual eviction pass) layers on top once it lands.
//!
//! The model (Alembic plan, decision #2, confirmed with Mark): **a tag or a pin
//! promotes a node to long-term**, retained and never evicted. Everything else is
//! short-term working memory, subject to the eviction policy. Engrams (slice A) are
//! the third level and are not nodes, so they are out of scope here.

use std::collections::HashMap;

use kernel::graph::{Graph, NodeKey};

/// A node's memory level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryLevel {
    /// Working memory: auto-captured from browsing, evictable by default.
    ShortTerm,
    /// Promoted memory: kept across sessions because the user tagged or pinned it.
    /// Never evicted by any policy.
    LongTerm,
}

/// Whether a node has been promoted to long-term. Promotion is an affirmative act:
/// at least one tag (decision #2 — "tagging adds to long-term, retained"). `is_pinned`
/// is deliberately *not* a promotion signal: it pins a node's physics position, an
/// orthogonal spatial choice, not a memory-keep. (A dedicated bookmark flag, if one is
/// added later, would join `tags` here.)
pub fn is_promoted(graph: &Graph, key: NodeKey) -> bool {
    graph
        .get_node(key)
        .is_some_and(|node| !node.tags.is_empty())
}

/// The memory level of a node.
pub fn level_of(graph: &Graph, key: NodeKey) -> MemoryLevel {
    if is_promoted(graph, key) {
        MemoryLevel::LongTerm
    } else {
        MemoryLevel::ShortTerm
    }
}

/// The eviction policy for short-term memory. User-overridable and shown in the
/// Alembic pane (never a silent default). Long-term (promoted) nodes are never
/// evicted by any policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EvictionPolicy {
    /// Never evict short-term memory (the explicit keep-everything choice).
    KeepForever,
    /// Evict short-term nodes whose last visit is older than `days` days.
    KeepDays(u32),
    /// Evict short-term nodes not visited in the last `sessions` app launches, per
    /// the graph's `visit.history` facet.
    /// A node never stamped (`last_session_visited == 0`, e.g. never re-visited since
    /// this field shipped) is left alone — undated, like an unset `last_visit_ms`.
    KeepSessions(u32),
}

impl Default for EvictionPolicy {
    /// A visible, conservative default: keep a month of working memory.
    fn default() -> Self {
        EvictionPolicy::KeepDays(30)
    }
}

impl EvictionPolicy {
    /// One-line description for the Recent section header (the visible policy).
    pub fn describe(&self) -> String {
        match self {
            EvictionPolicy::KeepForever => "keeping all recent memory".to_string(),
            EvictionPolicy::KeepDays(d) => format!("evicting recent memory after {d} day(s)"),
            EvictionPolicy::KeepSessions(n) => {
                format!("evicting recent memory after {n} session(s)")
            }
        }
    }

    /// The next policy when the user cycles the control in the Recent header: a small
    /// curated ladder `7d -> 30d -> 90d -> forever -> 7d`. An arbitrary `KeepDays(n)`
    /// snaps to the next rung above it. [`KeepSessions`](Self::KeepSessions) is not on
    /// this ladder yet (no UI sets it), so cycling away from it returns to the ladder's
    /// start rather than looping in place. (Editable eviction policy.)
    pub fn cycled(self) -> EvictionPolicy {
        match self {
            EvictionPolicy::KeepDays(d) if d < 30 => EvictionPolicy::KeepDays(30),
            EvictionPolicy::KeepDays(d) if d < 90 => EvictionPolicy::KeepDays(90),
            EvictionPolicy::KeepDays(_) => EvictionPolicy::KeepForever,
            EvictionPolicy::KeepForever => EvictionPolicy::KeepDays(7),
            EvictionPolicy::KeepSessions(_) => EvictionPolicy::KeepDays(7),
        }
    }

    /// Whether `node` is stale under this policy: by-time against `last_visit_ms` +
    /// `now_ms`, or by-session against the graph's visit-history facet +
    /// `current_session`.
    /// Both axes share the same rule — a node with no recorded timing (no visit entry,
    /// or `last_session_visited == 0`) is never stale; we don't drop what we can't date.
    fn is_stale(
        &self,
        graph: &Graph,
        key: NodeKey,
        last_visit_ms: &HashMap<String, u64>,
        now_ms: u64,
        current_session: u64,
    ) -> bool {
        match self {
            EvictionPolicy::KeepForever => false,
            EvictionPolicy::KeepDays(days) => {
                let cutoff = now_ms.saturating_sub(u64::from(*days) * 86_400_000);
                let Some(node_id) = graph.get_node(key).map(|node| node.id.to_string()) else {
                    return false;
                };
                last_visit_ms
                    .get(&node_id)
                    .is_some_and(|&visited| visited < cutoff)
            }
            EvictionPolicy::KeepSessions(sessions) => {
                let last_session_visited = graph.node_last_session_visited(key).unwrap_or_default();
                last_session_visited != 0
                    && current_session.saturating_sub(last_session_visited) >= u64::from(*sessions)
            }
        }
    }
}

/// The node ids of the short-term nodes `policy` would evict as of `now_ms` /
/// `current_session`, given each node's last-visit time (`last_visit_ms`, keyed by
/// `node_id`, supplied by the host from the graph's navigation history) for the
/// by-time policies, or the node's own `last_session_visited` stamp for
/// [`KeepSessions`](EvictionPolicy::KeepSessions).
///
/// A node is evictable only if it is short-term **and** stale under the policy (see
/// [`EvictionPolicy::is_stale`]) — a node with no recorded timing on the relevant axis
/// is left alone (we never drop what we cannot date), and promoted nodes are never
/// returned. The result is the set the host passes to an eviction pass; this function
/// decides *what*, never performs it.
pub fn evictable_short_term(
    graph: &Graph,
    last_visit_ms: &HashMap<String, u64>,
    policy: EvictionPolicy,
    now_ms: u64,
    current_session: u64,
) -> Vec<String> {
    graph
        .nodes()
        .filter(|(key, _)| level_of(graph, *key) == MemoryLevel::ShortTerm)
        .filter(|(key, _)| policy.is_stale(graph, *key, last_visit_ms, now_ms, current_session))
        .map(|(_, node)| node.id.to_string())
        .collect()
}

/// A census of a graph's memory levels, for the Alembic pane to show real counts
/// rather than a grounded guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MemoryCensus {
    /// Short-term (working) nodes.
    pub short_term: usize,
    /// Long-term (promoted) nodes.
    pub long_term: usize,
    /// How many of the short-term nodes the current policy would evict now.
    pub evictable: usize,
}

/// Count the memory levels of `nodes` and how many the `policy` would evict now.
pub fn census(
    graph: &Graph,
    last_visit_ms: &HashMap<String, u64>,
    policy: EvictionPolicy,
    now_ms: u64,
    current_session: u64,
) -> MemoryCensus {
    let long_term = graph
        .nodes()
        .filter(|(key, _)| is_promoted(graph, *key))
        .count();
    MemoryCensus {
        short_term: graph.node_count() - long_term,
        long_term,
        evictable: evictable_short_term(graph, last_visit_ms, policy, now_ms, current_session)
            .len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::default::Point2D;
    use kernel::graph::fixtures::GraphFixtures;
    use kernel::graph::node_facets::{ARRANGEMENT_PIN, VISIT_HISTORY, VisitHistoryFacet};
    use uuid::Uuid;

    const DAY_MS: u64 = 86_400_000;

    fn node(graph: &mut Graph, id: &str) -> NodeKey {
        graph.add_node_with_id(
            Uuid::new_v5(&Uuid::NAMESPACE_URL, id.as_bytes()),
            format!("https://{id}.example"),
            Point2D::zero(),
        )
    }

    fn set_session(graph: &mut Graph, key: NodeKey, session: u64) {
        let id = graph.get_node(key).unwrap().id;
        graph
            .facets_mut()
            .set(
                id,
                chartulary::FacetId::new(VISIT_HISTORY),
                serde_json::to_value(VisitHistoryFacet {
                    last_visited_ms: None,
                    last_session_visited: session,
                })
                .unwrap(),
                &chartulary::AcceptAll,
            )
            .unwrap();
    }

    #[test]
    fn a_tag_promotes_to_long_term_but_a_position_pin_does_not() {
        let mut graph = Graph::new();
        let plain = node(&mut graph, "plain");
        let tagged = node(&mut graph, "tagged");
        let pinned = node(&mut graph, "pinned");
        graph.insert_node_tag(tagged, "keep".to_string());
        let pinned_id = graph.get_node(pinned).unwrap().id;
        graph
            .facets_mut()
            .set(
                pinned_id,
                chartulary::FacetId::new(ARRANGEMENT_PIN),
                serde_json::json!(true),
                &chartulary::AcceptAll,
            )
            .unwrap();

        assert_eq!(level_of(&graph, plain), MemoryLevel::ShortTerm);
        assert_eq!(level_of(&graph, tagged), MemoryLevel::LongTerm);
        assert_eq!(
            level_of(&graph, pinned),
            MemoryLevel::ShortTerm,
            "a position-pin is orthogonal to memory level",
        );
    }

    #[test]
    fn keep_forever_evicts_nothing() {
        let mut graph = Graph::new();
        node(&mut graph, "a");
        node(&mut graph, "b");
        let mut times = HashMap::new();
        times.insert("a".to_string(), 0u64); // ancient, but policy keeps all
        let out =
            evictable_short_term(&graph, &times, EvictionPolicy::KeepForever, 100 * DAY_MS, 0);
        assert!(out.is_empty(), "KeepForever never evicts");
    }

    #[test]
    fn keep_days_evicts_only_stale_undated_safe_and_promoted_exempt() {
        let now = 100 * DAY_MS;
        let mut graph = Graph::new();
        let stale = node(&mut graph, "stale");
        let fresh = node(&mut graph, "fresh");
        node(&mut graph, "undated");
        let promoted = node(&mut graph, "promoted");
        graph.insert_node_tag(promoted, "keep".to_string());

        let mut times = HashMap::new();
        times.insert(
            graph.get_node(stale).unwrap().id.to_string(),
            now - 40 * DAY_MS,
        );
        times.insert(
            graph.get_node(fresh).unwrap().id.to_string(),
            now - 5 * DAY_MS,
        );
        times.insert(
            graph.get_node(promoted).unwrap().id.to_string(),
            now - 90 * DAY_MS,
        );
        // "undated" has no visit record -> never dropped (we don't drop what we can't date)

        let out = evictable_short_term(&graph, &times, EvictionPolicy::KeepDays(30), now, 0);
        assert_eq!(
            out,
            vec![graph.get_node(stale).unwrap().id.to_string()],
            "only the dated, stale, un-promoted node"
        );
    }

    #[test]
    fn keep_sessions_evicts_only_stale_unstamped_safe_and_promoted_exempt() {
        let current_session = 10u64;
        let mut graph = Graph::new();
        let stale = node(&mut graph, "stale");
        let fresh = node(&mut graph, "fresh");
        node(&mut graph, "unstamped");
        let promoted = node(&mut graph, "promoted");
        set_session(&mut graph, stale, 2);
        set_session(&mut graph, fresh, 9);
        set_session(&mut graph, promoted, 1);
        graph.insert_node_tag(promoted, "keep".to_string());

        let out = evictable_short_term(
            &graph,
            &HashMap::new(),
            EvictionPolicy::KeepSessions(3),
            0,
            current_session,
        );
        assert_eq!(
            out,
            vec![graph.get_node(stale).unwrap().id.to_string()],
            "only the dated, stale, un-promoted node"
        );
    }

    #[test]
    fn census_counts_levels_and_evictable() {
        let now = 100 * DAY_MS;
        let mut graph = Graph::new();
        let s1 = node(&mut graph, "s1");
        let s2 = node(&mut graph, "s2");
        let kept = node(&mut graph, "kept");
        graph.insert_node_tag(kept, "keep".to_string());

        let mut times = HashMap::new();
        times.insert(
            graph.get_node(s1).unwrap().id.to_string(),
            now - 60 * DAY_MS,
        );
        times.insert(graph.get_node(s2).unwrap().id.to_string(), now - 2 * DAY_MS);

        let c = census(&graph, &times, EvictionPolicy::KeepDays(30), now, 0);
        assert_eq!(c.short_term, 2);
        assert_eq!(c.long_term, 1);
        assert_eq!(c.evictable, 1, "only s1 is stale enough");
    }

    #[test]
    fn default_policy_is_a_visible_thirty_days() {
        assert_eq!(EvictionPolicy::default(), EvictionPolicy::KeepDays(30));
        assert!(EvictionPolicy::default().describe().contains("30"));
    }

    #[test]
    fn eviction_policy_cycles_through_the_ladder() {
        use EvictionPolicy::*;
        assert_eq!(KeepDays(7).cycled(), KeepDays(30));
        assert_eq!(KeepDays(30).cycled(), KeepDays(90));
        assert_eq!(KeepDays(90).cycled(), KeepForever);
        assert_eq!(KeepForever.cycled(), KeepDays(7));
        // The default (30d) advances to 90d.
        assert_eq!(EvictionPolicy::default().cycled(), KeepDays(90));
    }
}
