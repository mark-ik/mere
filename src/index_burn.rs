/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Batched-cosine index kernel (the `index-burn` feature).
//!
//! [`VectorIndex::nearest`](crate::VectorIndex::nearest) scores a query against
//! every entry on the CPU, `O(N·d)` per query. For a batch of `Q` queries that is
//! `O(Q·N·d)` — and that is a **matmul**: normalize both sides to unit rows and
//! the cosine similarity matrix is `queries · corpusᵀ`. So [`cosine_top_k`] runs
//! that one matmul on burn (ndarray, or wgpu under `index-burn-wgpu`), where
//! `O(Q·N·d)` matmul throughput is exactly what the hardware is built for, then
//! takes each query's top-k on the CPU over the read-back scores.
//!
//! This is the same tensor-program shape as a tensorized N-body force pass: a
//! dense pairwise interaction over `[Q,d]` and `[N,d]`, reduced. The GPU wins from
//! roughly a thousand entries up (dispatch + the `[Q,N]` readback dominate below
//! that); the crossover is measured, not assumed.
//!
//! **Honest bounds.** The result is exact (no approximation — this is brute force
//! on faster hardware, not HNSW). The top-k runs on the CPU over the full `[Q,N]`
//! score matrix, so the readback is `O(Q·N)`; the heavy `O(Q·N·d)` matmul is what
//! moves to the GPU. For very large `Q·N` a GPU top-k (returning only `[Q,k]`)
//! would shrink the readback; that is a refinement, not this cut.

use burn::tensor::{Tensor, TensorData, backend::Backend};

/// For each query row, the `k` corpus entries most similar by **cosine**, as
/// `(corpus_index, score)` pairs sorted best-first. Rows of `queries` and `corpus`
/// must share the same dimension `d`.
///
/// The `O(Q·N·d)` normalize-and-matmul runs on backend `B`; the per-query top-k
/// runs on the CPU over the read-back `[Q,N]` scores. Zero-length rows (a
/// token-free embedding) score 0 against everything, matching
/// [`VectorIndex`](crate::VectorIndex)'s cosine handling.
pub fn cosine_top_k<B: Backend>(
    queries: &[Vec<f32>],
    corpus: &[Vec<f32>],
    k: usize,
    device: &B::Device,
) -> Vec<Vec<(usize, f32)>> {
    let q = queries.len();
    let n = corpus.len();
    if q == 0 {
        return Vec::new();
    }
    if n == 0 || k == 0 {
        return vec![Vec::new(); q];
    }
    let dim = corpus[0].len();
    let k = k.min(n);

    // [Q, N] cosine similarity as one matmul on unit-normalized rows.
    let qn = l2_normalize_rows(rows_to_tensor::<B>(queries, dim, device));
    let cn = l2_normalize_rows(rows_to_tensor::<B>(corpus, dim, device));
    let sim = qn.matmul(cn.swap_dims(0, 1)); // [Q, d] · [d, N] = [Q, N]
    let scores = sim
        .into_data()
        .to_vec::<f32>()
        .expect("similarity readback is f32");

    // Per-query top-k over the read-back row (cheap O(N) vs the matmul above).
    let mut out = Vec::with_capacity(q);
    for qi in 0..q {
        let row = &scores[qi * n..(qi + 1) * n];
        let mut scored: Vec<(usize, f32)> = row.iter().copied().enumerate().collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        out.push(scored);
    }
    out
}

/// Stack `rows` (each length `dim`) into a `[rows.len(), dim]` tensor on `B`.
fn rows_to_tensor<B: Backend>(rows: &[Vec<f32>], dim: usize, device: &B::Device) -> Tensor<B, 2> {
    let mut flat = Vec::with_capacity(rows.len() * dim);
    for row in rows {
        flat.extend_from_slice(row);
    }
    Tensor::from_data(TensorData::new(flat, [rows.len(), dim]), device)
}

/// Scale each row of a `[R, d]` tensor to unit L2 length. A zero row (norm ~0)
/// stays ~zero (the `1e-12` guard divides it by epsilon, not by zero), so its
/// cosine against anything is 0 — matching [`VectorIndex`](crate::VectorIndex).
fn l2_normalize_rows<B: Backend>(t: Tensor<B, 2>) -> Tensor<B, 2> {
    let sq_sum = (t.clone() * t.clone()).sum_dim(1); // [R, 1]
    let inv_norm = sq_sum.sqrt().add_scalar(1e-12).recip(); // [R, 1]
    t * inv_norm // [R, d] * [R, 1] broadcasts over the row
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::VectorIndex;
    use crate::provider::SimilarityMetric;

    /// A small corpus with strictly-distinct cosines to a query (no tie
    /// ambiguity), plus the query.
    fn fixture() -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let corpus = vec![
            vec![1.0, 0.0, 0.0],  // 0: cosine 1.000 to the query
            vec![0.9, 0.1, 0.0],  // 1: cosine ~0.994
            vec![0.5, 0.5, 0.0],  // 2: cosine ~0.707
            vec![0.0, 1.0, 0.0],  // 3: cosine 0
            vec![0.0, 0.0, 1.0],  // 4: cosine 0
        ];
        let queries = vec![vec![1.0, 0.0, 0.0]];
        (queries, corpus)
    }

    #[test]
    fn matches_vector_index_nearest() {
        type B = burn::backend::NdArray<f32>;
        let (queries, corpus) = fixture();
        let k = 3;
        let got = cosine_top_k::<B>(&queries, &corpus, k, &Default::default());

        // Reference: the CPU flat index this kernel is meant to accelerate.
        let mut idx = VectorIndex::<usize>::new(3, SimilarityMetric::Cosine);
        for (i, v) in corpus.iter().enumerate() {
            idx.insert(i, v.clone()).unwrap();
        }
        let want = idx.nearest(&queries[0], k).unwrap();

        assert_eq!(got[0].len(), want.len(), "same k");
        for ((gi, gs), (wi, ws)) in got[0].iter().zip(&want) {
            assert_eq!(gi, wi, "same ranked index");
            assert!((gs - ws).abs() < 1e-5, "score {gs} vs reference {ws}");
        }
    }

    #[test]
    fn all_pairs_ranks_self_first() {
        // Queries == corpus: each item's nearest is itself at cosine ~1.
        type B = burn::backend::NdArray<f32>;
        let (_, corpus) = fixture();
        let got = cosine_top_k::<B>(&corpus, &corpus, 1, &Default::default());
        for (i, row) in got.iter().enumerate() {
            assert_eq!(row[0].0, i, "item {i} should be its own nearest");
            assert!((row[0].1 - 1.0).abs() < 1e-5, "self-cosine ~1, got {}", row[0].1);
        }
    }

    #[test]
    fn edges_are_handled() {
        type B = burn::backend::NdArray<f32>;
        let dev = Default::default();
        let corpus = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        // No queries → no rows.
        assert!(cosine_top_k::<B>(&[], &corpus, 2, &dev).is_empty());
        // Empty corpus / k=0 → one empty result per query.
        assert_eq!(
            cosine_top_k::<B>(&[vec![1.0, 0.0]], &[], 2, &dev),
            vec![Vec::<(usize, f32)>::new()]
        );
        assert_eq!(
            cosine_top_k::<B>(&[vec![1.0, 0.0]], &corpus, 0, &dev),
            vec![Vec::<(usize, f32)>::new()]
        );
        // k clamps to the corpus size.
        assert_eq!(cosine_top_k::<B>(&[vec![1.0, 0.0]], &corpus, 99, &dev)[0].len(), 2);
    }

    #[test]
    fn zero_query_scores_zero_everywhere() {
        // A token-free (zero) query vector: cosine 0 to all, like the flat index.
        type B = burn::backend::NdArray<f32>;
        let corpus = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        let got = cosine_top_k::<B>(&[vec![0.0, 0.0, 0.0]], &corpus, 2, &Default::default());
        for (_, s) in &got[0] {
            assert!(s.abs() < 1e-6, "zero query scores ~0, got {s}");
        }
    }

    /// ndarray↔wgpu parity: the kernel is backend-generic, so the GPU path must
    /// rank the same and score within float tolerance. Gated on the wgpu backend
    /// (a real GPU); run with `--features index-burn-wgpu`.
    #[cfg(feature = "index-burn-wgpu")]
    #[test]
    fn parity_ndarray_wgpu() {
        type Cpu = burn::backend::NdArray<f32>;
        type Gpu = burn::backend::Wgpu<f32, i32>;
        let (queries, corpus) = fixture();
        let cpu = cosine_top_k::<Cpu>(&queries, &corpus, 4, &Default::default());
        let gpu = cosine_top_k::<Gpu>(&queries, &corpus, 4, &Default::default());
        assert_eq!(cpu.len(), gpu.len());
        for (rc, rg) in cpu.iter().zip(&gpu) {
            assert_eq!(rc.len(), rg.len());
            for ((ci, cs), (gi, gs)) in rc.iter().zip(rg) {
                assert_eq!(ci, gi, "same ranked index across backends");
                assert!((cs - gs).abs() < 1e-4, "score parity {cs} vs {gs}");
            }
        }
    }
}
