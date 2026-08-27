// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::collections::HashMap;

impl<K, E, O, X> Stemma<K, E, O, X>
where
    K: EntryIdentityKey,
    E: MemoryPayload,
    O: OwnerIdentity,
    X: MemoryPayload,
{
    pub fn entry(&self, id: EntryId) -> Option<&EntryRecord<K, E>> {
        self.entries.get(id)
    }

    pub fn visit(&self, id: VisitId) -> Option<&VisitRecord<X>> {
        self.visits.get(id)
    }

    pub fn owner(&self, id: OwnerId) -> Option<&OwnerRecord<O>> {
        self.owners.get(id)
    }

    pub fn entries(&self) -> impl Iterator<Item = (EntryId, &EntryRecord<K, E>)> {
        self.entries.iter()
    }

    pub fn visits(&self) -> impl Iterator<Item = (VisitId, &VisitRecord<X>)> {
        self.visits.iter()
    }

    pub fn owners(&self) -> impl Iterator<Item = (OwnerId, &OwnerRecord<O>)> {
        self.owners.iter()
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn visit_count(&self) -> usize {
        self.visits.len()
    }

    pub fn owner_count(&self) -> usize {
        self.owners.len()
    }

    pub fn owner_id_by_identity(&self, identity: &O) -> Option<OwnerId> {
        self.owner_index.get(identity).copied()
    }

    pub fn current_visit_of_owner(&self, owner_id: OwnerId) -> Option<VisitId> {
        self.owners.get(owner_id).and_then(|owner| owner.current)
    }

    pub fn current_entry_of_owner(&self, owner_id: OwnerId) -> Option<EntryId> {
        let visit_id = self.current_visit_of_owner(owner_id)?;
        self.visits.get(visit_id).map(|visit| visit.entry)
    }

    pub fn linear_history_visits_of_owner(
        &self,
        owner_id: OwnerId,
    ) -> Result<Vec<VisitId>, StemmaError> {
        let owner = self
            .owners
            .get(owner_id)
            .ok_or(StemmaError::MissingOwner(owner_id))?;
        let Some(origin) = owner.origin else {
            return Ok(Vec::new());
        };

        let mut visits = vec![origin];
        let mut cursor = origin;
        loop {
            let next = self
                .visits
                .get(cursor)
                .and_then(|visit| visit.bindings.get(&owner_id))
                .and_then(|binding| binding.forward_child);
            match next {
                Some(next_visit) => {
                    visits.push(next_visit);
                    cursor = next_visit;
                }
                None => break,
            }
        }

        Ok(visits)
    }

    pub fn linear_history_entries_of_owner(
        &self,
        owner_id: OwnerId,
    ) -> Result<Vec<EntryId>, StemmaError> {
        self.linear_history_visits_of_owner(owner_id)?
            .into_iter()
            .map(|visit_id| {
                self.visits
                    .get(visit_id)
                    .map(|visit| visit.entry)
                    .ok_or(StemmaError::MissingVisit(visit_id))
            })
            .collect()
    }

    pub fn current_index_of_owner(&self, owner_id: OwnerId) -> Result<Option<usize>, StemmaError> {
        let current = match self.current_visit_of_owner(owner_id) {
            Some(visit_id) => visit_id,
            None => return Ok(None),
        };
        let linear = self.linear_history_visits_of_owner(owner_id)?;
        Ok(linear.iter().position(|visit_id| *visit_id == current))
    }

    pub fn owner_branch_projection(
        &self,
        owner_id: OwnerId,
    ) -> Result<OwnerBranchProjection<E>, StemmaError> {
        let current = self.current_visit_of_owner(owner_id);
        let linear = self.linear_history_visits_of_owner(owner_id)?;
        let current_index =
            current.and_then(|visit_id| linear.iter().position(|candidate| *candidate == visit_id));

        let visits = linear
            .iter()
            .enumerate()
            .map(|(idx, visit_id)| {
                let visit = self
                    .visits
                    .get(*visit_id)
                    .ok_or(StemmaError::MissingVisit(*visit_id))?;
                let entry = self
                    .entries
                    .get(visit.entry)
                    .ok_or(StemmaError::MissingEntry(visit.entry))?;
                let next_in_path = linear.get(idx + 1).copied();

                let alternate_children = visit
                    .children
                    .iter()
                    .copied()
                    .filter(|child_id| Some(*child_id) != next_in_path)
                    .filter_map(|child_id| {
                        let child = self.visits.get(child_id)?;
                        let child_entry = self.entries.get(child.entry)?;
                        Some(OwnerBranchAlternative {
                            visit_id: child_id,
                            entry_id: child.entry,
                            payload: child_entry.payload.clone(),
                            transition: child.inbound.map(|record| record.kind),
                            at_ms: child.created_at_ms,
                        })
                    })
                    .collect();

                Ok(OwnerBranchVisit {
                    visit_id: *visit_id,
                    entry_id: visit.entry,
                    payload: entry.payload.clone(),
                    transition: visit.inbound.map(|record| record.kind),
                    at_ms: visit.created_at_ms,
                    is_current: current == Some(*visit_id),
                    alternate_children,
                })
            })
            .collect::<Result<Vec<_>, StemmaError>>()?;

        Ok(OwnerBranchProjection {
            visits,
            current_index,
        })
    }

    pub fn entry_id_by_key(&self, key: &K) -> Option<EntryId> {
        self.entry_index.get(key).copied()
    }

    pub fn edge_views(&self) -> Vec<EdgeView> {
        let mut edges = Vec::new();
        for (visit_id, visit) in self.visits.iter() {
            let Some(parent_id) = visit.parent else {
                continue;
            };
            let Some(parent) = self.visits.get(parent_id) else {
                continue;
            };
            edges.push(EdgeView {
                from_visit: parent_id,
                to_visit: visit_id,
                from_entry: parent.entry,
                to_entry: visit.entry,
                transition: visit.inbound.map(|inbound| inbound.kind),
                at_ms: visit.created_at_ms,
            });
        }
        edges
    }

    pub fn aggregated_entry_edges(&self) -> Vec<AggregatedEntryEdgeView> {
        let mut aggregate: HashMap<(EntryId, EntryId), AggregatedEntryEdgeView> = HashMap::new();

        for edge in self.edge_views() {
            let key = (edge.from_entry, edge.to_entry);
            let view = aggregate
                .entry(key)
                .or_insert_with(|| AggregatedEntryEdgeView {
                    from_entry: edge.from_entry,
                    to_entry: edge.to_entry,
                    traversal_count: 0,
                    latest_transition_at_ms: 0,
                    transition_counts: HashMap::new(),
                });

            view.traversal_count += 1;
            view.latest_transition_at_ms = view.latest_transition_at_ms.max(edge.at_ms);
            if let Some(kind) = edge.transition {
                *view.transition_counts.entry(kind).or_insert(0) += 1;
            }
        }

        aggregate.into_values().collect()
    }
}
