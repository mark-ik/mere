// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
use kernel::persistence::GraphSnapshot;
use kernel::types::{ImageRef, ImageRole};

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
    store.put(&image_key(&hash.to_hex()), png).await?;
    Ok(ImageRef::new(*hash.as_bytes(), width, height))
}

/// Load the PNG bytes an [`ImageRef`] names, or `None` when the blob is absent
/// (e.g. a reference that outlived an orphan sweep, or a not-yet-synced blob).
pub async fn load_image(store: &mut dyn Store, image: &ImageRef) -> Result<Option<Vec<u8>>> {
    Ok(store.get(&image_key(&image.hex())).await?)
}

/// Drop the blob an [`ImageRef`] names, returning whether one was removed.
///
/// Blobs are shared (content-addressed), so this must only be called once no
/// live node references the digest — the caller (the Athanor orphan sweep) owns
/// that reachability check; this performs the drop it decides on.
pub async fn delete_image(store: &mut dyn Store, image: &ImageRef) -> Result<bool> {
    delete_image_hex(store, &image.hex()).await
}

/// Drop a blob by digest hex. This is the apply half used by Athanor's
/// mark/sweep proposal, which inventories keys before it has an `ImageRef`.
pub async fn delete_image_hex(store: &mut dyn Store, hex: &str) -> Result<bool> {
    // Probe first: muniment's delete is idempotent (absent keys are not an
    // error), but this reports whether a blob was actually removed — the
    // orphan sweep counts reclamations.
    let key = image_key(hex);
    let existed = store.get(&key).await?.is_some();
    store.delete(&key).await?;
    Ok(existed)
}

/// The hex digest of every stored image blob. The input to the orphan sweep:
/// diff this against the set of digests live nodes reference to find droppable
/// blobs. Order is store-defined; sort at the call site if a stable order is
/// wanted.
pub async fn stored_image_hexes(store: &mut dyn Store) -> Result<Vec<String>> {
    Ok(store
        .list(IMAGE_PREFIX)
        .await?
        .into_iter()
        .filter_map(|key| key.strip_prefix(IMAGE_PREFIX).map(str::to_string))
        .collect())
}

/// Externalize pre-phase-2 inline imagery in `snapshot`, in place.
///
/// Snapshots written before the node-image externalization carried raw
/// `thumbnail_png` / `favicon_rgba` bytes on every node. Conversion into a
/// `Graph` keeps references only, so this pass must run **before**
/// `Graph::from(snapshot)` or those pixels are dropped. It is one-time and
/// lossless: bytes go to the blob store, the resulting handles land in
/// `images` under `Preview` (the legacy thumbnail slot) and `Favicon`, and the
/// legacy fields are cleared so a re-saved snapshot emits references only.
///
/// Content-addressing makes it idempotent and self-deduplicating: re-running
/// it is a no-op, and a favicon shared by 200 nodes becomes one blob.
///
/// `encode_rgba_png` converts the legacy favicon's raw RGBA into PNG, supplied
/// by the caller because encoding images is not this crate's business. An
/// encode that returns `None` leaves that node's favicon behind rather than
/// dropping it silently, and it stays counted by
/// [`GraphSnapshot::legacy_image_count`].
///
/// Returns the number of blobs written.
pub async fn migrate_legacy_images(
    snapshot: &mut GraphSnapshot,
    store: &mut dyn Store,
    encode_rgba_png: impl Fn(&[u8], u32, u32) -> Option<Vec<u8>>,
) -> Result<usize> {
    let mut written = 0usize;
    for node in &mut snapshot.nodes {
        if let Some(png) = node.legacy_thumbnail_png.take() {
            let image = save_image(
                store,
                &png,
                node.legacy_thumbnail_width,
                node.legacy_thumbnail_height,
            )
            .await?;
            node.images.insert(ImageRole::Preview, image);
            node.legacy_thumbnail_width = 0;
            node.legacy_thumbnail_height = 0;
            written += 1;
        }
        // The legacy favicon is raw RGBA; the store holds PNG.
        if let Some(rgba) = node.legacy_favicon_rgba.take() {
            let (w, h) = (node.legacy_favicon_width, node.legacy_favicon_height);
            match encode_rgba_png(&rgba, w, h) {
                Some(png) => {
                    let image = save_image(store, &png, w, h).await?;
                    node.images.insert(ImageRole::Favicon, image);
                    node.legacy_favicon_width = 0;
                    node.legacy_favicon_height = 0;
                    written += 1;
                }
                None => {
                    // Put it back: an un-encodable favicon is a visible
                    // leftover, not a silent loss.
                    node.legacy_favicon_rgba = Some(rgba);
                }
            }
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;

    /// Minimal in-memory [`Store`] for the round-trip tests (mirrors the ones in
    /// `content_store` / `athanor`, with `iter_keys` for the sweep helper).
    // The in-memory test store is muniment's (2026-07-12): the
    // hand-rolled one was the same map behind the same seam.
    use muniment::MemoryBackend as MemStore;

    /// A `PersistedNode` shaped like one a pre-externalization snapshot held:
    /// no `images`, legacy fields free for the caller to fill.
    fn legacy_node(id: &str) -> kernel::persistence::PersistedNode {
        kernel::persistence::PersistedNode {
            node_id: id.to_string(),
            address: kernel::persistence::PersistedAddress::default(),
            url: format!("https://{id}.example"),
            cached_host: None,
            title: id.to_string(),
            body: None,
            tags: Vec::new(),
            tag_presentation: Default::default(),
            import_provenance: Vec::new(),
            is_pinned: false,
            images: Default::default(),
            legacy_thumbnail_png: None,
            legacy_thumbnail_width: 0,
            legacy_thumbnail_height: 0,
            legacy_favicon_rgba: None,
            legacy_favicon_width: 0,
            legacy_favicon_height: 0,
            session_state: None::<kernel::persistence::PersistedNodeSessionState>,
            mime_hint: None,
            classifications: Vec::new(),
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
            properties: Vec::new(),
            derivations: Vec::new(),
            last_session_visited: 0,
            nested: None,
            // A pre-externalization snapshot predates content addressing, so
            // this fixture is exactly the shape that carries no hash.
            content_hash: None,
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

    /// A snapshot from before the externalization loads, externalizes, and
    /// re-saves carrying references only — the phase-2 done-condition.
    #[test]
    fn a_legacy_snapshot_externalizes_and_keeps_no_pixels() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let mut snapshot = kernel::graph::Graph::new().to_snapshot();
            snapshot.nodes.push({
                let mut n = legacy_node("https://a.test/");
                n.legacy_thumbnail_png = Some(vec![0x89, b'P', b'N', b'G']);
                n.legacy_thumbnail_width = 64;
                n.legacy_thumbnail_height = 48;
                n.legacy_favicon_rgba = Some(vec![255, 0, 0, 255]);
                n.legacy_favicon_width = 1;
                n.legacy_favicon_height = 1;
                n
            });
            assert_eq!(snapshot.legacy_image_count(), 1, "starts legacy");

            let written =
                migrate_legacy_images(&mut snapshot, &mut store, |rgba, _, _| Some(rgba.to_vec()))
                    .await
                    .unwrap();

            assert_eq!(written, 2, "one preview blob and one favicon blob");
            assert_eq!(
                snapshot.legacy_image_count(),
                0,
                "nothing inline survives, so nothing is silently dropped later"
            );
            let node = &snapshot.nodes[0];
            let preview = node.images.get(&ImageRole::Preview).expect("preview ref");
            assert_eq!((preview.width, preview.height), (64, 48));
            assert_eq!(
                load_image(&mut store, preview).await.unwrap().as_deref(),
                Some(&[0x89, b'P', b'N', b'G'][..]),
                "the pixels are in the blob store, reachable by reference"
            );
            assert!(node.images.contains_key(&ImageRole::Favicon));

            // Re-saving now emits references only: the legacy fields are
            // `skip_serializing`, so no pixels reach the JSON.
            let json = serde_json::to_string(&snapshot).unwrap();
            assert!(
                !json.contains("thumbnail_png") && !json.contains("favicon_rgba"),
                "a re-saved snapshot carries no inline imagery"
            );

            // Idempotent: content-addressing means a second pass writes nothing.
            let again =
                migrate_legacy_images(&mut snapshot, &mut store, |rgba, _, _| Some(rgba.to_vec()))
                    .await
                    .unwrap();
            assert_eq!(again, 0, "re-running the migration is a no-op");
        });
    }

    /// An un-encodable favicon is left in place rather than vanishing.
    #[test]
    fn an_unencodable_favicon_is_left_behind_not_dropped() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let mut snapshot = kernel::graph::Graph::new().to_snapshot();
            snapshot.nodes.push({
                let mut n = legacy_node("https://b.test/");
                n.legacy_favicon_rgba = Some(vec![1, 2, 3, 4]);
                n.legacy_favicon_width = 1;
                n.legacy_favicon_height = 1;
                n
            });
            let written = migrate_legacy_images(&mut snapshot, &mut store, |_, _, _| None)
                .await
                .unwrap();
            assert_eq!(written, 0);
            assert_eq!(
                snapshot.legacy_image_count(),
                1,
                "still counted, so the loss stays visible"
            );
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
