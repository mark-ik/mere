// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

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
//!
//! Consolidation (this module's second pass) relates graph engrams that are
//! successive versions of the same material — significant URL overlap — but carry
//! no lineage link yet. Composing is the *only* way eidetic offers to link two
//! already-saved manifests (they are immutable; there is no post-hoc amendment), so
//! `apply_consolidation` reuses [`graph_engram::compose_graph_engrams`]. That is
//! content-addressed, so re-linking an already-consolidated pair on a later pass is
//! a safe no-op (same bytes, same id) rather than a duplicate.

use std::collections::{HashMap, HashSet};

use eidetic::{ManifestId, NoFetcher, ProvenanceOrigin, Result, Store, Timestamp, load_typed};
use kernel::persistence::GraphSnapshot;

use crate::content_store;
use crate::graph_engram::{self, GraphEngram, RedactionPolicy};
use crate::memory_levels::{EvictionPolicy, evictable_short_term};

/// How many of the most-recently-created engrams a consolidation pass considers.
/// Bounds the pairwise scan (thaw is per-candidate, comparison is per-pair) as the
/// store grows; revisit if engrams get numerous enough for this to matter (like B6).
const CONSOLIDATION_CANDIDATE_CAP: usize = 50;

/// The minimum URL-overlap fraction (of the smaller engram's url set) for a pair to
/// count as "the same material" rather than coincidental overlap (e.g. two unrelated
/// sessions that both visited one common site). A first-cut heuristic; tune once
/// real consolidation proposals can be eyeballed.
const SAME_MATERIAL_OVERLAP: f64 = 0.5;

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
    let evictable: HashSet<String> = evictable_short_term(
        &snapshot.nodes,
        last_visit_ms,
        policy,
        now_ms,
        current_session,
    )
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

/// A proposal to consolidate: pairs of graph engrams that look like successive
/// versions of the same material but carry no lineage link yet.
///
/// Athanor emits this; the host decides whether to apply it (the R0 propose/apply
/// split). Applying a pair composes it (the only linking mechanism eidetic offers),
/// so `apply_consolidation` names it explicitly rather than leaving it implicit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsolidationProposal {
    /// Each pair to relate. Composing a pair links them: the new engram's
    /// `upstream` names both.
    pub pairs: Vec<(ManifestId, ManifestId)>,
}

impl ConsolidationProposal {
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }
}

/// Propose consolidation: pairs of graph engrams whose node-url sets overlap enough
/// to be "the same material" at different points in time, and that no existing
/// engram already relates (no manifest's `upstream` already names both).
///
/// Reads the store — lists manifests (cheap; `provenance.upstream` rides on the
/// manifest, no thaw needed to check existing links) and thaws each of the newest
/// [`CONSOLIDATION_CANDIDATE_CAP`] *originally-generated* engrams once to compare url
/// sets — but writes nothing (R0). Only `Generated`-origin engrams are candidates:
/// composing a `Derived` (already-consolidated) engram into another would keep
/// re-merging composites rather than relating fresh version chains.
pub async fn propose_consolidation(store: &mut dyn Store) -> Result<ConsolidationProposal> {
    let mut manifests = graph_engram::list_graph_engrams(store).await?;
    manifests.sort_by_key(|m| std::cmp::Reverse(m.created_at.0));

    let already_linked = |a: ManifestId, b: ManifestId| {
        manifests
            .iter()
            .any(|m| m.provenance.upstream.contains(&a) && m.provenance.upstream.contains(&b))
    };

    let mut fetcher = NoFetcher;
    let mut candidates: Vec<(ManifestId, HashSet<String>)> = Vec::new();
    for manifest in manifests
        .iter()
        .filter(|m| m.provenance.origin == ProvenanceOrigin::Generated)
        .take(CONSOLIDATION_CANDIDATE_CAP)
    {
        let Some(engram) = load_typed::<GraphEngram>(store, &mut fetcher, manifest.id).await?
        else {
            continue;
        };
        let urls: HashSet<String> = engram
            .0
            .nodes
            .iter()
            .map(|n| n.url.clone())
            .filter(|u| !u.is_empty())
            .collect();
        candidates.push((manifest.id, urls));
    }

    let mut pairs = Vec::new();
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            let (id_a, urls_a) = &candidates[i];
            let (id_b, urls_b) = &candidates[j];
            let smaller = urls_a.len().min(urls_b.len());
            if smaller == 0 || already_linked(*id_a, *id_b) {
                continue;
            }
            let overlap = urls_a.intersection(urls_b).count();
            if (overlap as f64 / smaller as f64) >= SAME_MATERIAL_OVERLAP {
                pairs.push((*id_a, *id_b));
            }
        }
    }
    Ok(ConsolidationProposal { pairs })
}

/// Apply a consolidation proposal: compose each pair, linking them (R0: an accepted
/// proposal the host hands back). Returns how many pairs composed successfully —
/// genuine feedback, never a placebo, even though a pair already consolidated on a
/// prior pass re-composes to the same content-addressed id at no real cost.
pub async fn apply_consolidation(
    store: &mut dyn Store,
    proposal: &ConsolidationProposal,
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<usize> {
    let mut linked = 0;
    for &(a, b) in &proposal.pairs {
        if graph_engram::compose_graph_engrams(store, &[a, b], redaction, created_at)
            .await?
            .is_some()
        {
            linked += 1;
        }
    }
    Ok(linked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_store::{StoredContent, save_content};
    use async_trait::async_trait;
    use euclid::default::Point2D;
    use kernel::graph::Graph;
    use kernel::graph::fixtures::GraphFixtures;

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

    // The in-memory test store is muniment's (2026-07-12): the
// hand-rolled one was the same map behind the same seam.
use muniment::MemoryBackend as MemStore;




    #[test]
    fn apply_drops_proposed_content_and_leaves_the_rest() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let page = |b: &str| StoredContent {
                content_type: None,
                body: b.as_bytes().to_vec(),
            };
            save_content(&mut store, "https://stale.example/", &page("old"))
                .await
                .unwrap();
            save_content(&mut store, "https://fresh.example/", &page("new"))
                .await
                .unwrap();

            let proposal = ForgetProposal {
                urls: vec!["https://stale.example/".to_string()],
            };
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

    /// Save a graph engram over exactly `urls`, oldest-first by node insertion.
    async fn save_engram(store: &mut MemStore, urls: &[&str], at_ms: u64) -> ManifestId {
        let mut graph = Graph::new();
        for (i, url) in urls.iter().enumerate() {
            graph.add_node(url.to_string(), Point2D::new(i as f32, 0.0));
        }
        graph_engram::save_graph_engram(store, &graph, RedactionPolicy::default(), Timestamp(at_ms))
            .await
            .expect("save")
    }

    #[test]
    fn propose_consolidation_finds_overlapping_unlinked_engrams() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let a = save_engram(&mut store, &["https://x.example", "https://y.example"], 1).await;
            let b = save_engram(&mut store, &["https://y.example", "https://z.example"], 2).await;
            save_engram(&mut store, &["https://q.example"], 3).await; // unrelated

            let proposal = propose_consolidation(&mut store).await.expect("propose");
            assert_eq!(proposal.len(), 1, "only the overlapping pair is proposed");
            let (p_a, p_b) = proposal.pairs[0];
            assert!(
                (p_a == a && p_b == b) || (p_a == b && p_b == a),
                "the proposed pair is a and b",
            );
        });
    }

    #[test]
    fn propose_consolidation_ignores_low_overlap_pairs() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            // a and b both have 4 urls, sharing only 1 -> 1/4 = 0.25 of the smaller
            // (here, either) set, below the 0.5 threshold.
            save_engram(
                &mut store,
                &[
                    "https://1.example",
                    "https://2.example",
                    "https://3.example",
                    "https://4.example",
                ],
                1,
            )
            .await;
            save_engram(
                &mut store,
                &[
                    "https://4.example",
                    "https://5.example",
                    "https://6.example",
                    "https://7.example",
                ],
                2,
            )
            .await;

            let proposal = propose_consolidation(&mut store).await.expect("propose");
            assert!(
                proposal.is_empty(),
                "a lone shared url is not enough overlap"
            );
        });
    }

    #[test]
    fn apply_consolidation_composes_and_records_the_upstream_link() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let a = save_engram(&mut store, &["https://x.example", "https://y.example"], 1).await;
            let b = save_engram(&mut store, &["https://y.example", "https://z.example"], 2).await;

            let proposal = propose_consolidation(&mut store).await.expect("propose");
            assert_eq!(proposal.len(), 1);

            let linked = apply_consolidation(
                &mut store,
                &proposal,
                RedactionPolicy::default(),
                Timestamp(3),
            )
            .await
            .expect("apply");
            assert_eq!(linked, 1, "one pair composed");

            let manifests = graph_engram::list_graph_engrams(&mut store)
                .await
                .expect("list");
            assert!(
                manifests
                    .iter()
                    .any(|m| m.provenance.upstream.contains(&a)
                        && m.provenance.upstream.contains(&b)),
                "a new engram links a and b via upstream",
            );
        });
    }

    #[test]
    fn propose_consolidation_skips_a_pair_already_linked_by_a_prior_pass() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            save_engram(&mut store, &["https://x.example", "https://y.example"], 1).await;
            save_engram(&mut store, &["https://y.example", "https://z.example"], 2).await;

            let first = propose_consolidation(&mut store).await.expect("propose");
            apply_consolidation(&mut store, &first, RedactionPolicy::default(), Timestamp(3))
                .await
                .expect("apply");

            // The two source engrams are still Generated (compose never mutates its
            // sources) and still overlap exactly as before, but a linking engram
            // now names both -- a second pass must not re-propose them.
            let second = propose_consolidation(&mut store)
                .await
                .expect("propose again");
            assert!(
                second.is_empty(),
                "already-linked pairs are not re-proposed: {:?}",
                second.pairs,
            );
        });
    }
}
