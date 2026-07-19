// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Durable web-content cache (S3.2c) — a fetched page or subresource keyed by
//! URL, persisted through an eidetic [`Store`] so content survives restart and a
//! reload need not re-fetch.
//!
//! Body bytes are stored **raw** (media-friendly: no base64 / JSON-array bloat)
//! under `content/body/<url>`, with the response content-type alongside under
//! `content/ct/<url>`. The host supplies the concrete [`Store`] (fjall on native,
//! OPFS on wasm), so this seam is backend- and host-agnostic — hence wasm-clean,
//! unlike the filesystem [`session_graph_store`](crate::session_graph_store).
//!
//! These calls are `async` (the `Store` trait is, for wasm OPFS). A native host
//! `pollster::block_on`s them over an `eidetic-fjall` store whose futures are
//! ready, so the call is effectively synchronous on the UI thread.

use eidetic::{Result, Store};

/// Blob-key prefixes, namespacing cached content away from the manifests /
/// schemas / typed payloads that may share the same store.
const BODY_PREFIX: &str = "content/body/";
const CT_PREFIX: &str = "content/ct/";

/// A cached fetch: the response content-type (if any) and the raw body bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredContent {
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

fn body_key(url: &str) -> String {
    format!("{BODY_PREFIX}{url}")
}

fn ct_key(url: &str) -> String {
    format!("{CT_PREFIX}{url}")
}

/// Load cached content for `url`, or `None` when nothing is stored. The body is
/// the cache presence marker: with no stored body, this is a miss regardless of
/// any stray content-type entry.
pub async fn load_content(store: &mut dyn Store, url: &str) -> Result<Option<StoredContent>> {
    let Some(body) = store.get(&body_key(url)).await? else {
        return Ok(None);
    };
    let content_type = store
        .get(&ct_key(url))
        .await?
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    Ok(Some(StoredContent { content_type, body }))
}

/// Store `content` for `url`, overwriting any previous entry. A `None`
/// content-type leaves no content-type blob (so `load_content` reports `None`).
pub async fn save_content(store: &mut dyn Store, url: &str, content: &StoredContent) -> Result<()> {
    store.put(&body_key(url), &content.body).await?;
    if let Some(content_type) = &content.content_type {
        store
            .put(&ct_key(url), content_type.as_bytes())
            .await?;
    }
    Ok(())
}

/// Drop the cached content for `url` (its body + content-type blobs), returning
/// whether a body was actually removed. The forgetting half of the memory model:
/// short-term cached content is evicted while the graph node (structure) stays.
/// The body is the presence marker, so its removal is the reported outcome; the
/// content-type is dropped best-effort alongside. (Alembic slice C eviction /
/// Athanor forgetting.)
pub async fn evict_content(store: &mut dyn Store, url: &str) -> Result<bool> {
    // muniment's `delete` is idempotent and returns `()`, so probe the body —
    // the presence marker whose removal this function reports — before dropping
    // it, and drop the content-type best-effort alongside.
    let body = body_key(url);
    let existed = store.get(&body).await?.is_some();
    let _ = store.delete(&ct_key(url)).await?;
    store.delete(&body).await?;
    Ok(existed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;

    /// Minimal in-memory [`Store`] for the round-trip tests (mirrors the one in
    /// eidetic's own tests).
    // The in-memory test store is muniment's (2026-07-12): the
// hand-rolled one was the same map behind the same seam.
use muniment::MemoryBackend as MemStore;




    #[test]
    fn evict_drops_cached_content() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let content = StoredContent {
                content_type: Some("text/html".into()),
                body: b"<h1>bye</h1>".to_vec(),
            };
            save_content(&mut store, "https://gone.example/", &content)
                .await
                .unwrap();

            let dropped = evict_content(&mut store, "https://gone.example/")
                .await
                .unwrap();
            assert!(
                dropped,
                "the body was present, so eviction reports it removed"
            );
            assert!(
                load_content(&mut store, "https://gone.example/")
                    .await
                    .unwrap()
                    .is_none(),
                "content is gone after eviction",
            );
            // Evicting again is a no-op (nothing left to drop).
            assert!(
                !evict_content(&mut store, "https://gone.example/")
                    .await
                    .unwrap()
            );
        });
    }

    #[test]
    fn round_trips_body_and_content_type() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let content = StoredContent {
                content_type: Some("text/html; charset=utf-8".into()),
                body: b"<h1>hi</h1>".to_vec(),
            };
            save_content(&mut store, "https://example.com/", &content)
                .await
                .unwrap();
            let loaded = load_content(&mut store, "https://example.com/")
                .await
                .unwrap()
                .expect("stored");
            assert_eq!(
                loaded, content,
                "content-type + raw body survive the round trip"
            );
        });
    }

    #[test]
    fn body_only_round_trips_with_no_content_type() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let content = StoredContent {
                content_type: None,
                body: b"raw bytes".to_vec(),
            };
            save_content(&mut store, "mere://x", &content)
                .await
                .unwrap();
            assert_eq!(
                load_content(&mut store, "mere://x").await.unwrap().unwrap(),
                content
            );
        });
    }

    #[test]
    fn absent_url_loads_to_none() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            assert!(
                load_content(&mut store, "https://absent")
                    .await
                    .unwrap()
                    .is_none()
            );
        });
    }
}
