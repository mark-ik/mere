// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Hybrid fusion — the engine-agnostic seam between lexical and vector recall.
//!
//! BM25 + vector fusion is the standard retrieval-quality move, and the
//! geist's factual side rides the same seam (the derivation plan's E4).
//! This module deliberately knows nothing about either engine: the caller
//! brings two **rankings** (URLs, best first) — one from [`TrailIndex::search`],
//! one from whatever vector index it uses (`intel/embed` today) — and gets a
//! fused ranking back by reciprocal-rank fusion:
//!
//! `score(u) = w_lexical / (k + rank_lexical(u)) + w_vector / (k + rank_vector(u))`
//!
//! RRF is rank-based, so the two engines' incomparable score scales never
//! meet; `k` damps the head (60 is the literature's default); the weights
//! are a *setting* (the configurability rule), not a constant.
//!
//! [`TrailIndex::search`]: crate::index::TrailIndex::search

use std::collections::BTreeMap;

/// One fused result: where it ranked in each input, and the fused score.
#[derive(Clone, Debug, PartialEq)]
pub struct FusedHit {
    pub url: String,
    pub score: f64,
    /// 0-based rank in the lexical input, when present there.
    pub lexical_rank: Option<usize>,
    /// 0-based rank in the vector input, when present there.
    pub vector_rank: Option<usize>,
}

/// Reciprocal-rank-fuse two rankings (URLs, best first). `k` damps head
/// dominance (60 unless you have a reason); `weights` are
/// `(lexical, vector)`. Ties break lexicographically by URL so the output
/// is deterministic.
pub fn fuse(lexical: &[String], vector: &[String], k: f64, weights: (f64, f64)) -> Vec<FusedHit> {
    let mut by_url: BTreeMap<&str, FusedHit> = BTreeMap::new();
    for (rank, url) in lexical.iter().enumerate() {
        let entry = by_url.entry(url).or_insert_with(|| FusedHit {
            url: url.clone(),
            score: 0.0,
            lexical_rank: None,
            vector_rank: None,
        });
        entry.lexical_rank = Some(rank);
        entry.score += weights.0 / (k + rank as f64 + 1.0);
    }
    for (rank, url) in vector.iter().enumerate() {
        let entry = by_url.entry(url).or_insert_with(|| FusedHit {
            url: url.clone(),
            score: 0.0,
            lexical_rank: None,
            vector_rank: None,
        });
        entry.vector_rank = Some(rank);
        entry.score += weights.1 / (k + rank as f64 + 1.0);
    }
    let mut fused: Vec<FusedHit> = by_url.into_values().collect();
    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.url.cmp(&b.url))
    });
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_page_ranked_by_both_engines_beats_single_engine_pages() {
        let lexical = urls(&["https://both.example", "https://lex-only.example"]);
        let vector = urls(&["https://vec-only.example", "https://both.example"]);
        let fused = fuse(&lexical, &vector, 60.0, (1.0, 1.0));

        assert_eq!(fused[0].url, "https://both.example");
        assert_eq!(fused[0].lexical_rank, Some(0));
        assert_eq!(fused[0].vector_rank, Some(1));
        // Both single-engine hits surface too — fusion unions, never drops.
        assert_eq!(fused.len(), 3);
        let urls_out: Vec<&str> = fused.iter().map(|h| h.url.as_str()).collect();
        assert!(urls_out.contains(&"https://lex-only.example"));
        assert!(urls_out.contains(&"https://vec-only.example"));
    }

    #[test]
    fn weights_are_a_setting_that_actually_steers() {
        let lexical = urls(&["https://lex.example"]);
        let vector = urls(&["https://vec.example"]);

        let lexical_heavy = fuse(&lexical, &vector, 60.0, (2.0, 1.0));
        assert_eq!(lexical_heavy[0].url, "https://lex.example");

        let vector_heavy = fuse(&lexical, &vector, 60.0, (1.0, 2.0));
        assert_eq!(vector_heavy[0].url, "https://vec.example");
    }

    #[test]
    fn empty_inputs_fuse_to_the_other_side() {
        let vector = urls(&["https://only.example"]);
        let fused = fuse(&[], &vector, 60.0, (1.0, 1.0));
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].url, "https://only.example");
        assert_eq!(fused[0].lexical_rank, None);
        assert_eq!(fused[0].vector_rank, Some(0));
    }
}
