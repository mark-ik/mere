/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Structural (Jaccard) affinity scoring.

use super::*;

/// Pairwise **structural affinity** — the [Jaccard similarity] of node neighbourhoods,
/// `J(a, b) = |N(a) ∩ N(b)| / |N(a) ∪ N(b)|` over the distinct undirected neighbours of each node.
/// Two nodes score high when they share many neighbours, so structurally-equivalent nodes are
/// "similar" **even when they share no direct edge** — the signal that lets the affinity force draw
/// a community into a tight cluster (where gyre's `EdgeSpring` force only binds adjacent
/// pairs). This is the cheap, dependency-free structural stand-in for the later content-embedding
/// cosine affinity; both ride the same [`AffinityScores`] channel.
///
/// Only pairs whose affinity is `>= min_affinity` are emitted (and only pairs sharing at least one
/// neighbour can be non-zero), so the list stays sparse — its length tracks the graph's clustering,
/// not `n²`. Output is sorted by node-key pair, so the signal is reproducible run to run.
///
/// Note the star case: every leaf of a hub shares exactly that hub, so all leaf pairs score `1.0`
/// (they are structurally identical). That is the metric's honest answer — leaves of one hub *are*
/// a cluster — and the attract-only force settles them at its rest length rather than collapsing
/// them. (Graph signals — P4, the affinity signal.)
///
/// [Jaccard similarity]: https://en.wikipedia.org/wiki/Jaccard_index
pub fn structural_affinity(graph: &Graph, min_affinity: f32) -> AffinityScores {
    let nodes: Vec<NodeKey> = graph.nodes().map(|(k, _)| k).collect();
    // Distinct undirected adjacency (dedup the multigraph's parallel edges, drop self-loops) — the
    // same basis betweenness uses: structural similarity is over distinct neighbours, not edge
    // multiplicity. Each row is sorted, so a neighbour pair is emitted in canonical `(a < b)` order.
    let adj: HashMap<NodeKey, Vec<NodeKey>> = nodes
        .iter()
        .map(|&v| {
            let mut ns: Vec<NodeKey> = graph.neighbors_undirected(v).filter(|&w| w != v).collect();
            ns.sort();
            ns.dedup();
            (v, ns)
        })
        .collect();

    // Accumulate the shared-neighbour count only for pairs that share at least one neighbour: for
    // each node `v`, every unordered pair of `v`'s distinct neighbours gains one common neighbour
    // (`v` itself). This visits exactly the non-zero-Jaccard pairs, so the cost follows the graph's
    // clustering rather than scanning all `n²` pairs.
    let mut shared: HashMap<(NodeKey, NodeKey), u32> = HashMap::new();
    for v in &nodes {
        let ns = &adj[v];
        for i in 0..ns.len() {
            for j in (i + 1)..ns.len() {
                *shared.entry((ns[i], ns[j])).or_insert(0) += 1; // ns sorted => ns[i] < ns[j]
            }
        }
    }

    // J(a,b) = |N(a) ∩ N(b)| / |N(a) ∪ N(b)|, with the union from inclusion–exclusion
    // |A ∪ B| = |A| + |B| − |A ∩ B|. Threshold to keep the list lean, then sort for reproducibility.
    let mut pairs: Vec<((NodeKey, NodeKey), f32)> = shared
        .into_iter()
        .filter_map(|((a, b), inter)| {
            let union = (adj[&a].len() + adj[&b].len()).saturating_sub(inter as usize);
            if union == 0 {
                return None;
            }
            let jaccard = inter as f32 / union as f32;
            (jaccard >= min_affinity).then_some(((a, b), jaccard))
        })
        .collect();
    pairs.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));
    AffinityScores { pairs }
}
