/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Athanor's forgetting pass: propose which short-term cached content to evict, and
//! apply an accepted proposal by dropping that content.
//!
//! Athanor is the distillation daemon (Alembic plan slice D) — eventually an
//! [armillary](https://crates.io/crates/armillary) actor running steady-heat in the
//! background. This module is its pure pass *logic*, separate from the actor that
//! will schedule it, so it tests without a runtime. It honours eidetic **R0**: the
//! `propose_*` half is pure (reads a snapshot, mutates nothing), and the `apply_*`
//! half drops only short-term **cached content** (`content_store` blobs), never graph
//! truth (nodes / edges stay) and never engrams (immutable). Forgetting is the one
//! place blobs are dropped, and it drops only auto-captured short-term memory.
//!
//! Consolidation and facet extraction are Athanor's other passes (graph engrams are
//! already deduplicated by content-addressing, so consolidation is light); they land
//! as later passes alongside this one.

use std::collections::{HashMap, HashSet};

use eidetic::{Result, Store};
use kernel::persistence::GraphSnapshot;

use crate::content_store;
use crate::memory_levels::{EvictionPolicy, evictable_short_term};

/// A proposal to forget (evict the cached content of) a set of short-term nodes.
///
/// Athanor emits this; the host decides whether to apply it (the R0 propose/apply
/// split). It names urls, not nodes: forgetting drops cached **content**, leaving the
/// graph node in place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForgetProposal {
    /// The urls whose cached content is eligible for eviction.
    pub urls: Vec<String>,
}

impl ForgetProposal {
    pub fn is_empty(&self) -> bool {
        self.urls.is_empty()
    }

    pub fn len(&self) -> usize {
        self.urls.len()
    }
}

/// Propose forgetting: the urls of the short-term nodes `policy` would evict as of
/// `now_ms` / `current_session`, given per-node last-visit timing (`last_visit_ms`,
/// keyed by `node_id`, supplied by the host from the graph's navigation history) for
/// the by-time policies, or each node's own `last_session_visited` stamp for
/// [`KeepSessions`](EvictionPolicy::KeepSessions).
///
/// Pure: it reads `snapshot` and mutates nothing (R0). The eviction decision is
/// [`evictable_short_term`](crate::memory_levels::evictable_short_term) (short-term
/// only, dated-and-stale only, promoted exempt); this maps the evictable node ids to
/// their urls, since cached content is url-keyed.
pub fn propose_forgetting(
    snapshot: &GraphSnapshot,
    last_visit_ms: &HashMap<String, u64>,
    policy: EvictionPolicy,
    now_ms: u64,
    current_session: u64,
) -> ForgetProposal {
    let evictable: HashSet<String> =
        evictable_short_term(&snapshot.nodes, last_visit_ms, policy, now_ms, current_session)
            .into_iter()
            .collect();
    let urls = snapshot
        .nodes
        .iter()
        .filter(|node| evictable.contains(&node.node_id))
        .map(|node| node.url.clone())
        .filter(|url| !url.is_empty())
        .collect();
    ForgetProposal { urls }
}

/// Apply a forget proposal: drop the cached content for each url. Returns how many
/// urls had content actually removed (genuine feedback, never a placebo).
///
/// Drops only short-term cached blobs via [`content_store::evict_content`] (R0: not
/// graph truth, not engrams). An accepted proposal the host hands back here.
pub async fn apply_forgetting(store: &mut dyn Store, proposal: &ForgetProposal) -> Result<usize> {
    let mut dropped = 0;
    for url in &proposal.urls {
        if content_store::evict_content(store, url).await? {
            dropped += 1;
        }
    }
    Ok(dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_store::{StoredContent, save_content};
    use async_trait::async_trait;
    use euclid::default::Point2D;
    use kernel::graph::Graph;

    const DAY_MS: u64 = 86_400_000;

    /// An empty snapshot with three test nodes (stale / fresh / kept), built through
    /// the real graph API so node ids and urls are valid. "kept" is tagged (long-term).
    fn sample_snapshot() -> GraphSnapshot {
        let mut graph = Graph::new();
        graph.add_node("https://stale.example/".to_string(), Point2D::new(0.0, 0.0));
        graph.add_node("https://fresh.example/".to_string(), Point2D::new(0.0, 0.0));
        graph.add_node("https://kept.example/".to_string(), Point2D::new(0.0, 0.0));
        let mut snapshot = graph.to_snapshot();
        for node in &mut snapshot.nodes {
            if node.url.contains("kept") {
                node.tags.push("keep".to_string());
            }
        }
        snapshot
    }

    fn id_of(snapshot: &GraphSnapshot, fragment: &str) -> String {
        snapshot
            .nodes
            .iter()
            .find(|n| n.url.contains(fragment))
            .expect("a node with that url fragment")
            .node_id
            .clone()
    }

    #[test]
    fn proposes_only_stale_short_term_urls() {
        let now = 100 * DAY_MS;
        let snapshot = sample_snapshot();
        let mut times = HashMap::new();
        times.insert(id_of(&snapshot, "stale"), now - 60 * DAY_MS); // stale -> propose
        times.insert(id_of(&snapshot, "fresh"), now - 2 * DAY_MS); // fresh -> keep
        times.insert(id_of(&snapshot, "kept"), now - 90 * DAY_MS); // stale but tagged -> exempt

        let proposal = propose_forgetting(&snapshot, &times, EvictionPolicy::KeepDays(30), now, 0);
        assert_eq!(proposal.len(), 1, "only the stale short-term node");
        assert!(proposal.urls[0].contains("stale"));
    }

    #[test]
    fn proposes_only_stale_short_term_urls_by_session() {
        let snapshot = sample_snapshot();
        let stale_id = id_of(&snapshot, "stale");
        let fresh_id = id_of(&snapshot, "fresh");
        let kept_id = id_of(&snapshot, "kept");
        let mut snapshot = snapshot;
        for node in &mut snapshot.nodes {
            if node.node_id == stale_id {
                node.last_session_visited = 2; // 8 sessions ago -> propose
            } else if node.node_id == fresh_id {
                node.last_session_visited = 9; // 1 session ago -> keep
            } else if node.node_id == kept_id {
                node.last_session_visited = 1; // stale but tagged -> exempt
            }
        }

        let proposal = propose_forgetting(
            &snapshot,
            &HashMap::new(),
            EvictionPolicy::KeepSessions(3),
            0,
            10,
        );
        assert_eq!(proposal.len(), 1, "only the stale short-term node");
        assert!(proposal.urls[0].contains("stale"));
    }

    #[derive(Default)]
    struct MemStore {
        blobs: HashMap<String, Vec<u8>>,
    }

    #[async_trait(?Send)]
    impl Store for MemStore {
        async fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.blobs.get(key).cloned())
        }
        async fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()> {
            self.blobs.insert(key.to_string(), value.to_vec());
            Ok(())
        }
        async fn delete_blob(&mut self, key: &str) -> Result<bool> {
            Ok(self.blobs.remove(key).is_some())
        }
    }

    #[test]
    fn apply_drops_proposed_content_and_leaves_the_rest() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let page = |b: &str| StoredContent { content_type: None, body: b.as_bytes().to_vec() };
            save_content(&mut store, "https://stale.example/", &page("old")).await.unwrap();
            save_content(&mut store, "https://fresh.example/", &page("new")).await.unwrap();

            let proposal = ForgetProposal { urls: vec!["https://stale.example/".to_string()] };
            let dropped = apply_forgetting(&mut store, &proposal).await.unwrap();
            assert_eq!(dropped, 1, "one url's content removed");

            assert!(
                content_store::load_content(&mut store, "https://stale.example/")
                    .await
                    .unwrap()
                    .is_none(),
                "the forgotten content is gone",
            );
            assert!(
                content_store::load_content(&mut store, "https://fresh.example/")
                    .await
                    .unwrap()
                    .is_some(),
                "un-proposed content is untouched",
            );
        });
    }
}
