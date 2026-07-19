// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Louvain community detection (graph + snapshot lanes).

use super::*;

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
    pub fn from_graph(graph: &impl TopologyView) -> Self {
        let nodes: Vec<NodeKey> = graph.node_keys().collect();
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
            let mut best_gain =
                neigh_w.get(&ci).copied().unwrap_or(0.0) - sigma_tot[ci] * ki / two_m;
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
    let total_weight: f64 = snapshot
        .adjacency
        .iter()
        .map(|row| row.iter().map(|&(_, w)| w).sum::<f64>())
        .sum();
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
pub fn community_louvain(graph: &impl TopologyView) -> ClusterSet {
    community_louvain_on_snapshot(&CommunitySnapshot::from_graph(graph))
}
