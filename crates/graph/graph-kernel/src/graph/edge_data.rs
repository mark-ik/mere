/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-family edge runtime data structs and semantic IRI mapping.
//! Extracted from `edge_taxonomy.rs` per the 600-LOC ceiling.

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use rkyv::{Archive, Deserialize, Serialize};

use super::edge_taxonomy::{
    ArrangementSubKind, ContainmentSubKind, ImportedSubKind, NavigationTrigger, ProvenanceSubKind,
    RelationDurability, SemanticSubKind,
};

/// A temporal traversal event recorded on an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct Traversal {
    pub timestamp_ms: u64,
    pub trigger: NavigationTrigger,
}

impl Traversal {
    pub fn now(trigger: NavigationTrigger) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            timestamp_ms,
            trigger,
        }
    }
}

/// Durable traversal aggregates retained even when rolling-window records are evicted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct EdgeMetrics {
    pub total_navigations: u64,
    pub forward_navigations: u64,
    pub backward_navigations: u64,
    pub last_navigated_at: Option<u64>,
}

impl EdgeMetrics {
    fn new() -> Self {
        Self {
            total_navigations: 0,
            forward_navigations: 0,
            backward_navigations: 0,
            last_navigated_at: None,
        }
    }

    fn record(&mut self, traversal: Traversal) {
        self.total_navigations = self.total_navigations.saturating_add(1);
        if traversal.trigger.contributes_to_forward_count() {
            self.forward_navigations = self.forward_navigations.saturating_add(1);
        } else {
            self.backward_navigations = self.backward_navigations.saturating_add(1);
        }
        self.last_navigated_at = Some(traversal.timestamp_ms);
    }
}

impl Default for EdgeMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct SemanticData {
    pub sub_kinds: BTreeSet<SemanticSubKind>,
    pub label: Option<String>,
    /// Open predicate IRI (the statements-over-schema substrate, linked-data
    /// plan Phase 0). `None` when the edge's meaning is fully carried by
    /// `sub_kinds`; a canonical IRI ([`predicate_iri`]) for a recognized
    /// predicate, or a raw IRI for an unrecognized one. `sub_kinds` stays
    /// authoritative for behaviour; `predicate` carries identity + round-trip.
    pub predicate: Option<String>,
}

/// Base IRI for Mere's canonical relation vocabulary. Recognized
/// [`SemanticSubKind`]s map to `{REL_VOCAB}{slug}` (the "small Mere vocabulary
/// of canonical IRIs"); the JSON-LD export layer maps these onto standard
/// vocabularies (CiTO, schema.org, …) through its `@context`. Provisional base.
pub const REL_VOCAB: &str = "https://mere.computer/ns/rel#";

/// The canonical relation IRI for a recognized semantic sub-kind. Inverse of
/// [`sub_kind_from_iri`].
pub fn predicate_iri(sub_kind: SemanticSubKind) -> &'static str {
    match sub_kind {
        SemanticSubKind::Hyperlink => "https://mere.computer/ns/rel#hyperlink",
        SemanticSubKind::UserGrouped => "https://mere.computer/ns/rel#user-grouped",
        SemanticSubKind::AgentDerived => "https://mere.computer/ns/rel#agent-derived",
        SemanticSubKind::Cites => "https://mere.computer/ns/rel#cites",
        SemanticSubKind::Quotes => "https://mere.computer/ns/rel#quotes",
        SemanticSubKind::Summarizes => "https://mere.computer/ns/rel#summarizes",
        SemanticSubKind::Elaborates => "https://mere.computer/ns/rel#elaborates",
        SemanticSubKind::ExampleOf => "https://mere.computer/ns/rel#example-of",
        SemanticSubKind::Supports => "https://mere.computer/ns/rel#supports",
        SemanticSubKind::Contradicts => "https://mere.computer/ns/rel#contradicts",
        SemanticSubKind::Questions => "https://mere.computer/ns/rel#questions",
        SemanticSubKind::SameEntityAs => "https://mere.computer/ns/rel#same-entity-as",
        SemanticSubKind::DuplicateOf => "https://mere.computer/ns/rel#duplicate-of",
        SemanticSubKind::CanonicalMirrorOf => "https://mere.computer/ns/rel#canonical-mirror-of",
        SemanticSubKind::DependsOn => "https://mere.computer/ns/rel#depends-on",
        SemanticSubKind::Blocks => "https://mere.computer/ns/rel#blocks",
        SemanticSubKind::NextStep => "https://mere.computer/ns/rel#next-step",
    }
}

/// The recognized semantic sub-kind for a canonical relation IRI, if any.
/// Inverse of [`predicate_iri`]; `None` for an unrecognized (raw) IRI, which is
/// stored as an open predicate without a sub-kind.
pub fn sub_kind_from_iri(iri: &str) -> Option<SemanticSubKind> {
    use strum::IntoEnumIterator;
    SemanticSubKind::iter().find(|&sub_kind| predicate_iri(sub_kind) == iri)
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct TraversalData {
    pub traversals: Vec<Traversal>,
    pub metrics: EdgeMetrics,
}

impl TraversalData {
    pub(crate) fn push(&mut self, traversal: Traversal) {
        self.metrics.record(traversal);
        self.traversals.push(traversal);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ArrangementData {
    pub sub_kinds: BTreeSet<ArrangementSubKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ContainmentData {
    pub sub_kinds: BTreeSet<ContainmentSubKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ImportedData {
    pub sub_kinds: BTreeSet<ImportedSubKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ProvenanceData {
    pub sub_kinds: BTreeSet<ProvenanceSubKind>,
}

impl ContainmentData {
    pub(crate) fn insert(&mut self, sub_kind: ContainmentSubKind) -> bool {
        self.sub_kinds.insert(sub_kind)
    }

    pub(crate) fn remove(&mut self, sub_kind: ContainmentSubKind) -> bool {
        self.sub_kinds.remove(&sub_kind)
    }

    pub(crate) fn contains(&self, sub_kind: ContainmentSubKind) -> bool {
        self.sub_kinds.contains(&sub_kind)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sub_kinds.is_empty()
    }
}

impl ArrangementData {
    pub(crate) fn insert(&mut self, sub_kind: ArrangementSubKind) -> bool {
        self.sub_kinds.insert(sub_kind)
    }

    pub(crate) fn remove(&mut self, sub_kind: ArrangementSubKind) -> bool {
        self.sub_kinds.remove(&sub_kind)
    }

    pub(crate) fn contains(&self, sub_kind: ArrangementSubKind) -> bool {
        self.sub_kinds.contains(&sub_kind)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sub_kinds.is_empty()
    }

    pub(crate) fn has_durable_relation(&self) -> bool {
        self.sub_kinds
            .iter()
            .copied()
            .any(|sub_kind| sub_kind.durability() == RelationDurability::Durable)
    }

    pub(crate) fn has_session_relation(&self) -> bool {
        self.sub_kinds
            .iter()
            .copied()
            .any(|sub_kind| sub_kind.durability() == RelationDurability::Session)
    }
}
