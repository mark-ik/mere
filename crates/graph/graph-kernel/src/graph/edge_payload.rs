// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `EdgePayload` — the structural + temporal payload carried on every
//! graph edge. Composes the taxonomy types from
//! [`super::edge_taxonomy`] into a single payload struct whose
//! **typed sidecars are authoritative**: `EdgeFamily` is derived from
//! which sidecars are populated, not from a parallel index set.
//!
//! Per the 2026-05-11 relation-taxonomy plan stage 4: the legacy
//! `kinds: BTreeSet<EdgeKind>` cache and `families: BTreeSet<EdgeFamily>`
//! field are gone. [`Self::families`] returns a freshly-computed set
//! over the sidecars on each call; reads go through [`Self::has_relation`]
//! with a [`RelationSelector`] discriminant.

use std::collections::BTreeSet;

use rkyv::{Archive, Deserialize, Serialize};

use crate::types::GraphScope;

use super::edge_data::{
    ArrangementData, ContainmentData, EdgeMetrics, ImportedData, ProvenanceData, SemanticData,
    SemanticStatement, SemanticStatementSpec, StatementAssert, Traversal, TraversalData,
    predicate_iri,
};
use super::edge_taxonomy::{
    ArrangementSubKind, EdgeAssertion, EdgeFamily, RelationSelector, SemanticSubKind,
};

/// Edge semantics payload: structural assertions + temporal traversal events.
#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct EdgePayload {
    pub semantic: Option<SemanticData>,
    pub traversal: Option<TraversalData>,
    pub arrangement: Option<ArrangementData>,
    pub containment: Option<ContainmentData>,
    pub imported: Option<ImportedData>,
    pub provenance: Option<ProvenanceData>,
}

impl EdgePayload {
    pub fn new() -> Self {
        Self {
            semantic: None,
            traversal: None,
            arrangement: None,
            containment: None,
            imported: None,
            provenance: None,
        }
    }

    fn insert_semantic_relation(
        &mut self,
        sub_kind: SemanticSubKind,
        label: Option<String>,
    ) -> bool {
        self.insert_semantic_relation_in_scope(sub_kind, label, GraphScope::Default)
    }

    fn insert_semantic_relation_in_scope(
        &mut self,
        sub_kind: SemanticSubKind,
        label: Option<String>,
        graph_scope: GraphScope,
    ) -> bool {
        let data = self.semantic.get_or_insert_with(SemanticData::default);
        data.insert_statement(
            Some(sub_kind),
            predicate_iri(sub_kind).to_string(),
            label,
            graph_scope,
            None,
            None,
        )
    }

    fn remove_semantic_relation(&mut self, sub_kind: SemanticSubKind) -> bool {
        let Some(data) = self.semantic.as_mut() else {
            return false;
        };
        if !data.remove_statements_with_sub_kind(sub_kind) {
            return false;
        }
        if data.statements().is_empty() {
            self.semantic = None;
        }
        true
    }

    /// Write-side relation assertion. Returns `true` when the payload
    /// changed (new sub-kind inserted, or label refreshed).
    pub fn assert_relation(&mut self, assertion: EdgeAssertion) -> bool {
        match assertion {
            EdgeAssertion::Semantic {
                sub_kind, label, ..
            } => self.insert_semantic_relation(sub_kind, label),
            EdgeAssertion::Containment { sub_kind } => self
                .containment
                .get_or_insert_with(ContainmentData::default)
                .insert(sub_kind),
            EdgeAssertion::Arrangement { sub_kind } => self
                .arrangement
                .get_or_insert_with(ArrangementData::default)
                .insert(sub_kind),
            EdgeAssertion::Imported { sub_kind } => self
                .imported
                .get_or_insert_with(ImportedData::default)
                .sub_kinds
                .insert(sub_kind),
            EdgeAssertion::Provenance { sub_kind } => self
                .provenance
                .get_or_insert_with(ProvenanceData::default)
                .sub_kinds
                .insert(sub_kind),
        }
    }

    pub fn has_relation(&self, selector: RelationSelector) -> bool {
        match selector {
            RelationSelector::Family(family) => self.has_family(family),
            RelationSelector::Semantic(sub_kind) => self
                .semantic
                .as_ref()
                .is_some_and(|data| data.sub_kinds.contains(&sub_kind)),
            RelationSelector::Containment(sub_kind) => self
                .containment
                .as_ref()
                .is_some_and(|data| data.contains(sub_kind)),
            RelationSelector::Arrangement(sub_kind) => self
                .arrangement
                .as_ref()
                .is_some_and(|data| data.contains(sub_kind)),
            RelationSelector::Imported(sub_kind) => self
                .imported
                .as_ref()
                .is_some_and(|data| data.sub_kinds.contains(&sub_kind)),
            RelationSelector::Provenance(sub_kind) => self
                .provenance
                .as_ref()
                .is_some_and(|data| data.sub_kinds.contains(&sub_kind)),
        }
    }

    pub fn retract_relation(&mut self, selector: RelationSelector) -> bool {
        match selector {
            RelationSelector::Family(EdgeFamily::Traversal) => self.traversal.take().is_some(),
            RelationSelector::Family(_) => false,
            RelationSelector::Semantic(sub_kind) => self.remove_semantic_relation(sub_kind),
            RelationSelector::Containment(sub_kind) => {
                let Some(data) = self.containment.as_mut() else {
                    return false;
                };
                if !data.remove(sub_kind) {
                    return false;
                }
                if data.is_empty() {
                    self.containment = None;
                }
                true
            }
            RelationSelector::Arrangement(sub_kind) => {
                let Some(data) = self.arrangement.as_mut() else {
                    return false;
                };
                if !data.remove(sub_kind) {
                    return false;
                }
                if data.is_empty() {
                    self.arrangement = None;
                }
                true
            }
            RelationSelector::Imported(sub_kind) => {
                let Some(data) = self.imported.as_mut() else {
                    return false;
                };
                if !data.sub_kinds.remove(&sub_kind) {
                    return false;
                }
                if data.sub_kinds.is_empty() {
                    self.imported = None;
                }
                true
            }
            RelationSelector::Provenance(sub_kind) => {
                let Some(data) = self.provenance.as_mut() else {
                    return false;
                };
                if !data.sub_kinds.remove(&sub_kind) {
                    return false;
                }
                if data.sub_kinds.is_empty() {
                    self.provenance = None;
                }
                true
            }
        }
    }

    fn has_family(&self, family: EdgeFamily) -> bool {
        match family {
            EdgeFamily::Semantic => self.semantic.as_ref().is_some_and(|data| {
                !data.statements().is_empty()
                    || !data.sub_kinds.is_empty()
                    || data.predicate.is_some()
            }),
            EdgeFamily::Traversal => self.traversal.is_some(),
            EdgeFamily::Containment => self
                .containment
                .as_ref()
                .is_some_and(|data| !data.is_empty()),
            EdgeFamily::Arrangement => self
                .arrangement
                .as_ref()
                .is_some_and(|data| !data.is_empty()),
            EdgeFamily::Imported => self
                .imported
                .as_ref()
                .is_some_and(|data| !data.sub_kinds.is_empty()),
            EdgeFamily::Provenance => self
                .provenance
                .as_ref()
                .is_some_and(|data| !data.sub_kinds.is_empty()),
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.has_family(EdgeFamily::Semantic)
            && self.traversal.is_none()
            && !self.has_family(EdgeFamily::Containment)
            && !self.has_family(EdgeFamily::Arrangement)
            && !self.has_family(EdgeFamily::Imported)
            && !self.has_family(EdgeFamily::Provenance)
    }

    pub fn label(&self) -> Option<&str> {
        self.semantic
            .as_ref()
            .and_then(|data| data.label.as_deref())
    }

    pub fn semantic_data(&self) -> Option<&SemanticData> {
        self.semantic.as_ref()
    }

    pub fn semantic_statements(&self) -> &[SemanticStatement] {
        self.semantic
            .as_ref()
            .map(SemanticData::statements)
            .unwrap_or(&[])
    }

    pub(crate) fn assert_semantic_predicate(&mut self, predicate: String) -> bool {
        self.assert_semantic_predicate_in_scope(predicate, GraphScope::Default)
    }

    pub(crate) fn assert_semantic_relation_in_scope(
        &mut self,
        sub_kind: SemanticSubKind,
        label: Option<String>,
        graph_scope: GraphScope,
    ) -> bool {
        self.insert_semantic_relation_in_scope(sub_kind, label, graph_scope)
    }

    pub(crate) fn assert_semantic_predicate_in_scope(
        &mut self,
        predicate: String,
        graph_scope: GraphScope,
    ) -> bool {
        self.semantic
            .get_or_insert_with(SemanticData::default)
            .insert_statement(None, predicate, None, graph_scope, None, None)
    }

    /// Statement-aware assert on this pair bucket (creates the semantic
    /// sidecar if absent). Returns the fact handle + whether anything changed.
    pub(crate) fn assert_semantic_statement(
        &mut self,
        spec: SemanticStatementSpec,
    ) -> StatementAssert {
        self.semantic
            .get_or_insert_with(SemanticData::default)
            .assert_statement(spec)
    }

    /// Precise retract by fact handle. Clears the semantic sidecar when its
    /// last statement goes (the caller removes the petgraph edge when the
    /// whole payload empties, same as `retract_relation`).
    pub(crate) fn retract_semantic_statement(&mut self, statement_id: &str) -> bool {
        let Some(data) = self.semantic.as_mut() else {
            return false;
        };
        if !data.retract_statement(statement_id) {
            return false;
        }
        if data.statements().is_empty() {
            self.semantic = None;
        }
        true
    }

    pub(crate) fn push_persisted_semantic_statement(
        &mut self,
        statement: SemanticStatement,
    ) -> bool {
        self.semantic
            .get_or_insert_with(SemanticData::default)
            .push_persisted_statement(statement)
    }

    /// Set (or clear) the open predicate IRI on the semantic sidecar, creating
    /// the sidecar if needed. The predicate is independent of `sub_kinds`: a
    /// recognized edge carries both; a raw web predicate may carry only this.
    pub fn set_semantic_predicate(&mut self, predicate: Option<String>) {
        if predicate.is_none() && self.semantic.is_none() {
            return;
        }
        if let Some(data) = self.semantic.as_mut() {
            data.set_statement_predicate(predicate);
            if data.statements().is_empty() {
                self.semantic = None;
            }
            return;
        }
        let mut data = SemanticData::default();
        data.set_statement_predicate(predicate);
        if !data.statements().is_empty() {
            self.semantic = Some(data);
        }
    }

    pub fn traversal_data(&self) -> Option<&TraversalData> {
        self.traversal.as_ref()
    }

    pub fn arrangement_data(&self) -> Option<&ArrangementData> {
        self.arrangement.as_ref()
    }

    pub fn containment_data(&self) -> Option<&ContainmentData> {
        self.containment.as_ref()
    }

    pub fn imported_data(&self) -> Option<&ImportedData> {
        self.imported.as_ref()
    }

    pub fn provenance_data(&self) -> Option<&ProvenanceData> {
        self.provenance.as_ref()
    }

    pub fn has_arrangement_sub_kind(&self, sub_kind: ArrangementSubKind) -> bool {
        self.arrangement
            .as_ref()
            .is_some_and(|data| data.contains(sub_kind))
    }

    pub fn has_durable_arrangement_relation(&self) -> bool {
        self.arrangement
            .as_ref()
            .is_some_and(ArrangementData::has_durable_relation)
    }

    pub fn has_session_arrangement_relation(&self) -> bool {
        self.arrangement
            .as_ref()
            .is_some_and(ArrangementData::has_session_relation)
    }

    pub fn traversals(&self) -> &[Traversal] {
        self.traversal
            .as_ref()
            .map(|data| data.traversals.as_slice())
            .unwrap_or(&[])
    }

    pub fn metrics(&self) -> EdgeMetrics {
        self.traversal
            .as_ref()
            .map(|data| data.metrics)
            .unwrap_or_default()
    }

    pub fn push_traversal(&mut self, traversal: Traversal) {
        self.traversal
            .get_or_insert_with(TraversalData::default)
            .push(traversal);
    }

    /// Compute the set of [`EdgeFamily`] tags carried by this payload
    /// from the populated sidecars. Materialized fresh on each call —
    /// there is no parallel cache.
    pub fn families(&self) -> BTreeSet<EdgeFamily> {
        let mut out = BTreeSet::new();
        if self.has_family(EdgeFamily::Semantic) {
            out.insert(EdgeFamily::Semantic);
        }
        if self.has_family(EdgeFamily::Traversal) {
            out.insert(EdgeFamily::Traversal);
        }
        if self.has_family(EdgeFamily::Containment) {
            out.insert(EdgeFamily::Containment);
        }
        if self.has_family(EdgeFamily::Arrangement) {
            out.insert(EdgeFamily::Arrangement);
        }
        if self.has_family(EdgeFamily::Imported) {
            out.insert(EdgeFamily::Imported);
        }
        if self.has_family(EdgeFamily::Provenance) {
            out.insert(EdgeFamily::Provenance);
        }
        out
    }
}

impl Default for EdgePayload {
    fn default() -> Self {
        Self::new()
    }
}
