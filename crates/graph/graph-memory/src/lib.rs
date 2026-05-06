// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `graph-memory` — owner-scoped graph memory model.
//!
//! This crate adapts the shared-owner history-tree idea into a graph-oriented
//! memory model for Graphshell:
//!
//! - `Entry` is the deduplicated resource/content identity layer.
//! - `Visit` is a concrete, persisted occurrence in navigation history.
//! - `Owner` is a cursor-bearing actor such as a pane, tab, graph view, or
//!   session.
//! - `EdgeView` is a derived graph projection over visit parentage.
//!
//! The crate deliberately keeps one structural authority: visits own the tree.
//! Edges are projected from visits instead of being stored separately.

use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

new_key_type! { pub struct EntryId; }
new_key_type! { pub struct VisitId; }
new_key_type! { pub struct OwnerId; }

pub trait EntryIdentityKey:
    Clone + Eq + Hash + Debug + Serialize + for<'de> Deserialize<'de>
{
}

impl<T> EntryIdentityKey for T where
    T: Clone + Eq + Hash + Debug + Serialize + for<'de> Deserialize<'de>
{
}

pub trait OwnerIdentity: Clone + Eq + Hash + Debug + Serialize + for<'de> Deserialize<'de> {}

impl<T> OwnerIdentity for T where
    T: Clone + Eq + Hash + Debug + Serialize + for<'de> Deserialize<'de>
{
}

pub trait MemoryPayload: Clone + Debug + Serialize + for<'de> Deserialize<'de> {}

impl<T> MemoryPayload for T where T: Clone + Debug + Serialize + for<'de> Deserialize<'de> {}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub enum EntryPrivacy {
    LocalOnly,
    ShareCandidate,
    Shared,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub enum TransitionKind {
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

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct TransitionRecord {
    pub kind: TransitionKind,
    pub at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct EntryRecord<K: EntryIdentityKey, E: MemoryPayload> {
    pub key: K,
    pub payload: E,
    pub first_seen_at_ms: u64,
    pub last_seen_at_ms: u64,
    pub visit_count: u64,
    pub privacy: EntryPrivacy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerBinding {
    pub forward_child: Option<VisitId>,
    pub last_accessed_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct VisitRecord<X: MemoryPayload> {
    pub entry: EntryId,
    pub parent: Option<VisitId>,
    pub children: Vec<VisitId>,
    pub created_at_ms: u64,
    pub context: X,
    pub inbound: Option<TransitionRecord>,
    pub bindings: HashMap<OwnerId, OwnerBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct OwnerRecord<O: OwnerIdentity> {
    pub identity: O,
    pub origin: Option<VisitId>,
    pub current: Option<VisitId>,
    pub creator: Option<OwnerId>,
    pub pending_origin_parent: Option<VisitId>,
    pub owned_visits: HashSet<VisitId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeView {
    pub from_visit: VisitId,
    pub to_visit: VisitId,
    pub from_entry: EntryId,
    pub to_entry: EntryId,
    pub transition: Option<TransitionKind>,
    pub at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregatedEntryEdgeView {
    pub from_entry: EntryId,
    pub to_entry: EntryId,
    pub traversal_count: u64,
    pub latest_transition_at_ms: u64,
    pub transition_counts: HashMap<TransitionKind, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcReport {
    pub deleted_visits: Vec<VisitId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnerBranchAlternative<E: MemoryPayload> {
    pub visit_id: VisitId,
    pub entry_id: EntryId,
    pub payload: E,
    pub transition: Option<TransitionKind>,
    pub at_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnerBranchVisit<E: MemoryPayload> {
    pub visit_id: VisitId,
    pub entry_id: EntryId,
    pub payload: E,
    pub transition: Option<TransitionKind>,
    pub at_ms: u64,
    pub is_current: bool,
    pub alternate_children: Vec<OwnerBranchAlternative<E>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnerBranchProjection<E: MemoryPayload> {
    pub visits: Vec<OwnerBranchVisit<E>>,
    pub current_index: Option<usize>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Default,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
#[serde(bound = "")]
pub struct GraphMemorySnapshot<K, E, O, X>
where
    K: EntryIdentityKey,
    E: MemoryPayload,
    O: OwnerIdentity,
    X: MemoryPayload,
{
    pub entries: Vec<EntrySnapshot<K, E>>,
    pub visits: Vec<VisitSnapshot<X>>,
    pub owners: Vec<OwnerSnapshot<O>>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
#[serde(bound = "")]
pub struct EntrySnapshot<K: EntryIdentityKey, E: MemoryPayload> {
    pub key: K,
    pub payload: E,
    pub first_seen_at_ms: u64,
    pub last_seen_at_ms: u64,
    pub visit_count: u64,
    pub privacy: EntryPrivacy,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
#[serde(bound = "")]
pub struct VisitSnapshot<X: MemoryPayload> {
    pub entry: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub created_at_ms: u64,
    pub context: X,
    pub inbound: Option<TransitionRecord>,
    pub bindings: Vec<BindingSnapshot>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
#[serde(bound = "")]
pub struct OwnerSnapshot<O: OwnerIdentity> {
    pub identity: O,
    pub origin: Option<usize>,
    pub current: Option<usize>,
    pub creator: Option<usize>,
    pub pending_origin_parent: Option<usize>,
    pub owned_visits: Vec<usize>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct BindingSnapshot {
    pub owner: usize,
    pub forward_child: Option<usize>,
    pub last_accessed_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphMemoryError {
    MissingOwner(OwnerId),
    MissingEntry(EntryId),
    MissingVisit(VisitId),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct GraphMemory<K, E, O, X>
where
    K: EntryIdentityKey,
    E: MemoryPayload,
    O: OwnerIdentity,
    X: MemoryPayload,
{
    entries: SlotMap<EntryId, EntryRecord<K, E>>,
    visits: SlotMap<VisitId, VisitRecord<X>>,
    owners: SlotMap<OwnerId, OwnerRecord<O>>,
    entry_index: HashMap<K, EntryId>,
    owner_index: HashMap<O, OwnerId>,
}

impl<K, E, O, X> Default for GraphMemory<K, E, O, X>
where
    K: EntryIdentityKey,
    E: MemoryPayload,
    O: OwnerIdentity,
    X: MemoryPayload,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, E, O, X> GraphMemory<K, E, O, X>
where
    K: EntryIdentityKey,
    E: MemoryPayload,
    O: OwnerIdentity,
    X: MemoryPayload,
{
    pub fn new() -> Self {
        Self {
            entries: SlotMap::with_key(),
            visits: SlotMap::with_key(),
            owners: SlotMap::with_key(),
            entry_index: HashMap::new(),
            owner_index: HashMap::new(),
        }
    }

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
    ) -> Result<Vec<VisitId>, GraphMemoryError> {
        let owner = self
            .owners
            .get(owner_id)
            .ok_or(GraphMemoryError::MissingOwner(owner_id))?;
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
    ) -> Result<Vec<EntryId>, GraphMemoryError> {
        self.linear_history_visits_of_owner(owner_id)?
            .into_iter()
            .map(|visit_id| {
                self.visits
                    .get(visit_id)
                    .map(|visit| visit.entry)
                    .ok_or(GraphMemoryError::MissingVisit(visit_id))
            })
            .collect()
    }

    pub fn current_index_of_owner(
        &self,
        owner_id: OwnerId,
    ) -> Result<Option<usize>, GraphMemoryError> {
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
    ) -> Result<OwnerBranchProjection<E>, GraphMemoryError> {
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
                    .ok_or(GraphMemoryError::MissingVisit(*visit_id))?;
                let entry = self
                    .entries
                    .get(visit.entry)
                    .ok_or(GraphMemoryError::MissingEntry(visit.entry))?;
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
            .collect::<Result<Vec<_>, GraphMemoryError>>()?;

        Ok(OwnerBranchProjection {
            visits,
            current_index,
        })
    }

    pub fn entry_id_by_key(&self, key: &K) -> Option<EntryId> {
        self.entry_index.get(key).copied()
    }

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
    ) -> Result<VisitId, GraphMemoryError> {
        if !self.owners.contains_key(owner_id) {
            return Err(GraphMemoryError::MissingOwner(owner_id));
        }
        if !self.visits.contains_key(visit_id) {
            return Err(GraphMemoryError::MissingVisit(visit_id));
        }

        let current = self.owners.get(owner_id).and_then(|owner| owner.current);
        if let Some(current_id) = current {
            let mut maybe_forward = None;
            if let Some(current_visit) = self.visits.get(current_id) {
                if current_visit.children.contains(&visit_id) {
                    maybe_forward = Some(visit_id);
                }
            }
            if let Some(forward_child) = maybe_forward {
                if let Some(binding) = self.ensure_binding(current_id, owner_id, at_ms) {
                    binding.forward_child = Some(forward_child);
                }
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
    ) -> Result<VisitId, GraphMemoryError> {
        if !self.owners.contains_key(owner_id) {
            return Err(GraphMemoryError::MissingOwner(owner_id));
        }
        if !self.entries.contains_key(entry_id) {
            return Err(GraphMemoryError::MissingEntry(entry_id));
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

        if let Some(owner) = self.owners.get_mut(owner_id) {
            if owner.origin.is_none() {
                owner.origin = Some(visit_id);
            }
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
    ) -> Result<Option<VisitId>, GraphMemoryError> {
        if !self.owners.contains_key(owner_id) {
            return Err(GraphMemoryError::MissingOwner(owner_id));
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
    ) -> Result<Option<VisitId>, GraphMemoryError> {
        if !self.owners.contains_key(owner_id) {
            return Err(GraphMemoryError::MissingOwner(owner_id));
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

    pub fn delete_owner(&mut self, owner_id: OwnerId) -> Result<GcReport, GraphMemoryError> {
        let owner = self
            .owners
            .remove(owner_id)
            .ok_or(GraphMemoryError::MissingOwner(owner_id))?;
        self.owner_index.remove(&owner.identity);

        let owned_visits: Vec<_> = owner.owned_visits.into_iter().collect();
        for visit_id in &owned_visits {
            if let Some(visit) = self.visits.get_mut(*visit_id) {
                visit.bindings.remove(&owner_id);
            }
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
    ) -> Result<Option<VisitId>, GraphMemoryError> {
        let current = self
            .owners
            .get(owner_id)
            .ok_or(GraphMemoryError::MissingOwner(owner_id))?
            .current;
        let Some(current_id) = current else {
            return Ok(None);
        };
        let current_entry = self
            .visits
            .get(current_id)
            .ok_or(GraphMemoryError::MissingVisit(current_id))?
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
    ) -> Result<(), GraphMemoryError> {
        if !self.owners.contains_key(owner_id) {
            return Err(GraphMemoryError::MissingOwner(owner_id));
        }
        for visit_id in path {
            if !self.visits.contains_key(*visit_id) {
                return Err(GraphMemoryError::MissingVisit(*visit_id));
            }
        }

        let previous_owned = self
            .owners
            .get(owner_id)
            .ok_or(GraphMemoryError::MissingOwner(owner_id))?
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
                .ok_or(GraphMemoryError::MissingVisit(*visit_id))?;
            binding.forward_child = path.get(idx + 1).copied();
        }

        let owner = self
            .owners
            .get_mut(owner_id)
            .ok_or(GraphMemoryError::MissingOwner(owner_id))?;
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

    fn bind_owner_to_visit(
        &mut self,
        owner_id: OwnerId,
        visit_id: VisitId,
        at_ms: u64,
    ) -> Result<(), GraphMemoryError> {
        if !self.owners.contains_key(owner_id) {
            return Err(GraphMemoryError::MissingOwner(owner_id));
        }
        if !self.visits.contains_key(visit_id) {
            return Err(GraphMemoryError::MissingVisit(visit_id));
        }

        let binding = self.ensure_binding(visit_id, owner_id, at_ms);
        if binding.is_none() {
            return Err(GraphMemoryError::MissingVisit(visit_id));
        }

        let owner = self
            .owners
            .get_mut(owner_id)
            .ok_or(GraphMemoryError::MissingOwner(owner_id))?;
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
        Some(visit.bindings.entry(owner_id).or_insert(OwnerBinding {
            forward_child: None,
            last_accessed_at_ms: at_ms,
        }))
        .map(|binding| {
            binding.last_accessed_at_ms = at_ms;
            binding
        })
    }

    fn root_of(&self, visit_id: VisitId) -> Result<VisitId, GraphMemoryError> {
        let mut cursor = visit_id;
        loop {
            let visit = self
                .visits
                .get(cursor)
                .ok_or(GraphMemoryError::MissingVisit(cursor))?;
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
    ) -> Result<(), GraphMemoryError> {
        let subtree = self.collect_subtree(root_id);
        for visit_id in subtree.iter().rev() {
            let visit = self
                .visits
                .remove(*visit_id)
                .ok_or(GraphMemoryError::MissingVisit(*visit_id))?;
            if let Some(entry) = self.entries.get_mut(visit.entry) {
                entry.visit_count = entry.visit_count.saturating_sub(1);
            }
            deleted.push(*visit_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct EntryData {
        url: String,
        title: String,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    struct OwnerName(String);

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct VisitContext {
        label: String,
    }

    type Memory = GraphMemory<(String, Option<String>), EntryData, OwnerName, VisitContext>;

    fn entry(url: &str, title: &str) -> EntryData {
        EntryData {
            url: url.to_string(),
            title: title.to_string(),
        }
    }

    fn owner(name: &str) -> OwnerName {
        OwnerName(name.to_string())
    }

    fn ctx(label: &str) -> VisitContext {
        VisitContext {
            label: label.to_string(),
        }
    }

    #[test]
    fn entry_deduplicates_by_key_but_allows_contextual_duplicates() {
        let mut memory = Memory::new();

        let a1 = memory.resolve_or_create_entry(
            (
                "https://example.com".to_string(),
                Some("workspace-a".to_string()),
            ),
            entry("https://example.com", "First"),
            1,
            EntryPrivacy::LocalOnly,
        );
        let a2 = memory.resolve_or_create_entry(
            (
                "https://example.com".to_string(),
                Some("workspace-a".to_string()),
            ),
            entry("https://example.com", "Updated"),
            2,
            EntryPrivacy::ShareCandidate,
        );
        let b = memory.resolve_or_create_entry(
            (
                "https://example.com".to_string(),
                Some("workspace-b".to_string()),
            ),
            entry("https://example.com", "Parallel"),
            3,
            EntryPrivacy::LocalOnly,
        );

        assert_eq!(a1, a2);
        assert_ne!(a1, b);
        assert_eq!(memory.entry_count(), 2);
        assert_eq!(memory.entry(a1).unwrap().payload.title, "Updated");
        assert_eq!(
            memory.entry(a1).unwrap().privacy,
            EntryPrivacy::ShareCandidate
        );
    }

    #[test]
    fn spawned_owner_origin_attaches_under_creator_current_visit() {
        let mut memory = Memory::new();
        let root_entry = memory.resolve_or_create_entry(
            ("https://a.example".to_string(), None),
            entry("https://a.example", "A"),
            1,
            EntryPrivacy::LocalOnly,
        );
        let child_entry = memory.resolve_or_create_entry(
            ("https://b.example".to_string(), None),
            entry("https://b.example", "B"),
            2,
            EntryPrivacy::LocalOnly,
        );
        let spawned_entry = memory.resolve_or_create_entry(
            ("https://c.example".to_string(), None),
            entry("https://c.example", "C"),
            3,
            EntryPrivacy::LocalOnly,
        );

        let x = memory.ensure_owner(owner("x"), None);
        let _root = memory
            .visit_entry(x, root_entry, ctx("root"), TransitionKind::UrlTyped, 10)
            .unwrap();
        let current = memory
            .visit_entry(x, child_entry, ctx("child"), TransitionKind::LinkClick, 20)
            .unwrap();

        let y = memory.ensure_owner(owner("y"), Some(x));
        let spawned = memory
            .visit_entry(y, spawned_entry, ctx("spawn"), TransitionKind::TabSpawn, 30)
            .unwrap();

        assert_eq!(memory.owner(y).unwrap().creator, Some(x));
        assert_eq!(memory.owner(y).unwrap().origin, Some(spawned));
        assert_eq!(memory.visit(spawned).unwrap().parent, Some(current));
    }

    #[test]
    fn forward_child_is_owner_scoped_on_shared_visit() {
        let mut memory = Memory::new();

        let a = memory.resolve_or_create_entry(
            ("https://a.example".to_string(), None),
            entry("https://a.example", "A"),
            1,
            EntryPrivacy::LocalOnly,
        );
        let b = memory.resolve_or_create_entry(
            ("https://b.example".to_string(), None),
            entry("https://b.example", "B"),
            2,
            EntryPrivacy::LocalOnly,
        );
        let c = memory.resolve_or_create_entry(
            ("https://c.example".to_string(), None),
            entry("https://c.example", "C"),
            3,
            EntryPrivacy::LocalOnly,
        );

        let x = memory.ensure_owner(owner("x"), None);
        let a_visit = memory
            .visit_entry(x, a, ctx("a"), TransitionKind::UrlTyped, 10)
            .unwrap();
        let b_visit = memory
            .visit_entry(x, b, ctx("b"), TransitionKind::LinkClick, 20)
            .unwrap();

        let y = memory.ensure_owner(owner("y"), None);
        memory.adopt_visit(y, a_visit, 25).unwrap();
        let c_visit = memory
            .visit_entry(y, c, ctx("c"), TransitionKind::LinkClick, 30)
            .unwrap();

        memory.back(x, 1, 40).unwrap();
        memory.back(y, 1, 41).unwrap();

        assert_eq!(
            memory
                .visit(a_visit)
                .unwrap()
                .bindings
                .get(&x)
                .unwrap()
                .forward_child,
            Some(b_visit)
        );
        assert_eq!(
            memory
                .visit(a_visit)
                .unwrap()
                .bindings
                .get(&y)
                .unwrap()
                .forward_child,
            Some(c_visit)
        );

        assert_eq!(memory.forward(x, 1, 50).unwrap(), Some(b_visit));
        assert_eq!(memory.forward(y, 1, 51).unwrap(), Some(c_visit));
    }

    #[test]
    fn repeated_navigation_creates_distinct_visits_and_aggregates_edges() {
        let mut memory = Memory::new();

        let a = memory.resolve_or_create_entry(
            ("https://a.example".to_string(), None),
            entry("https://a.example", "A"),
            1,
            EntryPrivacy::LocalOnly,
        );
        let b = memory.resolve_or_create_entry(
            ("https://b.example".to_string(), None),
            entry("https://b.example", "B"),
            2,
            EntryPrivacy::LocalOnly,
        );

        let owner = memory.ensure_owner(owner("x"), None);
        let a_visit = memory
            .visit_entry(owner, a, ctx("a"), TransitionKind::UrlTyped, 10)
            .unwrap();
        let first_b = memory
            .visit_entry(owner, b, ctx("b1"), TransitionKind::LinkClick, 20)
            .unwrap();
        memory.back(owner, 1, 30).unwrap();
        let second_b = memory
            .visit_entry(owner, b, ctx("b2"), TransitionKind::Reload, 40)
            .unwrap();

        assert_ne!(first_b, second_b);
        assert_eq!(memory.visit(second_b).unwrap().parent, Some(a_visit));

        let aggregated = memory.aggregated_entry_edges();
        assert_eq!(aggregated.len(), 1);
        let edge = &aggregated[0];
        assert_eq!(edge.from_entry, a);
        assert_eq!(edge.to_entry, b);
        assert_eq!(edge.traversal_count, 2);
        assert_eq!(
            edge.transition_counts.get(&TransitionKind::LinkClick),
            Some(&1)
        );
        assert_eq!(
            edge.transition_counts.get(&TransitionKind::Reload),
            Some(&1)
        );
    }

    #[test]
    fn owner_branch_projection_reports_alternate_children() {
        let mut memory = Memory::new();

        let a = memory.resolve_or_create_entry(
            ("https://a.example".to_string(), None),
            entry("https://a.example", "A"),
            1,
            EntryPrivacy::LocalOnly,
        );
        let b = memory.resolve_or_create_entry(
            ("https://b.example".to_string(), None),
            entry("https://b.example", "B"),
            2,
            EntryPrivacy::LocalOnly,
        );
        let c = memory.resolve_or_create_entry(
            ("https://c.example".to_string(), None),
            entry("https://c.example", "C"),
            3,
            EntryPrivacy::LocalOnly,
        );

        let owner_id = memory.ensure_owner(owner("x"), None);
        let a_visit = memory
            .visit_entry(owner_id, a, ctx("a"), TransitionKind::UrlTyped, 10)
            .unwrap();
        let b_visit = memory
            .visit_entry(owner_id, b, ctx("b"), TransitionKind::LinkClick, 20)
            .unwrap();

        memory.back(owner_id, 1, 30).unwrap();
        let c_visit = memory
            .visit_entry(owner_id, c, ctx("c"), TransitionKind::Reload, 40)
            .unwrap();

        let projection = memory.owner_branch_projection(owner_id).unwrap();
        assert_eq!(projection.current_index, Some(1));
        assert_eq!(projection.visits.len(), 2);
        assert_eq!(projection.visits[0].visit_id, a_visit);
        assert_eq!(projection.visits[1].visit_id, c_visit);
        assert!(projection.visits[1].is_current);
        assert_eq!(projection.visits[0].alternate_children.len(), 1);
        assert_eq!(projection.visits[0].alternate_children[0].visit_id, b_visit);
        assert_eq!(
            projection.visits[0].alternate_children[0].payload.url,
            "https://b.example"
        );
    }

    #[test]
    fn deleting_last_owner_collects_ownerless_branch() {
        let mut memory = Memory::new();

        let d = memory.resolve_or_create_entry(
            ("https://d.example".to_string(), None),
            entry("https://d.example", "D"),
            1,
            EntryPrivacy::LocalOnly,
        );
        let e = memory.resolve_or_create_entry(
            ("https://e.example".to_string(), None),
            entry("https://e.example", "E"),
            2,
            EntryPrivacy::LocalOnly,
        );

        let owner = memory.ensure_owner(owner("solo"), None);
        let d_visit = memory
            .visit_entry(owner, d, ctx("d"), TransitionKind::UrlTyped, 10)
            .unwrap();
        let e_visit = memory
            .visit_entry(owner, e, ctx("e"), TransitionKind::LinkClick, 20)
            .unwrap();

        let report = memory.delete_owner(owner).unwrap();
        assert!(report.deleted_visits.contains(&d_visit));
        assert!(report.deleted_visits.contains(&e_visit));
        assert_eq!(memory.owner_count(), 0);
        assert_eq!(memory.visit_count(), 0);
    }
}
