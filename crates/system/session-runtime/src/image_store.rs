/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Durable content-addressed store for node preview imagery (node image
//! externalization plan) — favicons, previews, and last-render snapshots.
//!
//! The sibling of [`content_store`](crate::content_store): where that keys a
//! fetched page body by its URL, this keys an image blob by the **BLAKE3 digest
//! of its bytes** under `content/image/<hex>`, so identical images across nodes
//! (every github.com favicon, a re-deposited snapshot of an unchanged page)
//! collapse to one blob. The kernel holds only a small [`ImageRef`] (digest +
//! dimensions); the pixels live here, out of the graph truth.
//!
//! Content-addressing is BLAKE3 to match eidetic's engram / manifest identity
//! (so an image blob is iroh-sync-portable by the same hash). Bytes are stored
//! raw (media-friendly, like `content_store` bodies). Deletion is
//! caller-guarded: a blob is shared, so it is dropped only once no live node
//! references it — the Athanor orphan sweep, not a per-node drop.

use eidetic::{Hash, Result, Store};
use kernel::types::ImageRef;

/// Blob-key prefix, namespacing image blobs away from `content_store` bodies and
/// the manifests / schemas that share the same store.
const IMAGE_PREFIX: &str = "content/image/";

fn image_key(hex: &str) -> String {
    format!("{IMAGE_PREFIX}{hex}")
}

/// Store `png` bytes content-addressed and return an [`ImageRef`] (digest +
/// dimensions). Saving identical bytes twice writes one blob at one key, so two
/// nodes with the same favicon share storage. `width` / `height` are the decoded
/// dimensions the caller already knows; they ride on the reference, not the blob.
pub async fn save_image(
    store: &mut dyn Store,
    png: &[u8],
    width: u32,
    height: u32,
) -> Result<ImageRef> {
    let hash = Hash::of(png);
    store.save_blob(&image_key(&hash.to_hex()), png).await?;
    Ok(ImageRef::new(*hash.as_bytes(), width, height))
}

/// Load the PNG bytes an [`ImageRef`] names, or `None` when the blob is absent
/// (e.g. a reference that outlived an orphan sweep, or a not-yet-synced blob).
pub async fn load_image(store: &mut dyn Store, image: &ImageRef) -> Result<Option<Vec<u8>>> {
    store.load_blob(&image_key(&image.hex())).await
}

/// Drop the blob an [`ImageRef`] names, returning whether one was removed.
///
/// Blobs are shared (content-addressed), so this must only be called once no
/// live node references the digest — the caller (the Athanor orphan sweep) owns
/// that reachability check; this performs the drop it decides on.
pub async fn delete_image(store: &mut dyn Store, image: &ImageRef) -> Result<bool> {
    store.delete_blob(&image_key(&image.hex())).await
}

/// The hex digest of every stored image blob. The input to the orphan sweep:
/// diff this against the set of digests live nodes reference to find droppable
/// blobs. Order is store-defined; sort at the call site if a stable order is
/// wanted.
pub async fn stored_image_hexes(store: &mut dyn Store) -> Result<Vec<String>> {
    Ok(store
        .iter_keys(IMAGE_PREFIX)
        .await?
        .into_iter()
        .filter_map(|key| key.strip_prefix(IMAGE_PREFIX).map(str::to_string))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;

    /// Minimal in-memory [`Store`] for the round-trip tests (mirrors the ones in
    /// `content_store` / `athanor`, with `iter_keys` for the sweep helper).
    #[derive(Default)]
    struct MemStore {
        blobs: HashMap<String, Vec<u8>>,
    }

    #[async_trait(?Send)]
    impl Store for MemStore {
        async fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.blobs.get(key).cloned())
        }
        async fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()> {
            self.blobs.insert(key.to_string(), value.to_vec());
            Ok(())
        }
        async fn delete_blob(&mut self, key: &str) -> Result<bool> {
            Ok(self.blobs.remove(key).is_some())
        }
        async fn iter_keys(&mut self, prefix: &str) -> Result<Vec<String>> {
            Ok(self
                .blobs
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }
    }

    #[test]
    fn save_then_load_round_trips_the_bytes() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let png = b"\x89PNG\r\n\x1a\n fake favicon bytes";
            let image = save_image(&mut store, png, 32, 32).await.unwrap();
            assert_eq!(image.width, 32);
            assert_eq!(image.height, 32);
            assert_eq!(
                load_image(&mut store, &image).await.unwrap().as_deref(),
                Some(png.as_slice()),
                "the stored bytes come back unchanged"
            );
        });
    }

    #[test]
    fn identical_bytes_dedup_to_one_blob() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let png = b"same pixels";
            let a = save_image(&mut store, png, 16, 16).await.unwrap();
            let b = save_image(&mut store, png, 16, 16).await.unwrap();
            assert_eq!(a, b, "identical bytes produce the same reference");
            assert_eq!(
                stored_image_hexes(&mut store).await.unwrap().len(),
                1,
                "and one blob, not two"
            );
        });
    }

    #[test]
    fn distinct_bytes_get_distinct_keys() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let a = save_image(&mut store, b"one", 1, 1).await.unwrap();
            let b = save_image(&mut store, b"two", 1, 1).await.unwrap();
            assert_ne!(a.digest, b.digest, "different bytes hash differently");
            let mut hexes = stored_image_hexes(&mut store).await.unwrap();
            hexes.sort();
            assert_eq!(hexes.len(), 2);
            assert!(hexes.contains(&a.hex()));
            assert!(hexes.contains(&b.hex()));
        });
    }

    #[test]
    fn delete_drops_the_blob() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let image = save_image(&mut store, b"disposable", 8, 8).await.unwrap();
            assert!(delete_image(&mut store, &image).await.unwrap());
            assert!(
                load_image(&mut store, &image).await.unwrap().is_none(),
                "gone after delete"
            );
            assert!(
                !delete_image(&mut store, &image).await.unwrap(),
                "deleting again is a no-op"
            );
        });
    }
}
