// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use std::collections::{HashMap, HashSet};

impl<K, E, O, X> GraphMemory<K, E, O, X>
where
    K: EntryIdentityKey,
    E: MemoryPayload,
    O: OwnerIdentity,
    X: MemoryPayload,
{
    pub fn from_snapshot(snapshot: GraphMemorySnapshot<K, E, O, X>) -> Self {
        let mut memory = Self::new();

        let mut owner_ids = Vec::with_capacity(snapshot.owners.len());
        for owner in &snapshot.owners {
            let owner_id = memory.owners.insert(OwnerRecord {
                identity: owner.identity.clone(),
                origin: None,
                current: None,
                creator: None,
                pending_origin_parent: None,
                owned_visits: HashSet::new(),
            });
            memory.owner_index.insert(owner.identity.clone(), owner_id);
            owner_ids.push(owner_id);
        }

        let mut entry_ids = Vec::with_capacity(snapshot.entries.len());
        for entry in &snapshot.entries {
            let entry_id = memory.entries.insert(EntryRecord {
                key: entry.key.clone(),
                payload: entry.payload.clone(),
                first_seen_at_ms: entry.first_seen_at_ms,
                last_seen_at_ms: entry.last_seen_at_ms,
                visit_count: entry.visit_count,
                privacy: entry.privacy,
            });
            memory.entry_index.insert(entry.key.clone(), entry_id);
            entry_ids.push(entry_id);
        }

        let mut visit_ids = Vec::with_capacity(snapshot.visits.len());
        for visit in &snapshot.visits {
            let entry_id = entry_ids[visit.entry];
            let visit_id = memory.visits.insert(VisitRecord {
                entry: entry_id,
                parent: None,
                children: Vec::new(),
                created_at_ms: visit.created_at_ms,
                context: visit.context.clone(),
                inbound: visit.inbound,
                bindings: HashMap::new(),
            });
            visit_ids.push(visit_id);
        }

        for (idx, visit) in snapshot.visits.iter().enumerate() {
            let visit_id = visit_ids[idx];
            let record = memory
                .visits
                .get_mut(visit_id)
                .expect("visit just inserted");
            record.parent = visit.parent.map(|parent| visit_ids[parent]);
            record.children = visit
                .children
                .iter()
                .map(|child| visit_ids[*child])
                .collect();
            record.bindings = visit
                .bindings
                .iter()
                .map(|binding| {
                    (
                        owner_ids[binding.owner],
                        OwnerBinding {
                            forward_child: binding.forward_child.map(|visit| visit_ids[visit]),
                            last_accessed_at_ms: binding.last_accessed_at_ms,
                        },
                    )
                })
                .collect();
        }

        for (idx, owner) in snapshot.owners.iter().enumerate() {
            let owner_id = owner_ids[idx];
            let record = memory
                .owners
                .get_mut(owner_id)
                .expect("owner just inserted");
            record.origin = owner.origin.map(|visit| visit_ids[visit]);
            record.current = owner.current.map(|visit| visit_ids[visit]);
            record.creator = owner.creator.map(|creator| owner_ids[creator]);
            record.pending_origin_parent =
                owner.pending_origin_parent.map(|visit| visit_ids[visit]);
            record.owned_visits = owner
                .owned_visits
                .iter()
                .map(|visit| visit_ids[*visit])
                .collect();
        }

        memory
    }

    pub fn to_snapshot(&self) -> GraphMemorySnapshot<K, E, O, X> {
        let mut owner_index = HashMap::new();
        let owners: Vec<_> = self
            .owners
            .iter()
            .enumerate()
            .map(|(idx, (owner_id, owner))| {
                owner_index.insert(owner_id, idx);
                owner
            })
            .collect();

        let mut entry_index = HashMap::new();
        let entries: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .map(|(idx, (entry_id, entry))| {
                entry_index.insert(entry_id, idx);
                entry
            })
            .collect();

        let mut visit_index = HashMap::new();
        let visits: Vec<_> = self
            .visits
            .iter()
            .enumerate()
            .map(|(idx, (visit_id, visit))| {
                visit_index.insert(visit_id, idx);
                visit
            })
            .collect();

        GraphMemorySnapshot {
            entries: entries
                .into_iter()
                .map(|entry| EntrySnapshot {
                    key: entry.key.clone(),
                    payload: entry.payload.clone(),
                    first_seen_at_ms: entry.first_seen_at_ms,
                    last_seen_at_ms: entry.last_seen_at_ms,
                    visit_count: entry.visit_count,
                    privacy: entry.privacy,
                })
                .collect(),
            visits: visits
                .into_iter()
                .map(|visit| VisitSnapshot {
                    entry: entry_index[&visit.entry],
                    parent: visit.parent.map(|parent| visit_index[&parent]),
                    children: visit
                        .children
                        .iter()
                        .map(|child| visit_index[child])
                        .collect(),
                    created_at_ms: visit.created_at_ms,
                    context: visit.context.clone(),
                    inbound: visit.inbound,
                    bindings: visit
                        .bindings
                        .iter()
                        .map(|(owner_id, binding)| BindingSnapshot {
                            owner: owner_index[owner_id],
                            forward_child: binding.forward_child.map(|visit| visit_index[&visit]),
                            last_accessed_at_ms: binding.last_accessed_at_ms,
                        })
                        .collect(),
                })
                .collect(),
            owners: owners
                .into_iter()
                .map(|owner| OwnerSnapshot {
                    identity: owner.identity.clone(),
                    origin: owner.origin.map(|visit| visit_index[&visit]),
                    current: owner.current.map(|visit| visit_index[&visit]),
                    creator: owner.creator.map(|creator| owner_index[&creator]),
                    pending_origin_parent: owner
                        .pending_origin_parent
                        .map(|visit| visit_index[&visit]),
                    owned_visits: owner
                        .owned_visits
                        .iter()
                        .map(|visit| visit_index[visit])
                        .collect(),
                })
                .collect(),
        }
    }
}
