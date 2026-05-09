/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Save/load [`VectorIndex`] through an [`eidetic::Store`].
//!
//! The serialization format is JSON for now (human-inspectable, stable
//! across schema additions via serde). Binary formats (bincode / postcard)
//! are a follow-up if size or speed become real constraints.

use std::hash::Hash;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::VectorIndex;

/// Save a vector index to an eidetic store under `key`.
pub fn save_to_eidetic<K>(
    store: &mut dyn eidetic::Store,
    key: &str,
    index: &VectorIndex<K>,
) -> eidetic::Result<()>
where
    K: Hash + Eq + Clone + Serialize,
{
    let bytes = serde_json::to_vec(index)
        .map_err(|e| eidetic::Error::new(format!("vector-index serialize: {e}")))?;
    store.save_blob(key, &bytes)
}

/// Load a vector index from an eidetic store. Returns `Ok(None)` if no blob
/// exists at `key`; returns an error if a blob exists but cannot be parsed
/// (e.g. dimension mismatch with the deployed schema).
pub fn load_from_eidetic<K>(
    store: &mut dyn eidetic::Store,
    key: &str,
) -> eidetic::Result<Option<VectorIndex<K>>>
where
    K: Hash + Eq + Clone + DeserializeOwned,
{
    match store.load_blob(key)? {
        None => Ok(None),
        Some(bytes) => {
            let index: VectorIndex<K> = serde_json::from_slice(&bytes)
                .map_err(|e| eidetic::Error::new(format!("vector-index deserialize: {e}")))?;
            Ok(Some(index))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimilarityMetric;
    use std::collections::HashMap;

    /// Minimal in-memory store for tests. Real implementations would back
    /// onto fjall/redb/IndexedDB; this mirrors the shape without bringing in
    /// a storage backend.
    #[derive(Default)]
    struct InMemoryStore {
        blobs: HashMap<String, Vec<u8>>,
    }

    impl eidetic::Store for InMemoryStore {
        fn load_blob(&mut self, key: &str) -> eidetic::Result<Option<Vec<u8>>> {
            Ok(self.blobs.get(key).cloned())
        }
        fn save_blob(&mut self, key: &str, value: &[u8]) -> eidetic::Result<()> {
            self.blobs.insert(key.to_string(), value.to_vec());
            Ok(())
        }
    }

    fn make_index() -> VectorIndex<u32> {
        let mut idx = VectorIndex::<u32>::new(3, SimilarityMetric::Cosine);
        idx.insert(1, vec![1.0, 0.0, 0.0]).unwrap();
        idx.insert(2, vec![0.0, 1.0, 0.0]).unwrap();
        idx.insert(3, vec![0.5, 0.5, 0.0]).unwrap();
        idx
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut store = InMemoryStore::default();
        let original = make_index();
        save_to_eidetic(&mut store, "embeddings:test", &original).unwrap();
        let loaded: VectorIndex<u32> = load_from_eidetic(&mut store, "embeddings:test")
            .unwrap()
            .expect("blob present after save");

        assert_eq!(loaded.dimensions(), original.dimensions());
        assert_eq!(loaded.metric(), original.metric());
        assert_eq!(loaded.len(), original.len());
        for (k, v) in original.iter() {
            assert_eq!(loaded.get(k), Some(v));
        }
    }

    #[test]
    fn load_missing_returns_none() {
        let mut store = InMemoryStore::default();
        let loaded: Option<VectorIndex<u32>> =
            load_from_eidetic(&mut store, "embeddings:missing").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn corrupted_blob_returns_error() {
        use eidetic::Store as _;
        let mut store = InMemoryStore::default();
        store
            .save_blob("embeddings:bad", b"not valid json")
            .unwrap();
        let result: eidetic::Result<Option<VectorIndex<u32>>> =
            load_from_eidetic(&mut store, "embeddings:bad");
        assert!(result.is_err());
    }

    #[test]
    fn save_overwrites_previous() {
        let mut store = InMemoryStore::default();
        let first = make_index();
        save_to_eidetic(&mut store, "k", &first).unwrap();

        let mut second = first.clone();
        second.insert(99, vec![0.0, 0.0, 1.0]).unwrap();
        save_to_eidetic(&mut store, "k", &second).unwrap();

        let loaded: VectorIndex<u32> = load_from_eidetic(&mut store, "k").unwrap().unwrap();
        assert_eq!(loaded.len(), 4);
        assert!(loaded.contains(&99));
    }

    #[test]
    fn empty_index_roundtrips() {
        let mut store = InMemoryStore::default();
        let empty: VectorIndex<u32> = VectorIndex::new(8, SimilarityMetric::DotProduct);
        save_to_eidetic(&mut store, "empty", &empty).unwrap();
        let loaded: VectorIndex<u32> = load_from_eidetic(&mut store, "empty").unwrap().unwrap();
        assert_eq!(loaded.dimensions(), 8);
        assert_eq!(loaded.metric(), SimilarityMetric::DotProduct);
        assert!(loaded.is_empty());
    }
}
