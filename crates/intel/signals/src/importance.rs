/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-node importance metrics (degree, betweenness).

use super::*;

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
