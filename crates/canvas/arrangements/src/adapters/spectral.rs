/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! [`LayoutStrategy`] adapter for a **spectral** layout: node positions are the two non-trivial
//! smallest eigenvectors of the graph Laplacian, so the layout reflects connectivity — clusters
//! separate spatially and a path unrolls into a line. It complements community detection (the
//! clusters it separates are the ones the rings colour).
//!
//! Deterministic and dependency-free: power iteration with deflation on `B = cI - L` (whose largest
//! eigenvectors are L's smallest), fine for the node counts a browse graph reaches. It is the
//! expensive analytic layout the arrangement cache exists for — recomputed only on a structural
//! change, not per frame. (Graph signals — P5, spectral arrangement.)

use std::collections::HashMap;

use euclid::default::Point2D;
use kernel::graph::{Graph, NodeKey};

use super::shared::{bounds_of, build_positioned_edges};
use cartography::projection::{PositionedNode, Projection, ProjectionMetadata};
use cartography::request::ProjectionRequest;
use cartography::strategy::LayoutStrategy;

/// Where a spectral layout centers and how far it spreads.
#[derive(Debug, Clone, Copy)]
pub struct SpectralConfig {
    pub center: Point2D<f32>,
    /// The half-extent the eigenvector coords are auto-fit into (world units).
    pub scale: f32,
    /// Power-iteration steps per eigenvector. A few hundred converges for the node counts here.
    pub iterations: usize,
}

impl Default for SpectralConfig {
    fn default() -> Self {
        Self {
            center: Point2D::new(0.0, 0.0),
            scale: 320.0,
            iterations: 200,
        }
    }
}

/// Cartography-side adapter for the spectral layout.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpectralAdapter {
    pub config: SpectralConfig,
}

impl SpectralAdapter {
    pub const PROJECTION_ID: &'static str = "spectral.default";

    pub fn new(config: SpectralConfig) -> Self {
        Self { config }
    }
}

impl LayoutStrategy for SpectralAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let ordered_keys: Vec<NodeKey> = request.graph.nodes().map(|(key, _)| key).collect();
        let n = ordered_keys.len();
        let metadata = ProjectionMetadata {
            strategy_id: Some(self.projection_id().to_string()),
            settled: true,
        };
        if n == 0 {
            return Projection {
                metadata,
                ..Projection::empty()
            };
        }

        let adjacency = weighted_adjacency(request.graph, &ordered_keys);
        let coords = spectral_coords(&adjacency, self.config.iterations);
        let center = self.config.center;
        let scale = self.config.scale;

        // Auto-fit the (small, unit-ish) eigenvector coords into +/- scale, centered. If the spectral
        // coords collapse (an edgeless or perfectly symmetric graph has no structure to project),
        // fall back to a deterministic circle so nodes never pile on one point.
        let max_abs = coords
            .iter()
            .flat_map(|&(x, y)| [x.abs(), y.abs()])
            .fold(0.0_f64, f64::max);
        let mut positions: HashMap<NodeKey, Point2D<f32>> = HashMap::with_capacity(n);
        if max_abs > 1e-9 {
            let s = scale as f64 / max_abs;
            for (idx, &key) in ordered_keys.iter().enumerate() {
                let (x, y) = coords[idx];
                let pos = Point2D::new(center.x + (x * s) as f32, center.y + (y * s) as f32);
                positions.insert(key, pos);
            }
        } else {
            for (idx, &key) in ordered_keys.iter().enumerate() {
                let theta = idx as f32 / n as f32 * std::f32::consts::TAU;
                let pos = Point2D::new(
                    center.x + scale * theta.cos(),
                    center.y + scale * theta.sin(),
                );
                positions.insert(key, pos);
            }
        }

        let nodes: Vec<PositionedNode> = positions
            .iter()
            .map(|(key, pos)| PositionedNode {
                node: *key,
                position: *pos,
                radius: 0.0,
            })
            .collect();
        let edges = build_positioned_edges(request, &positions);
        Projection {
            nodes,
            edges,
            overlays: Vec::new(),
            minimap: None,
            content_bounds: bounds_of(&positions),
            metadata,
        }
    }
}

/// Index-based weighted undirected adjacency (multigraph multiplicity summed, self-loops dropped,
/// rows sorted for determinism), matching the structural weighting the other signals use.
fn weighted_adjacency(graph: &Graph, keys: &[NodeKey]) -> Vec<Vec<(usize, f64)>> {
    let index: HashMap<NodeKey, usize> = keys.iter().enumerate().map(|(i, &k)| (k, i)).collect();
    let mut rows: Vec<HashMap<usize, f64>> = vec![HashMap::new(); keys.len()];
    for (i, &key) in keys.iter().enumerate() {
        for nb in graph.neighbors_undirected(key) {
            if let Some(&j) = index.get(&nb) {
                if i != j {
                    *rows[i].entry(j).or_insert(0.0) += 1.0;
                }
            }
        }
    }
    rows.into_iter()
        .map(|m| {
            let mut row: Vec<(usize, f64)> = m.into_iter().collect();
            row.sort_unstable_by_key(|&(j, _)| j);
            row
        })
        .collect()
}

/// The (x, y) per-node coords: the 2nd and 3rd smallest Laplacian eigenvectors. Deterministic.
fn spectral_coords(adj: &[Vec<(usize, f64)>], iterations: usize) -> Vec<(f64, f64)> {
    let evs = smallest_laplacian_eigenvectors(adj, 2, iterations);
    (0..adj.len()).map(|i| (evs[0][i], evs[1][i])).collect()
}

/// Power-iterate the `count` smallest non-trivial Laplacian eigenvectors via `B = cI - L` (whose
/// largest eigenvectors are L's smallest), deflating against the all-ones vector and each previously
/// found one. Each returned vector is unit-norm with zero mean (or all-zero if the graph is too
/// small / degenerate to support it). `c = 2·max_degree` is the tight Gershgorin bound, so `B` is
/// positive-semidefinite and power iteration converges to the wanted (smallest-L) end.
fn smallest_laplacian_eigenvectors(
    adj: &[Vec<(usize, f64)>],
    count: usize,
    iterations: usize,
) -> Vec<Vec<f64>> {
    let n = adj.len();
    let degree: Vec<f64> = adj
        .iter()
        .map(|row| row.iter().map(|&(_, w)| w).sum())
        .collect();
    let c = degree.iter().copied().fold(0.0_f64, f64::max) * 2.0;
    let mut found: Vec<Vec<f64>> = Vec::new();
    for eigen_idx in 0..count {
        let mut v: Vec<f64> = (0..n).map(|i| start_value(i, eigen_idx, n)).collect();
        orthonormalize(&mut v, &found);
        // No edges (c == 0) leaves B = 0, so iteration cannot find structure: return the zeroed
        // vector and let the caller's circle fallback place the nodes.
        if c > 0.0 {
            for _ in 0..iterations {
                let mut w = vec![0.0; n];
                for i in 0..n {
                    let mut s = (c - degree[i]) * v[i];
                    for &(j, wij) in &adj[i] {
                        s += wij * v[j];
                    }
                    w[i] = s;
                }
                v = w;
                orthonormalize(&mut v, &found);
            }
        }
        found.push(v);
    }
    found
}

/// Subtract the all-ones component (zero the mean) and the projection onto each `found` (unit)
/// vector, then normalize. Leaves `v` all-zero if it collapses — the degenerate / too-small case.
fn orthonormalize(v: &mut [f64], found: &[Vec<f64>]) {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    for x in v.iter_mut() {
        *x -= mean;
    }
    for f in found {
        let dot: f64 = v.iter().zip(f).map(|(a, b)| a * b).sum();
        for (x, fx) in v.iter_mut().zip(f) {
            *x -= dot * fx;
        }
    }
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 1e-12 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// A deterministic starting vector for eigenvector `eigen_idx`: a low-frequency cosine that overlaps
/// the corresponding Laplacian eigenvector (a path graph's eigenvectors *are* cosines), varied by
/// index so successive starts are not parallel.
fn start_value(i: usize, eigen_idx: usize, n: usize) -> f64 {
    let t = i as f64 / n.max(1) as f64;
    ((eigen_idx + 1) as f64 * std::f64::consts::PI * t).cos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::fixtures::GraphFixtures;
    use cartography::request::ViewIntent;
    use cartography::signals::IntelligenceSignals;
    use kernel::geometry::PortablePoint;
    use kernel::graph::Graph;

    fn project(graph: &Graph) -> HashMap<NodeKey, Point2D<f32>> {
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let projection = SpectralAdapter::default().project(&request);
        projection
            .nodes
            .iter()
            .map(|n| (n.node, n.position))
            .collect()
    }

    #[test]
    fn projection_id_is_stable() {
        assert_eq!(
            SpectralAdapter::default().projection_id(),
            "spectral.default"
        );
    }

    #[test]
    fn empty_graph_is_empty_and_settled() {
        let graph = Graph::new();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let projection = SpectralAdapter::default().project(&request);
        assert!(projection.nodes.is_empty());
        assert!(projection.metadata.settled);
    }

    #[test]
    fn path_unrolls_into_a_monotonic_line() {
        // A path 0-1-2-3-4: the Fiedler vector orders the nodes along the path, so the x coordinate
        // is monotonic from one end to the other (a classic spectral result).
        let mut graph = Graph::new();
        let n: Vec<NodeKey> = (0..5)
            .map(|i| graph.add_node(format!("https://{i}.example"), PortablePoint::new(0.0, 0.0)))
            .collect();
        for w in n.windows(2) {
            graph.assert_semantic_predicate(w[0], w[1], "links".to_string());
        }
        let pos = project(&graph);
        let xs: Vec<f32> = n.iter().map(|k| pos[k].x).collect();
        let ascending = xs.windows(2).all(|w| w[0] < w[1]);
        let descending = xs.windows(2).all(|w| w[0] > w[1]);
        assert!(
            ascending || descending,
            "the path's nodes are ordered monotonically along the Fiedler axis: {xs:?}"
        );
    }

    #[test]
    fn two_clusters_separate() {
        // Two triangles {0,1,2} and {3,4,5} joined by one bridge: the spectral layout pushes the
        // two clusters to opposite ends, so the inter-cluster gap exceeds the intra-cluster spread.
        let mut graph = Graph::new();
        let n: Vec<NodeKey> = (0..6)
            .map(|i| graph.add_node(format!("https://{i}.example"), PortablePoint::new(0.0, 0.0)))
            .collect();
        for &(a, b) in &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)] {
            graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
        }
        let pos = project(&graph);
        let centroid = |keys: &[usize]| {
            let (mut x, mut y) = (0.0f32, 0.0f32);
            for &i in keys {
                x += pos[&n[i]].x;
                y += pos[&n[i]].y;
            }
            Point2D::new(x / keys.len() as f32, y / keys.len() as f32)
        };
        let a = centroid(&[0, 1, 2]);
        let b = centroid(&[3, 4, 5]);
        let gap = (a - b).length();
        let spread = |keys: &[usize], c: Point2D<f32>| {
            keys.iter()
                .map(|&i| (pos[&n[i]] - c).length())
                .fold(0.0f32, f32::max)
        };
        let intra = spread(&[0, 1, 2], a).max(spread(&[3, 4, 5], b));
        assert!(
            gap > intra,
            "the clusters separate: gap {gap} > intra-spread {intra}"
        );
    }

    #[test]
    fn is_deterministic() {
        let mut graph = Graph::new();
        let n: Vec<NodeKey> = (0..6)
            .map(|i| graph.add_node(format!("https://{i}.example"), PortablePoint::new(0.0, 0.0)))
            .collect();
        for &(a, b) in &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)] {
            graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
        }
        assert_eq!(
            project(&graph),
            project(&graph),
            "same graph => same layout"
        );
    }
}
