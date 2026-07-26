//! The edit spine: an event-sourced graph over codicil.
//!
//! [`GraphLog`] is the write side of the graph. Every mutation travels one
//! path: build an attributed [`Batch`], apply it to the materialized graph,
//! append it to a [`Codicil`]. The graph is the replay of the log, and because
//! live editing and replay share the same apply, they cannot diverge. The
//! convenience mutators here are single-spec commits at the current revision
//! (the trusted-UI path: attributed, no optimistic retry); a gate commits
//! multi-spec petitions through
//! [`commit_batch`](GraphLog::commit_batch) in [`commit`](crate::commit).
//!
//! Loading has two paths, and they agree:
//! - [`load_full`](GraphLog::load_full): read the log, replay every batch.
//! - [`load_checkpointed`](GraphLog::load_checkpointed): read a snapshot, then
//!   replay only the batches after it.
//!
//! Both migrate pre-gate logs (bare [`GraphEdit`] entries) on load, wrapping
//! each edit as a single-edit batch authored
//! [`Author::pre_gate`](crate::commit::Author::pre_gate).
//!
//! A snapshot is a compacted materialization (the nodes and edges as they
//! stand, plus the edge-id bookkeeping and the log length it reflects), stored
//! in a muniment slot. It is an optimization; the log is the authority.

use std::collections::HashMap;

use codicil::{Codicil, LogId, Provenance, Seq};
use muniment::{Backend, Codec, SlotStore, StoreError};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::caps::Identified;
use crate::commit::{Author, Batch, EditSpec};
use crate::edit::{DerivationRecord, EdgeId, GraphEdit, WriterId};
use crate::graph::{EdgeKey, Graph, NodeKey};

/// An event-sourced graph: a materialized [`Graph`] plus the [`Codicil`] of
/// attributed batches that produced it.
pub struct GraphLog<N: Identified, E> {
    pub(crate) graph: Graph<N, E>,
    pub(crate) log: Codicil<Batch<N, E>>,
    pub(crate) by_edge_id: HashMap<EdgeId, EdgeKey>,
    pub(crate) by_edge_key: HashMap<EdgeKey, EdgeId>,
    pub(crate) derivations: HashMap<N::Id, Vec<DerivationRecord<N::Id>>>,
    /// This replica's identity, the writer half of every id it mints.
    pub(crate) writer: WriterId,
    /// This replica's own counter. Advanced only past ids **this** writer
    /// minted, so replaying another replica's edits never consumes our range.
    pub(crate) next_edge: u64,
}

/// A compacted materialization of a [`GraphLog`], for checkpointed load.
#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "N: Identified + Serialize, N::Id: Serialize, E: Serialize",
    deserialize = "N: Identified + Deserialize<'de>, N::Id: Deserialize<'de>, E: Deserialize<'de>"
))]
struct Snapshot<N: Identified, E> {
    nodes: Vec<N>,
    edges: Vec<(EdgeId, N::Id, N::Id, E)>,
    derivations: Vec<(N::Id, DerivationRecord<N::Id>)>,
    /// Absent in 0.1.x snapshots, which were single-writer.
    #[serde(default)]
    writer: WriterId,
    next_edge: u64,
    /// The number of log entries baked into this snapshot; replay resumes here.
    at_seq: u64,
}

impl<N: Identified, E> Default for GraphLog<N, E> {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
            log: Codicil::new(),
            by_edge_id: HashMap::new(),
            by_edge_key: HashMap::new(),
            derivations: HashMap::new(),
            writer: WriterId::LOCAL,
            next_edge: 0,
        }
    }
}

impl<N: Identified, E> GraphLog<N, E> {
    /// A fresh, empty log-backed graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// A fresh, empty log-backed graph with a stable log identity, so it can later
    /// be forked with provenance pointing back at it.
    pub fn with_id(id: LogId) -> Self {
        let mut this = Self::new();
        this.log = Codicil::with_id(id);
        this
    }

    /// Declare which replica this is, so the ids it mints cannot collide with
    /// another replica's.
    ///
    /// Required for any container with more than one writer; a graph left at
    /// [`WriterId::LOCAL`] is asserting it is the only one. Bind `writer` to
    /// the replication identity, not to an [`Author`] label.
    pub fn for_writer(mut self, writer: WriterId) -> Self {
        self.writer = writer;
        self
    }

    /// This replica's identity.
    pub fn writer(&self) -> WriterId {
        self.writer
    }

    /// The materialized graph (read-only; mutate through the edit methods).
    pub fn graph(&self) -> &Graph<N, E> {
        &self.graph
    }

    /// The edit log.
    pub fn log(&self) -> &Codicil<Batch<N, E>> {
        &self.log
    }

    /// The current revision: the number of committed batches. What
    /// [`commit_batch`](GraphLog::commit_batch) compares `expected` against.
    pub fn revision(&self) -> u64 {
        self.log.len() as u64
    }

    /// The current graph key for a live edge id, if the edge is present.
    pub fn edge_key(&self, id: EdgeId) -> Option<EdgeKey> {
        self.by_edge_id.get(&id).copied()
    }

    /// This graph's log identity, if any.
    pub fn id(&self) -> Option<&LogId> {
        self.log.id()
    }

    /// Where this graph forked from, if it is a fork.
    pub fn provenance(&self) -> Option<&Provenance> {
        self.log.provenance()
    }

    /// The derivation records for a node: the other-graph nodes it was derived
    /// from. Empty for a natively-minted node.
    pub fn derivations(&self, node: &N::Id) -> &[DerivationRecord<N::Id>] {
        self.derivations.get(node).map(Vec::as_slice).unwrap_or(&[])
    }
}

impl<N: Identified + Clone, E: Clone> GraphLog<N, E> {
    /// Apply one edit to the materialized graph and its bookkeeping. The single
    /// mutation path, shared by live editing and replay, so the two never diverge.
    fn apply_edit(&mut self, edit: &GraphEdit<N, E>) {
        match edit {
            GraphEdit::InsertNode(node) => {
                self.graph.insert(node.clone());
            }
            GraphEdit::RemoveNode(id) => {
                if let Some(key) = self.graph.key_of(id) {
                    for edge_key in self.graph.incident_edges(key) {
                        if let Some(edge_id) = self.by_edge_key.remove(&edge_key) {
                            self.by_edge_id.remove(&edge_id);
                        }
                    }
                    self.graph.remove(key);
                }
                self.derivations.remove(id);
            }
            GraphEdit::Connect { id, from, to, edge } => {
                if let (Some(from_key), Some(to_key)) =
                    (self.graph.key_of(from), self.graph.key_of(to))
                {
                    let edge_key = self.graph.connect(from_key, to_key, edge.clone());
                    self.by_edge_id.insert(*id, edge_key);
                    self.by_edge_key.insert(edge_key, *id);
                }
                // Advance past this id even on a dangling connect, so replay keeps
                // assigning ids monotonically. Only for ids THIS replica minted:
                // replaying a peer's edits must not consume our counter range,
                // which is what keeps two writers' ids disjoint after a merge.
                if id.writer == self.writer {
                    self.next_edge = self.next_edge.max(id.counter + 1);
                }
            }
            GraphEdit::Disconnect(id) => {
                if let Some(edge_key) = self.by_edge_id.remove(id) {
                    self.by_edge_key.remove(&edge_key);
                    self.graph.disconnect(edge_key);
                }
            }
            GraphEdit::Derive { node, from } => {
                self.derivations
                    .entry(node.clone())
                    .or_default()
                    .push(from.clone());
            }
        }
    }

    /// Apply every edit in a batch, in order.
    pub(crate) fn apply_batch(&mut self, batch: &Batch<N, E>) {
        for edit in &batch.edits {
            self.apply_edit(edit);
        }
    }

    /// Commit a single spec at the current revision (cannot conflict), for the
    /// convenience mutators. Returns whether it committed.
    fn commit_one(
        &mut self,
        author: &Author,
        spec: EditSpec<N, E>,
    ) -> Option<crate::commit::Committed> {
        let revision = self.revision();
        self.commit_batch(author.clone(), revision, vec![spec]).ok()
    }

    /// Insert (or upsert, by identity) a node, attributed. Returns its graph key.
    pub fn insert_node(&mut self, author: &Author, node: N) -> NodeKey {
        let id = node.id().clone();
        self.commit_one(author, EditSpec::InsertNode(node))
            .expect("a single insert cannot conflict or reference unknowns");
        self.graph.key_of(&id).expect("just inserted")
    }

    /// Remove the node with `id` and its incident edges, attributed. A no-op
    /// (and no journal entry) if absent.
    pub fn remove_node(&mut self, author: &Author, id: &N::Id) {
        let _ = self.commit_one(author, EditSpec::RemoveNode(id.clone()));
    }

    /// Connect `from` to `to` with `edge`, attributed, returning the new edge's
    /// stable id, or `None` if either endpoint is absent.
    pub fn connect(
        &mut self,
        author: &Author,
        from: &N::Id,
        to: &N::Id,
        edge: E,
    ) -> Option<EdgeId> {
        self.commit_one(
            author,
            EditSpec::Connect {
                from: from.clone(),
                to: to.clone(),
                edge,
            },
        )
        .map(|committed| committed.edges[0])
    }

    /// Retract the edge with stable id `id`, attributed. Returns whether it was
    /// present.
    pub fn disconnect(&mut self, author: &Author, id: EdgeId) -> bool {
        self.commit_one(author, EditSpec::Disconnect(id)).is_some()
    }

    /// Record that `node` derives from a node in another graph, attributed. A
    /// no-op (and no journal entry) if `node` is absent.
    pub fn derive(&mut self, author: &Author, node: &N::Id, from: DerivationRecord<N::Id>) {
        let _ = self.commit_one(
            author,
            EditSpec::Derive {
                node: node.clone(),
                from,
            },
        );
    }

    /// Fork this graph under a new log identity. The fork is self-contained: it
    /// copies the whole edit history and carries a provenance header pointing back
    /// at the source (see [`provenance`](GraphLog::provenance)), then diverges
    /// independently.
    pub fn fork(&self, new_id: LogId) -> Self {
        Self::replay(self.log.fork(new_id))
    }

    /// Rebuild a graph by replaying a batch log from empty.
    pub fn replay(log: Codicil<Batch<N, E>>) -> Self {
        let mut this = Self::new();
        for batch in log.entries() {
            this.apply_batch(batch);
        }
        this.log = log;
        this
    }

    fn from_snapshot(snapshot: Snapshot<N, E>) -> (Self, u64) {
        let mut this = Self::new();
        for node in snapshot.nodes {
            this.graph.insert(node);
        }
        for (id, from, to, edge) in snapshot.edges {
            if let (Some(from_key), Some(to_key)) =
                (this.graph.key_of(&from), this.graph.key_of(&to))
            {
                let edge_key = this.graph.connect(from_key, to_key, edge);
                this.by_edge_id.insert(id, edge_key);
                this.by_edge_key.insert(edge_key, id);
            }
        }
        for (node, record) in snapshot.derivations {
            this.derivations.entry(node).or_default().push(record);
        }
        this.writer = snapshot.writer;
        this.next_edge = snapshot.next_edge;
        (this, snapshot.at_seq)
    }
}

// Persistence through muniment.
impl<N, E> GraphLog<N, E>
where
    N: Identified + Clone + Serialize + DeserializeOwned,
    N::Id: Serialize + DeserializeOwned,
    E: Clone + Serialize + DeserializeOwned,
{
    /// Load the log at `key`, migrating a pre-gate edit log (bare `GraphEdit`
    /// entries) into single-edit batches if that is what the slot holds.
    async fn load_log<B: Backend, C: Codec>(
        slots: &SlotStore<B, C>,
        key: &str,
    ) -> Result<Codicil<Batch<N, E>>, StoreError> {
        match Codicil::load(slots, key).await {
            Ok(log) => Ok(log),
            Err(StoreError::Codec(_)) => {
                let legacy: Codicil<GraphEdit<N, E>> = Codicil::load(slots, key).await?;
                Ok(crate::commit::migrate_pre_gate(legacy))
            }
            Err(other) => Err(other),
        }
    }

    /// Persist the edit log to a muniment slot.
    pub async fn save_log<B: Backend, C: Codec>(
        &self,
        slots: &SlotStore<B, C>,
        key: &str,
    ) -> Result<(), StoreError> {
        self.log.save(slots, key).await
    }

    /// Write a snapshot of the current state to a muniment slot.
    pub async fn snapshot<B: Backend, C: Codec>(
        &self,
        slots: &SlotStore<B, C>,
        key: &str,
    ) -> Result<(), StoreError> {
        let nodes: Vec<N> = self.graph.nodes().map(|(_, node)| node.clone()).collect();
        let keys: Vec<NodeKey> = self.graph.nodes().map(|(key, _)| key).collect();
        let mut edges = Vec::new();
        for key in keys {
            let from_id = self.graph.node(key).expect("node present").id().clone();
            for (edge_key, target, edge) in self.graph.out_edges(key) {
                if let Some(&edge_id) = self.by_edge_key.get(&edge_key) {
                    let to_id = self
                        .graph
                        .node(target)
                        .expect("target present")
                        .id()
                        .clone();
                    edges.push((edge_id, from_id.clone(), to_id, edge.clone()));
                }
            }
        }
        let derivations: Vec<(N::Id, DerivationRecord<N::Id>)> = self
            .derivations
            .iter()
            .flat_map(|(node, records)| {
                records
                    .iter()
                    .map(move |record| (node.clone(), record.clone()))
            })
            .collect();
        let snapshot = Snapshot {
            nodes,
            edges,
            derivations,
            writer: self.writer,
            next_edge: self.next_edge,
            at_seq: self.log.len() as u64,
        };
        slots.save(key, &snapshot).await
    }

    /// Load by replaying the whole log from the slot at `log_key`.
    pub async fn load_full<B: Backend, C: Codec>(
        slots: &SlotStore<B, C>,
        log_key: &str,
    ) -> Result<Self, StoreError> {
        let log = Self::load_log(slots, log_key).await?;
        Ok(Self::replay(log))
    }

    /// Load from a snapshot at `snap_key` plus the tail of the log at `log_key`. If
    /// no snapshot is present, this is a full replay.
    pub async fn load_checkpointed<B: Backend, C: Codec>(
        slots: &SlotStore<B, C>,
        log_key: &str,
        snap_key: &str,
    ) -> Result<Self, StoreError> {
        let snapshot: Option<Snapshot<N, E>> = slots.load(snap_key).await?;
        let log = Self::load_log(slots, log_key).await?;
        let (mut this, tail_start) = match snapshot {
            Some(snapshot) => Self::from_snapshot(snapshot),
            None => (Self::new(), 0),
        };
        for batch in log.from(Seq(tail_start)) {
            this.apply_batch(batch);
        }
        this.log = log;
        Ok(this)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{Container, Relation};
    use crate::taxonomy::{Recognized, RelationClass};
    use muniment::{JsonSlots, MemoryBackend};

    type Fingerprint = (Vec<String>, Vec<(String, String, String)>);

    /// A canonical, key-independent view of a graph: sorted node ids and sorted
    /// (from, to, relation) edge triples. Two graphs equal iff their fingerprints
    /// are equal, regardless of internal petgraph keys.
    fn fingerprint(graph: &Graph<Container, Relation>) -> Fingerprint {
        let mut nodes: Vec<String> = graph.nodes().map(|(_, n)| n.id.clone()).collect();
        nodes.sort();
        let keys: Vec<NodeKey> = graph.nodes().map(|(k, _)| k).collect();
        let mut edges = Vec::new();
        for key in keys {
            let from = graph.node(key).unwrap().id.clone();
            for (_, target, edge) in graph.out_edges(key) {
                let to = graph.node(target).unwrap().id.clone();
                edges.push((from.clone(), to, format!("{:?}", edge.class)));
            }
        }
        edges.sort();
        (nodes, edges)
    }

    fn cites() -> Relation {
        Relation::new(RelationClass::recognized(Recognized::Cites))
    }

    fn me() -> Author {
        Author::new("test")
    }

    #[test]
    fn replay_reconstructs_the_live_graph() {
        let mut live = GraphLog::new();
        live.insert_node(&me(), Container::new("a"));
        live.insert_node(&me(), Container::new("b"));
        let e = live
            .connect(&me(), &"a".to_string(), &"b".to_string(), cites())
            .unwrap();
        live.insert_node(&me(), Container::new("c"));
        live.connect(&me(), &"a".to_string(), &"c".to_string(), cites());
        live.disconnect(&me(), e);

        let replayed = GraphLog::replay(live.log().clone());
        assert_eq!(fingerprint(replayed.graph()), fingerprint(live.graph()));
    }

    #[test]
    fn removing_a_node_reaps_its_edges_on_replay() {
        let mut live = GraphLog::new();
        live.insert_node(&me(), Container::new("a"));
        live.insert_node(&me(), Container::new("b"));
        live.connect(&me(), &"a".to_string(), &"b".to_string(), cites());
        live.remove_node(&me(), &"b".to_string());

        let replayed = GraphLog::replay(live.log().clone());
        assert_eq!(replayed.graph().node_count(), 1);
        assert_eq!(
            replayed.graph().edge_count(),
            0,
            "the edge went with the node"
        );
        assert_eq!(fingerprint(replayed.graph()), fingerprint(live.graph()));
    }

    #[test]
    fn checkpointed_load_equals_full_replay() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());

            let mut live = GraphLog::new();
            live.insert_node(&me(), Container::new("a").with_tag("keep"));
            live.insert_node(&me(), Container::new("b"));
            live.insert_node(&me(), Container::new("c"));
            let e_ab = live
                .connect(&me(), &"a".to_string(), &"b".to_string(), cites())
                .unwrap();
            live.connect(&me(), &"a".to_string(), &"c".to_string(), cites());

            // Snapshot mid-history.
            live.snapshot(&slots, "snap").await.unwrap();

            // Then more edits, including retracting a pre-snapshot edge and removing
            // a pre-snapshot node (which reaps another pre-snapshot edge).
            live.remove_node(&me(), &"c".to_string());
            live.insert_node(&me(), Container::new("d"));
            live.connect(&me(), &"a".to_string(), &"d".to_string(), cites());
            live.disconnect(&me(), e_ab);

            live.save_log(&slots, "log").await.unwrap();

            let full = GraphLog::<Container, Relation>::load_full(&slots, "log")
                .await
                .unwrap();
            let checkpointed =
                GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
                    .await
                    .unwrap();

            let live_fp = fingerprint(live.graph());
            assert_eq!(
                fingerprint(full.graph()),
                live_fp,
                "full replay matches live"
            );
            assert_eq!(
                fingerprint(checkpointed.graph()),
                live_fp,
                "checkpoint + tail matches live"
            );
            // Final state: nodes a, b, d; only the a->d edge survives.
            assert_eq!(checkpointed.graph().node_count(), 3);
            assert_eq!(checkpointed.graph().edge_count(), 1);
        });
    }

    #[test]
    fn edge_ids_stay_stable_across_reload() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());
            let mut live = GraphLog::new();
            live.insert_node(&me(), Container::new("a"));
            live.insert_node(&me(), Container::new("b"));
            let e = live
                .connect(&me(), &"a".to_string(), &"b".to_string(), cites())
                .unwrap();
            live.save_log(&slots, "log").await.unwrap();

            // Reload, then retract the edge by the id assigned before the reload.
            let mut reloaded = GraphLog::<Container, Relation>::load_full(&slots, "log")
                .await
                .unwrap();
            assert!(reloaded.edge_key(e).is_some(), "the pre-reload id resolves");
            assert!(
                reloaded.disconnect(&me(), e),
                "retract by the stable id works"
            );
            assert_eq!(reloaded.graph().edge_count(), 0);
        });
    }

    #[test]
    fn fork_carries_history_and_provenance_then_diverges() {
        let mut source = GraphLog::new();
        // The source needs an identity for the fork to point back at it.
        source = GraphLog::replay(source.log().clone().fork(LogId::new("source")));
        source.insert_node(&me(), Container::new("a"));
        source.insert_node(&me(), Container::new("b"));
        source.connect(&me(), &"a".to_string(), &"b".to_string(), cites());

        let mut fork = source.fork(LogId::new("fork"));
        // The fork is a self-contained copy.
        assert_eq!(fingerprint(fork.graph()), fingerprint(source.graph()));
        // And it knows where it came from.
        let provenance = fork.provenance().expect("a fork has provenance");
        assert_eq!(provenance.source, Some(LogId::new("source")));
        assert_eq!(provenance.at, Seq(source.log().len() as u64));

        // Diverging the fork leaves the source untouched.
        fork.insert_node(&me(), Container::new("c"));
        fork.connect(&me(), &"a".to_string(), &"c".to_string(), cites());
        assert_eq!(source.graph().node_count(), 2, "source unchanged");
        assert_eq!(fork.graph().node_count(), 3);
    }

    #[test]
    fn derivation_records_survive_round_trip() {
        use crate::edit::DerivationKind;

        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());

            let mut live: GraphLog<Container, Relation> = GraphLog::new();
            live.insert_node(&me(), Container::new("clip"));
            // The node was copied from node "orig" in another graph, "source".
            live.derive(
                &me(),
                &"clip".to_string(),
                DerivationRecord {
                    source_log: LogId::new("source"),
                    source_node: "orig".to_string(),
                    kind: DerivationKind::CopiedFrom,
                },
            );
            live.save_log(&slots, "log").await.unwrap();

            let reloaded = GraphLog::<Container, Relation>::load_full(&slots, "log")
                .await
                .unwrap();
            let derivations = reloaded.derivations(&"clip".to_string());
            assert_eq!(derivations.len(), 1, "the derivation survived reload");
            assert_eq!(derivations[0].source_node, "orig");
            assert_eq!(derivations[0].kind, DerivationKind::CopiedFrom);
            // Natively-minted nodes have none.
            assert!(reloaded.derivations(&"absent".to_string()).is_empty());
        });
    }
}
