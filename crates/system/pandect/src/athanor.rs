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

use eidetic::{DeletedNode, ManifestId, ProvenanceOrigin, Result, Store, Timestamp, purge_deleted};
use kernel::graph::Graph;
use kernel::persistence::GraphSnapshot;
use kernel::types::ImageRole;

use crate::content_store;
use crate::graph_engram::{self, RedactionPolicy};
use crate::image_store;
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
    graph: &Graph,
    last_visit_ms: &HashMap<String, u64>,
    policy: EvictionPolicy,
    now_ms: u64,
    current_session: u64,
) -> ForgetProposal {
    let evictable: HashSet<String> =
        evictable_short_term(graph, last_visit_ms, policy, now_ms, current_session)
            .into_iter()
            .collect();
    let urls = graph
        .nodes()
        .filter(|(_, node)| evictable.contains(&node.id.to_string()))
        .map(|(_, node)| node.url().to_string())
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

/// Stored image blobs not named by any live node.
///
/// This is a mark/sweep proposal rather than a per-node deletion: image blobs
/// are content-addressed and shared, so one node dropping a reference cannot
/// decide that the blob itself is dead.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageGcProposal {
    pub hexes: Vec<String>,
}

impl ImageGcProposal {
    pub fn is_empty(&self) -> bool {
        self.hexes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.hexes.len()
    }
}

fn referenced_image_hexes(snapshot: &GraphSnapshot) -> HashSet<String> {
    snapshot
        .nodes
        .iter()
        .flat_map(|node| node.images.values())
        .map(|image| image.hex())
        .collect()
}

/// Diff the image store's inventory against every live node reference. Pure:
/// the caller supplies the inventory and this mutates neither graph nor store.
pub fn propose_image_gc(
    snapshot: &GraphSnapshot,
    stored_hexes: impl IntoIterator<Item = String>,
) -> ImageGcProposal {
    let referenced = referenced_image_hexes(snapshot);
    let mut hexes: Vec<String> = stored_hexes
        .into_iter()
        .filter(|hex| !referenced.contains(hex))
        .collect();
    hexes.sort();
    hexes.dedup();
    ImageGcProposal { hexes }
}

/// Apply an accepted image-GC proposal, re-checking the current snapshot
/// before each delete so a reference added after proposal creation wins.
pub async fn apply_image_gc(
    store: &mut dyn Store,
    snapshot: &GraphSnapshot,
    proposal: &ImageGcProposal,
) -> Result<usize> {
    let referenced = referenced_image_hexes(snapshot);
    let mut dropped = 0;
    for hex in &proposal.hexes {
        if !referenced.contains(hex) && image_store::delete_image_hex(store, hex).await? {
            dropped += 1;
        }
    }
    Ok(dropped)
}

/// A role-specific reference Athanor proposes forgetting from a stale
/// short-term node. The digest is included so apply can refuse a stale
/// proposal after a newer capture replaces the role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageReferenceDrop {
    pub node_id: String,
    pub role: ImageRole,
    pub hex: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageReferenceForgetProposal {
    pub drops: Vec<ImageReferenceDrop>,
}

/// Propose disposable image references for the same stale short-term nodes as
/// ordinary content forgetting. Favicons are re-fetchable; preview/snapshot
/// roles are retained because they may be the only offline representation.
/// Promoted nodes never enter `evictable_short_term`.
pub fn propose_image_reference_forgetting(
    graph: &Graph,
    last_visit_ms: &HashMap<String, u64>,
    policy: EvictionPolicy,
    now_ms: u64,
    current_session: u64,
) -> ImageReferenceForgetProposal {
    let evictable: HashSet<String> =
        evictable_short_term(graph, last_visit_ms, policy, now_ms, current_session)
            .into_iter()
            .collect();
    let drops = graph
        .nodes()
        .filter(|(_, node)| evictable.contains(&node.id.to_string()))
        .filter_map(|node| {
            let (_, node) = node;
            node.images
                .get(&ImageRole::Favicon)
                .map(|image| ImageReferenceDrop {
                    node_id: node.id.to_string(),
                    role: ImageRole::Favicon,
                    hex: image.hex(),
                })
        })
        .collect();
    ImageReferenceForgetProposal { drops }
}

/// Apply role-specific reference forgetting to a snapshot before it
/// materializes, re-checking both role and digest. Returns references removed.
/// The now-unreferenced blob is reclaimed by the separate GC pass.
pub fn apply_image_reference_forgetting(
    snapshot: &mut GraphSnapshot,
    proposal: &ImageReferenceForgetProposal,
) -> usize {
    let mut dropped = 0;
    for proposal in &proposal.drops {
        let Some(node) = snapshot
            .nodes
            .iter_mut()
            .find(|node| node.node_id == proposal.node_id)
        else {
            continue;
        };
        let still_same = node
            .images
            .get(&proposal.role)
            .is_some_and(|image| image.hex() == proposal.hex);
        if still_same {
            node.images.remove(&proposal.role);
            dropped += 1;
        }
    }
    dropped
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

    let mut candidates: Vec<(ManifestId, HashSet<String>)> = Vec::new();
    for manifest in manifests
        .iter()
        .filter(|m| m.provenance.origin == ProvenanceOrigin::Generated)
        .take(CONSOLIDATION_CANDIDATE_CAP)
    {
        let Some(engram) = graph_engram::load_graph_engram(store, manifest.id).await? else {
            continue;
        };
        let urls: HashSet<String> = engram
            .snapshot
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

/// A proposal to retire (permanently forget) staged deleted-node tombstones
/// whose time in the recycle bin has passed the retention window — the bin's
/// steady-heat auto-empty, the third of Athanor's passes.
///
/// Athanor emits it; the host applies it (the R0 propose/apply split, so a
/// "what will be forgotten" preview is possible before the oven runs). Unlike
/// the on-command "empty the bin" (which clears everything), retirement drops
/// only the aged-out subset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetirementProposal {
    /// The node ids whose tombstone(s) are old enough to purge, de-duplicated
    /// (a node deleted twice has two tombstones but one id).
    pub node_ids: Vec<String>,
}

impl RetirementProposal {
    pub fn is_empty(&self) -> bool {
        self.node_ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.node_ids.len()
    }
}

/// Propose retirement: the ids of tombstones deleted at or before
/// `now_ms - keep_ms` (the retention window). Pure (R0): it reads the listed
/// records ([`eidetic::list_deleted`]'s output) and mutates nothing.
pub fn propose_retirement(
    deleted: &[DeletedNode],
    keep_ms: u64,
    now_ms: u64,
) -> RetirementProposal {
    let cutoff = now_ms.saturating_sub(keep_ms);
    let mut node_ids: Vec<String> = deleted
        .iter()
        .filter(|d| d.deleted_at_ms <= cutoff)
        .map(|d| d.node_id.clone())
        .collect();
    node_ids.sort();
    node_ids.dedup();
    RetirementProposal { node_ids }
}

/// Apply a retirement proposal: permanently forget each proposed node
/// ([`eidetic::purge_deleted`], which drops all of a node's tombstones — R0:
/// an accepted proposal the host hands back). Returns how many tombstones were
/// removed, genuine feedback (a node with two tombstones removes both).
pub async fn apply_retirement(
    store: &mut dyn Store,
    proposal: &RetirementProposal,
) -> Result<usize> {
    let mut retired = 0;
    for node_id in &proposal.node_ids {
        retired += purge_deleted(store, node_id).await?;
    }
    Ok(retired)
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

    /// A graph with three test nodes (stale / fresh / kept), built through
    /// the real graph API so node ids and urls are valid. "kept" is tagged (long-term).
    fn sample_graph() -> Graph {
        let mut graph = Graph::new();
        graph.add_node("https://stale.example/".to_string(), Point2D::new(0.0, 0.0));
        graph.add_node("https://fresh.example/".to_string(), Point2D::new(0.0, 0.0));
        let kept = graph.add_node("https://kept.example/".to_string(), Point2D::new(0.0, 0.0));
        graph.insert_node_tag(kept, "keep".to_string());
        graph
    }

    fn id_of(graph: &Graph, fragment: &str) -> String {
        graph
            .nodes()
            .find(|(_, node)| node.url().contains(fragment))
            .expect("a node with that url fragment")
            .1
            .id
            .to_string()
    }

    #[test]
    fn proposes_only_stale_short_term_urls() {
        let now = 100 * DAY_MS;
        let graph = sample_graph();
        let mut times = HashMap::new();
        times.insert(id_of(&graph, "stale"), now - 60 * DAY_MS); // stale -> propose
        times.insert(id_of(&graph, "fresh"), now - 2 * DAY_MS); // fresh -> keep
        times.insert(id_of(&graph, "kept"), now - 90 * DAY_MS); // stale but tagged -> exempt

        let proposal = propose_forgetting(&graph, &times, EvictionPolicy::KeepDays(30), now, 0);
        assert_eq!(proposal.len(), 1, "only the stale short-term node");
        assert!(proposal.urls[0].contains("stale"));
    }

    #[test]
    fn proposes_only_stale_short_term_urls_by_session() {
        let mut graph = sample_graph();
        for (fragment, session) in [("stale", 2), ("fresh", 9), ("kept", 1)] {
            let key = graph
                .nodes()
                .find(|(_, node)| node.url().contains(fragment))
                .unwrap()
                .0;
            let id = graph.get_node(key).unwrap().id;
            graph
                .facets_mut()
                .set(
                    id,
                    chartulary::FacetId::new(kernel::graph::node_facets::VISIT_HISTORY),
                    serde_json::to_value(kernel::graph::VisitHistoryFacet {
                        last_visited_ms: None,
                        last_session_visited: session,
                    })
                    .unwrap(),
                    &chartulary::AcceptAll,
                )
                .unwrap();
        }

        let proposal = propose_forgetting(
            &graph,
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

    #[test]
    fn image_gc_marks_only_orphans_and_apply_rechecks_live_references() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let live = image_store::save_image(&mut store, b"live", 1, 1)
                .await
                .unwrap();
            let orphan = image_store::save_image(&mut store, b"orphan", 1, 1)
                .await
                .unwrap();
            let raced = image_store::save_image(&mut store, b"raced", 1, 1)
                .await
                .unwrap();
            let mut snapshot = sample_graph().to_snapshot();
            snapshot.nodes[0].images.insert(ImageRole::Favicon, live);

            let proposal = propose_image_gc(
                &snapshot,
                image_store::stored_image_hexes(&mut store).await.unwrap(),
            );
            assert_eq!(proposal.len(), 2);
            assert!(!proposal.hexes.contains(&live.hex()));
            assert!(proposal.hexes.contains(&orphan.hex()));
            assert!(proposal.hexes.contains(&raced.hex()));

            // A reference that appears after proposal creation wins at apply.
            snapshot.nodes[1].images.insert(ImageRole::Preview, raced);
            assert_eq!(
                apply_image_gc(&mut store, &snapshot, &proposal)
                    .await
                    .unwrap(),
                1
            );
            assert!(
                image_store::load_image(&mut store, &live)
                    .await
                    .unwrap()
                    .is_some()
            );
            assert!(
                image_store::load_image(&mut store, &raced)
                    .await
                    .unwrap()
                    .is_some()
            );
            assert!(
                image_store::load_image(&mut store, &orphan)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                apply_image_gc(&mut store, &snapshot, &proposal)
                    .await
                    .unwrap(),
                0,
                "re-running the accepted proposal is a no-op"
            );
        });
    }

    #[test]
    fn stale_short_term_drops_favicon_but_keeps_precious_images() {
        let now = 100 * DAY_MS;
        let mut graph = sample_graph();
        let stale_id = id_of(&graph, "stale");
        let kept_id = id_of(&graph, "kept");
        let favicon = kernel::types::ImageRef::new([1; 32], 1, 1);
        let snapshot_image = kernel::types::ImageRef::new([2; 32], 1, 1);
        for (_, node) in graph
            .nodes()
            .map(|(key, node)| (key, node.id))
            .collect::<Vec<_>>()
        {
            if node.to_string() == stale_id || node.to_string() == kept_id {
                let key = graph.get_node_key_by_id(node).unwrap();
                let node = graph.get_node_mut(key).unwrap();
                node.images.insert(ImageRole::Favicon, favicon);
                node.images.insert(ImageRole::Snapshot, snapshot_image);
            }
        }
        let times = HashMap::from([
            (stale_id.clone(), now - 60 * DAY_MS),
            (kept_id.clone(), now - 60 * DAY_MS),
        ]);

        let proposal = propose_image_reference_forgetting(
            &graph,
            &times,
            EvictionPolicy::KeepDays(30),
            now,
            0,
        );
        assert_eq!(proposal.drops.len(), 1, "the promoted node is exempt");
        assert_eq!(proposal.drops[0].node_id, stale_id);
        let mut snapshot = graph.to_snapshot();
        assert_eq!(
            apply_image_reference_forgetting(&mut snapshot, &proposal),
            1
        );

        let stale = snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == stale_id)
            .unwrap();
        assert!(!stale.images.contains_key(&ImageRole::Favicon));
        assert!(stale.images.contains_key(&ImageRole::Snapshot));
        let kept = snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == kept_id)
            .unwrap();
        assert!(kept.images.contains_key(&ImageRole::Favicon));
        assert!(kept.images.contains_key(&ImageRole::Snapshot));
        assert_eq!(
            apply_image_reference_forgetting(&mut snapshot, &proposal),
            0,
            "reference forgetting is idempotent"
        );
    }

    use eidetic::{list_deleted, record_deleted};

    fn deleted(node_id: &str, at_ms: u64) -> DeletedNode {
        DeletedNode {
            node_id: node_id.to_string(),
            url: format!("https://{node_id}.test"),
            title: None,
            tags: Vec::new(),
            graph_id: None,
            deleted_at_ms: at_ms,
            nested: None,
            facets: None,
        }
    }

    #[test]
    fn propose_retirement_names_only_the_aged_out_tombstones() {
        let now = 100 * DAY_MS;
        let records = [
            deleted("old", now - 40 * DAY_MS),  // 40d old -> retire
            deleted("fresh", now - 5 * DAY_MS), // 5d old -> keep
            deleted("edge", now - 30 * DAY_MS), // exactly the window -> retire (<=)
        ];
        let proposal = propose_retirement(&records, 30 * DAY_MS, now);
        assert_eq!(
            proposal.node_ids,
            vec!["edge".to_string(), "old".to_string()]
        );
        // A generous window keeps everything; a zero window keeps only the future.
        assert!(propose_retirement(&records, 365 * DAY_MS, now).is_empty());
    }

    #[test]
    fn apply_retirement_purges_the_aged_out_and_lists_the_survivors() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let now = 100 * DAY_MS;
            record_deleted(&mut store, &deleted("old", now - 40 * DAY_MS))
                .await
                .unwrap();
            record_deleted(&mut store, &deleted("fresh", now - 5 * DAY_MS))
                .await
                .unwrap();

            let listed = list_deleted(&mut store).await.unwrap();
            let proposal = propose_retirement(&listed, 30 * DAY_MS, now);
            let retired = apply_retirement(&mut store, &proposal).await.unwrap();
            assert_eq!(retired, 1, "only the aged-out tombstone purged");

            let survivors = list_deleted(&mut store).await.unwrap();
            assert_eq!(survivors.len(), 1);
            assert_eq!(survivors[0].node_id, "fresh", "the fresh one stays");
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
