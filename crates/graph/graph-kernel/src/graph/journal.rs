/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The edit spine: mere's captured-delta stream over the substrate log.
//!
//! The graph's durable mutations already exist as a stream of [`CapturedDelta`]s
//! (stable-id, serializable), emitted through the capture hook as each live
//! [`GraphDelta`](super::apply::GraphDelta) applies (see `capture.rs`). This module
//! gives that stream its principled home: a [`codicil::Codicil`] of captured
//! deltas, the append-only log primitive shared across the Merely apps. The
//! materialized graph is the *replay* of the journal, and because live editing and
//! replay both funnel through `apply_graph_delta`, the two cannot diverge.
//!
//! This is the edit spine over the substrate. mere keeps its own rich edit
//! vocabulary — `CapturedDelta` carries content edits (title, tags, body,
//! navigation) that chartulary's topology-only `GraphEdit` deliberately cannot
//! express — and codicil supplies the ordered, forkable, persistable log beneath
//! it. Checkpointing reuses mere's existing `GraphSnapshot`: load a snapshot, then
//! [`replay_from`](GraphJournal::replay_from) the journal tail past its sequence,
//! mirroring `chartulary::GraphLog::load_checkpointed`.
//!
//! WASM-clean: codicil and muniment are the substrate's portable primitives (the
//! same ones the browser persists through), so this module compiles to
//! `wasm32-unknown-unknown` like the rest of `graph/`.

use std::sync::{Arc, Mutex};

use codicil::{Codicil, LogId, Provenance, Seq};
use muniment::{Backend, Codec, SlotStore, StoreError};

use super::Graph;
use super::capture::{CapturedDelta, replay_captured_deltas, replay_captured_deltas_onto};

/// An append-only journal of a graph's captured edits: the codicil-backed edit
/// spine. The materialized [`Graph`] is the replay of this log; forking it forks
/// the graph's whole history with provenance, and it persists whole through one
/// muniment slot.
#[derive(Clone, Debug, Default)]
pub struct GraphJournal {
    log: Codicil<CapturedDelta>,
}

impl GraphJournal {
    /// A fresh, empty journal.
    pub fn new() -> Self {
        Self::default()
    }

    /// A journal with a stable identity, so it can later be forked with
    /// provenance pointing back at it.
    pub fn with_id(id: LogId) -> Self {
        Self {
            log: Codicil::with_id(id),
        }
    }

    /// Adopt an existing codicil of captured deltas (e.g. one just loaded from a
    /// store or received from a peer).
    pub fn from_log(log: Codicil<CapturedDelta>) -> Self {
        Self { log }
    }

    /// Append one captured edit, returning the [`Seq`] it was stamped with. This
    /// is the write path: a live mutation's capture lands here (usually via
    /// [`journal_capture_hook`]).
    pub fn record(&mut self, delta: CapturedDelta) -> Seq {
        self.log.append(delta)
    }

    /// The underlying codicil, for cursors, replication, and persistence.
    pub fn log(&self) -> &Codicil<CapturedDelta> {
        &self.log
    }

    /// Every captured edit, oldest first.
    pub fn entries(&self) -> &[CapturedDelta] {
        self.log.entries()
    }

    /// The number of edits recorded.
    pub fn len(&self) -> usize {
        self.log.len()
    }

    /// Whether the journal holds no edits.
    pub fn is_empty(&self) -> bool {
        self.log.is_empty()
    }

    /// The [`Seq`] the next recorded edit will receive — a durable cursor a
    /// checkpoint can store to resume replay from.
    pub fn next_seq(&self) -> Seq {
        self.log.next_seq()
    }

    /// This journal's log identity, if any.
    pub fn id(&self) -> Option<&LogId> {
        self.log.id()
    }

    /// Where this journal forked from, if it is a fork.
    pub fn provenance(&self) -> Option<&Provenance> {
        self.log.provenance()
    }

    /// Rebuild the whole graph by replaying every edit from empty.
    pub fn replay(&self) -> Graph {
        replay_captured_deltas(self.log.entries().iter().cloned())
    }

    /// Advance an already-materialized `graph` by the edits from `since` onward.
    /// The checkpoint-plus-tail path: restore a `GraphSnapshot`, then apply only
    /// the journal entries recorded after the snapshot's sequence. The incremental
    /// twin of [`replay`](Self::replay).
    pub fn replay_from(&self, since: Seq, graph: &mut Graph) {
        replay_captured_deltas_onto(graph, self.log.from(since).iter().cloned());
    }

    /// Fork this journal under a new identity: copies the whole edit history and
    /// records where it branched from, then diverges independently. The log-level
    /// mirror of a graph fork; the fork replays to an identical graph and then
    /// edits without touching the source.
    pub fn fork(&self, new_id: LogId) -> Self {
        Self {
            log: self.log.fork(new_id),
        }
    }

    /// Write the whole journal to the muniment slot at `key`.
    pub async fn save<B: Backend, C: Codec>(
        &self,
        slots: &SlotStore<B, C>,
        key: &str,
    ) -> Result<(), StoreError> {
        self.log.save(slots, key).await
    }

    /// Load a journal from the muniment slot at `key`, or an empty journal if the
    /// slot is absent.
    pub async fn load<B: Backend, C: Codec>(
        slots: &SlotStore<B, C>,
        key: &str,
    ) -> Result<Self, StoreError> {
        Ok(Self::from_log(Codicil::load(slots, key).await?))
    }
}

/// A shared journal plus a capture hook that records every emitted
/// [`CapturedDelta`] into it. Install the hook with
/// [`set_captured_delta_hook`](super::set_captured_delta_hook) and keep the handle
/// to replay or persist the journal:
///
/// ```ignore
/// let (journal, hook) = journal_capture_hook();
/// kernel::graph::set_captured_delta_hook(Some(hook));
/// // ... drive live mutations through apply_graph_delta ...
/// let restored = journal.lock().unwrap().replay();
/// ```
///
/// The capture hook is one per-thread slot, so this replaces any previously
/// installed hook. It is offered as the host's codicil-backed persistence path.
pub fn journal_capture_hook() -> (
    Arc<Mutex<GraphJournal>>,
    Arc<dyn Fn(&CapturedDelta) + Send + Sync + 'static>,
) {
    let journal = Arc::new(Mutex::new(GraphJournal::new()));
    let sink = Arc::clone(&journal);
    let hook: Arc<dyn Fn(&CapturedDelta) + Send + Sync + 'static> = Arc::new(move |delta| {
        if let Ok(mut journal) = sink.lock() {
            journal.record(delta.clone());
        }
    });
    (journal, hook)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::apply::{GraphDelta, GraphDeltaResult, apply_graph_delta};
    use crate::graph::set_captured_delta_hook;
    use crate::graph::{EdgeAssertion, SemanticSubKind};
    use euclid::default::Point2D;
    use uuid::Uuid;

    fn add(id: u128, url: &str) -> CapturedDelta {
        CapturedDelta::ReplayAddNodeWithIdIfMissing {
            id: Uuid::from_u128(id).to_string(),
            url: url.to_string(),
            position: [0.0, 0.0],
        }
    }

    /// A key-independent view: sorted node ids and sorted (from, to, kind) triples.
    /// Two graphs are equal iff their fingerprints are, regardless of petgraph keys.
    fn fingerprint(graph: &Graph) -> (Vec<String>, Vec<(String, String, String)>) {
        let mut nodes: Vec<String> = graph.nodes().map(|(_, n)| n.id.to_string()).collect();
        nodes.sort();
        let mut edges: Vec<(String, String, String)> = graph
            .relations()
            .map(|rel| {
                let from = graph.get_node(rel.from).map(|n| n.id.to_string()).unwrap_or_default();
                let to = graph.get_node(rel.to).map(|n| n.id.to_string()).unwrap_or_default();
                (from, to, format!("{:?}", rel.kind))
            })
            .collect();
        edges.sort();
        (nodes, edges)
    }

    #[test]
    fn records_and_replays_reconstruct_the_graph() {
        let mut journal = GraphJournal::new();
        journal.record(add(1, "https://a.test/"));
        journal.record(add(2, "https://b.test/"));
        journal.record(CapturedDelta::ReplayAssertRelationByIds {
            from_id: Uuid::from_u128(1).to_string(),
            to_id: Uuid::from_u128(2).to_string(),
            assertion: EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
        });
        journal.record(CapturedDelta::ReplaySetNodeTitleById {
            node_id: Uuid::from_u128(1).to_string(),
            title: "Paper A".to_string(),
        });

        let graph = journal.replay();
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.relations().count(), 1, "the cites relation replayed");
        let (_, node) = graph.get_node_by_id(Uuid::from_u128(1)).expect("node a present");
        assert_eq!(node.title, "Paper A", "the content edit replayed too");
    }

    #[test]
    fn fork_carries_history_then_diverges() {
        let mut source = GraphJournal::with_id(LogId::new("source"));
        source.record(add(1, "https://a.test/"));
        source.record(add(2, "https://b.test/"));

        let mut fork = source.fork(LogId::new("fork"));
        assert_eq!(fork.entries(), source.entries(), "the fork copies the history");
        let provenance = fork.provenance().expect("a fork has provenance");
        assert_eq!(provenance.source, Some(LogId::new("source")));
        assert_eq!(provenance.at, Seq(source.len() as u64));

        // Diverging the fork leaves the source untouched.
        fork.record(add(3, "https://c.test/"));
        assert_eq!(source.replay().node_count(), 2, "source unchanged");
        assert_eq!(fork.replay().node_count(), 3);
    }

    #[test]
    fn journal_round_trips_through_a_muniment_slot() {
        use muniment::{JsonSlots, MemoryBackend};
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());

            let mut journal = GraphJournal::new();
            journal.record(add(1, "https://a.test/"));
            journal.record(add(2, "https://b.test/"));
            journal.save(&slots, "journal").await.unwrap();

            let reloaded = GraphJournal::load(&slots, "journal").await.unwrap();
            assert_eq!(reloaded.len(), 2, "the log survived the round trip");
            assert_eq!(
                fingerprint(&reloaded.replay()),
                fingerprint(&journal.replay()),
                "the reloaded journal replays to the same graph"
            );
        });
    }

    #[test]
    fn the_capture_hook_feeds_the_journal_and_replay_matches_live() {
        // The edit-spine invariant: a graph built by live mutation and one rebuilt
        // by replaying the journal those mutations produced are identical.
        let (journal, hook) = journal_capture_hook();
        set_captured_delta_hook(Some(hook));

        let mut live = Graph::new();
        let a = match apply_graph_delta(
            &mut live,
            GraphDelta::AddNode {
                id: Some(Uuid::from_u128(1)),
                url: "https://a.test/".to_string(),
                position: Point2D::new(0.0, 0.0),
            },
        ) {
            GraphDeltaResult::NodeAdded(key) => key,
            other => panic!("expected NodeAdded, got {other:?}"),
        };
        let b = match apply_graph_delta(
            &mut live,
            GraphDelta::AddNode {
                id: Some(Uuid::from_u128(2)),
                url: "https://b.test/".to_string(),
                position: Point2D::new(1.0, 0.0),
            },
        ) {
            GraphDeltaResult::NodeAdded(key) => key,
            other => panic!("expected NodeAdded, got {other:?}"),
        };
        apply_graph_delta(
            &mut live,
            GraphDelta::AssertRelation {
                from: a,
                to: b,
                assertion: EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Cites,
                    label: None,
                    decay_progress: None,
                },
            },
        );
        apply_graph_delta(
            &mut live,
            GraphDelta::SetNodeTitle {
                key: a,
                title: "Paper A".to_string(),
            },
        );

        set_captured_delta_hook(None);

        let replayed = journal.lock().unwrap().replay();
        assert_eq!(
            fingerprint(&replayed),
            fingerprint(&live),
            "replay of the captured journal reconstructs the live graph"
        );
        let (_, node) = replayed.get_node_by_id(Uuid::from_u128(1)).expect("node a");
        assert_eq!(node.title, "Paper A");
    }
}
