/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `EdgePayload` — the structural + temporal payload carried on every
//! graph edge. Composes the taxonomy types from
//! [`super::edge_taxonomy`] into a single payload struct with full
//! family/kind/sub-kind syncing logic.
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel-mod
//! decomposition pass.

use std::collections::BTreeSet;

use rkyv::{Archive, Deserialize, Serialize};

use super::edge_taxonomy::{
    ArrangementData, ArrangementSubKind, ContainmentData, EdgeAssertion, EdgeFamily, EdgeKind,
    EdgeMetrics, EdgeType, ImportedData, ProvenanceData, RelationSelector, SemanticData,
    SemanticSubKind, Traversal, TraversalData, UserGroupedData,
};

/// Edge semantics payload: structural assertions + temporal traversal events.
#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct EdgePayload {
    pub families: BTreeSet<EdgeFamily>,
    pub kinds: BTreeSet<EdgeKind>,
    pub semantic: Option<SemanticData>,
    pub user_grouped: Option<UserGroupedData>,
    pub traversal: Option<TraversalData>,
    pub arrangement: Option<ArrangementData>,
    pub containment: Option<ContainmentData>,
    pub imported: Option<ImportedData>,
    pub provenance: Option<ProvenanceData>,
}

impl EdgePayload {
    pub fn new() -> Self {
        Self {
            families: BTreeSet::new(),
            kinds: BTreeSet::new(),
            semantic: None,
            user_grouped: None,
            traversal: None,
            arrangement: None,
            containment: None,
            imported: None,
            provenance: None,
        }
    }

    pub fn from_edge_type(edge_type: EdgeType, label: Option<String>) -> Self {
        let mut payload = Self::new();
        let _ = payload.add_edge_kind(edge_type, label);
        payload
    }

    fn sync_family_from_kind(&mut self, kind: EdgeKind) {
        match kind {
            EdgeKind::SemanticRelation
            | EdgeKind::Hyperlink
            | EdgeKind::UserGrouped
            | EdgeKind::AgentDerived => {
                let _ = self.families.insert(EdgeFamily::Semantic);
            }
            EdgeKind::TraversalDerived => {
                let _ = self.families.insert(EdgeFamily::Traversal);
            }
            EdgeKind::ArrangementRelation => {
                let _ = self.families.insert(EdgeFamily::Arrangement);
            }
            EdgeKind::ContainmentRelation => {
                let _ = self.families.insert(EdgeFamily::Containment);
            }
            EdgeKind::ImportedRelation => {
                let _ = self.families.insert(EdgeFamily::Imported);
            }
        }
    }

    fn prune_family(&mut self, family: EdgeFamily) {
        let keep = match family {
            EdgeFamily::Semantic => self
                .semantic
                .as_ref()
                .is_some_and(|data| !data.sub_kinds.is_empty()),
            EdgeFamily::Traversal => self.kinds.contains(&EdgeKind::TraversalDerived),
            EdgeFamily::Containment => self.kinds.contains(&EdgeKind::ContainmentRelation),
            EdgeFamily::Arrangement => self.kinds.contains(&EdgeKind::ArrangementRelation),
            EdgeFamily::Imported => self.kinds.contains(&EdgeKind::ImportedRelation),
            EdgeFamily::Provenance => self
                .provenance
                .as_ref()
                .is_some_and(|data| !data.sub_kinds.is_empty()),
        };
        if !keep {
            self.families.remove(&family);
        }
    }

    fn sync_semantic_kinds(&mut self) {
        let Some((sub_kinds, label)) = self
            .semantic
            .as_ref()
            .map(|data| (data.sub_kinds.clone(), data.label.clone()))
        else {
            self.kinds.remove(&EdgeKind::SemanticRelation);
            self.kinds.remove(&EdgeKind::Hyperlink);
            self.kinds.remove(&EdgeKind::UserGrouped);
            self.kinds.remove(&EdgeKind::AgentDerived);
            self.user_grouped = None;
            self.prune_family(EdgeFamily::Semantic);
            return;
        };

        let has_generic_semantics = sub_kinds.iter().copied().any(|sub_kind| {
            !matches!(
                sub_kind,
                SemanticSubKind::Hyperlink
                    | SemanticSubKind::UserGrouped
                    | SemanticSubKind::AgentDerived
            )
        });
        if has_generic_semantics {
            let inserted = self.kinds.insert(EdgeKind::SemanticRelation);
            if inserted {
                self.sync_family_from_kind(EdgeKind::SemanticRelation);
            }
        } else {
            self.kinds.remove(&EdgeKind::SemanticRelation);
        }

        if sub_kinds.contains(&SemanticSubKind::Hyperlink) {
            let inserted = self.kinds.insert(EdgeKind::Hyperlink);
            if inserted {
                self.sync_family_from_kind(EdgeKind::Hyperlink);
            }
        } else {
            self.kinds.remove(&EdgeKind::Hyperlink);
        }

        if sub_kinds.contains(&SemanticSubKind::UserGrouped) {
            let inserted = self.kinds.insert(EdgeKind::UserGrouped);
            if inserted {
                self.sync_family_from_kind(EdgeKind::UserGrouped);
            }
            let user_grouped = self
                .user_grouped
                .get_or_insert_with(UserGroupedData::default);
            user_grouped.label = label.clone();
        } else {
            self.kinds.remove(&EdgeKind::UserGrouped);
            self.user_grouped = None;
        }

        if sub_kinds.contains(&SemanticSubKind::AgentDerived) {
            let inserted = self.kinds.insert(EdgeKind::AgentDerived);
            if inserted {
                self.sync_family_from_kind(EdgeKind::AgentDerived);
            }
        } else {
            self.kinds.remove(&EdgeKind::AgentDerived);
        }

        self.prune_family(EdgeFamily::Semantic);
    }

    fn insert_semantic_relation(
        &mut self,
        sub_kind: SemanticSubKind,
        label: Option<String>,
    ) -> bool {
        let data = self.semantic.get_or_insert_with(SemanticData::default);
        let inserted = data.sub_kinds.insert(sub_kind);
        let mut changed = inserted;
        if let Some(label) = label
            && data.label.as_ref() != Some(&label)
        {
            data.label = Some(label);
            changed = true;
        }
        self.sync_semantic_kinds();
        changed
    }

    fn remove_semantic_relation(&mut self, sub_kind: SemanticSubKind) -> bool {
        let Some(data) = self.semantic.as_mut() else {
            return false;
        };
        if !data.sub_kinds.remove(&sub_kind) {
            return false;
        }
        if data.sub_kinds.is_empty() {
            self.semantic = None;
        }
        self.sync_semantic_kinds();
        true
    }

    pub fn assert_relation(&mut self, assertion: EdgeAssertion) -> bool {
        match assertion {
            EdgeAssertion::Semantic {
                sub_kind, label, ..
            } => self.insert_semantic_relation(sub_kind, label),
            EdgeAssertion::Containment { sub_kind } => {
                self.add_edge_kind(EdgeType::ContainmentRelation(sub_kind), None)
            }
            EdgeAssertion::Arrangement { sub_kind } => {
                self.add_edge_kind(EdgeType::ArrangementRelation(sub_kind), None)
            }
            EdgeAssertion::Imported { sub_kind } => {
                let inserted = self.add_edge_kind(EdgeType::ImportedRelation, None);
                let data = self.imported.get_or_insert_with(ImportedData::default);
                inserted | data.sub_kinds.insert(sub_kind)
            }
            EdgeAssertion::Provenance { sub_kind } => {
                let _ = self.families.insert(EdgeFamily::Provenance);
                let data = self.provenance.get_or_insert_with(ProvenanceData::default);
                data.sub_kinds.insert(sub_kind)
            }
        }
    }

    pub fn add_edge_kind(&mut self, edge_type: EdgeType, label: Option<String>) -> bool {
        match edge_type {
            EdgeType::Hyperlink => self.insert_semantic_relation(SemanticSubKind::Hyperlink, label),
            EdgeType::UserGrouped => {
                self.insert_semantic_relation(SemanticSubKind::UserGrouped, label)
            }
            EdgeType::History => {
                let inserted = self.kinds.insert(EdgeKind::TraversalDerived);
                self.sync_family_from_kind(EdgeKind::TraversalDerived);
                let had_data = self.traversal.is_some();
                let _ = self.traversal.get_or_insert_with(TraversalData::default);
                inserted || !had_data
            }
            EdgeType::ArrangementRelation(sub_kind) => {
                let inserted = self.kinds.insert(EdgeKind::ArrangementRelation);
                self.sync_family_from_kind(EdgeKind::ArrangementRelation);
                let data = self
                    .arrangement
                    .get_or_insert_with(ArrangementData::default);
                inserted | data.insert(sub_kind)
            }
            EdgeType::ContainmentRelation(sub_kind) => {
                let inserted = self.kinds.insert(EdgeKind::ContainmentRelation);
                self.sync_family_from_kind(EdgeKind::ContainmentRelation);
                let data = self
                    .containment
                    .get_or_insert_with(ContainmentData::default);
                inserted | data.insert(sub_kind)
            }
            EdgeType::ImportedRelation => {
                let inserted = self.kinds.insert(EdgeKind::ImportedRelation);
                self.sync_family_from_kind(EdgeKind::ImportedRelation);
                let _ = self.imported.get_or_insert_with(ImportedData::default);
                inserted
            }
            EdgeType::AgentDerived { .. } => {
                self.insert_semantic_relation(SemanticSubKind::AgentDerived, label)
            }
        }
    }

    pub fn add_edge_type(&mut self, edge_type: EdgeType) {
        let _ = self.add_edge_kind(edge_type, None);
    }

    pub fn has_relation(&self, selector: RelationSelector) -> bool {
        match selector {
            RelationSelector::Family(family) => self.families.contains(&family),
            RelationSelector::Semantic(sub_kind) => self
                .semantic
                .as_ref()
                .is_some_and(|data| data.sub_kinds.contains(&sub_kind)),
            RelationSelector::Containment(sub_kind) => {
                self.has_edge_type(EdgeType::ContainmentRelation(sub_kind))
            }
            RelationSelector::Arrangement(sub_kind) => {
                self.has_edge_type(EdgeType::ArrangementRelation(sub_kind))
            }
            RelationSelector::Imported(sub_kind) => {
                self.imported
                    .as_ref()
                    .is_some_and(|data| data.sub_kinds.contains(&sub_kind))
                    || (self
                        .imported
                        .as_ref()
                        .is_some_and(|data| data.sub_kinds.is_empty())
                        && self.has_edge_type(EdgeType::ImportedRelation))
            }
            RelationSelector::Provenance(sub_kind) => self
                .provenance
                .as_ref()
                .is_some_and(|data| data.sub_kinds.contains(&sub_kind)),
        }
    }

    pub fn has_edge_kind(&self, edge_type: EdgeType) -> bool {
        match edge_type {
            EdgeType::Hyperlink => self.kinds.contains(&EdgeKind::Hyperlink),
            EdgeType::UserGrouped => {
                self.kinds.contains(&EdgeKind::UserGrouped) && self.user_grouped.is_some()
            }
            EdgeType::History => {
                self.kinds.contains(&EdgeKind::TraversalDerived) && self.traversal.is_some()
            }
            EdgeType::ArrangementRelation(sub_kind) => {
                self.kinds.contains(&EdgeKind::ArrangementRelation)
                    && self
                        .arrangement
                        .as_ref()
                        .is_some_and(|data| data.contains(sub_kind))
            }
            EdgeType::ContainmentRelation(sub_kind) => {
                self.kinds.contains(&EdgeKind::ContainmentRelation)
                    && self
                        .containment
                        .as_ref()
                        .is_some_and(|data| data.contains(sub_kind))
            }
            EdgeType::ImportedRelation => self.kinds.contains(&EdgeKind::ImportedRelation),
            EdgeType::AgentDerived { .. } => self.kinds.contains(&EdgeKind::AgentDerived),
        }
    }

    pub fn has_edge_type(&self, edge_type: EdgeType) -> bool {
        self.has_edge_kind(edge_type)
    }

    pub fn has_kind(&self, kind: EdgeKind) -> bool {
        self.kinds.contains(&kind)
    }

    pub fn retract_relation(&mut self, selector: RelationSelector) -> bool {
        match selector {
            RelationSelector::Family(EdgeFamily::Traversal) => {
                self.remove_edge_type(EdgeType::History)
            }
            RelationSelector::Family(_) => false,
            RelationSelector::Semantic(sub_kind) => self.remove_semantic_relation(sub_kind),
            RelationSelector::Containment(sub_kind) => {
                self.remove_edge_type(EdgeType::ContainmentRelation(sub_kind))
            }
            RelationSelector::Arrangement(sub_kind) => {
                self.remove_edge_type(EdgeType::ArrangementRelation(sub_kind))
            }
            RelationSelector::Imported(sub_kind) => {
                if let Some(data) = self.imported.as_mut()
                    && data.sub_kinds.remove(&sub_kind)
                {
                    if data.sub_kinds.is_empty() {
                        self.imported = None;
                        let _ = self.remove_edge_type(EdgeType::ImportedRelation);
                    }
                    true
                } else {
                    self.remove_edge_type(EdgeType::ImportedRelation)
                }
            }
            RelationSelector::Provenance(sub_kind) => {
                if let Some(data) = self.provenance.as_mut()
                    && data.sub_kinds.remove(&sub_kind)
                {
                    if data.sub_kinds.is_empty() {
                        self.provenance = None;
                        self.prune_family(EdgeFamily::Provenance);
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn remove_edge_kind(&mut self, edge_type: EdgeType) -> bool {
        match edge_type {
            EdgeType::Hyperlink => self.remove_semantic_relation(SemanticSubKind::Hyperlink),
            EdgeType::UserGrouped => self.remove_semantic_relation(SemanticSubKind::UserGrouped),
            EdgeType::History if self.kinds.remove(&EdgeKind::TraversalDerived) => {
                self.traversal = None;
                self.prune_family(EdgeFamily::Traversal);
                true
            }
            EdgeType::ArrangementRelation(sub_kind)
                if self
                    .arrangement
                    .as_mut()
                    .is_some_and(|data| data.remove(sub_kind)) =>
            {
                if self
                    .arrangement
                    .as_ref()
                    .is_some_and(ArrangementData::is_empty)
                {
                    self.arrangement = None;
                    self.kinds.remove(&EdgeKind::ArrangementRelation);
                    self.prune_family(EdgeFamily::Arrangement);
                }
                true
            }
            EdgeType::ContainmentRelation(sub_kind)
                if self
                    .containment
                    .as_mut()
                    .is_some_and(|data| data.remove(sub_kind)) =>
            {
                if self
                    .containment
                    .as_ref()
                    .is_some_and(ContainmentData::is_empty)
                {
                    self.containment = None;
                    self.kinds.remove(&EdgeKind::ContainmentRelation);
                    self.prune_family(EdgeFamily::Containment);
                }
                true
            }
            EdgeType::ImportedRelation if self.kinds.remove(&EdgeKind::ImportedRelation) => {
                self.imported = None;
                self.prune_family(EdgeFamily::Imported);
                true
            }
            EdgeType::AgentDerived { .. } => {
                self.remove_semantic_relation(SemanticSubKind::AgentDerived)
            }
            _ => false,
        }
    }

    pub fn remove_edge_type(&mut self, edge_type: EdgeType) -> bool {
        self.remove_edge_kind(edge_type)
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty() && self.provenance.is_none()
    }

    pub fn label(&self) -> Option<&str> {
        self.semantic
            .as_ref()
            .and_then(|data| data.label.as_deref())
            .or_else(|| {
                self.user_grouped
                    .as_ref()
                    .and_then(|data| data.label.as_deref())
            })
    }

    pub fn semantic_data(&self) -> Option<&SemanticData> {
        self.semantic.as_ref()
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
        let _ = self.kinds.insert(EdgeKind::TraversalDerived);
        self.sync_family_from_kind(EdgeKind::TraversalDerived);
        self.traversal
            .get_or_insert_with(TraversalData::default)
            .push(traversal);
    }

    pub fn families(&self) -> &BTreeSet<EdgeFamily> {
        &self.families
    }
}

impl Default for EdgePayload {
    fn default() -> Self {
        Self::new()
    }
}
