// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Layer 4 — browsing memory, the domain the crate was named for.
//!
//! A [`BrowsingTrace`] is a chronological segment of one owner's (persona's)
//! traversals, stored as a typed payload (design pass §6.2). Traces are the
//! durable, portable impression of browsing — deliberately coarser than
//! `node-lineage`'s live per-owner branch detail, which remains the
//! graph-shaped working memory this module projects *from* (see
//! [`lineage`], feature `"lineage"`) rather than duplicates.
//!
//! [`BrowsingMemory`] composes traces into the user-facing reads: the recent
//! corridor, time-window slices, and traversal co-occurrence. Everything
//! saves **LocalOnly** — promotion to a wider audience is always a separate,
//! explicit act (design pass §8), never a side effect of remembering.
//!
//! Quota is a Layer-4 policy: [`BrowsingMemory::apply_quota`] keeps the N
//! most recent *stored* traces and deletes older manifests (blob bytes await
//! the explicit GC pass, per the manifest-deletion doctrine).

#[cfg(feature = "lineage")]
pub mod lineage;

use serde::{Deserialize, Serialize};

use crate::manifest::{NoFetcher, delete_manifest};
use crate::schema::{
    Hash, ManifestId, ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, SchemaRef,
    Timestamp, TrustEnvelope, TrustLevel,
};
use crate::typed::{TypedPayload, list_typed, load_typed, save_typed};
use crate::{Result, Store};

/// Canonical bytes of the `BrowsingTrace` schema engram's payload
/// (mere-native format, like the model-manifest schema). Their BLAKE3 hash
/// anchors [`BROWSING_TRACE_SCHEMA_REF`].
const BROWSING_TRACE_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"eidetic.BrowsingTrace/v1","body":{"version":1,"description":"A chronological segment of one owner's browsing traversals.","required":["owner","events","started_at_ms","ended_at_ms"],"fields":{"owner":{"type":"string"},"events":{"type":"array"},"started_at_ms":{"type":"integer"},"ended_at_ms":{"type":"integer"}}}}"#;

/// The well-known schema reference for [`BrowsingTrace`] payloads.
pub static BROWSING_TRACE_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(|| {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(
            BROWSING_TRACE_SCHEMA_PAYLOAD,
        )))
    });

/// Idempotently seed the `BrowsingTrace` schema engram into a Store.
/// Call once during init for any consumer that records or reads browsing
/// memory (the [`crate::bootstrap`] pattern).
pub async fn bootstrap_browsing_schema(store: &mut dyn Store) -> Result<()> {
    let id = BROWSING_TRACE_SCHEMA_REF.0;
    if crate::manifest::load_manifest(store, id).await?.is_some() {
        return Ok(());
    }
    // Write the payload bytes verbatim — re-serialization could reorder JSON
    // keys and break the hash anchor.
    let local_key = format!("blob:{}", Hash::of(BROWSING_TRACE_SCHEMA_PAYLOAD).to_hex());
    store.put(&local_key, BROWSING_TRACE_SCHEMA_PAYLOAD).await?;
    let manifest = crate::manifest::BlobManifest {
        id,
        schema: *crate::schema_def::META_SCHEMA_REF,
        content_hash: Hash::of(BROWSING_TRACE_SCHEMA_PAYLOAD),
        byte_size: BROWSING_TRACE_SCHEMA_PAYLOAD.len() as u64,
        created_at: Timestamp::ZERO,
        last_accessed: None,
        sources: vec![crate::manifest::BlobSource::Local { key: local_key }],
        privacy: PrivacyClass::PublicPortable,
        provenance: ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("eidetic/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp::ZERO,
        },
        trust: TrustEnvelope {
            level: TrustLevel::CheckpointAccepted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Accepted,
        },
        schema_metadata: serde_json::Value::Null,
        manifest_version: crate::manifest::BlobManifest::CURRENT_VERSION,
    };
    crate::manifest::save_manifest(store, &manifest).await
}

/// A visited page: canonical URL plus the title when one was known.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRef {
    pub url: String,
    pub title: Option<String>,
}

/// How a traversal happened. Mirrors `node-lineage`'s transition kinds as a
/// portable wire enum — the schema must not depend on the lineage crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TraceTransition {
    LinkClick,
    UrlTyped,
    Back,
    Forward,
    Reload,
    Redirect,
    TabSpawn,
    Restore,
    /// Brought in from another browser's history/bookmarks (the import
    /// bridge), not traversed live.
    Imported,
    Unknown,
}

/// One traversal: where the user went, from where, how, and when.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEvent {
    /// The page navigated from; `None` for an origin event (first visit of a
    /// session, a typed URL into a fresh surface, an import).
    pub from: Option<PageRef>,
    pub to: PageRef,
    pub transition: TraceTransition,
    pub at_ms: u64,
    /// Dwell on `to`, when the recorder knows it (live capture may; imports
    /// rarely do).
    pub dwell_ms: Option<u64>,
    /// The other pages this navigation was chosen *against* — the relational
    /// candidate set (the source page's outbound links, or a materialized
    /// neighborhood). `to` is the chosen one; the rest are the in-context
    /// "seen but not followed" negatives (the listwise relevance signal). Empty
    /// when no relational context was available. (Capture plan C2.)
    #[serde(default)]
    pub candidates: Vec<PageRef>,
}

/// A chronological segment of one owner's traversals — the typed payload
/// browsing memory accumulates. `owner` is an opaque persona tag carried
/// from day one (cheap to carry, expensive to retrofit); `""` means
/// unattributed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowsingTrace {
    pub owner: String,
    /// Events in chronological order.
    pub events: Vec<TraceEvent>,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
}

impl BrowsingTrace {
    /// Build a trace from chronologically ordered events. Bounds derive from
    /// the first and last event; an empty event list is allowed but inert.
    pub fn from_events(owner: impl Into<String>, events: Vec<TraceEvent>) -> Self {
        let started_at_ms = events.first().map(|e| e.at_ms).unwrap_or(0);
        let ended_at_ms = events.last().map(|e| e.at_ms).unwrap_or(0);
        Self {
            owner: owner.into(),
            events,
            started_at_ms,
            ended_at_ms,
        }
    }
}

impl TypedPayload for BrowsingTrace {
    fn schema_ref() -> SchemaRef {
        *BROWSING_TRACE_SCHEMA_REF
    }
}

/// Save one trace as a LocalOnly typed payload (the only privacy a recorder
/// writes; sharing is a separate explicit act). Returns the manifest id.
pub async fn save_trace(
    store: &mut dyn Store,
    trace: &BrowsingTrace,
    now_ms: u64,
) -> Result<ManifestId> {
    save_typed(
        store,
        trace,
        Vec::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("eidetic-browsing/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp(now_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        Timestamp(now_ms),
    )
    .await
}

/// The user-facing browsing memory: open (unflushed) segments per owner plus
/// the loaded trace corpus, with the design-pass reads over both.
pub struct BrowsingMemory {
    /// Stored traces (paired with their manifest ids) and adopted unsaved
    /// traces (`None`), kept sorted by `ended_at_ms`.
    traces: Vec<(Option<ManifestId>, BrowsingTrace)>,
    /// Open segment per owner tag, flushed at `segment_size` events.
    open: std::collections::BTreeMap<String, Vec<TraceEvent>>,
    /// Events per segment before [`record_traversal`](Self::record_traversal)
    /// asks for a flush. Configurable; quota and segmenting are policy, not
    /// constants.
    segment_size: usize,
}

impl BrowsingMemory {
    /// An empty memory with the given segment size (events per flushed
    /// trace).
    pub fn new(segment_size: usize) -> Self {
        Self {
            traces: Vec::new(),
            open: std::collections::BTreeMap::new(),
            segment_size: segment_size.max(1),
        }
    }

    /// Load every stored trace from the Store into a memory.
    pub async fn load(store: &mut dyn Store, segment_size: usize) -> Result<Self> {
        let mut memory = Self::new(segment_size);
        let mut fetcher = NoFetcher;
        for manifest in list_typed::<BrowsingTrace>(store).await? {
            if let Some(trace) =
                load_typed::<BrowsingTrace>(store, &mut fetcher, manifest.id).await?
            {
                memory.traces.push((Some(manifest.id), trace));
            }
        }
        memory.sort_traces();
        Ok(memory)
    }

    fn sort_traces(&mut self) {
        self.traces.sort_by_key(|(_, t)| t.ended_at_ms);
    }

    /// Append a traversal to `owner`'s open segment. Returns `true` when the
    /// segment has reached the configured size and wants a
    /// [`flush`](Self::flush).
    pub fn record_traversal(&mut self, owner: &str, event: TraceEvent) -> bool {
        let segment = self.open.entry(owner.to_string()).or_default();
        segment.push(event);
        segment.len() >= self.segment_size
    }

    /// Flush every non-empty open segment as a stored trace. Returns the new
    /// manifest ids.
    pub async fn flush(&mut self, store: &mut dyn Store, now_ms: u64) -> Result<Vec<ManifestId>> {
        let mut ids = Vec::new();
        let owners: Vec<String> = self.open.keys().cloned().collect();
        for owner in owners {
            let events = self.open.remove(&owner).unwrap_or_default();
            if events.is_empty() {
                continue;
            }
            let trace = BrowsingTrace::from_events(owner, events);
            let id = save_trace(store, &trace, now_ms).await?;
            ids.push(id);
            self.traces.push((Some(id), trace));
        }
        self.sort_traces();
        Ok(ids)
    }

    /// Adopt an already-built trace into the in-memory corpus without
    /// persisting it (projection and import outputs that the caller saves —
    /// or not — on its own schedule).
    pub fn adopt(&mut self, trace: BrowsingTrace) {
        self.traces.push((None, trace));
        self.sort_traces();
    }

    /// Every event, oldest first: stored corpus in trace order, then open
    /// segments.
    fn all_events(&self) -> impl Iterator<Item = &TraceEvent> {
        self.traces
            .iter()
            .flat_map(|(_, t)| t.events.iter())
            .chain(self.open.values().flatten())
    }

    /// The recent corridor: the last `n` traversals, returned in
    /// chronological order — the path that led here.
    pub fn recent_corridor(&self, n: usize) -> Vec<TraceEvent> {
        let all: Vec<&TraceEvent> = self.all_events().collect();
        let start = all.len().saturating_sub(n);
        all[start..].iter().map(|e| (*e).clone()).collect()
    }

    /// Pages visited in `[start_ms, end_ms]`, chronological, deduplicated by
    /// URL (first-seen title wins).
    pub fn nodes_visited_in_window(&self, start_ms: u64, end_ms: u64) -> Vec<PageRef> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for event in self.all_events() {
            if event.at_ms < start_ms || event.at_ms > end_ms {
                continue;
            }
            if seen.insert(event.to.url.clone()) {
                out.push(event.to.clone());
            }
        }
        out
    }

    /// How often `a` and `b` were traversed together: the count of direct
    /// traversals between them, in either direction. (Definition kept
    /// deliberately narrow and edge-shaped so it cannot drift from
    /// `node-lineage`'s aggregated edge view; broader togetherness measures
    /// are a consumer's fold.)
    pub fn co_occurrence(&self, a: &str, b: &str) -> u32 {
        self.all_events()
            .filter(|e| {
                let Some(from) = &e.from else { return false };
                (from.url == a && e.to.url == b) || (from.url == b && e.to.url == a)
            })
            .count() as u32
    }

    /// The loaded + adopted traces, oldest first.
    pub fn traces(&self) -> impl Iterator<Item = &BrowsingTrace> {
        self.traces.iter().map(|(_, t)| t)
    }

    /// Keep the `keep_n` most recent *stored* traces and delete the
    /// manifests of older ones (blob bytes await the explicit GC pass).
    /// Adopted unsaved traces are untouched — there is nothing stored to
    /// age out. Returns how many manifests were deleted.
    pub async fn apply_quota(&mut self, store: &mut dyn Store, keep_n: usize) -> Result<usize> {
        let stored: Vec<ManifestId> = self.traces.iter().filter_map(|(id, _)| *id).collect();
        if stored.len() <= keep_n {
            return Ok(0);
        }
        // traces is sorted oldest-first, so the overflow is the head.
        let overflow: Vec<ManifestId> = stored[..stored.len() - keep_n].to_vec();
        let mut deleted = 0;
        for id in &overflow {
            if delete_manifest(store, *id).await? {
                deleted += 1;
            }
        }
        self.traces
            .retain(|(id, _)| id.is_none_or(|id| !overflow.contains(&id)));
        Ok(deleted)
    }

    /// Forget every trace mentioning `url` (as either end of any event): delete its
    /// stored manifest and drop it from the loaded corpus and any open segment.
    /// Returns how many traces were forgotten. The `eidetic-search` index is rebuilt
    /// from this corpus per query, so a forgotten trace also leaves the index — a
    /// forget that left the index behind would not be a forget. (Capture plan C4.)
    pub async fn forget_url(&mut self, store: &mut dyn Store, url: &str) -> Result<usize> {
        let mentions = |trace: &BrowsingTrace| {
            trace
                .events
                .iter()
                .any(|e| e.to.url == url || e.from.as_ref().is_some_and(|f| f.url == url))
        };
        let mut forgotten = 0;
        let mut remaining = Vec::with_capacity(self.traces.len());
        for (id, trace) in std::mem::take(&mut self.traces) {
            if mentions(&trace) {
                if let Some(id) = id {
                    delete_manifest(store, id).await?;
                }
                forgotten += 1;
            } else {
                remaining.push((id, trace));
            }
        }
        self.traces = remaining;
        // Drop matching events from any unflushed open segment too.
        for events in self.open.values_mut() {
            events.retain(|e| !(e.to.url == url || e.from.as_ref().is_some_and(|f| f.url == url)));
        }
        Ok(forgotten)
    }
}

#[cfg(test)]
mod tests;
