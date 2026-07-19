// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Durable session manifest types — `GraphSessionManifest` is the
//! per-session record on disk that anchors session identity,
//! references its root graph + sub-graphs, holds the worker
//! manifest, engine-profile binding, and policy overrides.
//!
//! This module ships only the **types**. Lifecycle (load-all-at-
//! startup, persist-on-mutation, archive-on-kill) lands in the
//! `manifest_store` module per the 2026-05-11 manifest plan.
//!
//! v0 fields are minimal; later plans (`SessionServiceRunner`,
//! engine profile boundary, capability gates, fork-on-divergence)
//! populate the reserved fields. Every new field uses
//! `#[serde(default)]` so pre-existing manifests load unchanged.

use std::path::PathBuf;
use std::time::SystemTime;

use incipit::{GraphId, SessionId};
pub use identity::PersonaId;
use serde::{Deserialize, Serialize};

/// Current manifest schema version. Bump on incompatible changes
/// (field renames, semantic shifts). Adding `#[serde(default)]`
/// fields does **not** require a bump.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Reference to a consolidated engram (eidetic). v0 placeholder —
/// eidetic's surface for engram identity will land when the
/// consolidation gesture is wired. String form keeps it
/// hand-inspectable in the meantime.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngramId(pub String);

/// Where the engine's user-data folder lives for this session.
/// Persona-scoped is the default; sessions/graphs can escalate when
/// isolation is explicitly required. See the
/// `design_docs/mere_docs/research/2026-05-11_browser_multiplexer_framing.md`
/// §5.4.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineProfileBinding {
    /// Share the persona's engine UDF with every session.
    #[default]
    PersonaScoped,
    /// This session gets its own UDF.
    SessionScoped,
    /// A specific graph inside this session gets its own UDF
    /// (reserved; not used in v0).
    GraphScoped,
}

/// A background worker the session has declared. Real worker
/// implementations land alongside their workloads — `WorkerKind` is
/// the *vocabulary* the manifest persists, not a guarantee that a
/// runner can service every variant. Per the
/// [SessionServiceRunner plan](../../../../design_docs/mere_docs/implementation_strategy/2026-05-14_session_service_runner_plan.md)
/// §3, a runner returns `WorkerStartError::Unsupported` for kinds it
/// can't handle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkerKind {
    /// No worker declared. Placeholder retained so the enum can be
    /// serialised before any real worker kinds land.
    #[default]
    None,
    /// Background URL fetcher pool — populates document caches and
    /// surfaces preview content for tiles. v0b adds the implementation.
    FetcherPool,
    /// Embedding producer (BERT-class model) — runs on demand to
    /// populate the embed store.
    Embedder,
    /// Indexer that walks document content into the `SearchIndex`
    /// engram referenced by the framing brief §3.
    Indexer,
    /// Intelligence-signal producer — clusters, affinity, recency
    /// signals consumed by cartography strategies.
    IntelligenceSignalProducer,
}

/// Capability overrides for this session. v0 ships an empty struct
/// — the capability-gate catalogue brief lands the actual override
/// shape. Reserving the field now lets later plans add overrides
/// without a schema bump.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPolicy {
    /// Reserved for future capability-override entries. Empty in v0.
    #[serde(default)]
    pub overrides: Vec<SessionPolicyOverride>,
}

/// Placeholder for a single capability override. v0: unit-like;
/// real shape lands with the gate catalogue. Reserved so adding
/// real entries later doesn't require migrating already-saved
/// manifests.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPolicyOverride {}

/// The durable session record. One file on disk per session
/// (`<data_dir>/mere/host/sessions/<session_id>/manifest.json`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphSessionManifest {
    /// Schema version. Bumped on incompatible changes.
    pub schema_version: u32,
    /// Durable session identity. Stable across restarts.
    pub session_id: SessionId,
    /// The session's primary graph. v0: every session has one;
    /// `sub_graph_refs` is empty.
    pub root_graph_id: GraphId,
    /// Additional graphs referenced by this session (e.g. graphlets
    /// promoted to first-class sub-graphs). v0 empty.
    #[serde(default)]
    pub sub_graph_refs: Vec<GraphId>,
    /// User-set display name. `None` = host derives from graph
    /// contents (first non-intro URL or short UUID).
    #[serde(default)]
    pub display_name: Option<String>,
    /// Owning persona. v0 always `PersonaId::default_persona()`.
    #[serde(default)]
    pub persona_id: PersonaId,
    /// When the session was minted.
    pub created_at: SystemTime,
    /// Most recent mutation observed by the manifest store. Used
    /// for "sort by recent" affordances + idle-consolidation
    /// policies.
    pub updated_at: SystemTime,
    /// Parent session this one was forked from, if any. Phase 3's
    /// `Fork` operation populates this; the reference is **weak**
    /// (parent is allowed to be killed without affecting this
    /// child).
    #[serde(default)]
    pub parent_session: Option<SessionId>,
    /// Engrams produced from this session by consolidation. See
    /// `design_docs/mere_docs/research/2026-05-11_memory_tiers_brief.md`.
    #[serde(default)]
    pub consolidated_engrams: Vec<EngramId>,
    /// Most recent consolidation timestamp. Drives "consolidate on
    /// idle" policies (when opted in). `None` = never consolidated.
    #[serde(default)]
    pub last_consolidated_at: Option<SystemTime>,
    /// Background workers this session has declared. v0 empty;
    /// `SessionServiceRunner` plan populates.
    #[serde(default)]
    pub active_workers: Vec<WorkerKind>,
    /// Engine profile binding. Defaults to persona-scoped UDF.
    #[serde(default)]
    pub engine_profile: EngineProfileBinding,
    /// Session-scope capability policy. v0: empty overrides.
    #[serde(default)]
    pub policy: SessionPolicy,
    /// On-disk root for this session's data. Stored explicitly so
    /// the manifest is self-describing if it's ever copied out of
    /// its host directory.
    #[serde(default)]
    pub storage_path: Option<PathBuf>,
}

impl GraphSessionManifest {
    /// Build a fresh manifest for a brand-new session. `created_at`
    /// and `updated_at` are stamped now; everything else takes its
    /// default. `persona_id` defaults to the v0 single persona.
    pub fn new(session_id: SessionId, root_graph_id: GraphId) -> Self {
        let now = SystemTime::now();
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            session_id,
            root_graph_id,
            sub_graph_refs: Vec::new(),
            display_name: None,
            persona_id: PersonaId::default_persona(),
            created_at: now,
            updated_at: now,
            parent_session: None,
            consolidated_engrams: Vec::new(),
            last_consolidated_at: None,
            active_workers: Vec::new(),
            engine_profile: EngineProfileBinding::default(),
            policy: SessionPolicy::default(),
            storage_path: None,
        }
    }

    /// Mark this manifest dirty by advancing `updated_at` to now.
    /// Called by the manifest store on any mutation that should
    /// reset idle-consolidation timers.
    pub fn touch(&mut self) {
        self.updated_at = SystemTime::now();
    }

    /// Record a fork relationship — this session was created by
    /// forking off `parent`. v0 is a weak reference: nothing checks
    /// that the parent still exists.
    pub fn set_parent(&mut self, parent: SessionId) {
        self.parent_session = Some(parent);
    }

    /// Append a consolidation receipt. Bumps `last_consolidated_at`
    /// to now.
    pub fn record_consolidation(&mut self, engram: EngramId) {
        self.consolidated_engrams.push(engram);
        self.last_consolidated_at = Some(SystemTime::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn fixture_session() -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(0x1234_5678_90ab_cdef_1234_5678_90ab_cdef))
    }

    fn fixture_graph() -> GraphId {
        GraphId::from_uuid(Uuid::from_u128(0xabcd_ef01_2345_6789_abcd_ef01_2345_6789))
    }

    #[test]
    fn manifest_new_carries_schema_version_and_ids() {
        let m = GraphSessionManifest::new(fixture_session(), fixture_graph());
        assert_eq!(m.schema_version, MANIFEST_SCHEMA_VERSION);
        assert_eq!(m.session_id, fixture_session());
        assert_eq!(m.root_graph_id, fixture_graph());
        assert!(m.sub_graph_refs.is_empty());
        assert!(m.parent_session.is_none());
        assert!(m.consolidated_engrams.is_empty());
        assert_eq!(m.persona_id, PersonaId::default_persona());
        assert_eq!(m.engine_profile, EngineProfileBinding::PersonaScoped);
    }

    #[test]
    fn manifest_round_trips_through_serde_json() {
        let mut m = GraphSessionManifest::new(fixture_session(), fixture_graph());
        m.display_name = Some("research-session".to_string());
        m.set_parent(SessionId::from_uuid(Uuid::from_u128(0xdeadbeef)));
        m.record_consolidation(EngramId("engram-hash-abc123".to_string()));
        m.engine_profile = EngineProfileBinding::SessionScoped;

        let json = serde_json::to_string(&m).expect("serialize");
        let restored: GraphSessionManifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.session_id, m.session_id);
        assert_eq!(restored.root_graph_id, m.root_graph_id);
        assert_eq!(restored.display_name, m.display_name);
        assert_eq!(restored.parent_session, m.parent_session);
        assert_eq!(restored.consolidated_engrams, m.consolidated_engrams);
        assert_eq!(restored.engine_profile, m.engine_profile);
    }

    #[test]
    fn manifest_loads_minimal_legacy_shape_via_serde_default() {
        // Older manifests written before the `consolidated_engrams`,
        // `last_consolidated_at`, `active_workers`, `engine_profile`,
        // `policy`, `parent_session`, `sub_graph_refs`,
        // `display_name`, `storage_path`, and `persona_id` fields
        // landed should still deserialize because every late
        // addition uses `#[serde(default)]`.
        let minimal = format!(
            r#"{{
                "schema_version": 1,
                "session_id": "{}",
                "root_graph_id": "{}",
                "created_at": {{ "secs_since_epoch": 0, "nanos_since_epoch": 0 }},
                "updated_at": {{ "secs_since_epoch": 0, "nanos_since_epoch": 0 }}
            }}"#,
            fixture_session().as_uuid(),
            fixture_graph().as_uuid(),
        );
        let restored: GraphSessionManifest =
            serde_json::from_str(&minimal).expect("legacy-shape deserialize");
        assert_eq!(restored.session_id, fixture_session());
        assert_eq!(restored.root_graph_id, fixture_graph());
        assert!(restored.sub_graph_refs.is_empty());
        assert!(restored.consolidated_engrams.is_empty());
        assert_eq!(restored.engine_profile, EngineProfileBinding::PersonaScoped);
        assert_eq!(restored.persona_id, PersonaId::default_persona());
    }

    #[test]
    fn touch_advances_updated_at() {
        let mut m = GraphSessionManifest::new(fixture_session(), fixture_graph());
        let original = m.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        m.touch();
        assert!(m.updated_at > original);
    }

    #[test]
    fn record_consolidation_bumps_last_consolidated_at() {
        let mut m = GraphSessionManifest::new(fixture_session(), fixture_graph());
        assert!(m.last_consolidated_at.is_none());
        m.record_consolidation(EngramId("e1".to_string()));
        assert!(m.last_consolidated_at.is_some());
        assert_eq!(m.consolidated_engrams.len(), 1);
    }

    #[test]
    fn default_persona_is_stable_across_calls() {
        let a = PersonaId::default_persona();
        let b = PersonaId::default_persona();
        assert_eq!(a, b);
    }
}
