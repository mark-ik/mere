/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Graph engrams — freezing a live graph into an immutable, content-addressed
//! eidetic engram, and thawing one back into a live graph.
//!
//! This is the spine of the Alembic memory subsystem (slice A of
//! `design_docs/mere_docs/implementation_strategy/2026-06-24_alembic_implementation_plan.md`):
//! "Save as graph engram" freezes a [`Graph`] to a [`GraphSnapshot`], redacts
//! private / heavy fields, and writes it through the eidetic typed-payload layer
//! under the `mere.graph-snapshot/v1` schema; "Open as session" thaws an engram
//! back into a [`Graph`]. Browsing an engram is read-only (immutability holds);
//! editing forks a thaw.
//!
//! The graph binding lives here rather than in eidetic-core because eidetic is
//! deliberately graph-agnostic; session-runtime is the lowest crate that knows
//! both [`GraphSnapshot`] and the eidetic [`Store`]. It is store-backend-agnostic
//! (it speaks the `Store` trait), so it is not filesystem-gated like the
//! `graph.json` sidecar — a wasm host's OPFS store works the same way.

use eidetic::{
    BlobManifest, BlobSource, ManifestId, NoFetcher, PrivacyClass, ProvenanceOrigin,
    ProvenanceRecord, Result, SchemaRef, Store, Timestamp, TrustEnvelope, TypedPayload, list_typed,
    load_typed, save_typed,
};
use kernel::graph::Graph;
use kernel::persistence::GraphSnapshot;

/// Schema id bytes for the graph-snapshot engram schema. The [`SchemaRef`] is the
/// BLAKE3 of these bytes, so it is stable across builds and machines.
pub const GRAPH_SNAPSHOT_SCHEMA_ID: &[u8] = b"mere.graph-snapshot/v1";

/// The content-addressed schema reference every graph engram is tagged with.
///
/// The eidetic typed layer uses this as an identity tag — checked on load so a
/// mistyped read errors rather than deserializing garbage. Registering a full
/// schema-definition engram (with validators, for cross-language / federation
/// reads) is a later, optional step; the freeze/thaw spine does not need it.
pub fn graph_snapshot_schema_ref() -> SchemaRef {
    SchemaRef::from_id(ManifestId::of_blob(GRAPH_SNAPSHOT_SCHEMA_ID))
}

/// A [`GraphSnapshot`] bound to the graph-snapshot engram schema.
///
/// A newtype is required because both [`TypedPayload`] (eidetic) and
/// [`GraphSnapshot`] (kernel) are foreign to this crate. `#[serde(transparent)]`
/// keeps the stored bytes identical to a bare `GraphSnapshot`, so the payload is
/// readable by anything that knows the snapshot shape.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct GraphEngram(pub GraphSnapshot);

impl TypedPayload for GraphEngram {
    fn schema_ref() -> SchemaRef {
        graph_snapshot_schema_ref()
    }
    // Default serde_json serializer: consistent with `session_graph_store`'s
    // `graph.json`, hash-stable, and free of the rkyv-alignment-from-store
    // concern. A compact rkyv override is a later optimization (the architecture
    // doc's stated preference), not load-bearing for the spine.
}

/// What a graph engram keeps from the live snapshot.
///
/// The default is conservative (Alembic plan open decision #7): private / heavy
/// per-node fields are stripped, leaving graph structure, addresses, titles,
/// tags, classifications, provenance, and properties. Callers opt fields back in
/// explicitly with [`RedactionPolicy::include_all`] or by setting a flag — never
/// silently — so a shareable engram does not leak screenshots or form drafts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedactionPolicy {
    /// Keep `thumbnail_png` (page screenshots). Off by default (heavy, private).
    pub include_thumbnails: bool,
    /// Keep `favicon_rgba`. Off by default (heavy).
    pub include_favicons: bool,
    /// Keep `session_state` (scroll position + form drafts). Off by default (private).
    pub include_session_state: bool,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            include_thumbnails: false,
            include_favicons: false,
            include_session_state: false,
        }
    }
}

impl RedactionPolicy {
    /// Include everything — no redaction. For a local, trusted freeze where the
    /// heavy / private fields are wanted; never for a shareable engram.
    pub fn include_all() -> Self {
        Self {
            include_thumbnails: true,
            include_favicons: true,
            include_session_state: true,
        }
    }

    /// Strip the excluded fields from every node in `snapshot`, in place.
    pub fn apply(&self, snapshot: &mut GraphSnapshot) {
        for node in &mut snapshot.nodes {
            if !self.include_thumbnails {
                node.thumbnail_png = None;
                node.thumbnail_width = 0;
                node.thumbnail_height = 0;
            }
            if !self.include_favicons {
                node.favicon_rgba = None;
                node.favicon_width = 0;
                node.favicon_height = 0;
            }
            if !self.include_session_state {
                node.session_state = None;
            }
        }
    }
}

/// Provenance for a locally-frozen graph engram: generated by this tool at the
/// freeze time, no upstream ancestry (composition / merge fills `upstream` later).
fn graph_engram_provenance(created_at: Timestamp) -> ProvenanceRecord {
    ProvenanceRecord {
        origin: ProvenanceOrigin::Generated,
        upstream: Vec::new(),
        tooling: Some(concat!("session-runtime/graph-engram@", env!("CARGO_PKG_VERSION")).to_string()),
        generated_at: created_at,
    }
}

/// Freeze a live `graph` into a content-addressed graph engram, returning its
/// manifest id.
///
/// The snapshot is redacted per `redaction` (default conservative) before
/// serialization; the engram is `LocalOnly` and self-asserted — promotion to a
/// wider audience is always an explicit later act. `created_at` is the freeze
/// time in Unix milliseconds (the host passes "now"; tests pass a fixed value so
/// the call stays pure).
pub async fn save_graph_engram(
    store: &mut dyn Store,
    graph: &Graph,
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<ManifestId> {
    save_graph_snapshot_engram(store, graph.to_snapshot(), redaction, created_at).await
}

/// As [`save_graph_engram`], but from an already-materialized [`GraphSnapshot`].
///
/// The primitive a host uses when it has snapshotted the graph already — taking
/// the snapshot ends the borrow of the live graph, so a `&mut Store` borrowed
/// from the same owner can follow without a conflict. Also the entry the
/// Timeline's "distil this past state" reuses (slice E). `snapshot` is redacted
/// in place per `redaction`.
pub async fn save_graph_snapshot_engram(
    store: &mut dyn Store,
    mut snapshot: GraphSnapshot,
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<ManifestId> {
    redaction.apply(&mut snapshot);
    save_typed(
        store,
        &GraphEngram(snapshot),
        Vec::<BlobSource>::new(),
        PrivacyClass::LocalOnly,
        graph_engram_provenance(created_at),
        TrustEnvelope::self_asserted(),
        created_at,
    )
    .await
}

/// Thaw a stored graph engram back into a live [`Graph`]. `Ok(None)` if no engram
/// is stored under `id`.
///
/// The store is read-only here (eidetic replay-isolation), so opening an engram
/// never mutates it; editing the returned graph forks a thaw and only persists if
/// re-saved as a new engram.
pub async fn open_engram_as_session(store: &mut dyn Store, id: ManifestId) -> Result<Option<Graph>> {
    let mut fetcher = NoFetcher;
    let payload: Option<GraphEngram> = load_typed::<GraphEngram>(store, &mut fetcher, id).await?;
    Ok(payload.map(|engram| Graph::from_snapshot(&engram.0)))
}

/// List the manifests of every graph engram in the store. Order is store-defined;
/// callers that want newest-first should sort on `created_at`. Use
/// [`open_engram_as_session`] on an id to thaw one.
pub async fn list_graph_engrams(store: &mut dyn Store) -> Result<Vec<BlobManifest>> {
    list_typed::<GraphEngram>(store).await
}

/// Compose several graph engrams into one by URL-identity merge (Alembic tail B7 /
/// decision #1). Thaws each id's snapshot, folds them with
/// [`merge_snapshots`](crate::snapshot_merge::merge_snapshots) (the first id is the
/// canonical base), and saves the union as a new engram.
///
/// The new engram is `Derived` and its `ProvenanceRecord.upstream` records the source
/// ids — the lineage `upstream` (empty on every freeze until now) finally populated,
/// which is what the consolidation pass reads to relate version chains. `Ok(None)` if
/// `ids` is empty or any id is missing (a partial compose would silently lose a source,
/// so it aborts instead). Like a freeze, the result is `LocalOnly` + self-asserted.
pub async fn compose_graph_engrams(
    store: &mut dyn Store,
    ids: &[ManifestId],
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<Option<ManifestId>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let mut fetcher = NoFetcher;
    let mut acc: Option<GraphSnapshot> = None;
    for id in ids {
        let Some(engram) = load_typed::<GraphEngram>(store, &mut fetcher, *id).await? else {
            return Ok(None);
        };
        acc = Some(match acc {
            None => engram.0,
            Some(base) => crate::snapshot_merge::merge_snapshots(&base, &engram.0).0,
        });
    }
    let mut snapshot = acc.expect("ids is non-empty, so acc is Some");
    redaction.apply(&mut snapshot);
    let provenance = ProvenanceRecord {
        origin: ProvenanceOrigin::Derived,
        upstream: ids.to_vec(),
        tooling: Some(
            concat!("session-runtime/graph-engram-compose@", env!("CARGO_PKG_VERSION")).to_string(),
        ),
        generated_at: created_at,
    };
    let id = save_typed(
        store,
        &GraphEngram(snapshot),
        Vec::<BlobSource>::new(),
        PrivacyClass::LocalOnly,
        provenance,
        TrustEnvelope::self_asserted(),
        created_at,
    )
    .await?;
    Ok(Some(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::fixtures::GraphFixtures;
    use async_trait::async_trait;
    use euclid::default::Point2D;
    use kernel::persistence::PersistedNodeSessionState;
    use std::collections::HashMap;

    /// In-memory `Store` for the round-trip tests — the same shape
    /// `content_store`'s tests and eidetic's own tests use.
    #[derive(Default)]
    struct InMemoryStore {
        blobs: HashMap<String, Vec<u8>>,
    }

    #[async_trait(?Send)]
    impl Store for InMemoryStore {
        async fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.blobs.get(key).cloned())
        }
        async fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()> {
            self.blobs.insert(key.to_string(), value.to_vec());
            Ok(())
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

    fn sample_graph() -> Graph {
        let mut graph = Graph::new();
        graph.add_node("https://a.example".to_string(), Point2D::new(1.0, 2.0));
        graph.add_node("https://b.example".to_string(), Point2D::new(3.0, 4.0));
        graph
    }

    #[test]
    fn save_then_open_round_trips_the_graph() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let id = save_graph_engram(
                &mut store,
                &sample_graph(),
                RedactionPolicy::default(),
                Timestamp(1_700_000_000_000),
            )
            .await
            .expect("save");

            let opened = open_engram_as_session(&mut store, id)
                .await
                .expect("load ok")
                .expect("engram present after save");
            assert_eq!(opened.nodes().count(), 2, "both nodes survive freeze/thaw");
            assert!(
                opened.get_node_by_url("https://a.example").is_some(),
                "the URL index rebuilds from the thawed snapshot",
            );
        });
    }

    #[test]
    fn default_redaction_strips_private_fields() {
        // A snapshot carrying a thumbnail, favicon, and session state (scroll +
        // form draft); the default policy must drop all three but keep structure.
        let mut snapshot = sample_graph().to_snapshot();
        for node in &mut snapshot.nodes {
            node.thumbnail_png = Some(vec![1, 2, 3]);
            node.thumbnail_width = 2;
            node.thumbnail_height = 2;
            node.favicon_rgba = Some(vec![4, 5, 6]);
            node.favicon_width = 1;
            node.favicon_height = 1;
            node.session_state = Some(PersistedNodeSessionState {
                scroll_x: Some(10.0),
                scroll_y: Some(20.0),
                form_draft: Some("a secret draft".to_string()),
            });
        }

        RedactionPolicy::default().apply(&mut snapshot);

        for node in &snapshot.nodes {
            assert!(
                node.thumbnail_png.is_none() && node.thumbnail_width == 0,
                "thumbnail stripped",
            );
            assert!(
                node.favicon_rgba.is_none() && node.favicon_width == 0,
                "favicon stripped",
            );
            assert!(
                node.session_state.is_none(),
                "session state (scroll + form draft) stripped",
            );
        }
        assert_eq!(snapshot.nodes.len(), 2, "graph structure kept");
    }

    #[test]
    fn include_all_keeps_private_fields() {
        let mut snapshot = sample_graph().to_snapshot();
        for node in &mut snapshot.nodes {
            node.thumbnail_png = Some(vec![9]);
            node.favicon_rgba = Some(vec![9]);
        }
        RedactionPolicy::include_all().apply(&mut snapshot);
        assert!(
            snapshot.nodes.iter().all(|n| n.thumbnail_png.is_some()),
            "include_all keeps thumbnails (opt-in, never the default)",
        );
    }

    #[test]
    fn list_graph_engrams_finds_saved_engrams_tagged_with_the_schema() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            save_graph_engram(
                &mut store,
                &sample_graph(),
                RedactionPolicy::default(),
                Timestamp(1),
            )
            .await
            .expect("save");

            let engrams = list_graph_engrams(&mut store).await.expect("list");
            assert_eq!(engrams.len(), 1, "the saved engram is listed");
            assert_eq!(
                engrams[0].schema,
                graph_snapshot_schema_ref(),
                "tagged with the graph-snapshot schema",
            );
        });
    }

    #[test]
    fn engram_survives_a_store_close_and_reopen() {
        // The faithful "survives restart" proof: save through a real fjall store,
        // drop it (shutdown), reopen at the same path, and thaw the same graph.
        use eidetic_fjall::FjallStore;

        let dir = std::env::temp_dir().join("mere_graph_engram_fjall_reopen");
        let _ = std::fs::remove_dir_all(&dir);

        let id = pollster::block_on(async {
            let mut store = FjallStore::open(&dir).expect("open store");
            save_graph_engram(
                &mut store,
                &sample_graph(),
                RedactionPolicy::default(),
                Timestamp(1_700_000_000_000),
            )
            .await
            .expect("save")
            // `store` drops here — the keyspace closes, as on shutdown.
        });

        let opened = pollster::block_on(async {
            let mut store = FjallStore::open(&dir).expect("reopen store");
            open_engram_as_session(&mut store, id).await.expect("load ok")
        })
        .expect("the engram is present after a store reopen");
        assert_eq!(
            opened.nodes().count(),
            2,
            "the frozen graph survives a store close + reopen (persisted to disk)",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_unknown_id_returns_none() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let unknown = ManifestId::of_blob(b"never-saved");
            assert!(
                open_engram_as_session(&mut store, unknown)
                    .await
                    .expect("load ok")
                    .is_none(),
                "an unknown id thaws to None, not an error",
            );
        });
    }

    #[test]
    fn compose_unions_two_engrams_and_records_the_upstream_lineage() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            // Engram A: x, y. Engram B: y (shared url), z.
            let mut ga = Graph::new();
            ga.add_node("https://x.example".to_string(), Point2D::new(0.0, 0.0));
            ga.add_node("https://y.example".to_string(), Point2D::new(1.0, 0.0));
            let mut gb = Graph::new();
            gb.add_node("https://y.example".to_string(), Point2D::new(0.0, 0.0));
            gb.add_node("https://z.example".to_string(), Point2D::new(1.0, 0.0));

            let id_a = save_graph_engram(&mut store, &ga, RedactionPolicy::default(), Timestamp(1))
                .await
                .expect("save a");
            let id_b = save_graph_engram(&mut store, &gb, RedactionPolicy::default(), Timestamp(2))
                .await
                .expect("save b");

            let composed =
                compose_graph_engrams(&mut store, &[id_a, id_b], RedactionPolicy::default(), Timestamp(3))
                    .await
                    .expect("compose ok")
                    .expect("a non-empty id list composes an engram");

            // The thawed union carries x, y, z — the shared y is not doubled.
            let graph = open_engram_as_session(&mut store, composed)
                .await
                .expect("load ok")
                .expect("the composed engram is present");
            assert_eq!(graph.nodes().count(), 3, "x, y, z union; shared y not doubled");
            assert!(graph.get_node_by_url("https://x.example").is_some());
            assert!(graph.get_node_by_url("https://z.example").is_some());

            // The lineage: the composed engram is `Derived` and names both sources —
            // the `upstream` Vec that is empty on every freeze, finally populated.
            let manifests = list_graph_engrams(&mut store).await.expect("list");
            let m = manifests
                .iter()
                .find(|m| m.id == composed)
                .expect("the composed manifest is listed");
            assert_eq!(m.provenance.origin, ProvenanceOrigin::Derived, "a composed engram is Derived");
            assert_eq!(
                m.provenance.upstream,
                vec![id_a, id_b],
                "upstream records the source engrams",
            );
        });
    }

    #[test]
    fn compose_of_an_empty_id_list_is_none() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            assert!(
                compose_graph_engrams(&mut store, &[], RedactionPolicy::default(), Timestamp(1))
                    .await
                    .expect("ok")
                    .is_none(),
                "no sources -> nothing composed",
            );
        });
    }
}
