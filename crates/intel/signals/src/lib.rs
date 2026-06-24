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

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::geometry::PortablePoint;
    use kernel::graph::Graph;

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
}
