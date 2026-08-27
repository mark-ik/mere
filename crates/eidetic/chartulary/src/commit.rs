// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The attributed commit seam: every mutation is a [`Batch`] in the journal.
//!
//! B0.5 of the participant gate + packs plan (mere design_docs, 2026-07-17):
//! the journal's entry type is an attributed batch, committed atomically
//! against an expected revision. A petition from any denizen and a keystroke
//! from the trusted UI travel the same path: the UI's convenience mutators on
//! [`GraphLog`] are single-spec commits at the current revision (no conflict
//! possible, no optimistic retry), while a gate calls
//! [`commit_batch`](GraphLog::commit_batch) with the revision it read, and a
//! stale batch is refused wholesale, carrying the current revision to rebase
//! against.
//!
//! Chartulary **attributes**; it does not authenticate. Whether an author may
//! commit what it commits (grants, scopes) is the gate's job, one layer up.
//! App effects (fetch, navigation, windows) cannot roll back, so a consumer
//! enqueues them only after a commit returns, carrying the [`BatchId`];
//! nothing here runs an effect.
//!
//! A batch either applies in full or not at all: the revision is compared and
//! every spec is prechecked against the graph plus the batch's own earlier
//! specs before anything mutates, and the whole batch lands as one journal
//! entry (one slot write on save).

use std::collections::HashSet;

use codicil::{Codicil, LogId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::caps::Identified;
use crate::edit::{DerivationRecord, EdgeId, GraphEdit};
use crate::facet::FacetId;
use crate::spine::GraphLog;

/// An opaque author identity for journal attribution: a denizen id, a personae
/// fingerprint, `"ui"`, or the migration's `"pre-gate"`. Caller-chosen, like
/// [`LogId`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Author(pub String);

impl Author {
    /// Name an author.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identity string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The synthetic author stamped on entries migrated from a pre-gate log.
    pub fn pre_gate() -> Self {
        Self("pre-gate".into())
    }
}

/// A committed batch's identity: its sequence position in the journal. App
/// effects spawned by a petition carry this id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchId(pub u64);

/// One journal entry: an attributed group of edits that applied atomically.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "N: Identified + Serialize, N::Id: Serialize, E: Serialize",
    deserialize = "N: Identified + Deserialize<'de>, N::Id: Deserialize<'de>, E: Deserialize<'de>"
))]
pub struct Batch<N: Identified, E> {
    /// Who committed it.
    pub author: Author,
    /// The edits, in application order.
    pub edits: Vec<GraphEdit<N, E>>,
}

/// What a petitioner submits: an edit without internal bookkeeping. Edge ids
/// are minted by the commit (and returned in [`Committed::edges`]), so a spec
/// is authorable without reading the log's counters.
#[derive(Clone, Debug)]
pub enum EditSpec<N: Identified, E> {
    /// Insert (or upsert, by identity) a node.
    InsertNode(N),
    /// Remove a node and its incident edges.
    RemoveNode(N::Id),
    /// Connect two present nodes; the commit mints the edge id.
    Connect {
        /// The source node.
        from: N::Id,
        /// The target node.
        to: N::Id,
        /// The edge payload.
        edge: E,
    },
    /// Retract a pre-existing edge by its stable id.
    Disconnect(EdgeId),
    /// Record that a node derives from a node in another graph.
    Derive {
        /// The deriving node.
        node: N::Id,
        /// Where it came from.
        from: DerivationRecord<N::Id>,
    },
    /// Set one independently mergeable facet on a live node.
    SetFacet {
        /// The node carrying the facet.
        node: N::Id,
        /// The stable, namespaced facet identity.
        facet: FacetId,
        /// The host-validated facet value.
        value: Value,
    },
    /// Remove one independently mergeable facet from a live node.
    RemoveFacet {
        /// The node carrying the facet.
        node: N::Id,
        /// The stable, namespaced facet identity.
        facet: FacetId,
    },
}

/// Why a commit was refused. Nothing applied.
#[derive(Clone, Debug, PartialEq)]
pub enum CommitError<Id> {
    /// `expected` did not match the current revision; carries the current
    /// revision so the petitioner can rebase and resubmit.
    RevisionConflict {
        /// The revision the graph is actually at.
        current: u64,
    },
    /// A spec referenced a node that is neither live nor inserted earlier in
    /// the batch.
    UnknownNode(Id),
    /// A `Disconnect` referenced an edge that is not live (or was already
    /// disconnected earlier in the batch).
    UnknownEdge(EdgeId),
    /// A node tried to bear the graph it lives in (see
    /// [`commit_bearing_batch`](GraphLog::commit_bearing_batch)).
    SelfBearing(LogId),
}

/// A successful commit's receipt: the batch id plus the edge ids minted for
/// the batch's connects, in spec order.
#[derive(Clone, Debug, PartialEq)]
pub struct Committed {
    /// The journal position the batch landed at.
    pub batch: BatchId,
    /// Minted edge ids, one per `Connect` spec, in order.
    pub edges: Vec<EdgeId>,
}

impl<N, E> GraphLog<N, E>
where
    N: Identified + Clone,
    E: Clone,
{
    /// Commit an attributed batch against an expected revision. The whole
    /// batch applies atomically or not at all: the revision is compared and
    /// every spec prechecked (against the live graph plus the batch's own
    /// earlier specs) before anything mutates.
    pub fn commit_batch(
        &mut self,
        author: Author,
        expected: u64,
        specs: Vec<EditSpec<N, E>>,
    ) -> Result<Committed, CommitError<N::Id>> {
        let current = self.revision();
        if expected != current {
            return Err(CommitError::RevisionConflict { current });
        }

        // Precheck: batch-local view of node presence and edge liveness.
        let mut added: HashSet<N::Id> = HashSet::new();
        let mut removed: HashSet<N::Id> = HashSet::new();
        let mut dropped_edges: HashSet<EdgeId> = HashSet::new();
        let present =
            |id: &N::Id, added: &HashSet<N::Id>, removed: &HashSet<N::Id>, this: &Self| {
                !removed.contains(id) && (added.contains(id) || this.graph().key_of(id).is_some())
            };
        for spec in &specs {
            match spec {
                EditSpec::InsertNode(node) => {
                    removed.remove(node.id());
                    added.insert(node.id().clone());
                }
                EditSpec::RemoveNode(id) => {
                    if !present(id, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(id.clone()));
                    }
                    added.remove(id);
                    removed.insert(id.clone());
                }
                EditSpec::Connect { from, to, .. } => {
                    if !present(from, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(from.clone()));
                    }
                    if !present(to, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(to.clone()));
                    }
                }
                EditSpec::Disconnect(id) => {
                    if dropped_edges.contains(id) || self.edge_key(*id).is_none() {
                        return Err(CommitError::UnknownEdge(*id));
                    }
                    dropped_edges.insert(*id);
                }
                EditSpec::Derive { node, .. } => {
                    if !present(node, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(node.clone()));
                    }
                }
                EditSpec::SetFacet { node, .. } | EditSpec::RemoveFacet { node, .. } => {
                    if !present(node, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(node.clone()));
                    }
                }
            }
        }

        // Lower: mint edge ids for connects, in spec order.
        let mut edits = Vec::with_capacity(specs.len());
        let mut minted = Vec::new();
        for spec in specs {
            edits.push(match spec {
                EditSpec::InsertNode(node) => GraphEdit::InsertNode(node),
                EditSpec::RemoveNode(id) => GraphEdit::RemoveNode(id),
                EditSpec::Connect { from, to, edge } => {
                    let id = EdgeId::new(self.writer, self.next_edge);
                    self.next_edge += 1;
                    minted.push(id);
                    GraphEdit::Connect { id, from, to, edge }
                }
                EditSpec::Disconnect(id) => GraphEdit::Disconnect(id),
                EditSpec::Derive { node, from } => GraphEdit::Derive { node, from },
                EditSpec::SetFacet { node, facet, value } => {
                    GraphEdit::SetFacet { node, facet, value }
                }
                EditSpec::RemoveFacet { node, facet } => GraphEdit::RemoveFacet { node, facet },
            });
        }

        // Apply, then append as one journal entry.
        let batch = Batch { author, edits };
        self.apply_batch(&batch);
        let seq = self.log.append(batch);
        Ok(Committed {
            batch: BatchId(seq.index() as u64),
            edges: minted,
        })
    }
}

/// Wrap a pre-gate edit log (entries are bare [`GraphEdit`]s) as an attributed
/// batch log: one single-edit batch per entry, authored
/// [`Author::pre_gate`]. The log's identity carries over; fork provenance
/// cannot (codicil exposes no parts-constructor), which open question 7 of the
/// plan records.
pub fn migrate_pre_gate<N, E>(legacy: Codicil<GraphEdit<N, E>>) -> Codicil<Batch<N, E>>
where
    N: Identified + Clone,
    E: Clone,
{
    let mut log = match legacy.id() {
        Some(id) => Codicil::with_id(id.clone()),
        None => Codicil::new(),
    };
    for edit in legacy.entries() {
        log.append(Batch {
            author: Author::pre_gate(),
            edits: vec![edit.clone()],
        });
    }
    log
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{Container, Relation};
    use crate::edit::WriterId;
    use crate::facet::FacetId;
    use crate::taxonomy::{Recognized, RelationClass};
    use muniment::{JsonSlots, MemoryBackend};
    use serde_json::json;

    fn cites() -> Relation {
        Relation::new(RelationClass::recognized(Recognized::Cites))
    }

    fn gate() -> Author {
        Author::new("servitor.trail-keeper")
    }

    fn seeded() -> GraphLog<Container, Relation> {
        let ui = Author::new("ui");
        let mut log = GraphLog::new();
        log.insert_node(&ui, Container::new("a"));
        log.insert_node(&ui, Container::new("b"));
        log
    }

    fn writer(tag: u8) -> WriterId {
        WriterId([tag; 32])
    }

    /// M0/M1 of the commons multi-writer convergence plan (mere design_docs,
    /// 2026-07-26). Through 0.1.x `EdgeId` was a bare per-log counter, so two
    /// partitioned replicas both minted `EdgeId(0)` for unrelated edges. The
    /// identity is now `(writer, counter)`, and this asserts the ids differ.
    #[test]
    fn two_offline_writers_mint_distinct_edge_ids() {
        let alice = Author::new("alice");
        let bob = Author::new("bob");

        // Two replicas of one container, edited while partitioned.
        let mut a = GraphLog::<Container, Relation>::new().for_writer(writer(0xa1));
        a.insert_node(&alice, Container::new("a"));
        a.insert_node(&alice, Container::new("b"));
        let a_edge = a
            .connect(&alice, &"a".to_string(), &"b".to_string(), cites())
            .expect("alice connects her own pair");

        let mut b = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
        b.insert_node(&bob, Container::new("c"));
        b.insert_node(&bob, Container::new("d"));
        let b_edge = b
            .connect(&bob, &"c".to_string(), &"d".to_string(), cites())
            .expect("bob connects his own pair");

        assert_ne!(
            a_edge, b_edge,
            "distinct replicas mint distinct ids for distinct edges"
        );
        assert_eq!(
            a_edge.counter, b_edge.counter,
            "the counters still collide; the writer half is what separates them"
        );
    }

    /// The payoff: merging both journals leaves every edge addressable, so a
    /// retraction names exactly one edge.
    #[test]
    fn a_merged_journal_addresses_both_writers_edges() {
        let alice = Author::new("alice");
        let bob = Author::new("bob");

        let mut a = GraphLog::<Container, Relation>::new().for_writer(writer(0xa1));
        a.insert_node(&alice, Container::new("a"));
        a.insert_node(&alice, Container::new("b"));
        let a_edge = a
            .connect(&alice, &"a".to_string(), &"b".to_string(), cites())
            .expect("alice connects");

        let mut b = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
        b.insert_node(&bob, Container::new("c"));
        b.insert_node(&bob, Container::new("d"));
        let b_edge = b
            .connect(&bob, &"c".to_string(), &"d".to_string(), cites())
            .expect("bob connects");

        // Merge: one journal carrying both writers' batches, in the deterministic
        // order the replication layer would impose. Ids are carried in the
        // `Connect` edits, so replay reuses them rather than re-minting.
        let mut merged = codicil::Codicil::new();
        for batch in a.log().entries().iter().chain(b.log().entries()) {
            merged.append(batch.clone());
        }
        let merged = GraphLog::<Container, Relation>::replay(merged);

        assert_eq!(
            merged.graph().edge_count(),
            2,
            "both edges survive the merge"
        );
        let a_key = merged
            .edge_key(a_edge)
            .expect("alice's edge is addressable");
        let b_key = merged.edge_key(b_edge).expect("bob's edge is addressable");
        assert_ne!(a_key, b_key, "each id names a different edge");
    }

    /// Replaying a peer's edits must not consume this replica's counter range,
    /// or the next local mint would reuse an id the peer already used.
    #[test]
    fn replaying_a_peers_edits_does_not_advance_our_counter() {
        let alice = Author::new("alice");
        let bob = Author::new("bob");

        let mut b = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
        b.insert_node(&bob, Container::new("c"));
        b.insert_node(&bob, Container::new("d"));
        for _ in 0..5 {
            b.connect(&bob, &"c".to_string(), &"d".to_string(), cites())
                .expect("bob connects repeatedly");
        }

        // Alice replays bob's whole journal, then mints her own edge.
        let mut merged = codicil::Codicil::new();
        for batch in b.log().entries() {
            merged.append(batch.clone());
        }
        let mut a = GraphLog::<Container, Relation>::replay(merged).for_writer(writer(0xa1));
        a.insert_node(&alice, Container::new("a"));
        a.insert_node(&alice, Container::new("b"));
        let a_edge = a
            .connect(&alice, &"a".to_string(), &"b".to_string(), cites())
            .expect("alice connects");

        assert_eq!(
            a_edge,
            EdgeId::new(writer(0xa1), 0),
            "alice starts her own range at zero, undisturbed by bob's five edges"
        );
    }

    /// Merge two journals in a stated order, as the replication layer's
    /// deterministic `(verifying_key, log_id, seq_num)` sort would.
    fn merge(
        first: &GraphLog<Container, Relation>,
        second: &GraphLog<Container, Relation>,
    ) -> GraphLog<Container, Relation> {
        let mut merged = codicil::Codicil::new();
        for batch in first.log().entries().iter().chain(second.log().entries()) {
            merged.append(batch.clone());
        }
        GraphLog::replay(merged)
    }

    /// Merge two replicas that diverged from `base`: the shared prefix once,
    /// then each side's own tail.
    ///
    /// Concatenating two full journals instead would replay the shared history
    /// **twice**, and the second copy re-applies inserts the first side had
    /// already removed. A partition shares a common ancestor; only the tails
    /// are concurrent.
    fn merge_divergent(
        base: &GraphLog<Container, Relation>,
        first: &GraphLog<Container, Relation>,
        second: &GraphLog<Container, Relation>,
    ) -> GraphLog<Container, Relation> {
        let shared = base.log().entries().len();
        let mut merged = codicil::Codicil::new();
        for batch in base
            .log()
            .entries()
            .iter()
            .chain(first.log().entries().iter().skip(shared))
            .chain(second.log().entries().iter().skip(shared))
        {
            merged.append(batch.clone());
        }
        GraphLog::replay(merged)
    }

    /// M2 of the commons multi-writer convergence plan: **remove wins over a
    /// concurrent connect**, and it does so in *either* merge order, so the
    /// outcome does not depend on which writer's key sorts first.
    ///
    /// Order matters to the mechanism but not the result. Remove-then-connect
    /// drops the connect, because `Connect` only lands when both endpoints are
    /// present. Connect-then-remove lands the edge and then reaps it, because
    /// `RemoveNode` takes incident edges with it. The two paths converge.
    #[test]
    fn remove_wins_over_a_concurrent_connect_in_either_order() {
        let alice = Author::new("alice");
        let bob = Author::new("bob");

        // Shared history both replicas already had, then they partition.
        let mut base = GraphLog::<Container, Relation>::new();
        base.insert_node(&alice, Container::new("a"));
        base.insert_node(&alice, Container::new("b"));

        let mut remover = GraphLog::replay(base.log().clone()).for_writer(writer(0xa1));
        remover.remove_node(&alice, &"a".to_string());

        let mut connector = GraphLog::replay(base.log().clone()).for_writer(writer(0xb2));
        connector
            .connect(&bob, &"a".to_string(), &"b".to_string(), cites())
            .expect("bob connects before he learns of the removal");

        for (label, merged) in [
            (
                "remove then connect",
                merge_divergent(&base, &remover, &connector),
            ),
            (
                "connect then remove",
                merge_divergent(&base, &connector, &remover),
            ),
        ] {
            assert!(
                merged.graph().key_of(&"a".to_string()).is_none(),
                "{label}: the removed node stays removed"
            );
            assert_eq!(
                merged.graph().edge_count(),
                0,
                "{label}: the concurrent edge does not survive the removal"
            );
        }
    }

    /// M2: a concurrent edit to one node is **whole-node last-writer-wins**
    /// under the merge order, and the loser's payload is discarded entirely
    /// rather than merged field-by-field.
    ///
    /// This pins the coarseness, not just the convergence. Per-facet merge is
    /// the one-node facets lane's work; until a facet write is its own edit,
    /// concurrent edits to unrelated parts of a node still cost one of them.
    #[test]
    fn concurrent_node_edits_are_whole_node_last_writer_wins() {
        let alice = Author::new("alice");
        let bob = Author::new("bob");

        let mut a = GraphLog::<Container, Relation>::new().for_writer(writer(0xa1));
        a.insert_node(&alice, Container::new("n").with_title("alice's title"));

        let mut b = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
        b.insert_node(&bob, Container::new("n").with_tag("bob-only-tag"));

        let merged = merge(&a, &b);
        let node = merged
            .graph()
            .node(merged.graph().key_of(&"n".to_string()).expect("node n"))
            .expect("node payload");

        assert!(
            node.tags.iter().any(|t| t == "bob-only-tag"),
            "the last writer's payload survives"
        );
        assert_eq!(
            node.title, "",
            "the earlier writer's title is DISCARDED, not merged: whole-node LWW"
        );
    }

    #[test]
    fn concurrent_edits_to_different_facets_both_survive() {
        let alice = Author::new("alice");
        let bob = Author::new("bob");
        let node = "n".to_string();
        let title = FacetId::new("content.title");
        let pinned = FacetId::new("arrangement.pin");

        let mut base = GraphLog::<Container, Relation>::new();
        base.insert_node(&alice, Container::new(node.clone()));
        let mut a = GraphLog::replay(base.log().clone()).for_writer(writer(0xa1));
        let mut b = GraphLog::replay(base.log().clone()).for_writer(writer(0xb2));
        assert!(a.set_facet(&alice, &node, title.clone(), json!("Alice")));
        assert!(b.set_facet(&bob, &node, pinned.clone(), json!(true)));

        for merged in [
            merge_divergent(&base, &a, &b),
            merge_divergent(&base, &b, &a),
        ] {
            assert_eq!(merged.facets().get(&node, &title), Some(&json!("Alice")));
            assert_eq!(merged.facets().get(&node, &pinned), Some(&json!(true)));
        }
    }

    #[test]
    fn node_removal_suppresses_a_concurrent_facet_edit_in_either_order() {
        let alice = Author::new("alice");
        let bob = Author::new("bob");
        let node = "n".to_string();
        let title = FacetId::new("content.title");

        let mut base = GraphLog::<Container, Relation>::new();
        base.insert_node(&alice, Container::new(node.clone()));
        let mut remover = GraphLog::replay(base.log().clone()).for_writer(writer(0xa1));
        remover.remove_node(&alice, &node);
        let mut editor = GraphLog::replay(base.log().clone()).for_writer(writer(0xb2));
        assert!(editor.set_facet(&bob, &node, title.clone(), json!("gone")));

        for merged in [
            merge_divergent(&base, &remover, &editor),
            merge_divergent(&base, &editor, &remover),
        ] {
            assert!(merged.graph().key_of(&node).is_none());
            assert!(merged.facets().get(&node, &title).is_none());
        }
    }

    /// M2, the case the plan's "remove wins" phrasing does **not** cover, and
    /// which is recorded as open rather than silently decided.
    ///
    /// Remove-versus-*connect* is commutative (above). Remove-versus-*insert*
    /// is not: `InsertNode` is an upsert, so whichever of the two sorts later
    /// wins, and a concurrent insert that sorts after a removal **resurrects
    /// the node**. Both peers agree, so this converges; it is the *choice*
    /// that is unmade, because with no causality recorded a concurrent insert
    /// is indistinguishable from a deliberate re-creation after the delete.
    #[test]
    fn a_concurrent_insert_that_sorts_later_resurrects_a_removed_node() {
        let alice = Author::new("alice");
        let bob = Author::new("bob");

        let mut remover = GraphLog::<Container, Relation>::new().for_writer(writer(0xa1));
        remover.insert_node(&alice, Container::new("a"));
        remover.remove_node(&alice, &"a".to_string());

        let mut editor = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
        editor.insert_node(&bob, Container::new("a").with_title("still editing"));

        assert!(
            merge(&remover, &editor)
                .graph()
                .key_of(&"a".to_string())
                .is_some(),
            "insert sorted last: the node is back (current behavior, decision open)"
        );
        assert!(
            merge(&editor, &remover)
                .graph()
                .key_of(&"a".to_string())
                .is_none(),
            "remove sorted last: the node is gone"
        );
    }

    /// A 0.1.x journal wrote `EdgeId` as a bare integer. Those stores still
    /// load, as the single-writer graphs they always were.
    #[test]
    fn a_legacy_bare_counter_deserializes_as_a_local_id() {
        let legacy: EdgeId = serde_json::from_str("7").expect("bare counter still reads");
        assert_eq!(legacy, EdgeId::local(7));
        assert!(legacy.writer.is_local());

        let current: EdgeId =
            serde_json::from_str(&serde_json::to_string(&EdgeId::new(writer(0xa1), 3)).unwrap())
                .expect("current form round-trips");
        assert_eq!(current, EdgeId::new(writer(0xa1), 3));
    }

    #[test]
    fn stale_revision_is_refused_with_the_current_revision() {
        let mut log = seeded();
        let read = log.revision();
        log.insert_node(&Author::new("ui"), Container::new("c")); // concurrent edit

        let err = log
            .commit_batch(gate(), read, vec![EditSpec::RemoveNode("a".to_string())])
            .unwrap_err();
        assert_eq!(
            err,
            CommitError::RevisionConflict { current: read + 1 },
            "the refusal carries the revision to rebase against"
        );
        assert!(
            log.graph().key_of(&"a".to_string()).is_some(),
            "nothing applied"
        );
    }

    #[test]
    fn a_batch_applies_atomically_or_not_at_all() {
        let mut log = seeded();
        let before = (log.graph().node_count(), log.revision());

        // The second spec references an unknown node: the whole batch refuses.
        let err = log
            .commit_batch(
                gate(),
                log.revision(),
                vec![
                    EditSpec::InsertNode(Container::new("c")),
                    EditSpec::Connect {
                        from: "c".to_string(),
                        to: "ghost".to_string(),
                        edge: cites(),
                    },
                ],
            )
            .unwrap_err();
        assert_eq!(err, CommitError::UnknownNode("ghost".to_string()));
        assert_eq!(
            (log.graph().node_count(), log.revision()),
            before,
            "the insert earlier in the refused batch did not apply either"
        );
    }

    #[test]
    fn a_committed_batch_reads_back_attributed_with_minted_edges() {
        let mut log = seeded();
        let committed = log
            .commit_batch(
                gate(),
                log.revision(),
                vec![
                    EditSpec::InsertNode(Container::new("c")),
                    EditSpec::Connect {
                        from: "a".to_string(),
                        to: "c".to_string(),
                        edge: cites(),
                    },
                ],
            )
            .unwrap();

        assert_eq!(committed.edges.len(), 1, "one connect, one minted id");
        assert!(log.edge_key(committed.edges[0]).is_some());
        let entry = &log.log().entries()[committed.batch.0 as usize];
        assert_eq!(entry.author, gate(), "the journal knows who committed");
        assert_eq!(entry.edits.len(), 2, "the batch is one journal entry");
    }

    #[test]
    fn batch_local_inserts_are_connectable_and_removable() {
        let mut log = seeded();
        let committed = log.commit_batch(
            gate(),
            log.revision(),
            vec![
                EditSpec::InsertNode(Container::new("c")),
                EditSpec::Connect {
                    from: "c".to_string(),
                    to: "b".to_string(),
                    edge: cites(),
                },
                EditSpec::RemoveNode("c".to_string()),
            ],
        );
        assert!(committed.is_ok(), "batch-local presence tracking allows it");
        assert!(log.graph().key_of(&"c".to_string()).is_none());
        assert_eq!(log.graph().edge_count(), 0, "the reap took the edge");
    }

    #[test]
    fn effects_enqueue_only_after_a_commit_lands() {
        // The consumer discipline in miniature: effects carry the batch id and
        // never enqueue on refusal.
        let mut log = seeded();
        let mut effects: Vec<(BatchId, &str)> = Vec::new();

        if let Ok(committed) =
            log.commit_batch(gate(), 999, vec![EditSpec::RemoveNode("a".to_string())])
        {
            effects.push((committed.batch, "navigate"));
        }
        assert!(effects.is_empty(), "a refused petition spawns no effects");

        let committed = log
            .commit_batch(
                gate(),
                log.revision(),
                vec![EditSpec::RemoveNode("a".to_string())],
            )
            .unwrap();
        effects.push((committed.batch, "navigate"));
        assert_eq!(effects, vec![(committed.batch, "navigate")]);
    }

    #[test]
    fn a_pre_gate_edit_log_migrates_on_load() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());

            // A legacy log: bare GraphEdit entries, as B0-era code wrote them.
            let mut legacy: Codicil<GraphEdit<Container, Relation>> =
                Codicil::with_id(codicil::LogId::new("old"));
            legacy.append(GraphEdit::InsertNode(Container::new("a")));
            legacy.append(GraphEdit::InsertNode(Container::new("b")));
            legacy.append(GraphEdit::Connect {
                id: EdgeId::local(0),
                from: "a".to_string(),
                to: "b".to_string(),
                edge: cites(),
            });
            legacy.save(&slots, "log").await.unwrap();

            let loaded = GraphLog::<Container, Relation>::load_full(&slots, "log")
                .await
                .unwrap();
            assert_eq!(loaded.graph().node_count(), 2);
            assert_eq!(loaded.graph().edge_count(), 1);
            assert_eq!(
                loaded.id(),
                Some(&codicil::LogId::new("old")),
                "identity carried"
            );
            assert_eq!(loaded.revision(), 3, "one batch per legacy edit");
            assert!(
                loaded
                    .log()
                    .entries()
                    .iter()
                    .all(|b| b.author == Author::pre_gate()),
                "migrated entries carry the synthetic pre-gate author"
            );

            // And the next save writes the batch format.
            loaded.save_log(&slots, "log").await.unwrap();
            let reloaded = GraphLog::<Container, Relation>::load_full(&slots, "log")
                .await
                .unwrap();
            assert_eq!(reloaded.revision(), 3);
        });
    }
}
