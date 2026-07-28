// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-family edge runtime data structs and semantic IRI mapping.
//! Extracted from `edge_taxonomy.rs` per the 600-LOC ceiling.

use std::collections::BTreeSet;

use rkyv::{Archive, Deserialize, Serialize};

use crate::types::{GraphScope, mint_local_statement_id};

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
        let timestamp_ms = crate::time::unix_epoch_millis();
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

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct SemanticStatement {
    pub statement_id: String,
    pub predicate: String,
    pub recognized_sub_kind: Option<SemanticSubKind>,
    pub label: Option<String>,
    pub graph_scope: GraphScope,
    pub provenance_iri: Option<String>,
    pub asserted_at_ms: Option<u64>,
}

impl SemanticStatement {
    pub fn new(
        predicate: String,
        recognized_sub_kind: Option<SemanticSubKind>,
        label: Option<String>,
        graph_scope: GraphScope,
        provenance_iri: Option<String>,
        asserted_at_ms: Option<u64>,
    ) -> Self {
        Self {
            statement_id: mint_local_statement_id(),
            predicate,
            recognized_sub_kind,
            label,
            graph_scope,
            provenance_iri,
            asserted_at_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct SemanticData {
    /// Compatibility aggregate: kept during the bucket migration so existing
    /// readers can keep asking the pair-local edge what semantic kinds it
    /// carries. The statement list below is the durable truth.
    pub sub_kinds: BTreeSet<SemanticSubKind>,
    /// Compatibility aggregate over the statement labels.
    pub label: Option<String>,
    /// Open predicate IRI (the statements-over-schema substrate, linked-data
    /// plan Phase 0). `None` when the edge's meaning is fully carried by
    /// `sub_kinds`; a canonical IRI ([`predicate_iri`]) for a recognized
    /// predicate, or a raw IRI for an unrecognized one. `sub_kinds` stays
    /// authoritative for behaviour; `predicate` carries identity + round-trip.
    /// During the statement-bucket migration this is a compatibility view over
    /// the statement predicates, not the durable truth.
    pub predicate: Option<String>,
    /// The durable semantic facts carried between one node pair.
    pub statements: Vec<SemanticStatement>,
}

/// A statement to assert: everything a [`SemanticStatement`] carries except
/// its id, which the kernel mints (device-safe) at assert time. The
/// statement-aware write API's input — the petgraph-RDF plan's Phase 1
/// requirement that interactive writes can carry per-statement metadata, not
/// just the legacy edge-wide predicate stamp.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticStatementSpec {
    pub predicate: String,
    pub recognized_sub_kind: Option<SemanticSubKind>,
    pub label: Option<String>,
    pub graph_scope: GraphScope,
    pub provenance_iri: Option<String>,
    pub asserted_at_ms: Option<u64>,
}

/// What [`SemanticData::assert_statement`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementAssert {
    /// The asserted fact's handle — freshly minted, or the existing
    /// statement's id when content-dedup matched.
    pub statement_id: String,
    /// Whether anything changed (a new statement, or metadata updated on the
    /// deduped one). `false` = the exact statement already existed.
    pub changed: bool,
}

impl SemanticData {
    pub fn statements(&self) -> &[SemanticStatement] {
        &self.statements
    }

    /// Statement-aware assert: content-dedup on
    /// `(recognized_sub_kind, predicate, graph_scope)` (same key as
    /// [`insert_statement`](Self::insert_statement)), updating the deduped
    /// statement's metadata in place; a miss mints a device-safe id and
    /// appends. Always returns the fact handle.
    pub fn assert_statement(&mut self, spec: SemanticStatementSpec) -> StatementAssert {
        if let Some(existing) = self.statements.iter_mut().find(|statement| {
            statement.recognized_sub_kind == spec.recognized_sub_kind
                && statement.predicate == spec.predicate
                && statement.graph_scope == spec.graph_scope
        }) {
            let changed = existing.label != spec.label
                || existing.provenance_iri != spec.provenance_iri
                || existing.asserted_at_ms != spec.asserted_at_ms;
            let statement_id = existing.statement_id.clone();
            if changed {
                existing.label = spec.label;
                existing.provenance_iri = spec.provenance_iri;
                existing.asserted_at_ms = spec.asserted_at_ms;
                self.rebuild_compat();
            }
            return StatementAssert {
                statement_id,
                changed,
            };
        }
        let statement = SemanticStatement::new(
            spec.predicate,
            spec.recognized_sub_kind,
            spec.label,
            spec.graph_scope,
            spec.provenance_iri,
            spec.asserted_at_ms,
        );
        let statement_id = statement.statement_id.clone();
        self.statements.push(statement);
        self.rebuild_compat();
        StatementAssert {
            statement_id,
            changed: true,
        }
    }

    /// Precise retract by fact handle. Bucket-local linear scan by design
    /// (buckets stay small; a global id index waits for evidence). Returns
    /// whether a statement was removed.
    pub fn retract_statement(&mut self, statement_id: &str) -> bool {
        let before = self.statements.len();
        self.statements
            .retain(|statement| statement.statement_id != statement_id);
        if self.statements.len() == before {
            return false;
        }
        self.rebuild_compat();
        true
    }

    pub fn insert_statement(
        &mut self,
        recognized_sub_kind: Option<SemanticSubKind>,
        predicate: String,
        label: Option<String>,
        graph_scope: GraphScope,
        provenance_iri: Option<String>,
        asserted_at_ms: Option<u64>,
    ) -> bool {
        if let Some(existing) = self.statements.iter_mut().find(|statement| {
            statement.recognized_sub_kind == recognized_sub_kind
                && statement.predicate == predicate
                && statement.graph_scope == graph_scope
        }) {
            if existing.label != label
                || existing.provenance_iri != provenance_iri
                || existing.asserted_at_ms != asserted_at_ms
            {
                existing.label = label;
                existing.provenance_iri = provenance_iri;
                existing.asserted_at_ms = asserted_at_ms;
                self.rebuild_compat();
                return true;
            }
            return false;
        }

        self.statements.push(SemanticStatement::new(
            predicate,
            recognized_sub_kind,
            label,
            graph_scope,
            provenance_iri,
            asserted_at_ms,
        ));
        self.rebuild_compat();
        true
    }

    pub fn push_persisted_statement(&mut self, statement: SemanticStatement) -> bool {
        if self
            .statements
            .iter()
            .any(|existing| existing.statement_id == statement.statement_id)
        {
            return false;
        }
        self.statements.push(statement);
        self.rebuild_compat();
        true
    }

    pub fn remove_statements_with_sub_kind(&mut self, sub_kind: SemanticSubKind) -> bool {
        let before = self.statements.len();
        self.statements
            .retain(|statement| statement.recognized_sub_kind != Some(sub_kind));
        if self.statements.len() == before {
            return false;
        }
        self.rebuild_compat();
        true
    }

    pub fn set_statement_predicate(&mut self, predicate: Option<String>) {
        match predicate {
            Some(predicate) => {
                if self.statements.is_empty() {
                    self.statements.push(SemanticStatement::new(
                        predicate,
                        None,
                        self.label.clone(),
                        GraphScope::Default,
                        None,
                        None,
                    ));
                } else {
                    for statement in &mut self.statements {
                        statement.predicate = predicate.clone();
                    }
                }
            }
            None => {
                self.statements
                    .retain(|statement| statement.recognized_sub_kind.is_some());
                for statement in &mut self.statements {
                    if let Some(sub_kind) = statement.recognized_sub_kind {
                        statement.predicate = predicate_iri(sub_kind).to_string();
                    }
                }
            }
        }
        self.rebuild_compat();
    }

    fn rebuild_compat(&mut self) {
        self.sub_kinds = self
            .statements
            .iter()
            .filter_map(|statement| statement.recognized_sub_kind)
            .collect();

        let mut labels = self
            .statements
            .iter()
            .filter_map(|statement| statement.label.clone())
            .collect::<Vec<_>>();
        labels.sort();
        labels.dedup();
        self.label = match labels.len() {
            0 => None,
            1 => labels.into_iter().next(),
            _ => self
                .statements
                .iter()
                .rev()
                .find_map(|statement| statement.label.clone()),
        };

        let mut predicates = self
            .statements
            .iter()
            .map(|statement| statement.predicate.clone())
            .collect::<Vec<_>>();
        predicates.sort();
        predicates.dedup();
        self.predicate = if predicates.len() == 1 {
            predicates.into_iter().next()
        } else {
            None
        };
    }
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

/// Every recognized semantic sub-kind, in declaration order. The kernel owns the
/// enumeration of its recognized relation vocabulary; downstream projection code
/// (e.g. linked-data's standard-vocabulary alignment) maps over it without
/// re-deriving the list or pulling in `strum`.
pub fn all_semantic_sub_kinds() -> impl Iterator<Item = SemanticSubKind> {
    use strum::IntoEnumIterator;
    SemanticSubKind::iter()
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
