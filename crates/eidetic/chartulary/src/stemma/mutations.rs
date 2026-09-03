// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::collections::{HashMap, HashSet};

impl<K, E, O, X> Stemma<K, E, O, X>
where
    K: EntryIdentityKey,
    E: MemoryPayload,
    O: OwnerIdentity,
    X: MemoryPayload,
{
    pub fn ensure_owner(&mut self, identity: O, creator: Option<OwnerId>) -> OwnerId {
        if let Some(id) = self.owner_index.get(&identity).copied() {
            return id;
        }

        let pending_origin_parent = creator
            .and_then(|creator_id| self.owners.get(creator_id))
            .and_then(|owner| owner.current);

        let owner = OwnerRecord {
            identity: identity.clone(),
            origin: None,
            current: None,
            creator,
            pending_origin_parent,
            owned_visits: HashSet::new(),
        };
        let id = self.owners.insert(owner);
        self.owner_index.insert(identity, id);
        id
    }

    pub fn resolve_or_create_entry(
        &mut self,
        key: K,
        payload: E,
        at_ms: u64,
        privacy: EntryPrivacy,
    ) -> EntryId {
        if let Some(id) = self.entry_index.get(&key).copied() {
            if let Some(entry) = self.entries.get_mut(id) {
                entry.payload = payload;
                entry.last_seen_at_ms = at_ms;
                entry.privacy = privacy;
            }
            return id;
        }

        let id = self.entries.insert(EntryRecord {
            key: key.clone(),
            payload,
            first_seen_at_ms: at_ms,
            last_seen_at_ms: at_ms,
            visit_count: 0,
            privacy,
        });
        self.entry_index.insert(key, id);
        id
    }

    pub fn adopt_visit(
        &mut self,
        owner_id: OwnerId,
        visit_id: VisitId,
        at_ms: u64,
    ) -> Result<VisitId, StemmaError> {
        if !self.owners.contains_key(owner_id) {
            return Err(StemmaError::MissingOwner(owner_id));
        }
        if !self.visits.contains_key(visit_id) {
            return Err(StemmaError::MissingVisit(visit_id));
        }

        let current = self.owners.get(owner_id).and_then(|owner| owner.current);
        if let Some(current_id) = current {
            let mut maybe_forward = None;
            if let Some(current_visit) = self.visits.get(current_id)
                && current_visit.children.contains(&visit_id)
            {
                maybe_forward = Some(visit_id);
            }
            if let Some(forward_child) = maybe_forward
                && let Some(binding) = self.ensure_binding(current_id, owner_id, at_ms)
            {
                binding.forward_child = Some(forward_child);
            }
        }

        self.bind_owner_to_visit(owner_id, visit_id, at_ms)?;
        Ok(visit_id)
    }

    pub fn visit_entry(
        &mut self,
        owner_id: OwnerId,
        entry_id: EntryId,
        context: X,
        transition: TransitionKind,
        at_ms: u64,
    ) -> Result<VisitId, StemmaError> {
        if !self.owners.contains_key(owner_id) {
            return Err(StemmaError::MissingOwner(owner_id));
        }
        if !self.entries.contains_key(entry_id) {
            return Err(StemmaError::MissingEntry(entry_id));
        }

        let parent = match self.owners.get(owner_id).and_then(|owner| owner.current) {
            Some(current) => Some(current),
            None => self
                .owners
                .get_mut(owner_id)
                .and_then(|owner| owner.pending_origin_parent.take()),
        };

        let visit_id = self.visits.insert(VisitRecord {
            entry: entry_id,
            parent,
            children: Vec::new(),
            created_at_ms: at_ms,
            context,
            inbound: parent.map(|_| TransitionRecord {
                kind: transition,
                at_ms,
            }),
            bindings: HashMap::new(),
        });

        if let Some(parent_id) = parent {
            if let Some(parent_visit) = self.visits.get_mut(parent_id) {
                parent_visit.children.push(visit_id);
            }
            if let Some(binding) = self.ensure_binding(parent_id, owner_id, at_ms) {
                binding.forward_child = Some(visit_id);
            }
        }

        self.bind_owner_to_visit(owner_id, visit_id, at_ms)?;

        if let Some(owner) = self.owners.get_mut(owner_id)
            && owner.origin.is_none()
        {
            owner.origin = Some(visit_id);
        }

        if let Some(entry) = self.entries.get_mut(entry_id) {
            entry.last_seen_at_ms = at_ms;
            entry.visit_count += 1;
        }

        Ok(visit_id)
    }

    pub fn back(
        &mut self,
        owner_id: OwnerId,
        steps: usize,
        at_ms: u64,
    ) -> Result<Option<VisitId>, StemmaError> {
        if !self.owners.contains_key(owner_id) {
            return Err(StemmaError::MissingOwner(owner_id));
        }

        let mut moved_to = None;
        for _ in 0..steps.max(1) {
            let current_id = match self.owners.get(owner_id).and_then(|owner| owner.current) {
                Some(id) => id,
                None => break,
            };
            let parent_id = match self.visits.get(current_id).and_then(|visit| visit.parent) {
                Some(id) => id,
                None => break,
            };

            if let Some(binding) = self.ensure_binding(parent_id, owner_id, at_ms) {
                binding.forward_child = Some(current_id);
            }
            self.bind_owner_to_visit(owner_id, parent_id, at_ms)?;
            moved_to = Some(parent_id);
        }

        Ok(moved_to)
    }

    pub fn forward(
        &mut self,
        owner_id: OwnerId,
        steps: usize,
        at_ms: u64,
    ) -> Result<Option<VisitId>, StemmaError> {
        if !self.owners.contains_key(owner_id) {
            return Err(StemmaError::MissingOwner(owner_id));
        }

        let mut moved_to = None;
        for _ in 0..steps.max(1) {
            let current_id = match self.owners.get(owner_id).and_then(|owner| owner.current) {
                Some(id) => id,
                None => break,
            };
            let next_id = match self
                .visits
                .get(current_id)
                .and_then(|visit| visit.bindings.get(&owner_id))
                .and_then(|binding| binding.forward_child)
            {
                Some(id) => id,
                None => break,
            };

            self.bind_owner_to_visit(owner_id, next_id, at_ms)?;
            moved_to = Some(next_id);
        }

        Ok(moved_to)
    }

    pub fn delete_owner(&mut self, owner_id: OwnerId) -> Result<GcReport, StemmaError> {
        let owner = self
            .owners
            .remove(owner_id)
            .ok_or(StemmaError::MissingOwner(owner_id))?;
        self.owner_index.remove(&owner.identity);

        let owned_visits: Vec<_> = owner.owned_visits.into_iter().collect();
        // Drop this owner's bindings from *every* visit, not only its owned ones:
        // a spawned owner also leaves a forward-child binding on the creator's
        // visit it attached under (not in `owned_visits`). Leaving that dangling
        // makes `to_snapshot` panic indexing the removed owner. (GC fix 2026-06-06.)
        for visit in self.visits.values_mut() {
            visit.bindings.remove(&owner_id);
        }

        let mut roots = HashSet::new();
        for visit_id in owned_visits {
            if self.visits.contains_key(visit_id) {
                roots.insert(self.root_of(visit_id)?);
            }
        }

        let mut deleted = Vec::new();
        for root in roots {
            if self.branch_is_ownerless(root) {
                self.delete_branch(root, &mut deleted)?;
            }
        }

        Ok(GcReport {
            deleted_visits: deleted,
        })
    }

    pub fn reset_owner(
        &mut self,
        owner_id: OwnerId,
        context: X,
        at_ms: u64,
    ) -> Result<Option<VisitId>, StemmaError> {
        let current = self
            .owners
            .get(owner_id)
            .ok_or(StemmaError::MissingOwner(owner_id))?
            .current;
        let Some(current_id) = current else {
            return Ok(None);
        };
        let current_entry = self
            .visits
            .get(current_id)
            .ok_or(StemmaError::MissingVisit(current_id))?
            .entry;

        let owned: Vec<_> = self
            .owners
            .get(owner_id)
            .expect("owner checked above")
            .owned_visits
            .iter()
            .copied()
            .collect();

        for visit_id in owned {
            if let Some(visit) = self.visits.get_mut(visit_id) {
                visit.bindings.remove(&owner_id);
            }
        }

        if let Some(owner) = self.owners.get_mut(owner_id) {
            owner.origin = None;
            owner.current = None;
            owner.pending_origin_parent = None;
            owner.owned_visits.clear();
        }

        let reset_visit = self.visit_entry(
            owner_id,
            current_entry,
            context,
            TransitionKind::Restore,
            at_ms,
        )?;
        if let Some(visit) = self.visits.get_mut(reset_visit) {
            visit.parent = None;
            visit.inbound = None;
        }
        Ok(Some(reset_visit))
    }

    pub fn rebind_owner_to_path(
        &mut self,
        owner_id: OwnerId,
        path: &[VisitId],
        current_index: usize,
        at_ms: u64,
    ) -> Result<(), StemmaError> {
        if !self.owners.contains_key(owner_id) {
            return Err(StemmaError::MissingOwner(owner_id));
        }
        for visit_id in path {
            if !self.visits.contains_key(*visit_id) {
                return Err(StemmaError::MissingVisit(*visit_id));
            }
        }

        let previous_owned = self
            .owners
            .get(owner_id)
            .ok_or(StemmaError::MissingOwner(owner_id))?
            .owned_visits
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for visit_id in previous_owned {
            if let Some(visit) = self.visits.get_mut(visit_id) {
                visit.bindings.remove(&owner_id);
            }
        }

        for (idx, visit_id) in path.iter().enumerate() {
            let binding = self
                .ensure_binding(*visit_id, owner_id, at_ms)
                .ok_or(StemmaError::MissingVisit(*visit_id))?;
            binding.forward_child = path.get(idx + 1).copied();
        }

        let owner = self
            .owners
            .get_mut(owner_id)
            .ok_or(StemmaError::MissingOwner(owner_id))?;
        owner.origin = path.first().copied();
        owner.current = if path.is_empty() {
            None
        } else {
            Some(path[current_index.min(path.len().saturating_sub(1))])
        };
        owner.pending_origin_parent = None;
        owner.owned_visits = path.iter().copied().collect();

        Ok(())
    }

    fn bind_owner_to_visit(
        &mut self,
        owner_id: OwnerId,
        visit_id: VisitId,
        at_ms: u64,
    ) -> Result<(), StemmaError> {
        if !self.owners.contains_key(owner_id) {
            return Err(StemmaError::MissingOwner(owner_id));
        }
        if !self.visits.contains_key(visit_id) {
            return Err(StemmaError::MissingVisit(visit_id));
        }

        let binding = self.ensure_binding(visit_id, owner_id, at_ms);
        if binding.is_none() {
            return Err(StemmaError::MissingVisit(visit_id));
        }

        let owner = self
            .owners
            .get_mut(owner_id)
            .ok_or(StemmaError::MissingOwner(owner_id))?;
        owner.current = Some(visit_id);
        owner.owned_visits.insert(visit_id);
        Ok(())
    }

    fn ensure_binding(
        &mut self,
        visit_id: VisitId,
        owner_id: OwnerId,
        at_ms: u64,
    ) -> Option<&mut OwnerBinding> {
        let visit = self.visits.get_mut(visit_id)?;
        let binding = visit.bindings.entry(owner_id).or_insert(OwnerBinding {
            forward_child: None,
            last_accessed_at_ms: at_ms,
        });
        binding.last_accessed_at_ms = at_ms;
        Some(binding)
    }

    fn root_of(&self, visit_id: VisitId) -> Result<VisitId, StemmaError> {
        let mut cursor = visit_id;
        loop {
            let visit = self
                .visits
                .get(cursor)
                .ok_or(StemmaError::MissingVisit(cursor))?;
            match visit.parent {
                Some(parent) => cursor = parent,
                None => return Ok(cursor),
            }
        }
    }

    fn branch_is_ownerless(&self, root_id: VisitId) -> bool {
        self.collect_subtree(root_id).into_iter().all(|visit_id| {
            self.visits
                .get(visit_id)
                .is_none_or(|visit| visit.bindings.is_empty())
        })
    }

    fn collect_subtree(&self, root_id: VisitId) -> Vec<VisitId> {
        let mut stack = vec![root_id];
        let mut out = Vec::new();

        while let Some(visit_id) = stack.pop() {
            let Some(visit) = self.visits.get(visit_id) else {
                continue;
            };
            out.push(visit_id);
            for child in visit.children.iter().rev() {
                stack.push(*child);
            }
        }

        out
    }

    fn delete_branch(
        &mut self,
        root_id: VisitId,
        deleted: &mut Vec<VisitId>,
    ) -> Result<(), StemmaError> {
        let subtree = self.collect_subtree(root_id);
        for visit_id in subtree.iter().rev() {
            let visit = self
                .visits
                .remove(*visit_id)
                .ok_or(StemmaError::MissingVisit(*visit_id))?;
            if let Some(entry) = self.entries.get_mut(visit.entry) {
                entry.visit_count = entry.visit_count.saturating_sub(1);
            }
            deleted.push(*visit_id);
        }
        Ok(())
    }
}
