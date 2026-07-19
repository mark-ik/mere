// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Narrow contract for signals from intelligence layers.
//!
//! Cartography does not depend on `embed`. Producers
//! of these signals construct an [`IntelligenceSignals`] value and
//! hand it to cartography; the signal-producer crate's internal shapes
//! never leak through this type.

use kernel::graph::NodeKey;
use serde::{Deserialize, Serialize};

/// Signals from intelligence layers that strategies can consume.
///
/// Every field is optional: strategies that don't need a particular
/// signal simply ignore it, and producers that don't compute one yet
/// leave it `None`. This keeps the contract additive — adding a new
/// signal type doesn't break existing strategies.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceSignals {
    pub clusters: Option<ClusterSet>,
    pub affinity: Option<AffinityScores>,
    pub bridges: Option<BridgeNodes>,
    pub importance: Option<ImportanceWeights>,
    /// Per-node 2D coordinates from a projection step (UMAP / t-SNE /
    /// PCA / etc.) — host runs the projection externally and supplies
    /// the results here. Distinct from [`AffinityScores`] (pairwise
    /// similarity): embeddings are absolute 2D positions ready for
    /// layout consumption.
    pub embeddings: Option<NodeEmbeddings>,
}

/// A partition of nodes into named clusters with confidence scores.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClusterSet {
    pub clusters: Vec<Cluster>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cluster {
    pub id: String,
    pub label: Option<String>,
    pub members: Vec<NodeKey>,
    pub confidence: f32,
}

/// Pairwise affinity scores (e.g. embedding cosine similarity).
///
/// `pairs[(a, b)]` is the affinity from node `a` to node `b`; not
/// necessarily symmetric in general (an embedding model may produce
/// non-symmetric scores when query/document roles differ). Use
/// [`Self::lookup`] to canonicalize when symmetry is expected.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AffinityScores {
    pub pairs: Vec<((NodeKey, NodeKey), f32)>,
}

impl AffinityScores {
    /// Look up the score for an ordered pair, falling back to the
    /// reverse pair if the forward pair isn't recorded.
    pub fn lookup(&self, from: NodeKey, to: NodeKey) -> Option<f32> {
        self.pairs
            .iter()
            .find(|((a, b), _)| (*a == from && *b == to) || (*a == to && *b == from))
            .map(|(_, score)| *score)
    }
}

/// Nodes that bridge communities. Detected by embed
/// or by graph-structural betweenness.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BridgeNodes {
    pub bridges: Vec<NodeKey>,
}

/// Per-node importance weight (normalized 0..=1).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportanceWeights {
    pub weights: Vec<(NodeKey, f32)>,
}

impl ImportanceWeights {
    pub fn lookup(&self, node: NodeKey) -> Option<f32> {
        self.weights
            .iter()
            .find(|(n, _)| *n == node)
            .map(|(_, w)| *w)
    }
}

/// Per-node 2D embedding coordinates from an external projection
/// (UMAP / t-SNE / PCA / similar). Hosts that have a projection
/// pipeline populate this; the
/// [`graph_layout::adapters::SemanticEmbeddingAdapter`]
/// strategy reads it to place nodes at embedded positions directly.
///
/// Coordinates are typically in a host-chosen range (`[-1, 1]` or
/// `[0, 1]`); strategies apply their own `scale` and `origin` config
/// to map them to world-space layout positions. Stored as `(f32, f32)`
/// tuples rather than a geometry type so this contract stays
/// dependency-light.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeEmbeddings {
    pub coords: Vec<(NodeKey, (f32, f32))>,
}

impl NodeEmbeddings {
    pub fn lookup(&self, node: NodeKey) -> Option<(f32, f32)> {
        self.coords
            .iter()
            .find(|(n, _)| *n == node)
            .map(|(_, xy)| *xy)
    }

    pub fn is_empty(&self) -> bool {
        self.coords.is_empty()
    }

    pub fn len(&self) -> usize {
        self.coords.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intelligence_signals_default_is_all_none() {
        let s = IntelligenceSignals::default();
        assert!(s.clusters.is_none());
        assert!(s.affinity.is_none());
        assert!(s.bridges.is_none());
        assert!(s.importance.is_none());
        assert!(s.embeddings.is_none());
    }

    #[test]
    fn node_embeddings_lookup_returns_recorded_coord() {
        let a = NodeKey::new(0);
        let b = NodeKey::new(1);
        let embeddings = NodeEmbeddings {
            coords: vec![(a, (0.5, -0.3))],
        };
        assert_eq!(embeddings.lookup(a), Some((0.5, -0.3)));
        assert_eq!(embeddings.lookup(b), None);
        assert!(!embeddings.is_empty());
        assert_eq!(embeddings.len(), 1);
    }

    #[test]
    fn affinity_scores_lookup_returns_reverse_pair_when_forward_missing() {
        let a = NodeKey::new(0);
        let b = NodeKey::new(1);
        let scores = AffinityScores {
            pairs: vec![((a, b), 0.75)],
        };
        assert_eq!(scores.lookup(a, b), Some(0.75));
        assert_eq!(scores.lookup(b, a), Some(0.75));
    }

    #[test]
    fn affinity_scores_lookup_returns_none_for_unrecorded_pair() {
        let a = NodeKey::new(0);
        let b = NodeKey::new(1);
        let c = NodeKey::new(2);
        let scores = AffinityScores {
            pairs: vec![((a, b), 0.75)],
        };
        assert_eq!(scores.lookup(a, c), None);
    }

    #[test]
    fn importance_weights_lookup_returns_recorded_weight() {
        let a = NodeKey::new(0);
        let weights = ImportanceWeights {
            weights: vec![(a, 0.9)],
        };
        assert_eq!(weights.lookup(a), Some(0.9));
        assert_eq!(weights.lookup(NodeKey::new(1)), None);
    }
}
