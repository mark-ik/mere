//! stemma — an owner-scoped lineage model: the descent of entries through
//! branching visits.
//!
//! A stemma (as in *stemma codicum*, the family tree of manuscript copies) is a
//! record of descent. This crate is that record for a graph's content: which
//! things were engaged, in what order, branching where history diverged, shared
//! across the cursors that walked them. It is chartulary's lineage layer, a
//! projection over the edit spine and never a second store.
//!
//! - [`Entry`](EntryRecord) is the deduplicated content-identity layer; many
//!   visits can resolve to one entry.
//! - [`Visit`](VisitRecord) is a concrete, persisted occurrence with a parent and
//!   zero or more children: the branching unit.
//! - [`Owner`](OwnerRecord) is a cursor-bearing actor (a view, a session) that
//!   tracks an origin and a current visit and owns a set of visits.
//! - [`EdgeView`] is a derived projection over visit parentage.
//!
//! One structural authority: **visits own the tree; edges are projected from
//! visits, never stored separately.** The model is generic over the entry key
//! `K`, the entry payload `E`, the owner identity `O`, and the per-visit context
//! `X`. The one browser-flavored remnant of its origin is [`TransitionKind`],
//! whose variants name navigation events; generalizing it (to a type parameter or
//! a neutral vocabulary) is the open naming decision for this crate.
//!
//! ## Temporal-integrity contract (R0 invariant)
//!
//! Three invariants, embodied by the types and named so they hold as the model
//! grows. Per the substrate plan these become the contract of the whole graph
//! spine, not lineage's alone:
//!
//! 1. **Temporal-integrity** — a [`VisitRecord`] is an append-only occurrence;
//!    the past is never rewritten. Diverging (going back, then forward to a
//!    different child) *branches*, spawning new visits; it does not edit prior
//!    ones.
//! 2. **Replay-isolation** — deriving a projection or replaying lineage *reads*
//!    visits; it never mutates the tree. Re-deriving a past view is a pure read.
//! 3. **Shared-projection** — [`EdgeView`] (and any derived view) is a projection
//!    over the single visit authority, never a second store.
//!
//! ## Credit
//!
//! The Entry / Visit / Owner data model is adapted from Atlas Engineer's
//! [`history-tree`](https://github.com/atlas-engineer/history-tree) (Common Lisp,
//! BSD-3-Clause), written for the Nyxt browser. This is an independent Rust
//! reimplementation against the same abstract data model; no `history-tree`
//! source was translated. The conceptual debt is real and acknowledged.

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
/// How an owner *moved* to a visit: navigation vocabulary.
///
/// Generalizing this is the module doc's declared open decision. Two findings
/// from 2026-07-26, weighing woodshed's practice history against this crate:
///
/// 1. **If it is generalized, make it a type parameter**, the first option the
///    module doc lists — not an open `Open { kind: String }` tail. A parameter
///    keeps `Copy`, `Hash`, and the rkyv derives per instantiation, which
///    `AggregatedEntryEdgeView`'s `HashMap<TransitionKind, u64>` key and the
///    `visit.inbound.map(|r| r.kind)` reads all rely on; a `String` variant
///    costs every one of them. Mere's consumers would instantiate the browser
///    vocabulary and read exactly as they do now.
/// 2. **An app's engagement kinds are not a reason to do it.** These variants
///    answer "how did you get here", and `eidetic-core` maps them one-to-one
///    onto a browsing `TraceTransition`. Woodshed's previewed / staged /
///    rehearsed / completed answer "what did you do with it", which is a
///    different question — and the slot for it already exists: the generic
///    per-visit `context: X` on [`VisitRecord`], typed, aggregated over
///    [`Stemma::edge_views`] when the built-in per-kind counts do not fit. So
///    that consumer pull does not reach this enum.
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
pub struct StemmaSnapshot<K, E, O, X>
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
pub enum StemmaError {
    MissingOwner(OwnerId),
    MissingEntry(EntryId),
    MissingVisit(VisitId),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Stemma<K, E, O, X>
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

impl<K, E, O, X> Default for Stemma<K, E, O, X>
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

impl<K, E, O, X> Stemma<K, E, O, X>
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
