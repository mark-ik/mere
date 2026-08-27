// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Bridge A — query similarity rendered as a 2D scalar field over canvas
//! coordinates.
//!
//! Embeddings are high-dimensional; the field algebra is 2D. To make
//! query-similarity drive the canvas (heatmap background, focus-attraction
//! coupling), we render per-node similarity as a sum of weighted gaussians
//! centered at each node's known position:
//!
//! ```text
//! field(x, y) = Σ_i  s_i · gaussian(p_i, σ)(x, y)
//! ```
//!
//! Where `s_i` is the similarity between the query embedding and node `i`'s
//! embedding, `p_i` is node `i`'s 2D position on the canvas, and `σ` is a
//! tuning parameter (smaller = sharper peaks, larger = smoother field).
//!
//! Crucially this composes inside the existing field-algebra AST — no new
//! AST variant is required. The quint evaluator handles it.

use std::collections::HashMap;
use std::hash::Hash;

use quint::ast::ScalarField;
use quint::projection::FieldProjection;
use quint::registry::FieldId;

use esp::embed::VectorIndex;
use esp::embed::provider::SimilarityMetric;

/// Build a `ScalarField` rendering query-similarity as a sum of weighted
/// gaussians per known node position. Keys missing from `node_positions`
/// are skipped — only nodes the host has placed contribute.
pub fn build_query_similarity_field<K>(
    query: &[f32],
    index: &VectorIndex<K>,
    node_positions: &HashMap<K, (f32, f32)>,
    sigma: f32,
) -> ScalarField
where
    K: Hash + Eq + Clone,
{
    let mut field = ScalarField::Const(0.0);
    let metric = index.metric();
    for (key, vec) in index.iter() {
        let Some(&(x, y)) = node_positions.get(key) else {
            continue;
        };
        let similarity = score(metric, query, vec);
        let term = ScalarField::Scale(Box::new(ScalarField::gaussian_at(x, y, sigma)), similarity);
        field = ScalarField::Add(Box::new(field), Box::new(term));
    }
    field
}

/// Convenience: build the similarity field and register it on a
/// `FieldProjection`. Returns the registered `FieldId`.
pub fn register_query_similarity_field<K>(
    projection: &mut FieldProjection,
    name: &str,
    query: &[f32],
    index: &VectorIndex<K>,
    node_positions: &HashMap<K, (f32, f32)>,
    sigma: f32,
) -> FieldId
where
    K: Hash + Eq + Clone,
{
    let field = build_query_similarity_field(query, index, node_positions, sigma);
    projection.add_scalar(name, field)
}

fn score(metric: SimilarityMetric, a: &[f32], b: &[f32]) -> f32 {
    match metric {
        SimilarityMetric::Cosine => cosine(a, b),
        SimilarityMetric::DotProduct => dot(a, b),
        // Euclidean: lower-is-better; we negate so larger-is-more-similar
        // for use as a field weight (so a node *closer* to the query
        // contributes a *larger* gaussian peak).
        SimilarityMetric::Euclidean => -euclidean(a, b),
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na <= f32::EPSILON || nb <= f32::EPSILON {
        return 0.0;
    }
    dot(a, b) / (na * nb)
}

fn euclidean(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use esp::embed::StubEmbeddingProvider;
    use esp::embed::provider::EmbeddingProvider;
    use quint::eval::eval_scalar;
    use quint::registry::FieldRegistry;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    fn populate(
        provider: &StubEmbeddingProvider,
        nodes: &[(u32, &str, (f32, f32))],
    ) -> (VectorIndex<u32>, HashMap<u32, (f32, f32)>) {
        let mut index = VectorIndex::<u32>::new(provider.dimensions(), provider.metric());
        let mut positions = HashMap::new();
        for (id, text, pos) in nodes {
            let v = provider.embed_one(text).unwrap();
            index.insert(*id, v).unwrap();
            positions.insert(*id, *pos);
        }
        (index, positions)
    }

    #[test]
    fn empty_index_yields_zero_field() {
        let index = VectorIndex::<u32>::new(8, SimilarityMetric::Cosine);
        let positions = HashMap::new();
        let registry = FieldRegistry::new();
        let field = build_query_similarity_field(&[1.0; 8], &index, &positions, 50.0);
        // Sampling anywhere should give zero (Const(0.0) with no gaussians).
        let v = eval_scalar(&field, &registry, 100.0, 200.0, 0.0);
        assert!(approx(v, 0.0, 1.0e-6));
    }

    #[test]
    fn peak_lies_at_matching_node_position() {
        let provider = StubEmbeddingProvider::new(32).unwrap();
        let nodes = [
            (1, "rust async", (100.0, 100.0)),
            (2, "python typing", (500.0, 500.0)),
            (3, "haskell monads", (-200.0, 300.0)),
        ];
        let (index, positions) = populate(&provider, &nodes);
        let registry = FieldRegistry::new();

        // Query identical to node 1's text → similarity ≈ 1 at (100, 100).
        let query = provider.embed_one("rust async").unwrap();
        let field = build_query_similarity_field(&query, &index, &positions, 80.0);

        let at_one = eval_scalar(&field, &registry, 100.0, 100.0, 0.0);
        let at_two = eval_scalar(&field, &registry, 500.0, 500.0, 0.0);
        let at_three = eval_scalar(&field, &registry, -200.0, 300.0, 0.0);

        // Node 1's position should have the highest similarity.
        assert!(at_one > at_two);
        assert!(at_one > at_three);
        // And it should be close to 1.0 (its own similarity, modulo small
        // gaussian leakage from other nodes).
        assert!(at_one > 0.9);
    }

    #[test]
    fn missing_node_position_skipped() {
        let provider = StubEmbeddingProvider::new(16).unwrap();
        let mut index = VectorIndex::<u32>::new(provider.dimensions(), provider.metric());
        index.insert(1, provider.embed_one("a").unwrap()).unwrap();
        index.insert(2, provider.embed_one("b").unwrap()).unwrap();
        // Only node 1 has a position registered.
        let mut positions = HashMap::new();
        positions.insert(1, (50.0, 50.0));

        let registry = FieldRegistry::new();
        let query = provider.embed_one("a").unwrap();
        let field = build_query_similarity_field(&query, &index, &positions, 30.0);

        // At node 2's hypothetical position, no contribution from it (not in
        // positions map). Field there is just gaussian leakage from node 1.
        let v = eval_scalar(&field, &registry, 9999.0, 9999.0, 0.0);
        assert!(approx(v, 0.0, 1.0e-3));
    }

    #[test]
    fn register_returns_id_and_inserts_field() {
        let provider = StubEmbeddingProvider::new(16).unwrap();
        let nodes = [(1, "a", (0.0, 0.0)), (2, "b", (100.0, 0.0))];
        let (index, positions) = populate(&provider, &nodes);

        let mut projection = FieldProjection::new();
        let query = provider.embed_one("a").unwrap();
        let id = register_query_similarity_field(
            &mut projection,
            "focus_similarity",
            &query,
            &index,
            &positions,
            50.0,
        );

        assert_eq!(projection.registry.lookup("focus_similarity"), Some(id));
        assert!(projection.registry.get(id).is_some());
    }

    #[test]
    fn euclidean_metric_negates_to_attract_close_nodes() {
        let mut index = VectorIndex::<u32>::new(2, SimilarityMetric::Euclidean);
        // Two nodes with embeddings at distinct points.
        index.insert(1, vec![0.0, 0.0]).unwrap();
        index.insert(2, vec![10.0, 0.0]).unwrap();
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (200.0, 0.0));
        let registry = FieldRegistry::new();
        // Query equals node 1's embedding → distance to node 1 is 0,
        // distance to node 2 is 10. Negation makes node 1 contribute the
        // largest peak (zero) and node 2 a more-negative term, so node 1's
        // canvas position should hold the field maximum.
        let query = vec![0.0, 0.0];
        let field = build_query_similarity_field(&query, &index, &positions, 80.0);
        let at_one = eval_scalar(&field, &registry, 0.0, 0.0, 0.0);
        let at_two = eval_scalar(&field, &registry, 200.0, 0.0, 0.0);
        assert!(at_one > at_two);
    }
}
