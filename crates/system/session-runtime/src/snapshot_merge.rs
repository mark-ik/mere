/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Snapshot-level merge for engram compose (Alembic tail B7 / decision #1).
//!
//! Unions two [`GraphSnapshot`]s by URL identity, with the first (`a`) canonical.
//! Pure and deterministic. It lives here, not in `graph-kernel`, so it works
//! directly on the public `GraphSnapshot` structs without the kernel graph API:
//! [`PersistedEdge`] keys its endpoints by `from_node_id` / `to_node_id`
//! (stable `String`s), so a merge is plain Vec surgery (the audited
//! `engram_compose_merge_plan`). A kernel `Graph::merge_from` is a later, optional
//! promotion. This is what first populates the engram lineage
//! (`ProvenanceRecord.upstream`, via [`crate::graph_engram::compose_graph_engrams`])
//! that the consolidation pass will read.

use std::collections::{HashMap, HashSet};

use kernel::persistence::{GraphSnapshot, PersistedEdge};

/// What [`merge_snapshots`] did, for the Athanor proposal / diagnostics. Never a
/// placebo: every count reflects a real change to the merged snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Nodes added from `b` (a url not present in `a`).
    pub added_nodes: usize,
    /// Same-url nodes layered onto `a`'s node (tags / properties unioned).
    pub layered_nodes: usize,
    /// Edges added from `b` after endpoint remap.
    pub added_edges: usize,
    /// `b` edges dropped as duplicates of an `a` edge (same endpoints + kind).
    pub deduped_edges: usize,
    /// `b` fields not carried (first cut keeps only `a`'s field layer).
    pub dropped_fields: usize,
    /// `b` couplings not carried.
    pub dropped_couplings: usize,
}

/// Merge `b` into `a` by URL identity, returning the merged snapshot + a report.
///
/// `a` is canonical: a node whose url appears in both keeps `a`'s `node_id` (and
/// `b`'s id is remapped onto it), gaining `b`'s tags and properties (`a` wins a
/// property-predicate conflict). A url only in `b` is added verbatim. `b`'s edges
/// are remapped through that id map and unioned, deduped by `(from, to, kind)`.
/// `import_records` (the durable provenance) are unioned, their memberships
/// remapped. `a`'s `fields` / `couplings` / `navigation` are kept and `b`'s are
/// dropped (reported) — union is a later refinement once overlap has a rule.
pub fn merge_snapshots(a: &GraphSnapshot, b: &GraphSnapshot) -> (GraphSnapshot, MergeReport) {
    let mut report = MergeReport::default();
    let mut merged = a.clone();

    // url -> canonical node_id, seeded from `a`. Owned keys: `merged.nodes` is
    // mutated below, so the map cannot borrow it. Empty urls cannot identity-match.
    let mut url_to_id: HashMap<String, String> = HashMap::new();
    for n in &merged.nodes {
        if !n.url.is_empty() {
            url_to_id
                .entry(n.url.clone())
                .or_insert_with(|| n.node_id.clone());
        }
    }
    // node_id -> index into `merged.nodes`, for layering same-url nodes in place.
    let mut id_to_index: HashMap<String, usize> = merged
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.node_id.clone(), i))
        .collect();

    // `b` node_id -> canonical id in the merged snapshot.
    let mut remap: HashMap<String, String> = HashMap::with_capacity(b.nodes.len());
    for bn in &b.nodes {
        let canon = if bn.url.is_empty() {
            None
        } else {
            url_to_id.get(&bn.url).cloned()
        };
        if let Some(canon) = canon {
            // Same url as an `a` node -> layer onto it (id remapped onto A's).
            remap.insert(bn.node_id.clone(), canon.clone());
            if let Some(&idx) = id_to_index.get(&canon) {
                let an = &mut merged.nodes[idx];
                for t in &bn.tags {
                    if !an.tags.contains(t) {
                        an.tags.push(t.clone());
                    }
                }
                for p in &bn.properties {
                    if !an.properties.iter().any(|q| q.predicate == p.predicate) {
                        an.properties.push(p.clone());
                    }
                }
            }
            report.layered_nodes += 1;
        } else {
            // Url only in `b` (or empty) -> add verbatim, keeping its id.
            remap.insert(bn.node_id.clone(), bn.node_id.clone());
            if !bn.url.is_empty() {
                url_to_id.insert(bn.url.clone(), bn.node_id.clone());
            }
            id_to_index.insert(bn.node_id.clone(), merged.nodes.len());
            merged.nodes.push(bn.clone());
            report.added_nodes += 1;
        }
    }

    // Edges: remap `b` endpoints, union deduped by (from, to, kind signature).
    let mut seen: HashSet<(String, String, String)> =
        merged.edges.iter().map(edge_signature).collect();
    for be in &b.edges {
        let mut e = be.clone();
        if let Some(canon) = remap.get(&e.from_node_id) {
            e.from_node_id = canon.clone();
        }
        if let Some(canon) = remap.get(&e.to_node_id) {
            e.to_node_id = canon.clone();
        }
        if seen.insert(edge_signature(&e)) {
            merged.edges.push(e);
            report.added_edges += 1;
        } else {
            report.deduped_edges += 1;
        }
    }

    // import_records: union by record_id, remapping membership node ids so the
    // provenance still points at the canonical nodes. (The durable source that
    // syncs to `node.import_provenance` — decision #1's per-member provenance.)
    let existing: HashSet<String> = merged
        .import_records
        .iter()
        .map(|r| r.record_id.clone())
        .collect();
    for r in &b.import_records {
        if existing.contains(&r.record_id) {
            continue;
        }
        let mut rec = r.clone();
        for m in &mut rec.memberships {
            if let Some(canon) = remap.get(&m.node_id) {
                m.node_id = canon.clone();
            }
        }
        merged.import_records.push(rec);
    }

    // `b`'s field layer + navigation are dropped (first cut keeps `a`'s). Report it.
    report.dropped_fields = b.fields.len();
    report.dropped_couplings = b.couplings.len();
    merged.timestamp_secs = a.timestamp_secs.max(b.timestamp_secs);

    (merged, report)
}

/// A stable dedup key for an edge: its endpoints plus a `Debug` signature of its
/// family set + semantic data, so a relation asserted in both snapshots collapses
/// to one, while distinct sub-kinds (Cites vs Quotes) on the same pair are kept.
fn edge_signature(e: &PersistedEdge) -> (String, String, String) {
    (
        e.from_node_id.clone(),
        e.to_node_id.clone(),
        format!("{:?}|{:?}", e.families, e.semantic),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::default::Point2D;
    use kernel::graph::Graph;
    use kernel::graph::fixtures::GraphFixtures;

    /// A snapshot with one node per url (ids minted by the real graph API).
    fn snap(urls: &[&str]) -> GraphSnapshot {
        let mut g = Graph::new();
        for (i, u) in urls.iter().enumerate() {
            g.add_node(u.to_string(), Point2D::new(i as f32, 0.0));
        }
        g.to_snapshot()
    }

    /// A minimal kind-less edge between two node ids.
    fn edge(from: &str, to: &str) -> PersistedEdge {
        PersistedEdge {
            from_node_id: from.to_string(),
            to_node_id: to.to_string(),
            families: Vec::new(),
            semantic: None,
            traversal: None,
            containment: None,
            arrangement: None,
            imported: None,
            provenance: None,
        }
    }

    #[test]
    fn unions_nodes_by_url_without_doubling_the_shared_one() {
        let a = snap(&["https://x", "https://y"]);
        let b = snap(&["https://y", "https://z"]);
        let (merged, report) = merge_snapshots(&a, &b);
        let urls: HashSet<&str> = merged.nodes.iter().map(|n| n.url.as_str()).collect();
        let want: HashSet<&str> = ["https://x", "https://y", "https://z"]
            .into_iter()
            .collect();
        assert_eq!(urls, want, "x, y, z union");
        assert_eq!(merged.nodes.len(), 3, "y is not doubled");
        assert_eq!(report.added_nodes, 1, "only z is new");
        assert_eq!(report.layered_nodes, 1, "y is shared");
    }

    #[test]
    fn layers_tags_onto_the_canonical_node_keeping_a_id() {
        let mut a = snap(&["https://y"]);
        a.nodes[0].tags = vec!["from-a".into()];
        let a_id = a.nodes[0].node_id.clone();
        let mut b = snap(&["https://y"]);
        b.nodes[0].tags = vec!["from-b".into()];
        assert_ne!(a_id, b.nodes[0].node_id, "the two graphs mint distinct ids");

        let (merged, _) = merge_snapshots(&a, &b);
        assert_eq!(merged.nodes.len(), 1, "same url collapses to one node");
        assert_eq!(merged.nodes[0].node_id, a_id, "A's node_id is canonical");
        let tags: HashSet<&str> = merged.nodes[0].tags.iter().map(String::as_str).collect();
        assert!(
            tags.contains("from-a") && tags.contains("from-b"),
            "both sources' tags coexist on the layered node",
        );
    }

    #[test]
    fn remaps_b_edges_through_the_shared_url() {
        let a = snap(&["https://x", "https://y"]);
        let ay = a.nodes[1].node_id.clone();
        let mut b = snap(&["https://y", "https://z"]);
        let by = b.nodes[0].node_id.clone();
        let bz = b.nodes[1].node_id.clone();
        b.edges.push(edge(&by, &bz)); // b: y -> z

        let (merged, report) = merge_snapshots(&a, &b);
        assert_eq!(report.added_edges, 1);
        let e = merged
            .edges
            .iter()
            .find(|e| e.to_node_id == bz)
            .expect("the y->z edge");
        assert_eq!(
            e.from_node_id, ay,
            "b's edge endpoint is remapped to A's canonical y id",
        );
    }

    #[test]
    fn dedups_a_relation_asserted_in_both() {
        let mut a = snap(&["https://x", "https://y"]);
        let (ax, ay) = (a.nodes[0].node_id.clone(), a.nodes[1].node_id.clone());
        a.edges.push(edge(&ax, &ay));
        let mut b = snap(&["https://x", "https://y"]);
        let (bx, by) = (b.nodes[0].node_id.clone(), b.nodes[1].node_id.clone());
        b.edges.push(edge(&bx, &by)); // same relation, different ids

        let (merged, report) = merge_snapshots(&a, &b);
        assert_eq!(
            merged.edges.len(),
            1,
            "the shared x->y relation is not doubled"
        );
        assert_eq!(report.deduped_edges, 1);
        assert_eq!(report.added_edges, 0);
    }
}
