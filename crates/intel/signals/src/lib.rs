// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

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

mod affinity;
mod bridges;
mod community;
mod importance;

pub use affinity::*;
pub use bridges::*;
pub use community::*;
pub use importance::*;

/// The minimal graph view the structural signals need: the node set and undirected
/// adjacency. Every signal here (degree, betweenness, community, bridges, affinity) is
/// pure topology, so binding them to this seam instead of a concrete graph makes them
/// run on any graph that can enumerate its nodes and a node's undirected neighbours.
/// mere's `kernel::graph::Graph` and the generic `chartulary::Graph` both implement it,
/// so the same algorithms serve the browser and any substrate graph — the analytics
/// become promotable. (Graph signals — the promotable seam.)
pub trait TopologyView {
    /// Every node's stable key.
    fn node_keys(&self) -> impl Iterator<Item = NodeKey> + '_;

    /// The neighbours of `key` in either direction, one entry per incident edge
    /// (parallel edges repeat; the signals collapse multiplicity where they need
    /// distinct adjacency).
    fn neighbors_undirected(&self, key: NodeKey) -> impl Iterator<Item = NodeKey> + '_;
}

impl TopologyView for Graph {
    fn node_keys(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.nodes().map(|(key, _)| key)
    }

    fn neighbors_undirected(&self, key: NodeKey) -> impl Iterator<Item = NodeKey> + '_ {
        // The inherent method (inherent wins over the trait method of the same name).
        Graph::neighbors_undirected(self, key)
    }
}

impl<N: chartulary::Identified, E> TopologyView for chartulary::Graph<N, E> {
    fn node_keys(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.nodes().map(|(key, _)| key)
    }

    fn neighbors_undirected(&self, key: NodeKey) -> impl Iterator<Item = NodeKey> + '_ {
        chartulary::Graph::neighbors_undirected(self, key)
    }
}

#[cfg(test)]
mod tests;

/// Produce the cheap, synchronous signal snapshot for `graph`: degree-based importance. The
/// other contract fields (clusters / affinity / bridges / embeddings) stay `None` until their
/// producers land. Recomputed on call — degree is cheap enough to run inline; the cache that
/// gates recomputation is a later slice. (Graph signals — P1, the spine.)
pub fn produce_cheap_signals(graph: &impl TopologyView) -> IntelligenceSignals {
    IntelligenceSignals {
        importance: Some(degree_importance(graph)),
        ..IntelligenceSignals::default()
    }
}

#[cfg(test)]
mod substrate_tests {
    use super::*;
    use chartulary::{Container, Graph as ChartGraph, Recognized, Relation, RelationClass};

    /// The structural signals run on a generic `chartulary::Graph`, not just mere's
    /// `kernel::graph::Graph` — the promotability payoff of the `TopologyView` seam.
    #[test]
    fn structural_signals_run_on_a_generic_chartulary_graph() {
        // A star: hub `h` cited by three leaves.
        let mut g: ChartGraph<Container, Relation> = ChartGraph::new();
        let h = g.insert(Container::new("h"));
        for leaf in ["a", "b", "c"] {
            let k = g.insert(Container::new(leaf));
            g.connect(
                k,
                h,
                Relation::new(RelationClass::recognized(Recognized::Cites)),
            );
        }

        // Degree importance ranks the hub top (it has all three edges).
        let weights = degree_importance(&g);
        let hub = g.key_of(&"h".to_string()).expect("hub present");
        let hub_weight = weights
            .weights
            .iter()
            .find(|(key, _)| *key == hub)
            .map(|(_, weight)| *weight);
        assert_eq!(hub_weight, Some(1.0), "the hub is the most-connected node");

        // The heavier signals accept the generic graph too (compile + run proof).
        let _clusters = community_louvain(&g);
        let _affinity = structural_affinity(&g, 0.0);
        let _articulation = articulation_points(&g);
    }
}
