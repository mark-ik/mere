/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `CanvasSearchSurface` — host-agnostic adapter combining
//! [`SemanticSearch`], per-node 2D positions, current focus query, and
//! field-bridge (Bridge A) into one stateful surface.
//!
//! This is the API a UI surface (graphshell command palette, future
//! search bar, focus picker) wires against. It owns the state a host
//! needs to drive: ingest nodes as they're created, update positions as
//! layout shifts, set/clear a focus query, and emit a query-similarity
//! scalar field for canvas heatmap rendering or focus-attraction
//! couplings.
//!
//! This module does **not** know about graphshell or any UI toolkit —
//! it composes existing embed + aether
//! primitives. Graphshell-side palette wiring (action registration,
//! input event routing) is its own slice.

use std::collections::HashMap;
use std::hash::Hash;

use aether::projection::FieldProjection;
use aether::registry::FieldId;

use crate::VectorIndex;
use crate::field_bridge::register_query_similarity_field;
use crate::provider::EmbeddingProvider;
use crate::search::{SearchError, SemanticSearch};

/// Stateful canvas-aware search surface.
///
/// Owns: an embedding provider + vector index (via `SemanticSearch`),
/// per-node 2D positions on the canvas, and an optional current focus
/// query embedding. Hosts call [`Self::ingest`] / [`Self::forget`] /
/// [`Self::move_node`] as the graph mutates, [`Self::set_focus_query`]
/// when the user types in the search bar, and
/// [`Self::register_focus_field`] when the canvas wants to render a
/// heatmap.
pub struct CanvasSearchSurface<K: Hash + Eq + Clone, P: EmbeddingProvider> {
    search: SemanticSearch<K, P>,
    positions: HashMap<K, (f32, f32)>,
    focus_query: Option<Vec<f32>>,
    sigma: f32,
}

impl<K: Hash + Eq + Clone, P: EmbeddingProvider> CanvasSearchSurface<K, P> {
    /// Construct from a provider and a default heatmap sigma (gaussian
    /// peak width in canvas units; ~80 for typical force-directed
    /// layouts).
    pub fn new(provider: P, sigma: f32) -> Self {
        Self {
            search: SemanticSearch::new(provider),
            positions: HashMap::new(),
            focus_query: None,
            sigma,
        }
    }

    /// Construct from an existing search facade (e.g. one with a
    /// pre-loaded index).
    pub fn from_search(search: SemanticSearch<K, P>, sigma: f32) -> Self {
        Self {
            search,
            positions: HashMap::new(),
            focus_query: None,
            sigma,
        }
    }

    /// Ingest a node: embed the text, place at the given canvas position.
    pub fn ingest(&mut self, key: K, text: &str, position: (f32, f32)) -> Result<(), SearchError> {
        self.search.ingest(key.clone(), text)?;
        self.positions.insert(key, position);
        Ok(())
    }

    /// Forget a node entirely (drop from index and position map).
    pub fn forget(&mut self, key: &K) {
        self.search.forget(key);
        self.positions.remove(key);
    }

    /// Update a node's canvas position (e.g. after a layout pass moves
    /// it). Does not touch the embedding.
    pub fn move_node(&mut self, key: K, position: (f32, f32)) {
        self.positions.insert(key, position);
    }

    /// Set the current focus query. The query string is embedded once
    /// and cached; subsequent [`Self::register_focus_field`] calls reuse
    /// the cached embedding without re-running the model.
    pub fn set_focus_query(&mut self, query: &str) -> Result<(), SearchError> {
        let q = self.search.provider().embed_one(query)?;
        self.focus_query = Some(q);
        Ok(())
    }

    /// Clear the focus query. After this, [`Self::register_focus_field`]
    /// returns `None`.
    pub fn clear_focus(&mut self) {
        self.focus_query = None;
    }

    /// Whether a focus query is currently set.
    pub fn has_focus(&self) -> bool {
        self.focus_query.is_some()
    }

    /// Top-`k` semantic neighbours for an arbitrary query string.
    /// Independent of the current focus query.
    pub fn search(&self, query: &str, k: usize) -> Result<Vec<(K, f32)>, SearchError> {
        self.search.search(query, k)
    }

    /// Top-`k` semantic neighbours for the current focus query, if set.
    pub fn search_focus(&self, k: usize) -> Result<Option<Vec<(K, f32)>>, SearchError> {
        let Some(q) = &self.focus_query else {
            return Ok(None);
        };
        Ok(Some(self.search.index().nearest(q, k)?))
    }

    /// Register the current focus query as a similarity scalar field on
    /// the projection. Returns the registered `FieldId`, or `None` if
    /// no focus query is set or no node positions are known.
    ///
    /// `name` is the field's name in the registry; choose something
    /// stable (e.g. `"focus_similarity"`) so subsequent registrations
    /// overwrite the previous entry.
    pub fn register_focus_field(
        &self,
        projection: &mut FieldProjection,
        name: &str,
    ) -> Option<FieldId> {
        let q = self.focus_query.as_deref()?;
        if self.positions.is_empty() {
            return None;
        }
        Some(register_query_similarity_field(
            projection,
            name,
            q,
            self.search.index(),
            &self.positions,
            self.sigma,
        ))
    }

    /// Number of nodes in the index.
    pub fn len(&self) -> usize {
        self.search.len()
    }

    pub fn is_empty(&self) -> bool {
        self.search.is_empty()
    }

    /// Borrow the current focus query embedding, if any.
    pub fn focus_query(&self) -> Option<&[f32]> {
        self.focus_query.as_deref()
    }

    /// Borrow the position map (e.g. for a debug overlay).
    pub fn positions(&self) -> &HashMap<K, (f32, f32)> {
        &self.positions
    }

    /// Set the heatmap sigma in canvas units.
    pub fn set_sigma(&mut self, sigma: f32) {
        self.sigma = sigma;
    }

    /// Borrow the underlying index (e.g. for persistence).
    pub fn index(&self) -> &VectorIndex<K> {
        self.search.index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StubEmbeddingProvider;
    use aether::eval::eval_scalar;
    use aether::registry::{FieldDef, FieldRegistry};

    fn provider() -> StubEmbeddingProvider {
        StubEmbeddingProvider::new(64).unwrap()
    }

    #[test]
    fn new_surface_is_empty_no_focus() {
        let s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        assert!(s.is_empty());
        assert!(!s.has_focus());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn ingest_tracks_both_index_and_position() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        s.ingest(1, "rust async", (100.0, 100.0)).unwrap();
        s.ingest(2, "python typing", (200.0, 200.0)).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s.positions().len(), 2);
        assert_eq!(s.positions().get(&1), Some(&(100.0, 100.0)));
    }

    #[test]
    fn forget_removes_from_both_index_and_positions() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        s.ingest(1, "alpha", (0.0, 0.0)).unwrap();
        s.ingest(2, "beta", (10.0, 10.0)).unwrap();
        s.forget(&1);
        assert_eq!(s.len(), 1);
        assert!(!s.positions().contains_key(&1));
        assert!(s.positions().contains_key(&2));
    }

    #[test]
    fn move_node_updates_position_only() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        s.ingest(1, "alpha", (0.0, 0.0)).unwrap();
        s.move_node(1, (500.0, 500.0));
        assert_eq!(s.positions().get(&1), Some(&(500.0, 500.0)));
        // Index unchanged: same vector still there.
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn set_and_clear_focus_query() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        assert!(!s.has_focus());
        s.set_focus_query("alpha").unwrap();
        assert!(s.has_focus());
        assert!(s.focus_query().is_some());
        s.clear_focus();
        assert!(!s.has_focus());
        assert!(s.focus_query().is_none());
    }

    #[test]
    fn register_focus_field_without_query_returns_none() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        s.ingest(1, "alpha", (0.0, 0.0)).unwrap();
        let mut projection = FieldProjection::new();
        let id = s.register_focus_field(&mut projection, "focus");
        assert!(id.is_none());
    }

    #[test]
    fn register_focus_field_without_nodes_returns_none() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        s.set_focus_query("alpha").unwrap();
        let mut projection = FieldProjection::new();
        let id = s.register_focus_field(&mut projection, "focus");
        assert!(id.is_none());
    }

    #[test]
    fn register_focus_field_emits_field_when_query_and_nodes_present() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        s.ingest(1, "alpha", (100.0, 100.0)).unwrap();
        s.ingest(2, "beta", (200.0, 200.0)).unwrap();
        s.set_focus_query("alpha").unwrap();

        let mut projection = FieldProjection::new();
        let id = s
            .register_focus_field(&mut projection, "focus_similarity")
            .expect("field registered");

        // Field should evaluate to its peak near node 1's position
        // (which embedded "alpha", same as the query).
        let registry = FieldRegistry::new();
        let field = match projection.registry.get(id) {
            Some(FieldDef::Scalar(f)) => f,
            _ => panic!("expected scalar field"),
        };
        let at_one = eval_scalar(field, &registry, 100.0, 100.0, 0.0);
        let at_two = eval_scalar(field, &registry, 200.0, 200.0, 0.0);
        // With the hashed provider, "alpha" and "beta" produce
        // uncorrelated unit vectors. Cosine ranges in roughly [-1, 1].
        // Node 1 is at the query embedding, so cosine = 1.
        // The field at node 1 should clearly exceed the field at node 2.
        assert!(at_one > at_two);
    }

    #[test]
    fn search_independent_of_focus_query() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        s.ingest(1, "alpha", (0.0, 0.0)).unwrap();
        s.ingest(2, "beta", (10.0, 0.0)).unwrap();
        s.set_focus_query("gamma").unwrap();
        // search() takes its own query, ignores focus.
        let result = s.search("alpha", 1).unwrap();
        assert_eq!(result[0].0, 1);
    }

    #[test]
    fn search_focus_returns_none_without_focus_set() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        s.ingest(1, "alpha", (0.0, 0.0)).unwrap();
        let result = s.search_focus(5).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn search_focus_returns_top_k_when_focus_set() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 50.0);
        s.ingest(1, "alpha", (0.0, 0.0)).unwrap();
        s.ingest(2, "beta", (10.0, 0.0)).unwrap();
        s.ingest(3, "gamma", (20.0, 0.0)).unwrap();
        s.set_focus_query("alpha").unwrap();
        let result = s.search_focus(2).unwrap().expect("focus set");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, 1, "self-match top-1");
    }

    #[test]
    fn set_sigma_takes_effect_on_subsequent_field_registration() {
        let mut s = CanvasSearchSurface::<u32, _>::new(provider(), 10.0);
        s.ingest(1, "alpha", (0.0, 0.0)).unwrap();
        s.set_focus_query("alpha").unwrap();

        let mut tight = FieldProjection::new();
        s.register_focus_field(&mut tight, "f").unwrap();

        s.set_sigma(200.0);
        let mut wide = FieldProjection::new();
        s.register_focus_field(&mut wide, "f").unwrap();

        // Both fields evaluate to ~1.0 at node 1's position. The
        // difference is in spread — at distance 100 from node 1, the
        // wide-sigma field should be much higher than the tight one.
        let registry = FieldRegistry::new();
        let tight_id = tight.registry.lookup("f").unwrap();
        let wide_id = wide.registry.lookup("f").unwrap();
        let FieldDef::Scalar(tight_f) = tight.registry.get(tight_id).unwrap() else {
            panic!()
        };
        let FieldDef::Scalar(wide_f) = wide.registry.get(wide_id).unwrap() else {
            panic!()
        };
        let tight_at_far = eval_scalar(tight_f, &registry, 100.0, 0.0, 0.0).abs();
        let wide_at_far = eval_scalar(wide_f, &registry, 100.0, 0.0, 0.0).abs();
        assert!(wide_at_far > tight_at_far);
    }
}
