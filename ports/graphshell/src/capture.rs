// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Consented browser-history intake into Mere and Eidetic browsing memory.
//!
//! Browser APIs stay at the extension edge. This module owns the portable
//! event shape, configurable privacy policy, graph projection, durable batch
//! boundary, and forget operation.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use chartulary::FacetId;
use eidetic::browsing::{
    BrowsingMemory, PageRef, TraceEvent, TraceTransition, bootstrap_browsing_schema,
};
use eidetic::{NoFetcher, PrivacyClass, list_typed, load_typed, manifest::delete_manifest};
use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
use mere::kernel::graph::{EdgeFamily, NavigationTrigger, RelationKind, RelationSelector};
use muniment::{Backend, StoreError, WriteOp};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::access::{
    ACCESS_HISTORY_FACET, AccessAction, AccessHistory, AccessObservation, AccessRecord,
    AccessTransition, bootstrap_access_record_schema, record_observation, save_access_record,
};
use crate::mere_host::{MereHost, MereHostError};
use crate::product::ProductError;

pub const BROWSER_HISTORY_FACET: &str = "graphshell.browser-history/v1";
pub const BROWSER_HISTORY_HANDLER_PREFIX: &str = "browser.history/";

#[cfg(not(target_arch = "wasm32"))]
pub trait CaptureBackend: Backend + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Backend + Sync> CaptureBackend for T {}

#[cfg(target_arch = "wasm32")]
pub trait CaptureBackend: Backend {}
#[cfg(target_arch = "wasm32")]
impl<T: Backend> CaptureBackend for T {}

/// Buffered writes for one capture transaction, plus where each key's most
/// recent write sits in them.
///
/// The ordered vec is what commits; `latest` exists only so a read does not
/// rescan it. `forget_url` interleaves a read per access-record manifest with
/// a `Delete` push into the same growing buffer, and unlike browsing traces
/// those manifests have no retention pass, so a linear overlay scan made that
/// loop quadratic in a set that only ever grows.
#[derive(Default)]
struct StagedWrites {
    ops: Vec<WriteOp>,
    latest: HashMap<String, usize>,
}

impl StagedWrites {
    fn push(&mut self, op: WriteOp) {
        let key = match &op {
            WriteOp::Put { key, .. } | WriteOp::Delete { key } => key.clone(),
        };
        self.latest.insert(key, self.ops.len());
        self.ops.push(op);
    }
}

/// Read-through, write-buffer backend for one capture transaction.
struct CaptureBatch<'a, B> {
    base: &'a B,
    staged: Mutex<StagedWrites>,
}

impl<'a, B: Backend> CaptureBatch<'a, B> {
    fn new(base: &'a B) -> Self {
        Self {
            base,
            staged: Mutex::new(StagedWrites::default()),
        }
    }

    /// Drain the buffer into the base backend. One transaction commits once
    /// and both callers drop the batch afterwards, so taking the buffer under
    /// the lock is cheaper than copying it and leaves no stale overlay behind.
    async fn commit(&self) -> Result<(), StoreError> {
        let staged = std::mem::take(&mut *self.staged.lock().unwrap());
        self.base.apply(&staged.ops).await
    }

    fn overlay(&self, key: &str) -> Option<Option<Vec<u8>>> {
        let staged = self.staged.lock().unwrap();
        match &staged.ops[*staged.latest.get(key)?] {
            WriteOp::Put { value, .. } => Some(Some(value.clone())),
            WriteOp::Delete { .. } => Some(None),
        }
    }

    fn overlay_keys(
        &self,
        mut keys: BTreeSet<String>,
        accepts: impl Fn(&str) -> bool,
    ) -> Vec<String> {
        // Only a key's last write decides whether it is present, so the index
        // gives the same answer as replaying the log. A key `accepts` rejects
        // is one the base listing never returned either, so its `Delete` was
        // already a no-op on this set.
        let staged = self.staged.lock().unwrap();
        for (key, index) in &staged.latest {
            match &staged.ops[*index] {
                WriteOp::Put { .. } if accepts(key) => {
                    keys.insert(key.clone());
                }
                WriteOp::Delete { .. } => {
                    keys.remove(key);
                }
                _ => {}
            }
        }
        keys.into_iter().collect()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<'a, B: CaptureBackend> Backend for CaptureBatch<'a, B> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        match self.overlay(key) {
            Some(value) => Ok(value),
            None => self.base.get(key).await,
        }
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.staged.lock().unwrap().push(WriteOp::Put {
            key: key.to_string(),
            value: bytes.to_vec(),
        });
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.staged.lock().unwrap().push(WriteOp::Delete {
            key: key.to_string(),
        });
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let keys = self.base.list(prefix).await?.into_iter().collect();
        Ok(self.overlay_keys(keys, |key| key.starts_with(prefix)))
    }

    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        let keys = self.base.scan(start, end).await?.into_iter().collect();
        Ok(self.overlay_keys(keys, |key| key >= start && key < end))
    }

    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        let mut staged = self.staged.lock().unwrap();
        for op in ops {
            staged.push(op.clone());
        }
        Ok(())
    }
}

/// User-owned browser capture settings. Capture begins disabled; consent is an
/// explicit state transition performed by the host UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryCapturePolicy {
    pub enabled: bool,
    pub accepted_schemes: BTreeSet<String>,
    pub excluded_origins: BTreeSet<String>,
    pub strip_query: bool,
    pub strip_fragment: bool,
    pub dedupe_window_ms: u64,
    pub segment_size: usize,
    pub retention_traces: usize,
}

impl HistoryCapturePolicy {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            accepted_schemes: ["http".to_string(), "https".to_string()]
                .into_iter()
                .collect(),
            excluded_origins: BTreeSet::new(),
            strip_query: false,
            strip_fragment: true,
            dedupe_window_ms: 1_000,
            segment_size: 64,
            retention_traces: 2_048,
        }
    }

    /// The conservative recommended policy after the user grants browser
    /// history access. Every field remains editable by the host.
    pub fn consented() -> Self {
        Self {
            enabled: true,
            ..Self::disabled()
        }
    }
}

/// One browser visit at the extension boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserVisit {
    pub source: String,
    pub visit_id: Option<String>,
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
    pub referrer_url: Option<String>,
    pub transition: String,
    pub at_ms: u64,
    pub private: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedVisit {
    pub source: String,
    pub visit_id: Option<String>,
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
    pub referrer_url: Option<String>,
    pub transition: CaptureTransition,
    pub at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTransition {
    LinkClick,
    UrlTyped,
    Back,
    Forward,
    Reload,
    Redirect,
    TabSpawn,
    Restore,
    Imported,
    Unknown,
}

impl CaptureTransition {
    fn from_browser(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "link" | "link_click" => Self::LinkClick,
            "typed" | "url_typed" => Self::UrlTyped,
            "back" => Self::Back,
            "forward" => Self::Forward,
            "reload" => Self::Reload,
            "redirect" | "auto_subframe" | "manual_subframe" => Self::Redirect,
            "tab_spawn" | "generated" => Self::TabSpawn,
            "restore" | "reopen" => Self::Restore,
            "imported" => Self::Imported,
            _ => Self::Unknown,
        }
    }

    fn trace(self) -> TraceTransition {
        match self {
            Self::LinkClick => TraceTransition::LinkClick,
            Self::UrlTyped => TraceTransition::UrlTyped,
            Self::Back => TraceTransition::Back,
            Self::Forward => TraceTransition::Forward,
            Self::Reload => TraceTransition::Reload,
            Self::Redirect => TraceTransition::Redirect,
            Self::TabSpawn => TraceTransition::TabSpawn,
            Self::Restore => TraceTransition::Restore,
            Self::Imported => TraceTransition::Imported,
            Self::Unknown => TraceTransition::Unknown,
        }
    }

    fn graph(self) -> NavigationTrigger {
        match self {
            Self::LinkClick => NavigationTrigger::LinkClick,
            Self::UrlTyped => NavigationTrigger::AddressBarEntry,
            Self::Back => NavigationTrigger::Back,
            Self::Forward => NavigationTrigger::Forward,
            Self::Reload => NavigationTrigger::Programmatic,
            Self::Redirect => NavigationTrigger::Redirect,
            Self::TabSpawn => NavigationTrigger::PanePromotion,
            Self::Restore => NavigationTrigger::ReopenSession,
            Self::Imported => NavigationTrigger::ImportedHistory,
            Self::Unknown => NavigationTrigger::Unknown,
        }
    }

    fn access(self) -> AccessTransition {
        match self {
            Self::LinkClick => AccessTransition::LinkClick,
            Self::UrlTyped => AccessTransition::UrlTyped,
            Self::Back => AccessTransition::Back,
            Self::Forward => AccessTransition::Forward,
            Self::Reload => AccessTransition::Reload,
            Self::Redirect => AccessTransition::Redirect,
            Self::TabSpawn => AccessTransition::TabSpawn,
            Self::Restore => AccessTransition::Restore,
            Self::Imported => AccessTransition::Imported,
            Self::Unknown => AccessTransition::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureDropReason {
    Disabled,
    PrivateWindow,
    InvalidAddress,
    UnsupportedScheme,
    ExcludedOrigin,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureOutcome {
    Accepted { node: Uuid },
    Dropped(CaptureDropReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgetMode {
    /// Remove captured history while preserving the addressed object.
    HistoryOnly,
    /// Also remove the object when browser capture originally created it.
    RemoveCapturedObject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserHistoryFacetV1 {
    pub created_by_capture: bool,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
    pub visit_count: u64,
    pub latest_title: Option<String>,
    pub latest_favicon_url: Option<String>,
    pub last_transition: CaptureTransition,
    pub sources: BTreeSet<String>,
}

#[derive(Debug)]
pub enum CaptureError {
    Store(String),
    Host(MereHostError),
    Product(ProductError),
    RejectedAddress(CaptureDropReason),
    InvalidFacet(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "browser memory: {error}"),
            Self::Host(error) => write!(formatter, "{error}"),
            Self::Product(error) => write!(formatter, "{error}"),
            Self::RejectedAddress(reason) => {
                write!(
                    formatter,
                    "browser history address was rejected: {reason:?}"
                )
            }
            Self::InvalidFacet(error) => write!(formatter, "browser history facet: {error}"),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<MereHostError> for CaptureError {
    fn from(value: MereHostError) -> Self {
        Self::Host(value)
    }
}

impl From<ProductError> for CaptureError {
    fn from(value: ProductError) -> Self {
        Self::Product(value)
    }
}

/// Capture state loaded from the same Muniment backend used by the local Mere
/// host. The Eidetic payloads and graph document occupy distinct key spaces.
pub struct BrowserHistoryCapture {
    policy: HistoryCapturePolicy,
    memory: BrowsingMemory,
    seen_visit_ids: HashSet<(String, String)>,
    last_accepted: Option<(String, u64)>,
}

impl BrowserHistoryCapture {
    pub async fn load<B: Backend>(
        store: &mut B,
        policy: HistoryCapturePolicy,
    ) -> Result<Self, CaptureError> {
        bootstrap_browsing_schema(store)
            .await
            .map_err(|error| CaptureError::Store(error.to_string()))?;
        bootstrap_access_record_schema(store)
            .await
            .map_err(|error| CaptureError::Store(error.to_string()))?;
        let memory = BrowsingMemory::load(store, policy.segment_size)
            .await
            .map_err(|error| CaptureError::Store(error.to_string()))?;
        let mut seen_visit_ids = HashSet::new();
        let mut fetcher = NoFetcher;
        for manifest in list_typed::<AccessRecord>(store)
            .await
            .map_err(|error| CaptureError::Store(error.to_string()))?
        {
            if let Some(record) = load_typed::<AccessRecord>(store, &mut fetcher, manifest.id)
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?
                && let Some(source_event_id) = record.source_event_id
            {
                seen_visit_ids.insert((record.capture_source, source_event_id));
            }
        }
        Ok(Self {
            policy,
            memory,
            seen_visit_ids,
            last_accepted: None,
        })
    }

    pub fn policy(&self) -> &HistoryCapturePolicy {
        &self.policy
    }

    pub fn set_policy(&mut self, policy: HistoryCapturePolicy) {
        self.policy = policy;
    }

    pub fn memory(&self) -> &BrowsingMemory {
        &self.memory
    }

    /// Project one service-worker delivery as one durable batch. Every
    /// accepted event is flushed before return, so worker suspension cannot
    /// strand an in-memory partial segment.
    pub async fn ingest_batch<B: CaptureBackend>(
        &mut self,
        host: &mut MereHost<B>,
        store: &mut B,
        visits: impl IntoIterator<Item = BrowserVisit>,
        persona: &str,
        device: &str,
        saved_at_secs: u64,
    ) -> Result<Vec<CaptureOutcome>, CaptureError> {
        let memory_before = self.memory.clone();
        let mut batch = CaptureBatch::new(&*store);
        let staged = async {
            let mut outcomes = Vec::new();
            let mut pending_seen = HashSet::new();
            let mut pending_last = self.last_accepted.clone();
            for visit in visits {
                let normalized =
                    match self.normalize_against(visit, &pending_seen, pending_last.as_ref()) {
                        Ok(visit) => visit,
                        Err(reason) => {
                            outcomes.push(CaptureOutcome::Dropped(reason));
                            continue;
                        }
                    };
                let projected = host.project_browser_visit(&normalized, persona, device)?;
                save_access_record(&mut batch, &projected.record)
                    .await
                    .map_err(|error| CaptureError::Store(error.to_string()))?;
                self.memory.record_traversal(
                    persona,
                    TraceEvent {
                        from: normalized.referrer_url.as_ref().map(|url| PageRef {
                            url: url.clone(),
                            title: None,
                        }),
                        to: PageRef {
                            url: normalized.url.clone(),
                            title: normalized.title.clone(),
                        },
                        transition: normalized.transition.trace(),
                        at_ms: normalized.at_ms,
                        dwell_ms: None,
                        candidates: Vec::new(),
                    },
                );
                if let Some(visit_id) = &normalized.visit_id {
                    pending_seen.insert((normalized.source.clone(), visit_id.clone()));
                }
                pending_last = Some((normalized.url.clone(), normalized.at_ms));
                outcomes.push(CaptureOutcome::Accepted {
                    node: projected.node,
                });
            }
            self.memory
                .flush(&mut batch, saved_at_secs.saturating_mul(1_000))
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?;
            self.memory
                .apply_quota(&mut batch, self.policy.retention_traces)
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?;
            host.persist_through(&batch, saved_at_secs).await?;
            Ok::<_, CaptureError>((outcomes, pending_seen, pending_last))
        }
        .await;
        let (outcomes, pending_seen, pending_last) = match staged {
            Ok(staged) => staged,
            Err(error) => {
                self.memory = memory_before;
                return Err(error);
            }
        };
        if let Err(error) = batch.commit().await {
            self.memory = memory_before;
            return Err(CaptureError::Store(error.to_string()));
        }
        self.seen_visit_ids.extend(pending_seen);
        self.last_accepted = pending_last;
        Ok(outcomes)
    }

    pub async fn forget_url<B: CaptureBackend>(
        &mut self,
        host: &mut MereHost<B>,
        store: &mut B,
        url: &str,
        mode: ForgetMode,
        saved_at_secs: u64,
    ) -> Result<usize, CaptureError> {
        // A newly excluded origin must remain forgettable. Exclusions govern
        // intake, not the user's ability to erase records already stored.
        let mut forget_policy = self.policy.clone();
        forget_policy.excluded_origins.clear();
        let canonical =
            canonical_url(url, &forget_policy).map_err(CaptureError::RejectedAddress)?;
        let memory_before = self.memory.clone();
        let mut batch = CaptureBatch::new(&*store);
        let staged = async {
            let forgotten = self
                .memory
                .forget_url(&mut batch, &canonical)
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?;
            let mut forgotten_source_events = HashSet::new();
            let mut fetcher = NoFetcher;
            for manifest in list_typed::<AccessRecord>(&mut batch)
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?
            {
                let Some(record) =
                    load_typed::<AccessRecord>(&mut batch, &mut fetcher, manifest.id)
                        .await
                        .map_err(|error| CaptureError::Store(error.to_string()))?
                else {
                    continue;
                };
                if record.address != canonical
                    && record.referring_address.as_deref() != Some(canonical.as_str())
                {
                    continue;
                }
                if let Some(source_event_id) = record.source_event_id {
                    forgotten_source_events.insert((record.capture_source, source_event_id));
                }
                delete_manifest(&mut batch, manifest.id)
                    .await
                    .map_err(|error| CaptureError::Store(error.to_string()))?;
            }
            host.forget_browser_history(&canonical, mode)?;
            host.persist_through(&batch, saved_at_secs).await?;
            Ok::<_, CaptureError>((forgotten, forgotten_source_events))
        }
        .await;
        let (forgotten, forgotten_source_events) = match staged {
            Ok(staged) => staged,
            Err(error) => {
                self.memory = memory_before;
                return Err(error);
            }
        };
        if let Err(error) = batch.commit().await {
            self.memory = memory_before;
            return Err(CaptureError::Store(error.to_string()));
        }
        self.seen_visit_ids
            .retain(|key| !forgotten_source_events.contains(key));
        Ok(forgotten)
    }

    #[cfg(test)]
    fn normalize(&self, visit: BrowserVisit) -> Result<NormalizedVisit, CaptureDropReason> {
        self.normalize_against(visit, &HashSet::new(), self.last_accepted.as_ref())
    }

    fn normalize_against(
        &self,
        visit: BrowserVisit,
        pending_seen: &HashSet<(String, String)>,
        pending_last: Option<&(String, u64)>,
    ) -> Result<NormalizedVisit, CaptureDropReason> {
        if !self.policy.enabled {
            return Err(CaptureDropReason::Disabled);
        }
        if visit.private {
            return Err(CaptureDropReason::PrivateWindow);
        }
        let source = visit.source.trim().to_ascii_lowercase();
        if visit.visit_id.as_ref().is_some_and(|visit_id| {
            self.seen_visit_ids
                .contains(&(source.clone(), visit_id.clone()))
                || pending_seen.contains(&(source.clone(), visit_id.clone()))
        }) {
            return Err(CaptureDropReason::Duplicate);
        }
        let url = canonical_url(&visit.url, &self.policy)?;
        if pending_last.is_some_and(|(last_url, at_ms)| {
            last_url == &url && visit.at_ms.abs_diff(*at_ms) <= self.policy.dedupe_window_ms
        }) {
            return Err(CaptureDropReason::Duplicate);
        }
        let referrer_url = visit
            .referrer_url
            .as_deref()
            .and_then(|url| canonical_url(url, &self.policy).ok());
        Ok(NormalizedVisit {
            source,
            visit_id: visit.visit_id,
            url,
            title: clean_optional(visit.title),
            favicon_url: clean_optional(visit.favicon_url),
            referrer_url,
            transition: CaptureTransition::from_browser(&visit.transition),
            at_ms: visit.at_ms,
        })
    }
}

fn canonical_url(
    address: &str,
    policy: &HistoryCapturePolicy,
) -> Result<String, CaptureDropReason> {
    let mut url = Url::parse(address).map_err(|_| CaptureDropReason::InvalidAddress)?;
    if !policy.accepted_schemes.contains(url.scheme()) {
        return Err(CaptureDropReason::UnsupportedScheme);
    }
    let origin = url.origin().ascii_serialization();
    if policy.excluded_origins.contains(&origin) {
        return Err(CaptureDropReason::ExcludedOrigin);
    }
    if policy.strip_query {
        url.set_query(None);
    }
    if policy.strip_fragment {
        url.set_fragment(None);
    }
    Ok(url.into())
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl<B: Backend> MereHost<B> {
    fn project_browser_visit(
        &mut self,
        visit: &NormalizedVisit,
        persona: &str,
        device: &str,
    ) -> Result<ProjectedVisit, CaptureError> {
        let created_by_capture = self.graph().get_node_by_url(&visit.url).is_none();
        let node = self.create_address(&visit.url, visit.title.as_deref().unwrap_or_default())?;
        let key = self
            .graph()
            .get_node_key_by_id(node)
            .ok_or_else(|| CaptureError::InvalidFacet("created node is missing".to_string()))?;
        let referring_container_id = if let Some(referrer) = &visit.referrer_url
            && referrer != &visit.url
        {
            Some(self.create_address(referrer, "")?)
        } else {
            None
        };
        let source_identity = format!(
            "{}\u{0}{}\u{0}{}\u{0}{}",
            visit.source,
            visit.visit_id.as_deref().unwrap_or_default(),
            visit.url,
            visit.at_ms
        );
        let observation = AccessObservation {
            record_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, source_identity.as_bytes()),
            action: if visit.transition == CaptureTransition::Imported {
                AccessAction::Import
            } else {
                AccessAction::Open
            },
            persona: persona.to_string(),
            device: device.to_string(),
            application: format!("browser.extension.{}", visit.source),
            handler: format!("{BROWSER_HISTORY_HANDLER_PREFIX}{}", visit.source),
            at_ms: visit.at_ms,
            dwell_ms: None,
            referring_container_id,
            referring_address: visit.referrer_url.clone(),
            transition: visit.transition.access(),
            capture_source: visit.source.clone(),
            source_event_id: visit.visit_id.clone(),
            privacy: PrivacyClass::LocalOnly,
        };
        let (record, inserted) = self
            .mutate_product_graph(|graph| record_observation(graph, key, &observation))
            .map_err(MereHostError::from)?;
        if !inserted {
            return Ok(ProjectedVisit { node, record });
        }

        let mut facet = self
            .facet_value(&visit.url, BROWSER_HISTORY_FACET)
            .map(|value| {
                serde_json::from_value::<BrowserHistoryFacetV1>(value.clone())
                    .map_err(|error| CaptureError::InvalidFacet(error.to_string()))
            })
            .transpose()?
            .unwrap_or_else(|| BrowserHistoryFacetV1 {
                created_by_capture,
                first_seen_ms: visit.at_ms,
                last_seen_ms: visit.at_ms,
                visit_count: 0,
                latest_title: None,
                latest_favicon_url: None,
                last_transition: visit.transition,
                sources: BTreeSet::new(),
            });
        facet.first_seen_ms = facet.first_seen_ms.min(visit.at_ms);
        facet.last_seen_ms = facet.last_seen_ms.max(visit.at_ms);
        facet.visit_count = facet.visit_count.saturating_add(1);
        if visit.title.is_some() {
            facet.latest_title = visit.title.clone();
        }
        if visit.favicon_url.is_some() {
            facet.latest_favicon_url = visit.favicon_url.clone();
        }
        facet.last_transition = visit.transition;
        facet.sources.insert(visit.source.clone());
        self.set_facet(
            key,
            BROWSER_HISTORY_FACET,
            serde_json::to_value(facet)
                .map_err(|error| CaptureError::InvalidFacet(error.to_string()))?,
        )?;

        if let Some(from) = referring_container_id {
            let from_key = self.graph().get_node_key_by_id(from).ok_or_else(|| {
                CaptureError::InvalidFacet("referrer node is missing".to_string())
            })?;
            self.mutate_product_graph(|graph| {
                apply_graph_delta(
                    graph,
                    GraphDelta::AppendTraversal {
                        from: from_key,
                        to: key,
                        trigger: visit.transition.graph(),
                        timestamp_ms: Some(visit.at_ms),
                    },
                );
            });
        }
        Ok(ProjectedVisit { node, record })
    }

    fn forget_browser_history(
        &mut self,
        address: &str,
        mode: ForgetMode,
    ) -> Result<(), CaptureError> {
        let Some((key, node)) = self.graph().get_node_by_url(address) else {
            return Ok(());
        };
        let node_id = node.id;
        let capture_created = self
            .facet_value(address, BROWSER_HISTORY_FACET)
            .and_then(|value| serde_json::from_value::<BrowserHistoryFacetV1>(value.clone()).ok())
            .is_some_and(|facet| facet.created_by_capture);
        if mode == ForgetMode::RemoveCapturedObject && capture_created {
            self.mutate_product_graph(|graph| {
                apply_graph_delta(graph, GraphDelta::RemoveNode { key });
            });
            return Ok(());
        }

        let traversal_pairs = self
            .graph()
            .relations()
            .filter(|relation| {
                relation.kind == RelationKind::Traversal
                    && (relation.from == key || relation.to == key)
            })
            .map(|relation| (relation.from, relation.to))
            .collect::<BTreeSet<_>>();
        self.mutate_product_graph(|graph| {
            if let Some(value) = graph
                .facets()
                .get(&node_id, &FacetId::new(ACCESS_HISTORY_FACET))
                .cloned()
                && let Ok(mut history) = serde_json::from_value::<AccessHistory>(value)
            {
                history
                    .records
                    .retain(|record| !record.handler.starts_with(BROWSER_HISTORY_HANDLER_PREFIX));
                if history.records.is_empty() {
                    apply_graph_delta(
                        graph,
                        GraphDelta::RemoveNodeFacet {
                            key,
                            facet: ACCESS_HISTORY_FACET.to_string(),
                        },
                    );
                } else {
                    apply_graph_delta(
                        graph,
                        GraphDelta::SetNodeFacet {
                            key,
                            facet: ACCESS_HISTORY_FACET.to_string(),
                            value: serde_json::to_value(history)
                                .expect("AccessHistory always serializes"),
                        },
                    );
                }
            }
            apply_graph_delta(
                graph,
                GraphDelta::RemoveNodeFacet {
                    key,
                    facet: BROWSER_HISTORY_FACET.to_string(),
                },
            );
            for (from, to) in traversal_pairs {
                apply_graph_delta(
                    graph,
                    GraphDelta::RetractRelations {
                        from,
                        to,
                        selector: RelationSelector::Family(EdgeFamily::Traversal),
                    },
                );
            }
        });
        Ok(())
    }
}

struct ProjectedVisit {
    node: Uuid,
    record: AccessRecord,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use crate::access::AccessContext;
    use crate::access::{AccessRecordFilter, query_access_records};
    use muniment::MemoryBackend;

    use super::*;
    use crate::mere_host::{
        FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, HOST_SLOT, SelectedPersonaRef,
    };

    const PERSONA: &str = "personae://persona/capture";
    const DEVICE: &str = "personae://device/browser";

    fn selected() -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            profile: "profile:graphshell-capture".to_string(),
        }
    }

    fn visit(id: &str, url: &str, referrer_url: Option<&str>, at_ms: u64) -> BrowserVisit {
        BrowserVisit {
            source: "Firefox".to_string(),
            visit_id: Some(id.to_string()),
            url: url.to_string(),
            title: Some(format!("  {id} title  ")),
            favicon_url: Some("https://assets.example/favicon.ico".to_string()),
            referrer_url: referrer_url.map(str::to_string),
            transition: "link".to_string(),
            at_ms,
            private: false,
        }
    }

    #[derive(Clone)]
    struct RejectingApplyBackend {
        inner: MemoryBackend,
        reject: Arc<AtomicBool>,
    }

    impl RejectingApplyBackend {
        fn new() -> Self {
            Self {
                inner: MemoryBackend::new(),
                reject: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    #[async_trait]
    impl Backend for RejectingApplyBackend {
        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
            self.inner.get(key).await
        }

        async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
            self.inner.put(key, bytes).await
        }

        async fn delete(&self, key: &str) -> Result<(), StoreError> {
            self.inner.delete(key).await
        }

        async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
            self.inner.list(prefix).await
        }

        async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
            self.inner.scan(start, end).await
        }

        async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
            if self.reject.load(Ordering::SeqCst) {
                return Err(StoreError::Backend(
                    "capture transaction rejected".to_string(),
                ));
            }
            self.inner.apply(ops).await
        }
    }

    #[test]
    fn policy_rejects_unconsented_private_internal_excluded_and_duplicate_visits() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut store = backend.clone();
            let disabled =
                BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::disabled())
                    .await
                    .unwrap();
            assert_eq!(
                disabled
                    .normalize(visit("1", "https://example.net/a", None, 1))
                    .unwrap_err(),
                CaptureDropReason::Disabled
            );

            let mut policy = HistoryCapturePolicy::consented();
            policy
                .excluded_origins
                .insert("https://private.example".to_string());
            policy.strip_query = true;
            let capture = BrowserHistoryCapture::load(&mut store, policy)
                .await
                .unwrap();

            let mut private = visit("2", "https://example.net/a", None, 2);
            private.private = true;
            assert_eq!(
                capture.normalize(private).unwrap_err(),
                CaptureDropReason::PrivateWindow
            );
            assert_eq!(
                capture
                    .normalize(visit("3", "about:config", None, 3))
                    .unwrap_err(),
                CaptureDropReason::UnsupportedScheme
            );
            assert_eq!(
                capture
                    .normalize(visit("4", "https://private.example/a", None, 4))
                    .unwrap_err(),
                CaptureDropReason::ExcludedOrigin
            );
            let normalized = capture
                .normalize(visit("5", "https://example.net/a?secret=1#part", None, 5))
                .unwrap();
            assert_eq!(normalized.url, "https://example.net/a");
        });
    }

    #[test]
    fn consented_batch_projects_access_traversal_memory_reopen_and_forget() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut memory_store = backend.clone();
            let mut host = MereHost::fixture(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
            )
            .unwrap();
            let mut capture =
                BrowserHistoryCapture::load(&mut memory_store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            let from = "https://capture.example/from";
            let to = "https://capture.example/to";
            let mut private = visit("private", "https://capture.example/private", None, 3_000);
            private.private = true;
            let outcomes = capture
                .ingest_batch(
                    &mut host,
                    &mut memory_store,
                    [
                        visit("from", from, None, 1_000),
                        visit("to", to, Some(from), 2_000),
                        visit("to", to, Some(from), 2_000),
                        private,
                    ],
                    PERSONA,
                    DEVICE,
                    2,
                )
                .await
                .unwrap();
            assert_eq!(
                outcomes
                    .iter()
                    .filter(|outcome| matches!(outcome, CaptureOutcome::Accepted { .. }))
                    .count(),
                2
            );
            assert_eq!(
                outcomes,
                vec![
                    outcomes[0].clone(),
                    outcomes[1].clone(),
                    CaptureOutcome::Dropped(CaptureDropReason::Duplicate),
                    CaptureOutcome::Dropped(CaptureDropReason::PrivateWindow),
                ]
            );
            let access = host.access_history_for(to).unwrap();
            assert_eq!(access.records.len(), 1);
            assert_eq!(access.records[0].persona, PERSONA);
            assert_eq!(access.records[0].device, DEVICE);
            assert_eq!(access.records[0].handler, "browser.history/firefox");
            let facet: BrowserHistoryFacetV1 = serde_json::from_value(
                host.facet_value(to, BROWSER_HISTORY_FACET).unwrap().clone(),
            )
            .unwrap();
            assert!(facet.created_by_capture);
            assert_eq!(facet.visit_count, 1);
            assert_eq!(facet.latest_title.as_deref(), Some("to title"));
            assert!(host.graph().relations().any(|relation| {
                relation.kind == RelationKind::Traversal
                    && host.graph().get_node(relation.from).unwrap().url() == from
                    && host.graph().get_node(relation.to).unwrap().url() == to
            }));
            assert_eq!(capture.memory().recent_corridor(8).len(), 2);

            let reopened = MereHost::open(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
                AccessContext {
                    persona: PERSONA.to_string(),
                    device: DEVICE.to_string(),
                    at_ms: 4_000,
                },
            )
            .await
            .unwrap();
            assert!(reopened.graph().get_node_by_url(to).is_some());
            let reloaded_memory = BrowsingMemory::load(&mut memory_store, 64).await.unwrap();
            assert_eq!(reloaded_memory.recent_corridor(8).len(), 2);
            let manifests = list_typed::<AccessRecord>(&mut memory_store).await.unwrap();
            assert_eq!(manifests.len(), 2);
            assert!(
                manifests
                    .iter()
                    .all(|manifest| manifest.privacy == PrivacyClass::LocalOnly)
            );
            let filtered = query_access_records(
                &mut memory_store,
                &AccessRecordFilter {
                    start_ms: Some(1_500),
                    end_ms: Some(2_500),
                    persona: Some(PERSONA.to_string()),
                    device: Some(DEVICE.to_string()),
                },
            )
            .await
            .unwrap();
            assert_eq!(filtered.len(), 1);
            assert_eq!(filtered[0].address, to);
            assert!(
                query_access_records(
                    &mut memory_store,
                    &AccessRecordFilter {
                        persona: Some("persona:someone-else".to_string()),
                        ..AccessRecordFilter::default()
                    },
                )
                .await
                .unwrap()
                .is_empty()
            );
            let mut reloaded_capture =
                BrowserHistoryCapture::load(&mut memory_store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            assert_eq!(
                reloaded_capture
                    .ingest_batch(
                        &mut host,
                        &mut memory_store,
                        [visit("to", to, Some(from), 2_000)],
                        PERSONA,
                        DEVICE,
                        3,
                    )
                    .await
                    .unwrap(),
                vec![CaptureOutcome::Dropped(CaptureDropReason::Duplicate)]
            );

            let mut excluded_after_capture = capture.policy().clone();
            excluded_after_capture
                .excluded_origins
                .insert("https://capture.example".to_string());
            capture.set_policy(excluded_after_capture);
            capture
                .forget_url(
                    &mut host,
                    &mut memory_store,
                    to,
                    ForgetMode::RemoveCapturedObject,
                    3,
                )
                .await
                .unwrap();
            assert!(host.graph().get_node_by_url(to).is_none());
            assert!(
                BrowsingMemory::load(&mut memory_store, 64)
                    .await
                    .unwrap()
                    .recent_corridor(8)
                    .is_empty()
            );
            assert_eq!(
                list_typed::<AccessRecord>(&mut memory_store)
                    .await
                    .unwrap()
                    .len(),
                1,
                "forget deletes access authority records that mention the address"
            );
            assert!(
                host.graph().get_node_by_url(FIXTURE_WEB_ADDRESS).is_some(),
                "forget remains scoped to the captured address"
            );
            capture.set_policy(HistoryCapturePolicy::consented());
            assert_eq!(
                host.access_history_for(FIXTURE_WEB_ADDRESS)
                    .unwrap()
                    .records
                    .len(),
                2
            );
            let recaptured = capture
                .ingest_batch(
                    &mut host,
                    &mut memory_store,
                    [visit("to", to, None, 4_000)],
                    PERSONA,
                    DEVICE,
                    4,
                )
                .await
                .unwrap();
            assert!(
                matches!(recaptured.as_slice(), [CaptureOutcome::Accepted { .. }]),
                "forget also clears stable source-event dedupe state"
            );
        });
    }

    #[test]
    fn capture_batch_is_atomic_and_the_same_delivery_retries_cleanly() {
        pollster::block_on(async {
            let backend = RejectingApplyBackend::new();
            let mut store = backend.clone();
            let mut host = MereHost::fixture(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
            )
            .unwrap();
            let mut capture =
                BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            let address = "https://capture.example/atomic";
            backend.reject.store(true, Ordering::SeqCst);
            assert!(
                capture
                    .ingest_batch(
                        &mut host,
                        &mut store,
                        [visit("atomic", address, None, 5_000)],
                        PERSONA,
                        DEVICE,
                        5,
                    )
                    .await
                    .is_err()
            );
            assert!(backend.get(HOST_SLOT).await.unwrap().is_none());
            assert!(
                list_typed::<AccessRecord>(&mut store)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert!(
                BrowsingMemory::load(&mut store, 64)
                    .await
                    .unwrap()
                    .recent_corridor(8)
                    .is_empty()
            );

            backend.reject.store(false, Ordering::SeqCst);
            assert!(matches!(
                capture
                    .ingest_batch(
                        &mut host,
                        &mut store,
                        [visit("atomic", address, None, 5_000)],
                        PERSONA,
                        DEVICE,
                        5,
                    )
                    .await
                    .unwrap()
                    .as_slice(),
                [CaptureOutcome::Accepted { .. }]
            ));
            let reopened = MereHost::open(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
                AccessContext {
                    persona: PERSONA.to_string(),
                    device: DEVICE.to_string(),
                    at_ms: 6_000,
                },
            )
            .await
            .unwrap();
            assert!(reopened.graph().get_node_by_url(address).is_some());
            assert_eq!(
                list_typed::<AccessRecord>(&mut store).await.unwrap().len(),
                1
            );
            assert_eq!(
                BrowsingMemory::load(&mut store, 64)
                    .await
                    .unwrap()
                    .recent_corridor(8)
                    .len(),
                1
            );
        });
    }
}
