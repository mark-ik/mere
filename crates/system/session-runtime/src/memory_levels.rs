/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

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

use kernel::persistence::PersistedNode;

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
pub fn is_promoted(node: &PersistedNode) -> bool {
    !node.tags.is_empty()
}

/// The memory level of a node.
pub fn level_of(node: &PersistedNode) -> MemoryLevel {
    if is_promoted(node) {
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
    /// [`PersistedNode::last_session_visited`](kernel::persistence::PersistedNode::last_session_visited).
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
    /// `now_ms`, or by-session against `node.last_session_visited` + `current_session`.
    /// Both axes share the same rule — a node with no recorded timing (no visit entry,
    /// or `last_session_visited == 0`) is never stale; we don't drop what we can't date.
    fn is_stale(
        &self,
        node: &PersistedNode,
        last_visit_ms: &HashMap<String, u64>,
        now_ms: u64,
        current_session: u64,
    ) -> bool {
        match self {
            EvictionPolicy::KeepForever => false,
            EvictionPolicy::KeepDays(days) => {
                let cutoff = now_ms.saturating_sub(u64::from(*days) * 86_400_000);
                last_visit_ms
                    .get(&node.node_id)
                    .is_some_and(|&visited| visited < cutoff)
            }
            EvictionPolicy::KeepSessions(sessions) => {
                node.last_session_visited != 0
                    && current_session.saturating_sub(node.last_session_visited)
                        >= u64::from(*sessions)
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
    nodes: &[PersistedNode],
    last_visit_ms: &HashMap<String, u64>,
    policy: EvictionPolicy,
    now_ms: u64,
    current_session: u64,
) -> Vec<String> {
    nodes
        .iter()
        .filter(|node| level_of(node) == MemoryLevel::ShortTerm)
        .filter(|node| policy.is_stale(node, last_visit_ms, now_ms, current_session))
        .map(|node| node.node_id.clone())
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
    nodes: &[PersistedNode],
    last_visit_ms: &HashMap<String, u64>,
    policy: EvictionPolicy,
    now_ms: u64,
    current_session: u64,
) -> MemoryCensus {
    let long_term = nodes.iter().filter(|n| is_promoted(n)).count();
    MemoryCensus {
        short_term: nodes.len() - long_term,
        long_term,
        evictable: evictable_short_term(nodes, last_visit_ms, policy, now_ms, current_session)
            .len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::persistence::{PersistedAddress, PersistedNodeSessionState};

    const DAY_MS: u64 = 86_400_000;

    /// A minimal short-term node (no tags, not pinned) with the given id.
    fn node(id: &str) -> PersistedNode {
        PersistedNode {
            node_id: id.to_string(),
            address: PersistedAddress::default(),
            url: format!("https://{id}.example"),
            cached_host: None,
            title: id.to_string(),
            body: None,
            tags: Vec::new(),
            tag_presentation: Default::default(),
            import_provenance: Vec::new(),
            is_pinned: false,
            thumbnail_png: None,
            thumbnail_width: 0,
            thumbnail_height: 0,
            favicon_rgba: None,
            favicon_width: 0,
            favicon_height: 0,
            session_state: None::<PersistedNodeSessionState>,
            mime_hint: None,
            classifications: Vec::new(),
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
            properties: Vec::new(),
            derivations: Vec::new(),
            last_session_visited: 0,
        }
    }

    #[test]
    fn a_tag_promotes_to_long_term_but_a_position_pin_does_not() {
        assert_eq!(level_of(&node("plain")), MemoryLevel::ShortTerm);

        let mut tagged = node("tagged");
        tagged.tags.push("keep".to_string());
        assert_eq!(level_of(&tagged), MemoryLevel::LongTerm, "a tag promotes");

        // is_pinned is a physics position-pin, not a memory-keep: it must not promote.
        let mut pinned = node("pinned");
        pinned.is_pinned = true;
        assert_eq!(
            level_of(&pinned),
            MemoryLevel::ShortTerm,
            "a position-pin is orthogonal to memory level",
        );
    }

    #[test]
    fn keep_forever_evicts_nothing() {
        let nodes = vec![node("a"), node("b")];
        let mut times = HashMap::new();
        times.insert("a".to_string(), 0u64); // ancient, but policy keeps all
        let out =
            evictable_short_term(&nodes, &times, EvictionPolicy::KeepForever, 100 * DAY_MS, 0);
        assert!(out.is_empty(), "KeepForever never evicts");
    }

    #[test]
    fn keep_days_evicts_only_stale_undated_safe_and_promoted_exempt() {
        let now = 100 * DAY_MS;
        let mut nodes = vec![node("stale"), node("fresh"), node("undated"), node("promoted")];
        nodes[3].tags.push("keep".to_string()); // promoted by a tag: exempt even if stale

        let mut times = HashMap::new();
        times.insert("stale".to_string(), now - 40 * DAY_MS); // older than 30d -> evict
        times.insert("fresh".to_string(), now - 5 * DAY_MS); // within 30d -> keep
        times.insert("promoted".to_string(), now - 90 * DAY_MS); // stale but promoted -> keep
        // "undated" has no visit record -> never dropped (we don't drop what we can't date)

        let out = evictable_short_term(&nodes, &times, EvictionPolicy::KeepDays(30), now, 0);
        assert_eq!(out, vec!["stale".to_string()], "only the dated, stale, un-promoted node");
    }

    #[test]
    fn keep_sessions_evicts_only_stale_unstamped_safe_and_promoted_exempt() {
        let current_session = 10u64;
        let mut nodes = vec![node("stale"), node("fresh"), node("unstamped"), node("promoted")];
        nodes[0].last_session_visited = 2; // 8 sessions ago, older than 3 -> evict
        nodes[1].last_session_visited = 9; // 1 session ago -> keep
        // "unstamped" keeps last_session_visited == 0 (never stamped) -> never dropped
        nodes[3].last_session_visited = 1; // stale but promoted -> keep
        nodes[3].tags.push("keep".to_string());

        let out = evictable_short_term(
            &nodes,
            &HashMap::new(),
            EvictionPolicy::KeepSessions(3),
            0,
            current_session,
        );
        assert_eq!(out, vec!["stale".to_string()], "only the dated, stale, un-promoted node");
    }

    #[test]
    fn census_counts_levels_and_evictable() {
        let now = 100 * DAY_MS;
        let mut nodes = vec![node("s1"), node("s2"), node("kept")];
        nodes[2].tags.push("keep".to_string()); // long-term

        let mut times = HashMap::new();
        times.insert("s1".to_string(), now - 60 * DAY_MS); // evictable
        times.insert("s2".to_string(), now - 2 * DAY_MS); // fresh

        let c = census(&nodes, &times, EvictionPolicy::KeepDays(30), now, 0);
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
