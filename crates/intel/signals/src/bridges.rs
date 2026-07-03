/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Bridge / articulation-point detection.

use super::*;

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

/// Which notion of "critical connector" the bridge ring highlights. `Betweenness` is the *broker*
/// (a node on many shortest paths — high traffic); `Articulation` is the *cut vertex* (a node whose
/// removal disconnects part of the graph — single point of failure). They overlap but differ: a hub
/// inside a dense cluster can have high betweenness yet not be a cut vertex, and a low-degree node
/// joining two blobs is a cut vertex with modest betweenness. (Graph signals — bridges.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BridgeMetric {
    /// Betweenness brokers (thresholded normalized betweenness). The default.
    #[default]
    Betweenness,
    /// Articulation points (cut vertices), via Tarjan / Hopcroft–Tarjan low-link DFS.
    Articulation,
}

impl BridgeMetric {
    /// The persisted string code for the cartography sidecar. (Graph signals — bridge persistence.)
    pub fn as_code(self) -> &'static str {
        match self {
            BridgeMetric::Betweenness => "betweenness",
            BridgeMetric::Articulation => "articulation",
        }
    }

    /// Parse a persisted code, defaulting to [`Betweenness`](BridgeMetric::Betweenness).
    pub fn from_code(code: &str) -> Self {
        match code {
            "articulation" => BridgeMetric::Articulation,
            _ => BridgeMetric::Betweenness,
        }
    }
}

/// The graph's bridge nodes under the chosen `metric`: betweenness brokers (thresholded) or
/// articulation points. The single entry the host reads; the metric is a per-scene choice (the
/// `threshold` only applies to betweenness — articulation is binary). (Graph signals — bridges.)
pub fn bridges(graph: &Graph, metric: BridgeMetric, threshold: f32) -> BridgeNodes {
    match metric {
        BridgeMetric::Betweenness => bridge_nodes(graph, threshold),
        BridgeMetric::Articulation => articulation_points(graph),
    }
}

/// The graph's **articulation points** (cut vertices): the nodes whose removal increases the number
/// of connected components — each is a single point of failure joining parts of the graph that would
/// otherwise fall apart. Computed by the standard low-link DFS (Hopcroft–Tarjan), run **iteratively**
/// so a deep graph cannot overflow the stack, over the distinct undirected adjacency (parallel edges
/// collapsed, self-loops dropped). A 2-connected graph (a clique, a cycle) has none; a tree's
/// internal nodes are all articulation points. Returned in the `BridgeNodes` node-list contract, in
/// ascending key order. (Graph signals — bridges / articulation points.)
pub fn articulation_points(graph: &Graph) -> BridgeNodes {
    let nodes: Vec<NodeKey> = graph.nodes().map(|(k, _)| k).collect();
    let index: HashMap<NodeKey, usize> = nodes.iter().enumerate().map(|(i, &k)| (k, i)).collect();
    let n = nodes.len();
    // Distinct undirected adjacency as indices (dedup parallel edges, drop self-loops).
    let adj: Vec<Vec<usize>> = nodes
        .iter()
        .map(|&v| {
            let mut ns: Vec<usize> = graph
                .neighbors_undirected(v)
                .filter(|&w| w != v)
                .filter_map(|w| index.get(&w).copied())
                .collect();
            ns.sort_unstable();
            ns.dedup();
            ns
        })
        .collect();

    const UNVISITED: usize = usize::MAX;
    let mut disc = vec![UNVISITED; n]; // discovery time
    let mut low = vec![0usize; n]; // lowest discovery reachable
    let mut is_ap = vec![false; n];
    let mut timer = 0usize;

    for s in 0..n {
        if disc[s] != UNVISITED {
            continue;
        }
        // Iterative DFS: each stack frame is `(node, parent-or-NONE, next-neighbour-index)`.
        let mut root_children = 0usize;
        disc[s] = timer;
        low[s] = timer;
        timer += 1;
        let mut stack: Vec<(usize, usize, usize)> = vec![(s, UNVISITED, 0)];
        while let Some(&(u, parent, i)) = stack.last() {
            if i < adj[u].len() {
                stack.last_mut().expect("stack non-empty in this branch").2 += 1;
                let v = adj[u][i];
                if v == parent {
                    continue; // skip the single tree edge back to the parent
                }
                if disc[v] == UNVISITED {
                    disc[v] = timer;
                    low[v] = timer;
                    timer += 1;
                    if u == s {
                        root_children += 1;
                    }
                    stack.push((v, u, 0));
                } else {
                    // Back edge u -> v: u can reach v's discovery time.
                    low[u] = low[u].min(disc[v]);
                }
            } else {
                // Finished u: fold its low into its parent and test the parent (non-root) for the
                // articulation condition `low[child] >= disc[parent]`.
                stack.pop();
                if parent != UNVISITED {
                    low[parent] = low[parent].min(low[u]);
                    if parent != s && low[u] >= disc[parent] {
                        is_ap[parent] = true;
                    }
                }
            }
        }
        // The DFS root is an articulation point iff it has more than one DFS-tree child.
        if root_children > 1 {
            is_ap[s] = true;
        }
    }

    let bridges = (0..n).filter(|&i| is_ap[i]).map(|i| nodes[i]).collect();
    BridgeNodes { bridges }
}
