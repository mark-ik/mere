// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deleted-node tombstones: an append-only record of nodes removed from a
//! session graph, kept in the private eidetic memory so a deletion is auditable
//! and (later) recoverable.
//!
//! Each [`DeletedNode`] is an immutable, content-hashed [`TypedPayload`] — the
//! R0 temporal-integrity contract: a tombstone is never edited, and deleting the
//! same url again is simply another tombstone. [`record_deleted`] saves one;
//! [`list_deleted`] returns them newest-first, the projection a "recently
//! removed" view (the lineage / eidetic context pane) reads.

use serde::{Deserialize, Serialize};

use crate::manifest::NoFetcher;
use crate::schema::{
    ManifestId, ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, SchemaRef,
    Timestamp, TrustEnvelope, TrustLevel,
};
use crate::typed::{TypedPayload, list_typed, load_typed, save_typed};
use crate::{Result, Store};

/// A tombstone for a node removed from a session graph: enough to show it in a
/// "recently removed" list and to re-mint it (url + title + tags) later.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletedNode {
    /// The kernel node UUID (as a string), stable across the node's life.
    pub node_id: String,
    /// The node's location (url) at deletion.
    pub url: String,
    /// The node's title at deletion, when it had one.
    pub title: Option<String>,
    /// The node's tags at deletion.
    pub tags: Vec<String>,
    /// The graph (session root) the node was deleted from, when known.
    pub graph_id: Option<String>,
    /// Deletion time, unix milliseconds.
    pub deleted_at_ms: u64,
}

impl TypedPayload for DeletedNode {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::of_blob(b"schema:mere/deleted-node/v1"))
    }
}

/// Record a tombstone for a deleted node into the private memory store, returning
/// its content-addressed manifest id. Local-only, self-asserted,
/// generated-provenance: a private audit record, not a shared engram.
pub async fn record_deleted(store: &mut dyn Store, deleted: &DeletedNode) -> Result<ManifestId> {
    let at = Timestamp(deleted.deleted_at_ms);
    save_typed(
        store,
        deleted,
        Vec::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some("mere/deleted-node/v1".to_string()),
            generated_at: at,
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        at,
    )
    .await
}

/// List recorded tombstones, newest deletion first. Resolves each tombstone's
/// local blob (no network fetch) — a shared-projection over the single tombstone
/// store (R0), never a second authoritative store.
pub async fn list_deleted(store: &mut dyn Store) -> Result<Vec<DeletedNode>> {
    let manifests = list_typed::<DeletedNode>(store).await?;
    let mut fetcher = NoFetcher;
    let mut out = Vec::with_capacity(manifests.len());
    for manifest in manifests {
        if let Some(node) = load_typed::<DeletedNode>(store, &mut fetcher, manifest.id).await? {
            out.push(node);
        }
    }
    out.sort_by(|a, b| b.deleted_at_ms.cmp(&a.deleted_at_ms));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;

        // The in-memory test store is muniment's (2026-07-12): eidetic's
    // hand-rolled one was the same map behind the same seam.
    use muniment::MemoryBackend as InMemoryStore;




    fn tombstone(url: &str, at: u64) -> DeletedNode {
        DeletedNode {
            node_id: format!("id-{url}"),
            url: url.to_string(),
            title: Some(format!("title for {url}")),
            tags: vec!["reading".to_string()],
            graph_id: Some("graph-1".to_string()),
            deleted_at_ms: at,
        }
    }

    #[test]
    fn recorded_tombstones_list_newest_first() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            record_deleted(&mut store, &tombstone("https://a.test", 1_000))
                .await
                .unwrap();
            record_deleted(&mut store, &tombstone("https://b.test", 3_000))
                .await
                .unwrap();
            record_deleted(&mut store, &tombstone("https://c.test", 2_000))
                .await
                .unwrap();

            let listed = list_deleted(&mut store).await.unwrap();
            assert_eq!(listed.len(), 3);
            // Newest deletion first (3000 > 2000 > 1000).
            assert_eq!(listed[0].url, "https://b.test");
            assert_eq!(listed[1].url, "https://c.test");
            assert_eq!(listed[2].url, "https://a.test");
            // The tombstone carries the node's tags + title for the recovery view.
            assert_eq!(listed[0].tags, vec!["reading".to_string()]);
            assert_eq!(listed[0].title.as_deref(), Some("title for https://b.test"));
        });
    }

    #[test]
    fn list_deleted_is_empty_on_a_fresh_store() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            assert!(list_deleted(&mut store).await.unwrap().is_empty());
        });
    }
}
