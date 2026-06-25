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

pub use cartography::{AffinityScores, BridgeNodes, Cluster, ClusterSet, Overlay};

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
    let index: HashMap<NodeKey, usize> =
        nodes.iter().enumerate().map(|(i, &k)| (k, i)).collect();
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

/// One pass of Louvain **local moving** on a weighted graph with self-loops: each node starts in its
/// own community and repeatedly moves to the neighbouring community of greatest modularity gain until
/// no move improves it. `adj[i]` is the inter-node adjacency (no self-loops); `self_loops[i]` is i's
/// self-loop weight (intra-community weight folded in at a coarser level). Returns `comm[i]` = i's
/// community. Candidates are visited in sorted order so a modularity tie breaks deterministically.
fn louvain_local_moving(adj: &[Vec<(usize, f64)>], self_loops: &[f64]) -> Vec<usize> {
    let n = adj.len();
    // Degree k[i] = incident inter-node weight + 2× the self-loop (an undirected self-loop touches
    // the node at both ends), so the modularity is preserved across aggregation levels.
    let k: Vec<f64> = (0..n)
        .map(|i| adj[i].iter().map(|&(_, w)| w).sum::<f64>() + 2.0 * self_loops[i])
        .collect();
    let two_m: f64 = k.iter().sum();
    if two_m == 0.0 {
        return (0..n).collect(); // no edges: every node its own community
    }
    let mut comm: Vec<usize> = (0..n).collect();
    let mut sigma_tot: Vec<f64> = k.clone();
    loop {
        let mut moved = false;
        for i in 0..n {
            let ci = comm[i];
            let ki = k[i];
            sigma_tot[ci] -= ki; // tentatively pull i out of its community
            // Weight from i into each neighbouring community (the self-loop is not an inter-node
            // edge, so it never contributes to a move-target weight).
            let mut neigh_w: HashMap<usize, f64> = HashMap::new();
            for &(j, w) in &adj[i] {
                if j != i {
                    *neigh_w.entry(comm[j]).or_insert(0.0) += w;
                }
            }
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
    comm
}

/// Compact `comm` (arbitrary community ids) to a dense `0..num` in first-seen order, returning the
/// remapped vector and the community count. (Deterministic: first-seen over the node order.)
fn compact_communities(comm: &[usize]) -> (Vec<usize>, usize) {
    let mut remap: HashMap<usize, usize> = HashMap::new();
    let compact: Vec<usize> = comm
        .iter()
        .map(|&c| {
            let next = remap.len();
            *remap.entry(c).or_insert(next)
        })
        .collect();
    (compact, remap.len())
}

/// **Aggregate** a graph by community: each community `c` (dense id in `0..num_comms`) becomes one
/// super-node. Inter-community edges sum into the new adjacency; intra-community edges and the prior
/// self-loops fold into the new self-loop. This preserves total degree (and hence modularity), so
/// re-running local moving on the result is the next Louvain level. Returns `(new_adj, new_self)`.
fn louvain_aggregate(
    adj: &[Vec<(usize, f64)>],
    self_loops: &[f64],
    comm: &[usize],
    num_comms: usize,
) -> (Vec<Vec<(usize, f64)>>, Vec<f64>) {
    let mut new_adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); num_comms];
    let mut new_self = vec![0.0_f64; num_comms];
    // Prior self-loops stay intra at the coarser level.
    for (i, &w) in self_loops.iter().enumerate() {
        new_self[comm[i]] += w;
    }
    for (i, row) in adj.iter().enumerate() {
        let ci = comm[i];
        for &(j, w) in row {
            let cj = comm[j];
            if ci == cj {
                // Intra-community edge -> self-loop. Each undirected edge appears twice in the
                // adjacency (i->j and j->i), so add w/2 each time to total the edge weight once.
                new_self[ci] += w / 2.0;
            } else {
                *new_adj[ci].entry(cj).or_insert(0.0) += w;
            }
        }
    }
    let new_adj = new_adj
        .into_iter()
        .map(|row| {
            let mut row: Vec<(usize, f64)> = row.into_iter().collect();
            row.sort_unstable_by_key(|&(j, _)| j);
            row
        })
        .collect();
    (new_adj, new_self)
}

/// **Multi-level (hierarchical) Louvain** modularity optimization on a [`CommunitySnapshot`]: run
/// local moving, then *aggregate* each community into a super-node and repeat, until a pass no longer
/// coarsens the graph. Each level escapes the resolution where single-level local moving stalls, so
/// the partition is at least as good (and usually better) than one pass. Returns a flat `ClusterSet`
/// over the original nodes (every node in exactly one cluster; an isolated node is its own
/// singleton). Deterministic (sorted tie-breaks + first-seen compaction) and `Graph`-independent, so
/// it runs inline or on the background worker. (Graph signals — community detection, P3 + multi-level.)
pub fn community_louvain_on_snapshot(snapshot: &CommunitySnapshot) -> ClusterSet {
    let nodes = &snapshot.nodes;
    let n = nodes.len();
    if n == 0 {
        return ClusterSet::default();
    }
    // No edges at all: every node is its own singleton community (no level can merge them).
    let total_weight: f64 =
        snapshot.adjacency.iter().map(|row| row.iter().map(|&(_, w)| w).sum::<f64>()).sum();
    if total_weight == 0.0 {
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

    // The working graph for the current level; level 0 is the original nodes with no self-loops.
    let mut adj: Vec<Vec<(usize, f64)>> = snapshot.adjacency.clone();
    let mut self_loops: Vec<f64> = vec![0.0; n];
    // `super_of[orig]` = the current-level super-node each original node belongs to.
    let mut super_of: Vec<usize> = (0..n).collect();

    loop {
        let comm = louvain_local_moving(&adj, &self_loops);
        let (compact, num_comms) = compact_communities(&comm);
        // Carry every original node through this level's merge.
        for s in super_of.iter_mut() {
            *s = compact[*s];
        }
        // A pass that did not coarsen the graph (each super-node its own community) has converged;
        // a single surviving community cannot coarsen further either.
        if num_comms == adj.len() || num_comms == 1 {
            break;
        }
        let (new_adj, new_self) = louvain_aggregate(&adj, &self_loops, &compact, num_comms);
        adj = new_adj;
        self_loops = new_self;
    }

    // Group the original nodes by their final super-node (dense `0..final_num`, in node order).
    let final_num = super_of.iter().copied().max().map_or(0, |m| m + 1);
    let mut members: Vec<Vec<NodeKey>> = vec![Vec::new(); final_num];
    for (orig, &sup) in super_of.iter().enumerate() {
        members[sup].push(nodes[orig]);
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
    fn community_three_triangles_in_a_line_are_three_communities() {
        // T1{0,1,2}-T2{3,4,5}-T3{6,7,8} joined by single bridges 2-3 and 5-6: the multi-level loop
        // keeps the three dense triangles apart (each bridge is too weak to merge across).
        let (graph, k) = graph_from_edges(
            9,
            &[
                (0, 1), (1, 2), (2, 0), // T1
                (3, 4), (4, 5), (5, 3), // T2
                (6, 7), (7, 8), (8, 6), // T3
                (2, 3), (5, 6), // bridges
            ],
        );
        let set = community_louvain(&graph);
        assert_eq!(set.clusters.len(), 3, "three triangles => three communities");
        assert_eq!(community_of(&set, k[0]), community_of(&set, k[1]), "T1 coheres");
        assert_eq!(community_of(&set, k[0]), community_of(&set, k[2]));
        assert_eq!(community_of(&set, k[3]), community_of(&set, k[5]), "T2 coheres");
        assert_eq!(community_of(&set, k[6]), community_of(&set, k[8]), "T3 coheres");
        assert_ne!(community_of(&set, k[0]), community_of(&set, k[3]), "T1 != T2");
        assert_ne!(community_of(&set, k[3]), community_of(&set, k[6]), "T2 != T3");
        // Every node lands in exactly one cluster.
        let total: usize = set.clusters.iter().map(|c| c.members.len()).sum();
        assert_eq!(total, 9, "the partition covers every node once");
    }

    #[test]
    fn community_detection_is_deterministic() {
        // The same snapshot must yield the identical partition run to run (sorted tie-breaks +
        // first-seen compaction): the community-ring colours depend on it.
        let (graph, _) =
            graph_from_edges(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)]);
        let snapshot = CommunitySnapshot::from_graph(&graph);
        let a = community_louvain_on_snapshot(&snapshot);
        let b = community_louvain_on_snapshot(&snapshot);
        assert_eq!(a, b, "multi-level Louvain is deterministic");
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

    /// The articulation-point set as a sorted Vec, for order-independent comparison.
    fn aps(graph: &Graph) -> Vec<NodeKey> {
        let mut v = articulation_points(graph).bridges;
        v.sort();
        v
    }

    #[test]
    fn articulation_points_finds_a_paths_interior() {
        // Path 0-1-2-3: removing an interior node (1 or 2) splits the path; the endpoints do not.
        let (graph, k) = graph_from_edges(4, &[(0, 1), (1, 2), (2, 3)]);
        let mut expected = vec![k[1], k[2]];
        expected.sort();
        assert_eq!(aps(&graph), expected, "the path interior nodes are cut vertices");
    }

    #[test]
    fn articulation_points_empty_for_a_clique() {
        // A triangle is 2-connected: removing any node leaves the other two connected.
        let (graph, _) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
        assert!(articulation_points(&graph).bridges.is_empty(), "a clique has no cut vertex");
    }

    #[test]
    fn articulation_points_empty_for_a_cycle() {
        // A 4-cycle is 2-connected: removing one node leaves a path, still connected.
        let (graph, _) = graph_from_edges(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
        assert!(articulation_points(&graph).bridges.is_empty(), "a cycle has no cut vertex");
    }

    #[test]
    fn articulation_points_finds_the_bowtie_hinge() {
        // Two triangles {0,1,2} and {2,3,4} sharing node 2: only 2 is a cut vertex (its removal
        // splits {0,1} from {3,4}).
        let (graph, k) = graph_from_edges(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)]);
        assert_eq!(aps(&graph), vec![k[2]], "the shared hinge is the only cut vertex");
    }

    #[test]
    fn bridges_dispatch_picks_the_metric() {
        // Bowtie: node 2 is both the betweenness broker and the cut vertex (here they agree). The
        // dispatcher routes to each producer by metric.
        let (graph, k) = graph_from_edges(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)]);
        assert_eq!(bridges(&graph, BridgeMetric::Betweenness, 0.5).bridges, vec![k[2]]);
        assert_eq!(bridges(&graph, BridgeMetric::Articulation, 0.5).bridges, vec![k[2]]);
        // The codes round-trip for persistence.
        assert_eq!(BridgeMetric::from_code(BridgeMetric::Articulation.as_code()), BridgeMetric::Articulation);
        assert_eq!(BridgeMetric::from_code("nonsense"), BridgeMetric::Betweenness);
    }

    /// The affinity recorded for an unordered pair, or `None` if it was not emitted.
    fn affinity_of(scores: &AffinityScores, a: NodeKey, b: NodeKey) -> Option<f32> {
        scores.lookup(a, b)
    }

    #[test]
    fn affinity_empty_for_an_edgeless_graph() {
        let (graph, _) = graph_from_edges(3, &[]);
        assert!(
            structural_affinity(&graph, 0.0).pairs.is_empty(),
            "no edges => no shared neighbours => no affinity"
        );
    }

    #[test]
    fn triangle_members_all_share_affinity() {
        // Every pair in a triangle shares the third node: J = 1/(2 + 2 − 1) = 1/3 for all three.
        let (graph, k) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
        let scores = structural_affinity(&graph, 0.0);
        assert_eq!(scores.pairs.len(), 3, "all three triangle pairs are similar");
        for &(a, b) in &[(0, 1), (1, 2), (2, 0)] {
            let j = affinity_of(&scores, k[a], k[b]).expect("pair present");
            assert!((j - 1.0 / 3.0).abs() < 1e-5, "triangle Jaccard is 1/3, got {j}");
        }
    }

    #[test]
    fn affinity_clusters_within_triangles_not_across() {
        // Two triangles {0,1,2},{3,4,5} joined by the bridge 2-3. Same-triangle nodes share a
        // neighbour (affinity ~1/3); the far cross pairs share none (absent) — affinity reads as
        // the clusters, the property the affinity force exploits.
        let (graph, k) =
            graph_from_edges(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)]);
        let scores = structural_affinity(&graph, 0.0);
        let within_a = affinity_of(&scores, k[0], k[1]).expect("within triangle A");
        let within_b = affinity_of(&scores, k[4], k[5]).expect("within triangle B");
        assert!((within_a - 1.0 / 3.0).abs() < 1e-5, "a clean within-A pair is 1/3");
        assert!((within_b - 1.0 / 3.0).abs() < 1e-5, "a clean within-B pair is 1/3");
        assert_eq!(affinity_of(&scores, k[1], k[4]), None, "a far cross pair shares no neighbour");
        assert_eq!(affinity_of(&scores, k[0], k[5]), None, "another far cross pair is absent");
    }

    #[test]
    fn clique_pairs_share_equal_affinity() {
        // A 4-clique: every pair shares the other two nodes, J = 2/(3 + 3 − 2) = 1/2, uniform.
        let (graph, k) = graph_from_edges(4, &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]);
        let scores = structural_affinity(&graph, 0.0);
        assert_eq!(scores.pairs.len(), 6, "all six clique pairs are similar");
        for &(a, b) in &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
            let j = affinity_of(&scores, k[a], k[b]).expect("pair present");
            assert!((j - 0.5).abs() < 1e-5, "clique Jaccard is 1/2, got {j}");
        }
    }

    #[test]
    fn affinity_threshold_prunes_weak_pairs() {
        // Triangle pairs sit at 1/3 ≈ 0.333: a 0.5 floor drops them all; a 0.1 floor keeps all three.
        let (graph, _) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
        assert!(
            structural_affinity(&graph, 0.5).pairs.is_empty(),
            "a 0.5 floor prunes the 1/3 pairs"
        );
        assert_eq!(
            structural_affinity(&graph, 0.1).pairs.len(),
            3,
            "a 0.1 floor keeps the triangle pairs"
        );
    }
}
