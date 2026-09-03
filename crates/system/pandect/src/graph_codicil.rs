// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Graph codicils — freezing a live graph into an immutable, content-addressed
//! eidetic codicil, and thawing one back into a live graph.
//!
//! This is the spine of the Alembic memory subsystem (slice A of
//! `design_docs/mere_docs/implementation_strategy/2026-06-24_alembic_implementation_plan.md`):
//! "Save as graph codicil" freezes a [`Graph`] to its [`GraphSnapshot`] plus
//! atomic facet store, redacts private / heavy state, and writes it through the
//! eidetic typed-payload layer under the `mere.graph-snapshot/v2` schema; "Open
//! as session" thaws a codicil back into a [`Graph`]. Browsing a codicil is
//! read-only (immutability holds); editing forks a thaw.
//!
//! The graph binding lives here rather than in eidetic-core because eidetic is
//! deliberately graph-agnostic; pandect is the lowest crate that knows
//! both [`GraphSnapshot`] and the eidetic [`Store`]. It is store-backend-agnostic
//! (it speaks the `Store` trait), so it is not filesystem-gated like the
//! `graph.json` sidecar — a wasm host's OPFS store works the same way.

use eidetic::{
    BlobManifest, BlobSource, Error, ManifestId, NoFetcher, PayloadSealer, PrivacyClass,
    ProvenanceOrigin, ProvenanceRecord, Result, SchemaRef, Store, Timestamp, TrustEnvelope,
    TypedPayload, list_typed, load_typed_sealed, save_typed_sealed,
};
use kernel::graph::Graph;
use kernel::persistence::GraphSnapshot;
use kernel::types::ImageRole;
use uuid::Uuid;

use crate::NodeFacetStore;
use eidetic::manifest::load_manifest;

/// Schema id bytes for the graph-snapshot codicil schema. The [`SchemaRef`] is the
/// BLAKE3 of these bytes, so it is stable across builds and machines.
pub const GRAPH_SNAPSHOT_SCHEMA_ID: &[u8] = b"mere.graph-snapshot/v2";
const LEGACY_GRAPH_SNAPSHOT_SCHEMA_ID: &[u8] = b"mere.graph-snapshot/v1";

/// The content-addressed schema reference every graph codicil is tagged with.
///
/// The eidetic typed layer uses this as an identity tag — checked on load so a
/// mistyped read errors rather than deserializing garbage. Registering a full
/// schema-definition codicil (with validators, for cross-language / federation
/// reads) is a later, optional step; the freeze/thaw spine does not need it.
pub fn graph_snapshot_schema_ref() -> SchemaRef {
    SchemaRef::from_id(ManifestId::of_blob(GRAPH_SNAPSHOT_SCHEMA_ID))
}

fn legacy_graph_snapshot_schema_ref() -> SchemaRef {
    SchemaRef::from_id(ManifestId::of_blob(LEGACY_GRAPH_SNAPSHOT_SCHEMA_ID))
}

/// A graph snapshot and its one live facet store, bound to the graph-codicil
/// schema.
///
/// Version 1 carried a bare [`GraphSnapshot`]. Version 2 adds the sidecar because
/// optional node metadata no longer lives in snapshot columns.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GraphCodicil {
    pub snapshot: GraphSnapshot,
    #[serde(default)]
    pub facets: NodeFacetStore,
}

impl TypedPayload for GraphCodicil {
    fn schema_ref() -> SchemaRef {
        graph_snapshot_schema_ref()
    }
    // Default serde_json serializer: consistent with `session_graph_store`'s
    // `graph.json`, hash-stable, and free of the rkyv-alignment-from-store
    // concern. A compact rkyv override is a later optimization (the architecture
    // doc's stated preference), not load-bearing for the spine.
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
struct LegacyGraphCodicil(GraphSnapshot);

impl TypedPayload for LegacyGraphCodicil {
    fn schema_ref() -> SchemaRef {
        legacy_graph_snapshot_schema_ref()
    }
}

impl GraphCodicil {
    fn into_graph(self) -> Graph {
        let mut graph = Graph::from_snapshot(&self.snapshot);
        graph.overlay_facets(self.facets);
        graph
    }
}

/// What a graph codicil keeps from the live snapshot.
///
/// The default is conservative (Alembic plan open decision #7): private / heavy
/// per-node state is stripped, leaving graph structure, addresses, titles,
/// tags, classifications, provenance, properties, and other portable facets.
/// Callers opt state back in explicitly with [`RedactionPolicy::include_all`]
/// or by setting a flag, so a shareable codicil does not leak screenshots,
/// drafts, or browser-runtime facets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedactionPolicy {
    /// Keep preview/snapshot image references. Off by default (heavy, private).
    pub include_thumbnails: bool,
    /// Keep favicon image references. Off by default (heavy).
    pub include_favicons: bool,
    /// Keep legacy session state and `web.*` runtime facets. Off by default.
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
    /// heavy / private fields are wanted; never for a shareable codicil.
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
            // Redaction drops the *reference*; the blob is the orphan sweep's
            // business. An exported codicil therefore names no image the
            // recipient could resolve, which is what redaction owes.
            if !self.include_thumbnails {
                node.images.remove(&ImageRole::Preview);
                node.images.remove(&ImageRole::Snapshot);
            }
            if !self.include_favicons {
                node.images.remove(&ImageRole::Favicon);
            }
            if !self.include_session_state {
                node.session_state = None;
            }
        }
    }

    fn apply_facets(&self, facets: &mut NodeFacetStore) {
        if self.include_session_state {
            return;
        }
        let private_runtime_facets = [
            "web.scroll",
            "web.form_draft",
            "web.viewer",
            "web.compat",
            "web.content",
            "web.page_scale",
        ];
        let nodes = facets.iter().map(|(node, _)| *node).collect::<Vec<_>>();
        for node in nodes {
            for facet in private_runtime_facets {
                facets.remove(&node, &chartulary::FacetId::new(facet));
            }
        }
    }
}

/// Provenance for a locally-frozen graph codicil: generated by this tool at the
/// freeze time, no upstream ancestry (composition / merge fills `upstream` later).
fn graph_codicil_provenance(created_at: Timestamp) -> ProvenanceRecord {
    ProvenanceRecord {
        origin: ProvenanceOrigin::Generated,
        upstream: Vec::new(),
        tooling: Some(concat!("pandect/graph-codicil@", env!("CARGO_PKG_VERSION")).to_string()),
        generated_at: created_at,
    }
}

/// Freeze a live `graph` into a content-addressed graph codicil, returning its
/// manifest id.
///
/// The snapshot is redacted per `redaction` (default conservative) before
/// serialization; the codicil is `LocalOnly` and self-asserted — promotion to a
/// wider audience is always an explicit later act. `created_at` is the freeze
/// time in Unix milliseconds (the host passes "now"; tests pass a fixed value so
/// the call stays pure).
pub async fn save_graph_codicil(
    store: &mut dyn Store,
    graph: &Graph,
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<ManifestId> {
    save_graph_codicil_sealed(store, None, graph, redaction, created_at).await
}

/// As [`save_graph_codicil`], but sealing the codicil at rest under `sealer`.
///
/// A graph codicil is `LocalOnly` (the private lane), so with a `Some(sealer)` its
/// stored bytes are sealed under the persona epoch and unreadable at rest without
/// the wallet. `None` keeps the cleartext behavior. Read back with
/// [`open_codicil_as_session_sealed`].
pub async fn save_graph_codicil_sealed(
    store: &mut dyn Store,
    sealer: Option<&dyn PayloadSealer>,
    graph: &Graph,
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<ManifestId> {
    save_graph_snapshot_codicil_sealed(
        store,
        sealer,
        graph.to_snapshot(),
        graph.facets().clone(),
        redaction,
        created_at,
    )
    .await
}

/// As [`save_graph_codicil`], but from an already-materialized snapshot and
/// facet store.
///
/// The primitive a host uses when it has snapshotted the graph already — taking
/// the snapshot ends the borrow of the live graph, so a `&mut Store` borrowed
/// from the same owner can follow without a conflict. Also the entry the
/// Timeline's "distil this past state" reuses (slice E). Both parts are
/// redacted in place per `redaction`.
pub async fn save_graph_snapshot_codicil(
    store: &mut dyn Store,
    snapshot: GraphSnapshot,
    facets: NodeFacetStore,
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<ManifestId> {
    save_graph_snapshot_codicil_sealed(store, None, snapshot, facets, redaction, created_at).await
}

/// As [`save_graph_snapshot_codicil`], but sealing the codicil at rest under
/// `sealer` (private-lane encrypt-at-rest for the graph snapshot).
pub async fn save_graph_snapshot_codicil_sealed(
    store: &mut dyn Store,
    sealer: Option<&dyn PayloadSealer>,
    mut snapshot: GraphSnapshot,
    mut facets: NodeFacetStore,
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<ManifestId> {
    redaction.apply(&mut snapshot);
    redaction.apply_facets(&mut facets);
    save_typed_sealed(
        store,
        sealer,
        &GraphCodicil { snapshot, facets },
        Vec::<BlobSource>::new(),
        PrivacyClass::LocalOnly,
        graph_codicil_provenance(created_at),
        TrustEnvelope::self_asserted(),
        created_at,
    )
    .await
}

/// Thaw a stored graph codicil back into a live [`Graph`]. `Ok(None)` if no codicil
/// is stored under `id`.
///
/// The store is read-only here (eidetic replay-isolation), so opening a codicil
/// never mutates it; editing the returned graph forks a thaw and only persists if
/// re-saved as a new codicil.
pub async fn open_codicil_as_session(
    store: &mut dyn Store,
    id: ManifestId,
) -> Result<Option<Graph>> {
    open_codicil_as_session_sealed(store, None, id).await
}

/// Load the persisted graph-codicil payload without materializing a live graph.
///
/// Legacy v1 payloads are upgraded in memory to the v2 snapshot + facet-store
/// shape.
pub async fn load_graph_codicil(
    store: &mut dyn Store,
    id: ManifestId,
) -> Result<Option<GraphCodicil>> {
    load_graph_codicil_sealed(store, None, id).await
}

/// As [`open_codicil_as_session`], but unsealing a sealed codicil with `sealer`.
/// A sealed codicil opened with `sealer = None` is a hard error (from the eidetic
/// seal seam), never a silent failure.
pub async fn open_codicil_as_session_sealed(
    store: &mut dyn Store,
    sealer: Option<&dyn PayloadSealer>,
    id: ManifestId,
) -> Result<Option<Graph>> {
    let mut fetcher = NoFetcher;
    let Some(manifest) = load_manifest(store, id).await? else {
        return Ok(None);
    };
    if manifest.schema == graph_snapshot_schema_ref() {
        let payload = load_typed_sealed::<GraphCodicil>(store, &mut fetcher, sealer, id).await?;
        return Ok(payload.map(GraphCodicil::into_graph));
    }
    if manifest.schema == legacy_graph_snapshot_schema_ref() {
        let payload =
            load_typed_sealed::<LegacyGraphCodicil>(store, &mut fetcher, sealer, id).await?;
        return Ok(payload.map(|codicil| Graph::from_snapshot(&codicil.0)));
    }
    Err(Error::new(format!(
        "manifest {} is not a graph codicil",
        id
    )))
}

/// List manifests for current v2 and readable legacy v1 graph codicils. Order is
/// store-defined; callers that want newest-first should sort on `created_at`.
pub async fn list_graph_codicils(store: &mut dyn Store) -> Result<Vec<BlobManifest>> {
    let mut manifests = list_typed::<GraphCodicil>(store).await?;
    manifests.extend(list_typed::<LegacyGraphCodicil>(store).await?);
    Ok(manifests)
}

/// Compose several graph codicils into one by URL-identity merge (Alembic tail B7 /
/// decision #1). Thaws each id's snapshot, folds them with
/// [`merge_snapshots`](crate::snapshot_merge::merge_snapshots) (the first id is the
/// canonical base), and saves the union as a new codicil.
///
/// The new codicil is `Derived` and its `ProvenanceRecord.upstream` records the source
/// ids — the lineage `upstream` (empty on every freeze until now) finally populated,
/// which is what the consolidation pass reads to relate version chains. `Ok(None)` if
/// `ids` is empty or any id is missing (a partial compose would silently lose a source,
/// so it aborts instead). Like a freeze, the result is `LocalOnly` + self-asserted.
pub async fn compose_graph_codicils(
    store: &mut dyn Store,
    ids: &[ManifestId],
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<Option<ManifestId>> {
    compose_graph_codicils_sealed(store, None, ids, redaction, created_at).await
}

/// As [`compose_graph_codicils`], but sealer-aware: unseals sealed sources on read
/// and seals the composed result at rest. All source codicils must be readable
/// with `sealer` (they share the persona's epoch history); without this variant a
/// compose over sealed sources would fail at the first sealed read.
pub async fn compose_graph_codicils_sealed(
    store: &mut dyn Store,
    sealer: Option<&dyn PayloadSealer>,
    ids: &[ManifestId],
    redaction: RedactionPolicy,
    created_at: Timestamp,
) -> Result<Option<ManifestId>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let mut acc: Option<GraphCodicil> = None;
    for id in ids {
        let Some(codicil) = load_graph_codicil_sealed(store, sealer, *id).await? else {
            return Ok(None);
        };
        acc = Some(match acc {
            None => codicil,
            Some(base) => merge_graph_codicils(base, codicil),
        });
    }
    let mut codicil = acc.expect("ids is non-empty, so acc is Some");
    redaction.apply(&mut codicil.snapshot);
    redaction.apply_facets(&mut codicil.facets);
    let provenance = ProvenanceRecord {
        origin: ProvenanceOrigin::Derived,
        upstream: ids.to_vec(),
        tooling: Some(
            concat!("pandect/graph-codicil-compose@", env!("CARGO_PKG_VERSION")).to_string(),
        ),
        generated_at: created_at,
    };
    let id = save_typed_sealed(
        store,
        sealer,
        &codicil,
        Vec::<BlobSource>::new(),
        PrivacyClass::LocalOnly,
        provenance,
        TrustEnvelope::self_asserted(),
        created_at,
    )
    .await?;
    Ok(Some(id))
}

async fn load_graph_codicil_sealed(
    store: &mut dyn Store,
    sealer: Option<&dyn PayloadSealer>,
    id: ManifestId,
) -> Result<Option<GraphCodicil>> {
    let Some(manifest) = load_manifest(store, id).await? else {
        return Ok(None);
    };
    let mut fetcher = NoFetcher;
    if manifest.schema == graph_snapshot_schema_ref() {
        return load_typed_sealed::<GraphCodicil>(store, &mut fetcher, sealer, id).await;
    }
    if manifest.schema == legacy_graph_snapshot_schema_ref() {
        return Ok(
            load_typed_sealed::<LegacyGraphCodicil>(store, &mut fetcher, sealer, id)
                .await?
                .map(|legacy| {
                    let graph = Graph::from_snapshot(&legacy.0);
                    GraphCodicil {
                        snapshot: graph.to_snapshot(),
                        facets: graph.facets().clone(),
                    }
                }),
        );
    }
    Err(Error::new(format!(
        "manifest {} is not a graph codicil",
        id
    )))
}

fn merge_graph_codicils(a: GraphCodicil, b: GraphCodicil) -> GraphCodicil {
    let (snapshot, _, remap) =
        crate::snapshot_merge::merge_snapshots_with_remap(&a.snapshot, &b.snapshot);
    let mut facets = a.facets;
    for (node, node_facets) in b.facets.iter() {
        let canonical = remap
            .get(&node.to_string())
            .and_then(|id| Uuid::parse_str(id).ok())
            .unwrap_or(*node);
        for (facet, value) in node_facets.iter() {
            if facets.get(&canonical, facet).is_none() {
                facets
                    .set(
                        canonical,
                        facet.clone(),
                        value.clone(),
                        &chartulary::AcceptAll,
                    )
                    .expect("AcceptAll cannot reject a composed facet");
            }
        }
    }
    GraphCodicil { snapshot, facets }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use euclid::default::Point2D;
    use kernel::graph::fixtures::GraphFixtures;
    use kernel::persistence::PersistedNodeSessionState;
    use kernel::types::ImageRef;
    use std::collections::HashMap;

    /// In-memory `Store` for the round-trip tests — the same shape
    /// `content_store`'s tests and eidetic's own tests use.
    // The in-memory test store is muniment's (2026-07-12): the
    // hand-rolled one was the same map behind the same seam.
    use muniment::MemoryBackend as InMemoryStore;

    fn set_pinned(graph: &mut Graph, url: &str, pinned: bool) {
        let (_, node) = graph.get_node_by_url(url).unwrap();
        let node_id = node.id;
        graph
            .facets_mut()
            .set(
                node_id,
                chartulary::FacetId::new(kernel::graph::node_facets::ARRANGEMENT_PIN),
                serde_json::json!(pinned),
                &chartulary::AcceptAll,
            )
            .unwrap();
    }

    fn sample_graph() -> Graph {
        let mut graph = Graph::new();
        graph.add_node("https://a.example".to_string(), Point2D::new(1.0, 2.0));
        graph.add_node("https://b.example".to_string(), Point2D::new(3.0, 4.0));
        set_pinned(&mut graph, "https://a.example", true);
        graph
    }

    #[test]
    fn save_then_open_round_trips_the_graph() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let id = save_graph_codicil(
                &mut store,
                &sample_graph(),
                RedactionPolicy::default(),
                Timestamp(1_700_000_000_000),
            )
            .await
            .expect("save");

            let opened = open_codicil_as_session(&mut store, id)
                .await
                .expect("load ok")
                .expect("codicil present after save");
            assert_eq!(opened.nodes().count(), 2, "both nodes survive freeze/thaw");
            assert!(
                opened.get_node_by_url("https://a.example").is_some(),
                "the URL index rebuilds from the thawed snapshot",
            );
            let (a, _) = opened.get_node_by_url("https://a.example").unwrap();
            assert_eq!(
                opened.node_is_pinned(a),
                Some(true),
                "the graph-owned facet store survives freeze/thaw"
            );
        });
    }

    #[test]
    fn legacy_v1_codicil_loads_and_imports_its_inline_metadata() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let mut snapshot = sample_graph().to_snapshot();
            snapshot.nodes[0].is_pinned = true;
            let id = save_typed_sealed(
                &mut store,
                None,
                &LegacyGraphCodicil(snapshot),
                Vec::<BlobSource>::new(),
                PrivacyClass::LocalOnly,
                graph_codicil_provenance(Timestamp(1)),
                TrustEnvelope::self_asserted(),
                Timestamp(1),
            )
            .await
            .unwrap();

            let opened = open_codicil_as_session(&mut store, id)
                .await
                .unwrap()
                .unwrap();
            let (a, _) = opened.get_node_by_url("https://a.example").unwrap();
            assert_eq!(opened.node_is_pinned(a), Some(true));
        });
    }

    #[test]
    fn default_redaction_strips_private_web_facets_but_keeps_other_facets() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let mut graph = sample_graph();
            let (_, node) = graph.get_node_by_url("https://a.example").unwrap();
            let node_id = node.id;
            graph
                .facets_mut()
                .set(
                    node_id,
                    chartulary::FacetId::new("web.form_draft"),
                    serde_json::json!("secret"),
                    &chartulary::AcceptAll,
                )
                .unwrap();
            graph
                .facets_mut()
                .set(
                    node_id,
                    chartulary::FacetId::new("web.page_scale"),
                    serde_json::json!(1.5),
                    &chartulary::AcceptAll,
                )
                .unwrap();
            graph
                .facets_mut()
                .set(
                    node_id,
                    chartulary::FacetId::new("foreign.keep"),
                    serde_json::json!({"portable": true}),
                    &chartulary::AcceptAll,
                )
                .unwrap();

            let id =
                save_graph_codicil(&mut store, &graph, RedactionPolicy::default(), Timestamp(1))
                    .await
                    .unwrap();
            let opened = open_codicil_as_session(&mut store, id)
                .await
                .unwrap()
                .unwrap();
            assert!(
                opened
                    .facets()
                    .get(&node_id, &chartulary::FacetId::new("web.form_draft"))
                    .is_none()
            );
            assert!(
                opened
                    .facets()
                    .get(&node_id, &chartulary::FacetId::new("web.page_scale"))
                    .is_none()
            );
            assert_eq!(
                opened
                    .facets()
                    .get(&node_id, &chartulary::FacetId::new("foreign.keep")),
                Some(&serde_json::json!({"portable": true}))
            );
        });
    }

    #[test]
    fn default_redaction_strips_private_fields() {
        // A snapshot carrying a thumbnail, favicon, and session state (scroll +
        // form draft); the default policy must drop all three but keep structure.
        let mut snapshot = sample_graph().to_snapshot();
        for node in &mut snapshot.nodes {
            node.images
                .insert(ImageRole::Preview, ImageRef::new([1u8; 32], 2, 2));
            node.images
                .insert(ImageRole::Favicon, ImageRef::new([4u8; 32], 1, 1));
            node.session_state = Some(PersistedNodeSessionState {
                scroll_x: Some(10.0),
                scroll_y: Some(20.0),
                form_draft: Some("a secret draft".to_string()),
                last_visited_ms: None,
            });
        }

        RedactionPolicy::default().apply(&mut snapshot);

        for node in &snapshot.nodes {
            assert!(
                !node.images.contains_key(&ImageRole::Preview),
                "thumbnail stripped",
            );
            assert!(
                !node.images.contains_key(&ImageRole::Favicon),
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
            node.images
                .insert(ImageRole::Preview, ImageRef::new([9u8; 32], 1, 1));
            node.images
                .insert(ImageRole::Favicon, ImageRef::new([9u8; 32], 1, 1));
        }
        RedactionPolicy::include_all().apply(&mut snapshot);
        assert!(
            snapshot
                .nodes
                .iter()
                .all(|n| n.images.contains_key(&ImageRole::Preview)),
            "include_all keeps thumbnails (opt-in, never the default)",
        );
    }

    #[test]
    fn list_graph_codicils_finds_saved_codicils_tagged_with_the_schema() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            save_graph_codicil(
                &mut store,
                &sample_graph(),
                RedactionPolicy::default(),
                Timestamp(1),
            )
            .await
            .expect("save");

            let codicils = list_graph_codicils(&mut store).await.expect("list");
            assert_eq!(codicils.len(), 1, "the saved codicil is listed");
            assert_eq!(
                codicils[0].schema,
                graph_snapshot_schema_ref(),
                "tagged with the graph-snapshot schema",
            );
        });
    }

    #[test]
    fn codicil_survives_a_store_close_and_reopen() {
        // The faithful "survives restart" proof: save through a real fjall store,
        // drop it (shutdown), reopen at the same path, and thaw the same graph.
        use eidetic_fjall::FjallStore;

        let dir = std::env::temp_dir().join("mere_graph_codicil_fjall_reopen");
        let _ = std::fs::remove_dir_all(&dir);

        let id = pollster::block_on(async {
            let mut store = FjallStore::open(&dir).expect("open store");
            save_graph_codicil(
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
            open_codicil_as_session(&mut store, id)
                .await
                .expect("load ok")
        })
        .expect("the codicil is present after a store reopen");
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
                open_codicil_as_session(&mut store, unknown)
                    .await
                    .expect("load ok")
                    .is_none(),
                "an unknown id thaws to None, not an error",
            );
        });
    }

    #[test]
    fn compose_unions_two_codicils_and_records_the_upstream_lineage() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            // Codicil A: x, y. Codicil B: y (shared url), z.
            let mut ga = Graph::new();
            ga.add_node("https://x.example".to_string(), Point2D::new(0.0, 0.0));
            ga.add_node("https://y.example".to_string(), Point2D::new(1.0, 0.0));
            set_pinned(&mut ga, "https://y.example", false);
            let mut gb = Graph::new();
            gb.add_node("https://y.example".to_string(), Point2D::new(0.0, 0.0));
            gb.add_node("https://z.example".to_string(), Point2D::new(1.0, 0.0));
            set_pinned(&mut gb, "https://y.example", true);
            set_pinned(&mut gb, "https://z.example", true);

            let id_a =
                save_graph_codicil(&mut store, &ga, RedactionPolicy::default(), Timestamp(1))
                    .await
                    .expect("save a");
            let id_b =
                save_graph_codicil(&mut store, &gb, RedactionPolicy::default(), Timestamp(2))
                    .await
                    .expect("save b");

            let composed = compose_graph_codicils(
                &mut store,
                &[id_a, id_b],
                RedactionPolicy::default(),
                Timestamp(3),
            )
            .await
            .expect("compose ok")
            .expect("a non-empty id list composes a codicil");

            // The thawed union carries x, y, z — the shared y is not doubled.
            let graph = open_codicil_as_session(&mut store, composed)
                .await
                .expect("load ok")
                .expect("the composed codicil is present");
            assert_eq!(
                graph.nodes().count(),
                3,
                "x, y, z union; shared y not doubled"
            );
            assert!(graph.get_node_by_url("https://x.example").is_some());
            assert!(graph.get_node_by_url("https://z.example").is_some());
            let (y, _) = graph.get_node_by_url("https://y.example").unwrap();
            let (z, _) = graph.get_node_by_url("https://z.example").unwrap();
            assert_eq!(
                graph.node_is_pinned(y),
                Some(false),
                "the canonical source wins a same-node facet conflict"
            );
            assert_eq!(
                graph.node_is_pinned(z),
                Some(true),
                "facets on an added node are remapped into the union"
            );

            // The lineage: the composed codicil is `Derived` and names both sources —
            // the `upstream` Vec that is empty on every freeze, finally populated.
            let manifests = list_graph_codicils(&mut store).await.expect("list");
            let m = manifests
                .iter()
                .find(|m| m.id == composed)
                .expect("the composed manifest is listed");
            assert_eq!(
                m.provenance.origin,
                ProvenanceOrigin::Derived,
                "a composed codicil is Derived"
            );
            assert_eq!(
                m.provenance.upstream,
                vec![id_a, id_b],
                "upstream records the source codicils",
            );
        });
    }

    #[test]
    fn compose_of_an_empty_id_list_is_none() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            assert!(
                compose_graph_codicils(&mut store, &[], RedactionPolicy::default(), Timestamp(1))
                    .await
                    .expect("ok")
                    .is_none(),
                "no sources -> nothing composed",
            );
        });
    }
}
