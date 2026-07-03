// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `node-lineage` — owner-scoped navigation-lineage model.
//!
//! Adapts the shared-owner history-tree idea (Atlas Engineer's `history-tree`,
//! BSD-3-Clause; see README) into a graph-oriented lineage model for Mere:
//!
//! - `Entry` is the deduplicated resource/content identity layer.
//! - `Visit` is a concrete, persisted occurrence in navigation lineage.
//! - `Owner` is a cursor-bearing actor such as a pane, tab, graph view, or
//!   session.
//! - `EdgeView` is a derived graph projection over visit parentage.
//!
//! The crate deliberately keeps one structural authority: visits own the tree.
//! Edges are projected from visits instead of being stored separately.
//!
//! The lineage concept layers at two granularities:
//!
//! - **url → url** (within-tile, branchable internal lineage): navigating in a
//!   tile extends a visit thread; navigating back and then forward to a
//!   different link spawns a branch in the same tile's visit tree.
//! - **node → node** / **tile → tile** (external lineage on the graph itself):
//!   when a within-tile branch is promoted into its own anchor — assuming an
//!   identity external to the original node or tile — it surfaces as a
//!   directed edge in the canonical graph.
//!
//! Both granularities use the same Entry/Visit/Owner machinery; "promotion to
//! anchor" is the affirmative gesture that crosses the boundary.
//!
//! ## Temporal-integrity contract (R0 invariant)
//!
//! Adopted from the donor `graphshell` history/lineage subsystem (per the
//! [adoption roadmap](../../../../../design_docs/mere_docs/implementation_strategy/2026-05-27_adoption_roadmap.md)
//! R0; the same contract binds the content side in
//! [`eidetic-core`](https://docs.rs/eidetic)). Three invariants, already
//! embodied by this crate's types and named here so they hold as the lineage
//! model grows:
//!
//! 1. **Temporal-integrity** — a [`VisitRecord`] is an append-only occurrence;
//!    the past is never rewritten. Navigating back and then forward to a
//!    different link *branches* (spawns new visits); it does not edit prior
//!    visits.
//! 2. **Replay-isolation** — deriving a projection or replaying lineage *reads*
//!    visits; it never mutates the visit tree. Re-deriving a past view is a
//!    pure read over the visit authority.
//! 3. **Shared-projection** — `EdgeView` (and any "recent" / derived view) is a
//!    projection over the single visit authority, never a second store. This is
//!    the crate's standing rule: visits own the tree; edges are projected.

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
}

mod mutations;
mod queries;
mod snapshot;
#[cfg(test)]
mod tests;
