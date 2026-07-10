/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host wiring for **content-affinity arrangement** (burn brief Lane 5, P4):
//! embed each node's text, derive top-K similar pairs, and inject them into the
//! orrery as the affinity-force signal — semantically-related nodes cluster even
//! with no direct edge, superseding the internal structural-Jaccard signal while
//! set.
//!
//! The provider is chosen at build time, mirroring [`infer_host`](crate::infer_host):
//!
//! - Default (this `content-affinity` feature, no `semantic-embeddings`): the
//!   **lexical** feature-hashing provider — pure Rust, burn-free, always
//!   available. It clusters by *shared vocabulary* (a real, cheap signal), not
//!   deep meaning.
//! - `semantic-embeddings` (the upgrade, a separate slice): a Burn-backed BERT
//!   model for true semantic similarity. The extension point is
//!   [`build_embedding_provider`]; it wants an embed boxed loader mirroring
//!   `infer::decoder::load_wgpu_provider`, plus the D1 device decision.
//!
//! Burn stays out of the orrery: the orrery takes plain `(NodeKey, NodeKey, f32)`
//! triples through [`mere::canvas::set_content_affinity`](mere::canvas::Canvas::set_content_affinity);
//! this module (behind the feature) owns the provider.

use std::time::{Duration, Instant};

use embed::{EmbeddingProvider, LexicalEmbeddingProvider, VectorIndex, affinity_pairs};
use mere::kernel::graph::{Graph, Node, NodeKey};

/// Output dimension (hash buckets) for the lexical provider — enough to keep
/// token collisions low for short texts (titles + tag sets).
const EMBED_DIMENSIONS: usize = 384;
/// Each node's this-many nearest neighbours enter the affinity signal.
const AFFINITY_TOP_K: usize = 6;
/// Minimum cosine similarity for a pair to enter the signal. Prunes the weak-
/// overlap long tail so only meaningful clusters pull. Tuned for the lexical
/// provider's cosine range; a semantic provider would want a higher floor.
const AFFINITY_MIN_SIMILARITY: f32 = 0.35;
/// Throttle floor: the graph can move many revisions in a burst (a crawl importing
/// dozens of nodes), and re-embedding the whole graph each time would thrash the UI
/// thread. Coalesce recomputes to at most one per this interval; a blocked pass
/// retries on a later frame (the revision gate stays armed). (O(N²) index scan —
/// large graphs want the off-thread actor; see the plan.)
const DEFAULT_RECOMPUTE_GAP: Duration = Duration::from_millis(750);

/// Host-side content-affinity state: the embedding provider plus the recompute
/// gate. Lives on `Content` behind the `content-affinity` feature; the orrery
/// render path calls [`maybe_recompute`](Self::maybe_recompute) and injects the
/// result.
pub(crate) struct ContentArrangement {
    provider: Box<dyn EmbeddingProvider>,
    /// The graph revision the live signal was computed for; `None` before the
    /// first compute. Recompute only when this moves (a content/topology change).
    last_revision: Option<u64>,
    /// When the last recompute ran, for the throttle floor.
    last_compute: Option<Instant>,
    /// Minimum spacing between recomputes.
    recompute_gap: Duration,
}

impl ContentArrangement {
    /// Build with the configured embedding provider and default throttle.
    pub(crate) fn new() -> Self {
        Self {
            provider: build_embedding_provider(),
            last_revision: None,
            last_compute: None,
            recompute_gap: DEFAULT_RECOMPUTE_GAP,
        }
    }

    /// Recompute the content-affinity signal for `graph` and return the fresh
    /// pairs — but only when the graph revision has moved since the last compute
    /// and the throttle floor has elapsed. Returns `None` when nothing changed or
    /// the throttle blocks (a later frame retries), so the caller injects only on a
    /// real change.
    pub(crate) fn maybe_recompute(
        &mut self,
        graph: &Graph,
    ) -> Option<Vec<(NodeKey, NodeKey, f32)>> {
        let revision = graph.revision();
        if self.last_revision == Some(revision) {
            return None; // already current for this revision
        }
        if let Some(t) = self.last_compute {
            if t.elapsed() < self.recompute_gap {
                return None; // throttled — leave last_revision stale so a later frame retries
            }
        }
        self.last_revision = Some(revision);
        self.last_compute = Some(Instant::now());
        Some(compute_content_affinity(graph, self.provider.as_ref()))
    }

    /// Build with an explicit throttle gap (tests: `ZERO` disables the throttle so
    /// every revision change recomputes; a large gap forces the blocked path).
    #[cfg(test)]
    pub(crate) fn with_gap(recompute_gap: Duration) -> Self {
        Self {
            recompute_gap,
            ..Self::new()
        }
    }
}

/// Build the content-affinity signal for `graph`: embed each node's text, index
/// it, and derive each node's top-K similar neighbours as `(a, b, weight)`
/// triples. Pure over the graph + provider; the caller owns throttling and
/// injection. An embedding failure logs and yields no pairs (the orrery keeps its
/// prior signal / falls back to structural on `None`).
pub(crate) fn compute_content_affinity(
    graph: &Graph,
    provider: &dyn EmbeddingProvider,
) -> Vec<(NodeKey, NodeKey, f32)> {
    let nodes: Vec<(NodeKey, String)> = graph.nodes().map(|(k, n)| (k, node_text(n))).collect();
    if nodes.len() < 2 {
        return Vec::new(); // an affinity pair needs two nodes
    }
    // One batched embed call (amortises dispatch; a GPU provider needs the batch).
    let texts: Vec<&str> = nodes.iter().map(|(_, t)| t.as_str()).collect();
    let vectors = match provider.embed(&texts) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(target: "meerkat::semantic", error = %e, "node embedding failed; no content affinity this pass");
            return Vec::new();
        }
    };
    let mut index = VectorIndex::new(provider.dimensions(), provider.metric());
    for ((key, _), vector) in nodes.iter().zip(vectors) {
        // Dimension is the provider's own, so insert cannot mismatch; ignore the Result.
        let _ = index.insert(*key, vector);
    }
    affinity_pairs(&index, AFFINITY_TOP_K, AFFINITY_MIN_SIMILARITY).unwrap_or_else(|e| {
        tracing::warn!(target: "meerkat::semantic", error = ?e, "affinity derivation failed");
        Vec::new()
    })
}

/// The text a node contributes to its embedding: its title (always present — the
/// URL when a page has no title), its curated tags (semantic labels), and its
/// literal **property values** — the `schema:description` / `og:description` and
/// kin that ingest extracts from the page and stores on the node. Those are the
/// page's own summary of its content: real *content text*, already on the node,
/// so no cache read and no capture-consent gate (they are graph data, not the
/// browsing trail). URL-valued properties (`schema:url`, `og:image`) are dropped —
/// they tokenize to scheme/host noise, not meaning.
///
/// Tags and values are sorted for a stable string across runs (bag-of-words is
/// order-free, but a future order-sensitive provider benefits). The full raw page
/// body (the eidetic content cache) is a richer but noisier source whose per-node
/// parse over the whole graph wants the off-thread embedding actor; deferred.
fn node_text(node: &Node) -> String {
    let mut parts: Vec<&str> = vec![node.title.as_str()];
    let mut tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
    tags.sort_unstable();
    parts.extend(tags);
    let mut values: Vec<&str> = node
        .properties
        .iter()
        .map(|p| p.value.as_str())
        // Skip URL-valued properties: their tokens (https, the host) are noise, and
        // several nodes sharing a scheme would spuriously cluster.
        .filter(|v| !v.contains("://"))
        .collect();
    values.sort_unstable();
    parts.extend(values);
    parts.join(" ")
}

/// Build the embedding provider. Default: the lexical feature-hashing provider
/// (burn-free, clusters by shared vocabulary). The `semantic-embeddings` feature
/// (a later slice) upgrades this to a Burn-backed BERT model — the return type is
/// already `Box<dyn EmbeddingProvider>`, so only this function changes.
fn build_embedding_provider() -> Box<dyn EmbeddingProvider> {
    tracing::info!(target: "meerkat::semantic", dims = EMBED_DIMENSIONS, "content-affinity provider = lexical (feature-hashing, burn-free)");
    Box::new(
        LexicalEmbeddingProvider::new(EMBED_DIMENSIONS)
            .expect("EMBED_DIMENSIONS is a nonzero constant"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere::kernel::geometry::PortablePoint;
    // The public `add_node(url, position) -> NodeKey` used to stage test graphs comes from the
    // kernel `fixtures` feature's trait (the inherent one is `pub(crate)`); meerkat dev-deps it.
    use mere::kernel::graph::fixtures::GraphFixtures;

    fn graph_with_urls(urls: &[&str]) -> Graph {
        let mut graph = Graph::new();
        for (i, url) in urls.iter().enumerate() {
            graph.add_node((*url).to_string(), PortablePoint::new(i as f32, 0.0));
        }
        graph
    }

    #[test]
    fn compute_yields_valid_symmetric_deduped_pairs() {
        // Wiring check (not a semantic-quality check — that is embed's job): the
        // triples are weight-in-range and each unordered pair appears at most once.
        let graph = graph_with_urls(&[
            "https://rust-async-tokio.example",
            "https://rust-async-runtime.example",
            "https://italian-pasta-recipe.example",
        ]);
        let provider = LexicalEmbeddingProvider::new(EMBED_DIMENSIONS).unwrap();
        let pairs = compute_content_affinity(&graph, &provider);

        let mut seen = std::collections::HashSet::new();
        for &(a, b, w) in &pairs {
            assert!((0.0..=1.0).contains(&w), "weight out of range: {w}");
            assert_ne!(a, b, "no self-pair");
            let canon = if a < b { (a, b) } else { (b, a) };
            assert!(seen.insert(canon), "duplicate pair");
        }
    }

    #[test]
    fn compute_needs_two_nodes() {
        let provider = LexicalEmbeddingProvider::new(EMBED_DIMENSIONS).unwrap();
        assert!(compute_content_affinity(&Graph::new(), &provider).is_empty());
        let one = graph_with_urls(&["https://only-one.example"]);
        assert!(compute_content_affinity(&one, &provider).is_empty());
    }

    #[test]
    fn recompute_is_revision_gated() {
        // Same revision → recompute once, then `None`; a mutation (new revision)
        // recomputes again (throttle disabled with a zero gap).
        let mut arr = ContentArrangement::with_gap(Duration::ZERO);
        let mut graph = graph_with_urls(&["https://a.example", "https://b.example"]);

        assert!(arr.maybe_recompute(&graph).is_some(), "first pass computes");
        assert!(
            arr.maybe_recompute(&graph).is_none(),
            "same revision does not recompute"
        );

        graph.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        assert!(
            arr.maybe_recompute(&graph).is_some(),
            "a new revision recomputes"
        );
    }

    #[test]
    fn recompute_is_throttled_within_the_gap() {
        // A large gap forces the throttle: the second (distinct-revision) pass is
        // blocked, and the revision gate stays armed so a later frame would retry.
        let mut arr = ContentArrangement::with_gap(Duration::from_secs(3600));
        let mut graph = graph_with_urls(&["https://a.example", "https://b.example"]);
        assert!(arr.maybe_recompute(&graph).is_some(), "first pass computes");

        graph.add_node(
            "https://c.example".to_string(),
            PortablePoint::new(2.0, 0.0),
        );
        assert!(
            arr.maybe_recompute(&graph).is_none(),
            "a fresh revision within the throttle gap is blocked"
        );
    }
}
