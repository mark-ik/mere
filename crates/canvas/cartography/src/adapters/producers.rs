// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Graph work whose result is a disclosure.
//!
//! Two arrangements read graph topology: rings come from a breadth-first walk,
//! and spectral coordinates come from the graph Laplacian. Neither computation
//! belongs in a solver — `sceno`'s contract is that solvers never learn a
//! source's native truth, and a score carries no relations to learn it from.
//!
//! Both reduce to a small per-item value, so the walk happens once here, beside
//! the graph that already exists, and what crosses into the score is a ring
//! index or a pair of coordinates. This is the same shape `Hulls` has always
//! used: the adapter discloses a coordinate, the solver tiles.

use std::collections::{HashMap, VecDeque};

use kernel::graph::{Graph, NodeKey};
use sceno::Vec2;

/// Breadth-first ring index from `focus`. The focus is ring zero.
///
/// Nodes unreachable from the focus are absent from the map rather than given a
/// sentinel ring: "no ring" is what `RadialUnreachablePolicy` exists to answer,
/// and a sentinel would have to be a number the solver could not tell from a
/// real ring.
pub fn radial_rings(graph: &Graph, focus: NodeKey) -> HashMap<NodeKey, u32> {
    let mut ring_of: HashMap<NodeKey, u32> = HashMap::new();
    if graph.get_node(focus).is_none() {
        return ring_of;
    }
    ring_of.insert(focus, 0);

    let mut queue: VecDeque<NodeKey> = VecDeque::from([focus]);
    while let Some(key) = queue.pop_front() {
        let next = ring_of[&key] + 1;
        for neighbour in graph.neighbors_undirected(key) {
            if !ring_of.contains_key(&neighbour) {
                ring_of.insert(neighbour, next);
                queue.push_back(neighbour);
            }
        }
    }
    ring_of
}

/// Undirected degree plus one, as the angular weight for
/// [`sceno::RadialAngularPolicy::Weighted`].
///
/// The plus-one is why this is disclosed rather than derived: it guarantees a
/// zero-degree node still gets a slot, and it matches the solver's own default
/// of `1.0` for an item that disclosed no weight, so an isolated node and an
/// undisclosed one occupy the same arc.
pub fn degree_weights(graph: &Graph) -> HashMap<NodeKey, f32> {
    graph
        .nodes()
        .map(|(key, _)| {
            let degree = graph
                .neighbors_undirected(key)
                .filter(|neighbour| *neighbour != key)
                .count();
            (key, (degree + 1) as f32)
        })
        .collect()
}

/// Per-node coordinates from the two smallest non-trivial Laplacian
/// eigenvectors, normalized into roughly `[-1, 1]`.
///
/// Normalizing here rather than in the solver is deliberate: eigenvector
/// components come out a few thousandths wide, and only this side knows that.
/// [`sceno::Embedded`] then applies the caller's origin and scale, which is the
/// same arithmetic the old `SpectralAdapter` did inline after its own auto-fit.
///
/// Returns an empty map when the graph has no structure to project — an
/// edgeless or perfectly symmetric graph collapses every component to zero.
/// Empty rather than all-zeros, so `EmbeddingFallback` decides what happens to
/// nodes with nothing to place them by, instead of stacking them all at once
/// point.
pub fn spectral_coords(graph: &Graph, iterations: usize) -> HashMap<NodeKey, Vec2> {
    let keys: Vec<NodeKey> = graph.nodes().map(|(key, _)| key).collect();
    if keys.is_empty() {
        return HashMap::new();
    }
    let adjacency = weighted_adjacency(graph, &keys);
    let vectors = smallest_laplacian_eigenvectors(&adjacency, 2, iterations);
    let coords: Vec<(f64, f64)> = (0..keys.len())
        .map(|index| (vectors[0][index], vectors[1][index]))
        .collect();

    let max_abs = coords
        .iter()
        .flat_map(|(x, y)| [x.abs(), y.abs()])
        .fold(0.0_f64, f64::max);
    if max_abs <= 1e-9 {
        return HashMap::new();
    }
    keys.iter()
        .zip(&coords)
        .map(|(key, (x, y))| {
            (
                *key,
                Vec2::new((x / max_abs) as f32, (y / max_abs) as f32),
            )
        })
        .collect()
}

/// Index-based weighted undirected adjacency: multigraph multiplicity summed,
/// self-loops dropped, rows sorted for determinism.
fn weighted_adjacency(graph: &Graph, keys: &[NodeKey]) -> Vec<Vec<(usize, f64)>> {
    let index: HashMap<NodeKey, usize> =
        keys.iter().enumerate().map(|(i, key)| (*key, i)).collect();
    let mut rows: Vec<HashMap<usize, f64>> = vec![HashMap::new(); keys.len()];
    for (i, key) in keys.iter().enumerate() {
        for neighbour in graph.neighbors_undirected(*key) {
            if let Some(&j) = index.get(&neighbour) {
                if i != j {
                    *rows[i].entry(j).or_insert(0.0) += 1.0;
                }
            }
        }
    }
    rows.into_iter()
        .map(|row| {
            let mut row: Vec<(usize, f64)> = row.into_iter().collect();
            row.sort_unstable_by_key(|(j, _)| *j);
            row
        })
        .collect()
}

/// Power-iterate the `count` smallest non-trivial Laplacian eigenvectors via
/// `B = cI - L`, whose largest eigenvectors are `L`'s smallest, deflating
/// against the all-ones vector and each previously found one.
///
/// `c = 2·max_degree` is the tight Gershgorin bound, so `B` is
/// positive-semidefinite and the iteration converges to the wanted end. Each
/// returned vector is unit-norm with zero mean, or all-zero when the graph is
/// too small or degenerate to support one.
fn smallest_laplacian_eigenvectors(
    adjacency: &[Vec<(usize, f64)>],
    count: usize,
    iterations: usize,
) -> Vec<Vec<f64>> {
    let n = adjacency.len();
    let degree: Vec<f64> = adjacency
        .iter()
        .map(|row| row.iter().map(|(_, weight)| weight).sum())
        .collect();
    let c = degree.iter().copied().fold(0.0_f64, f64::max) * 2.0;

    let mut found: Vec<Vec<f64>> = Vec::new();
    for eigen_index in 0..count {
        let mut vector: Vec<f64> = (0..n).map(|i| start_value(i, eigen_index, n)).collect();
        orthonormalize(&mut vector, &found);
        // No edges leaves `B = 0`, so iteration cannot find structure. The
        // zeroed vector falls through to the caller's empty-map return.
        if c > 0.0 {
            for _ in 0..iterations {
                let mut next = vec![0.0; n];
                for i in 0..n {
                    let mut sum = (c - degree[i]) * vector[i];
                    for (j, weight) in &adjacency[i] {
                        sum += weight * vector[*j];
                    }
                    next[i] = sum;
                }
                vector = next;
                orthonormalize(&mut vector, &found);
            }
        }
        found.push(vector);
    }
    found
}

/// Subtract the all-ones component and the projection onto each found vector,
/// then normalize. Leaves the vector all-zero if it collapses.
fn orthonormalize(vector: &mut [f64], found: &[Vec<f64>]) {
    let n = vector.len() as f64;
    let mean = vector.iter().sum::<f64>() / n;
    for value in vector.iter_mut() {
        *value -= mean;
    }
    for previous in found {
        let dot: f64 = vector.iter().zip(previous).map(|(a, b)| a * b).sum();
        for (value, component) in vector.iter_mut().zip(previous) {
            *value -= dot * component;
        }
    }
    let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm > 1e-12 {
        for value in vector.iter_mut() {
            *value /= norm;
        }
    }
}

/// A deterministic starting vector: a low-frequency cosine that overlaps the
/// corresponding Laplacian eigenvector — a path graph's eigenvectors *are*
/// cosines — varied by index so successive starts are not parallel.
fn start_value(i: usize, eigen_index: usize, n: usize) -> f64 {
    let t = i as f64 / n.max(1) as f64;
    ((eigen_index + 1) as f64 * std::f64::consts::PI * t).cos()
}
