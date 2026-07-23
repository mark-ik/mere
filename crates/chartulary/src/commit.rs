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

use crate::caps::Identified;
use crate::edit::{DerivationRecord, EdgeId, GraphEdit};
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
                    let id = EdgeId(self.next_edge);
                    self.next_edge += 1;
                    minted.push(id);
                    GraphEdit::Connect { id, from, to, edge }
                }
                EditSpec::Disconnect(id) => GraphEdit::Disconnect(id),
                EditSpec::Derive { node, from } => GraphEdit::Derive { node, from },
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
    use crate::taxonomy::{Recognized, RelationClass};
    use muniment::{JsonSlots, MemoryBackend};

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
                id: EdgeId(0),
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
