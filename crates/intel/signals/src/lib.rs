/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Graph-structural **signal producer** — the graph-signals-layer plan's `intel/signals`.
//!
//! Computes per-node / per-pair signals from the kernel [`Graph`] and hands them to
//! cartography's narrow [`IntelligenceSignals`] contract. This crate OWNS the production
//! lifecycle; cartography keeps only the contract (so cartography never depends on a producer).
//!
//! First signal: **degree-based importance** — a cheap, synchronous signal computed inline. The
//! generation + per-signal dirty-bit **cache** (that gates recomputation and backgrounds the
//! expensive signals: betweenness / communities / affinity) and those richer signals land in
//! later slices; this slice is the spine (producer -> snapshot -> `project_orrery_strategy`).
//! (Graph signals — P1.)

use std::collections::{HashMap, VecDeque};

use cartography::{ImportanceWeights, IntelligenceSignals};
use kernel::graph::{Graph, NodeKey};

pub use cartography::{BridgeNodes, Cluster, ClusterSet};

/// Which graph-structural metric drives a node's **importance** weight. `Degree` is the cheap,
/// synchronous default (connection count); `Betweenness` is the structural broker metric (how
/// often a node lies on shortest paths) — a node bridging two clusters scores high even at
/// modest degree. Both normalize to `0..=1` against the graph's max. (Graph signals.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ImportanceMetric {
    /// Undirected degree (the cheapest proxy; the default).
    #[default]
    Degree,
    /// Betweenness centrality (Brandes' algorithm) — structural brokerage.
    Betweenness,
}

impl ImportanceMetric {
    /// The persisted string code for the cartography sidecar (kept downstream crates enum-free).
    /// (Graph signals — metric persistence.)
    pub fn as_code(self) -> &'static str {
        match self {
            ImportanceMetric::Degree => "degree",
            ImportanceMetric::Betweenness => "betweenness",
        }
    }

    /// Parse a persisted code, defaulting to [`Degree`](ImportanceMetric::Degree) for an unknown
    /// string. (Graph signals — metric persistence.)
    pub fn from_code(code: &str) -> Self {
        match code {
            "betweenness" => ImportanceMetric::Betweenness,
            _ => ImportanceMetric::Degree,
        }
    }
}

/// Per-node importance under the chosen `metric`, normalized `0..=1`. The single entry the host
/// reads; the metric is a per-scene choice. (Graph signals — importance.)
pub fn importance(graph: &Graph, metric: ImportanceMetric) -> ImportanceWeights {
    match metric {
        ImportanceMetric::Degree => degree_importance(graph),
        ImportanceMetric::Betweenness => betweenness_importance(graph),
    }
}

/// Produce the cheap, synchronous signal snapshot for `graph`: degree-based importance. The
/// other contract fields (clusters / affinity / bridges / embeddings) stay `None` until their
/// producers land. Recomputed on call — degree is cheap enough to run inline; the cache that
/// gates recomputation is a later slice. (Graph signals — P1, the spine.)
pub fn produce_cheap_signals(graph: &Graph) -> IntelligenceSignals {
    IntelligenceSignals {
        importance: Some(degree_importance(graph)),
        ..IntelligenceSignals::default()
    }
}

/// Per-node **degree importance**: each node's undirected neighbour count, normalized so the
/// most-connected node is `1.0` (a graph with no edges yields all-zero weights). Degree is the
/// cheapest importance proxy and matches the existing size-by-degree exactly (same
/// `neighbors_undirected` count), so routing it through the signal contract is behaviour-
/// preserving; betweenness / PageRank replace the metric here without touching the contract.
/// (Graph signals — degree importance.)
pub fn degree_importance(graph: &Graph) -> ImportanceWeights {
    let degrees: Vec<(NodeKey, f32)> = graph
        .nodes()
        .map(|(key, _)| (key, graph.neighbors_undirected(key).count() as f32))
        .collect();
    let max = degrees.iter().fold(0.0_f32, |m, &(_, d)| m.max(d));
    let weights = degrees
        .into_iter()
        .map(|(key, d)| (key, if max > 0.0 { d / max } else { 0.0 }))
        .collect();
    ImportanceWeights { weights }
}

/// Per-node **betweenness centrality** (Brandes' algorithm on the undirected, unweighted graph),
/// normalized so the top broker is `1.0`. Betweenness counts how often a node lies on shortest
/// paths between other pairs, so a node bridging otherwise-separate clusters scores high even at
/// low degree. O(V·E); cheap at current graph scale (the off-thread background lane is the
/// scale-trigger refinement). Parallel edges are collapsed and self-loops dropped (shortest paths
/// are over distinct adjacency). (Graph signals — betweenness importance.)
pub fn betweenness_importance(graph: &Graph) -> ImportanceWeights {
    let nodes: Vec<NodeKey> = graph.nodes().map(|(k, _)| k).collect();
    // Distinct undirected adjacency: dedup the multigraph's parallel edges, drop self-loops.
    let adj: HashMap<NodeKey, Vec<NodeKey>> = nodes
        .iter()
        .map(|&v| {
            let mut ns: Vec<NodeKey> = graph.neighbors_undirected(v).filter(|&w| w != v).collect();
            ns.sort();
            ns.dedup();
            (v, ns)
        })
        .collect();

    let mut bc: HashMap<NodeKey, f64> = nodes.iter().map(|&v| (v, 0.0)).collect();

    for &s in &nodes {
        // Single-source shortest paths (BFS): order stack, predecessors, path counts, distances.
        let mut stack: Vec<NodeKey> = Vec::new();
        let mut pred: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        let mut sigma: HashMap<NodeKey, f64> = nodes.iter().map(|&v| (v, 0.0)).collect();
        let mut dist: HashMap<NodeKey, i64> = nodes.iter().map(|&v| (v, -1)).collect();
        sigma.insert(s, 1.0);
        dist.insert(s, 0);
        let mut queue: VecDeque<NodeKey> = VecDeque::new();
        queue.push_back(s);
        while let Some(v) = queue.pop_front() {
            stack.push(v);
            for &w in &adj[&v] {
                if dist[&w] < 0 {
                    queue.push_back(w);
                    dist.insert(w, dist[&v] + 1);
                }
                if dist[&w] == dist[&v] + 1 {
                    *sigma.get_mut(&w).expect("sigma seeded for all nodes") += sigma[&v];
                    pred.entry(w).or_default().push(v);
                }
            }
        }
        // Back-to-front dependency accumulation.
        let mut delta: HashMap<NodeKey, f64> = nodes.iter().map(|&v| (v, 0.0)).collect();
        while let Some(w) = stack.pop() {
            if let Some(ps) = pred.get(&w) {
                for &v in ps {
                    let contribution = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                    *delta.get_mut(&v).expect("delta seeded for all nodes") += contribution;
                }
            }
            if w != s {
                *bc.get_mut(&w).expect("bc seeded for all nodes") += delta[&w];
            }
        }
    }
    // Undirected graph: each shortest path is counted from both endpoints, so halve.
    for value in bc.values_mut() {
        *value /= 2.0;
    }
    let max = bc.values().copied().fold(0.0_f64, f64::max);
    let weights = bc
        .into_iter()
        .map(|(key, b)| (key, if max > 0.0 { (b / max) as f32 } else { 0.0 }))
        .collect();
    ImportanceWeights { weights }
}

/// The graph's **bridge nodes** — the structural brokers, taken as the nodes whose normalized
/// betweenness is at least `threshold` (`0..=1`, where `1.0` is the top broker). These are the nodes
/// that lie on many shortest paths, so removing one most fragments reachability; the cartography
/// `BridgeNodes` contract documents exactly this "detected by graph-structural betweenness" notion.
/// A structureless graph (a clique, or all-leaves) has no high-betweenness node, so the set is empty.
/// (Graph signals — bridges.)
pub fn bridge_nodes(graph: &Graph, threshold: f32) -> BridgeNodes {
    let bridges = betweenness_importance(graph)
        .weights
        .into_iter()
        .filter(|&(_, w)| w >= threshold)
        .map(|(key, _)| key)
        .collect();
    BridgeNodes { bridges }
}

/// A `Send` snapshot of the graph structure community detection needs — the node set and the
/// parallel-edge-collapsed weighted undirected adjacency (index-based), self-loops dropped. Built
/// on the owner's thread by [`from_graph`](CommunitySnapshot::from_graph); the heavy Louvain
/// iteration then runs on it ([`community_louvain_on_snapshot`]) either inline or on a background
/// worker, without the (non-`Send`, borrowed) `Graph` crossing a thread boundary. (Graph signals —
/// the background lane.)
#[derive(Clone, Debug)]
pub struct CommunitySnapshot {
    nodes: Vec<NodeKey>,
    /// `adjacency[i]` = `[(neighbour_index, weight)]`, sorted by neighbour index.
    adjacency: Vec<Vec<(usize, f64)>>,
}

impl CommunitySnapshot {
    /// Extract the weighted adjacency from `graph` (multigraph multiplicity summed, self-loops
    /// dropped). Cheap relative to the Louvain iteration it feeds. (Graph signals — background lane.)
    pub fn from_graph(graph: &Graph) -> Self {
        let nodes: Vec<NodeKey> = graph.nodes().map(|(k, _)| k).collect();
        let index: HashMap<NodeKey, usize> =
            nodes.iter().enumerate().map(|(i, &k)| (k, i)).collect();
        // `neighbors_undirected` yields one entry per incident edge, so the running count captures
        // multiplicity directly.
        let mut adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); nodes.len()];
        for (i, &node) in nodes.iter().enumerate() {
            for nb in graph.neighbors_undirected(node) {
                if let Some(&j) = index.get(&nb) {
                    if i != j {
                        *adj[i].entry(j).or_insert(0.0) += 1.0;
                    }
                }
            }
        }
        // Freeze each row to a Vec sorted by neighbour index: `Send`, and a deterministic iteration
        // order (so the weight sums — hence any modularity tie — are reproducible run to run).
        let adjacency = adj
            .into_iter()
            .map(|row| {
                let mut row: Vec<(usize, f64)> = row.into_iter().collect();
                row.sort_unstable_by_key(|&(j, _)| j);
                row
            })
            .collect();
        Self { nodes, adjacency }
    }
}

/// Single-level **Louvain** modularity optimization on a [`CommunitySnapshot`]: each node starts in
/// its own community and repeatedly moves to the neighbouring community that most increases
/// modularity, until no move improves it. Returns a `ClusterSet` partition (every node lands in
/// exactly one cluster; an isolated node is its own singleton). The compute half — independent of
/// the `Graph`, so it runs inline or on a background worker.
///
/// This is the genuinely *expensive*-class structural signal — the one the background cache exists
/// to carry — though it is still fast at current graph scale. True multi-level (hierarchical)
/// Louvain with community aggregation is a later refinement; single-level already yields a real
/// partition on clustered graphs and shares this contract, so the upgrade is internal. (Graph
/// signals — community detection, P3.)
pub fn community_louvain_on_snapshot(snapshot: &CommunitySnapshot) -> ClusterSet {
    let nodes = &snapshot.nodes;
    let adj = &snapshot.adjacency;
    let n = nodes.len();
    if n == 0 {
        return ClusterSet::default();
    }
    let k: Vec<f64> = adj.iter().map(|row| row.iter().map(|&(_, w)| w).sum()).collect();
    let two_m: f64 = k.iter().sum(); // = 2m (each undirected edge counted from both ends)
    if two_m == 0.0 {
        // No edges: every node is its own singleton community.
        return ClusterSet {
            clusters: nodes
                .iter()
                .enumerate()
                .map(|(i, &key)| Cluster {
                    id: format!("c{i}"),
                    label: None,
                    members: vec![key],
                    confidence: 1.0,
                })
                .collect(),
        };
    }

    // Local moving: comm[i] is i's community; sigma_tot[c] is the summed degree of community c.
    let mut comm: Vec<usize> = (0..n).collect();
    let mut sigma_tot: Vec<f64> = k.clone();
    loop {
        let mut moved = false;
        for i in 0..n {
            let ci = comm[i];
            let ki = k[i];
            // Tentatively pull i out of its community.
            sigma_tot[ci] -= ki;
            // Weight from i into each neighbouring community.
            let mut neigh_w: HashMap<usize, f64> = HashMap::new();
            for &(j, w) in &adj[i] {
                *neigh_w.entry(comm[j]).or_insert(0.0) += w;
            }
            // Pick the community maximizing the modularity gain `w_ic - sigma_tot[c]*k_i/2m`;
            // the baseline is returning to `ci`, so i only moves on a strict improvement. Iterate
            // the candidates in sorted order (not HashMap order) so a modularity tie breaks
            // deterministically — the partition (and the community-ring colours) is then
            // reproducible across runs, not hash-seed-dependent.
            let mut candidates: Vec<usize> = neigh_w.keys().copied().collect();
            candidates.sort_unstable();
            let mut best_c = ci;
            let mut best_gain = neigh_w.get(&ci).copied().unwrap_or(0.0) - sigma_tot[ci] * ki / two_m;
            for &c in &candidates {
                let gain = neigh_w[&c] - sigma_tot[c] * ki / two_m;
                if gain > best_gain {
                    best_gain = gain;
                    best_c = c;
                }
            }
            comm[i] = best_c;
            sigma_tot[best_c] += ki;
            if best_c != ci {
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }

    // Compact the surviving community ids (first-seen order) into a `ClusterSet`.
    let mut remap: HashMap<usize, usize> = HashMap::new();
    let mut members: Vec<Vec<NodeKey>> = Vec::new();
    for i in 0..n {
        let idx = *remap.entry(comm[i]).or_insert_with(|| {
            members.push(Vec::new());
            members.len() - 1
        });
        members[idx].push(nodes[i]);
    }
    ClusterSet {
        clusters: members
            .into_iter()
            .enumerate()
            .map(|(i, m)| Cluster {
                id: format!("c{i}"),
                label: None,
                members: m,
                confidence: 1.0,
            })
            .collect(),
    }
}

/// Community detection on `graph`: extract a [`CommunitySnapshot`] then run Louvain inline. The
/// synchronous entry; the background lane uses [`CommunitySnapshot::from_graph`] +
/// [`community_louvain_on_snapshot`] across a thread. (Graph signals — community detection, P3.)
pub fn community_louvain(graph: &Graph) -> ClusterSet {
    community_louvain_on_snapshot(&CommunitySnapshot::from_graph(graph))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::geometry::PortablePoint;
    use kernel::graph::Graph;

    /// The cluster id that `set` assigns `key`, for partition-structure assertions.
    fn community_of(set: &ClusterSet, key: NodeKey) -> Option<&str> {
        set.clusters
            .iter()
            .find(|c| c.members.contains(&key))
            .map(|c| c.id.as_str())
    }

    #[test]
    fn degree_importance_normalizes_to_the_most_connected_node() {
        // A hub linked to two leaves: hub degree 2, each leaf degree 1.
        let mut graph = Graph::new();
        let hub = graph.add_node("https://hub.example".to_string(), PortablePoint::new(0.0, 0.0));
        let a = graph.add_node("https://a.example".to_string(), PortablePoint::new(1.0, 0.0));
        let b = graph.add_node("https://b.example".to_string(), PortablePoint::new(2.0, 0.0));
        graph.assert_semantic_predicate(hub, a, "links".to_string());
        graph.assert_semantic_predicate(hub, b, "links".to_string());

        let importance = degree_importance(&graph);
        assert_eq!(importance.lookup(hub), Some(1.0), "the most-connected node is 1.0");
        assert_eq!(importance.lookup(a), Some(0.5), "a degree-1 leaf is half the hub");
        assert_eq!(importance.lookup(b), Some(0.5));
    }

    #[test]
    fn no_edges_yields_all_zero_importance() {
        let mut graph = Graph::new();
        let only = graph.add_node("https://one.example".to_string(), PortablePoint::new(0.0, 0.0));
        let importance = degree_importance(&graph);
        assert_eq!(importance.lookup(only), Some(0.0), "no edges => zero importance, not NaN");
    }

    #[test]
    fn produce_cheap_signals_fills_only_importance() {
        let mut graph = Graph::new();
        graph.add_node("https://one.example".to_string(), PortablePoint::new(0.0, 0.0));
        let signals = produce_cheap_signals(&graph);
        assert!(signals.importance.is_some(), "importance is produced");
        assert!(signals.clusters.is_none(), "the un-produced signals stay None");
        assert!(signals.affinity.is_none());
        assert!(signals.bridges.is_none());
        assert!(signals.embeddings.is_none());
    }

    #[test]
    fn betweenness_marks_the_broker_on_a_path() {
        // a-b-c: b is on the only a–c shortest path => betweenness max; the endpoints are 0.
        let mut graph = Graph::new();
        let a = graph.add_node("https://a.example".to_string(), PortablePoint::new(0.0, 0.0));
        let b = graph.add_node("https://b.example".to_string(), PortablePoint::new(1.0, 0.0));
        let c = graph.add_node("https://c.example".to_string(), PortablePoint::new(2.0, 0.0));
        graph.assert_semantic_predicate(a, b, "links".to_string());
        graph.assert_semantic_predicate(b, c, "links".to_string());
        let imp = betweenness_importance(&graph);
        assert_eq!(imp.lookup(b), Some(1.0), "the broker b is the normalized max");
        assert_eq!(imp.lookup(a), Some(0.0), "an endpoint has zero betweenness");
        assert_eq!(imp.lookup(c), Some(0.0));
    }

    #[test]
    fn betweenness_rewards_the_bridge_node() {
        // A bowtie: triangles {0,1,2} and {2,3,4} share node 2. Every cross-triangle shortest path
        // routes through 2, so it scores the betweenness max — well above a peripheral node.
        let mut graph = Graph::new();
        let n: Vec<_> = (0..5)
            .map(|i| {
                graph.add_node(format!("https://{i}.example"), PortablePoint::new(i as f32, 0.0))
            })
            .collect();
        for &(a, b) in &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)] {
            graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
        }
        let imp = betweenness_importance(&graph);
        let bridge = imp.lookup(n[2]).unwrap();
        let peripheral = imp.lookup(n[0]).unwrap();
        assert_eq!(bridge, 1.0, "the bridge node is the normalized max");
        assert!(bridge > peripheral, "the bridge outscores a peripheral node: {bridge} vs {peripheral}");
    }

    /// Build a graph of `n` nodes (urls `https://{i}.example`) wired by undirected edge pairs.
    fn graph_from_edges(n: usize, edges: &[(usize, usize)]) -> (Graph, Vec<NodeKey>) {
        let mut graph = Graph::new();
        let keys: Vec<NodeKey> = (0..n)
            .map(|i| graph.add_node(format!("https://{i}.example"), PortablePoint::new(i as f32, 0.0)))
            .collect();
        for &(a, b) in edges {
            graph.assert_semantic_predicate(keys[a], keys[b], "links".to_string());
        }
        (graph, keys)
    }

    #[test]
    fn community_splits_two_triangles_joined_by_a_bridge() {
        // Triangles {0,1,2} and {3,4,5} joined by the single edge 2-3: modularity keeps them
        // two communities (each triangle's 3 internal edges outweigh the lone bridge).
        let (graph, k) = graph_from_edges(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)]);
        let set = community_louvain(&graph);
        assert_eq!(set.clusters.len(), 2, "two triangles => two communities");
        assert_eq!(community_of(&set, k[0]), community_of(&set, k[1]), "triangle A coheres");
        assert_eq!(community_of(&set, k[0]), community_of(&set, k[2]));
        assert_eq!(community_of(&set, k[3]), community_of(&set, k[5]), "triangle B coheres");
        assert_ne!(community_of(&set, k[0]), community_of(&set, k[3]), "the bridge does not merge them");
    }

    #[test]
    fn community_collapses_a_clique_to_one() {
        // A triangle is a single dense community.
        let (graph, k) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
        let set = community_louvain(&graph);
        assert_eq!(set.clusters.len(), 1, "a clique is one community");
        assert_eq!(set.clusters[0].members.len(), 3);
        assert_eq!(community_of(&set, k[0]), community_of(&set, k[2]));
    }

    #[test]
    fn community_separates_disconnected_components() {
        // Two disjoint edges => two communities.
        let (graph, k) = graph_from_edges(4, &[(0, 1), (2, 3)]);
        let set = community_louvain(&graph);
        assert_eq!(set.clusters.len(), 2, "disjoint components are distinct communities");
        assert_eq!(community_of(&set, k[0]), community_of(&set, k[1]));
        assert_ne!(community_of(&set, k[0]), community_of(&set, k[2]));
    }

    #[test]
    fn community_edgeless_graph_is_all_singletons() {
        let (graph, k) = graph_from_edges(3, &[]);
        let set = community_louvain(&graph);
        assert_eq!(set.clusters.len(), 3, "no edges => every node its own community");
        assert_ne!(community_of(&set, k[0]), community_of(&set, k[1]));
    }

    #[test]
    fn bridge_nodes_picks_the_high_betweenness_broker() {
        // Bowtie: triangles {0,1,2} and {2,3,4} share node 2 (betweenness 1.0); the rest are ~0.
        // At threshold 0.5 only the broker qualifies as a bridge.
        let (graph, k) = graph_from_edges(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)]);
        assert_eq!(bridge_nodes(&graph, 0.5).bridges, vec![k[2]], "only the broker is a bridge");
    }

    #[test]
    fn bridge_nodes_empty_for_a_clique() {
        // A triangle: every node's betweenness is 0, so there are no bridges.
        let (graph, _) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
        assert!(bridge_nodes(&graph, 0.5).bridges.is_empty(), "a clique has no bridges");
    }
}
