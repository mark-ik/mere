//! The commons spine: a chartulary container graph as a replicated domain.
//!
//! M3 of the commons multi-writer convergence plan. Before this, nothing in
//! the tree bridged chartulary to the replication layer: a chartulary `Batch`
//! is not a p2panda `Operation`, so a graph edit could not ride a lane.
//!
//! ## The shape
//!
//! One journal batch is one signed operation. The container id is the signed
//! addressing extension, so a batch for one container cannot replay into
//! another. Each replica writes its own per-author log, which is what p2panda
//! reconciles.
//!
//! ## Why receipt does not apply edits
//!
//! The obvious `accept` closure applies each received batch to a live
//! `GraphLog` as it arrives. That does not converge. The merge rules settled
//! in M1/M2 hold under the replication layer's canonical
//! `(verifying_key, log_id, seq_num)` order, and a live drain delivers in
//! *arrival* order, which differs per peer. Whole-node last-writer-wins would
//! then mean "last to arrive here", and two peers would disagree.
//!
//! So receipt only **stores**, and the graph is a fold over the store in
//! canonical order ([`Replica::materialize`]). `resolve` already returns a
//! `BTreeMap` keyed by verifying key, so the first sort key comes for free.
//! This is the same "recorded fact versus derived state" split the statement
//! kernel brief draws: the log accumulates, the graph is recomputed.

use chartulary::{Batch, Container, GraphLog, Relation, WriterId};
use codicil::Codicil;
use muniment::Backend;
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use serde::{Deserialize, Serialize};
use stickleback::{
    Admission, MunimentStore, OperationPolicy, OperationProcessor, Reject, StoreTarget,
};

/// One log per author. The commons has no second log class (no separate
/// checkpoint lane yet), so the log id is a constant.
pub const COMMONS_LOG: u64 = 0;

/// The signed addressing extension: which container this batch edits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonsExt {
    /// The container's stable id, and the LogSync topic.
    pub container: [u8; 32],
}

/// A chartulary journal batch, the unit that rides the lane.
pub type CommonsBatch = Batch<Container, Relation>;

/// A malformed commons operation.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("commons operation has no body")]
    MissingBody,
    #[error("commons operation body is not a batch")]
    Malformed,
}

/// Sign one journal batch into an operation on `container`'s log, at the
/// author's per-author position.
pub fn to_operation(
    signing_seed: [u8; 32],
    container: [u8; 32],
    batch: &CommonsBatch,
    seq_num: u32,
    backlink: Option<[u8; 32]>,
) -> Operation<CommonsExt> {
    let signing_key = SigningKey::from_bytes(&signing_seed);
    let body_bytes = encode_cbor(batch).expect("a chartulary batch always CBOR-encodes");
    let body = Body::new(&body_bytes);
    let mut header = Header {
        version: 1,
        verifying_key: signing_key.verifying_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        seq_num,
        backlink: backlink.map(Hash::from),
        extensions: CommonsExt { container },
    };
    header.sign(&signing_key);
    let hash = header.hash();
    Operation {
        hash,
        header,
        body: Some(body),
    }
}

/// Decode the batch carried by an operation. Does not check the signature.
pub fn from_operation(op: &Operation<CommonsExt>) -> Result<CommonsBatch, WireError> {
    let body = op.body.as_ref().ok_or(WireError::MissingBody)?;
    decode_cbor(body.to_bytes().as_slice()).map_err(|_| WireError::Malformed)
}

/// Admission for the commons lane: the operation must address this container,
/// carry a decodable batch, and be validly signed.
///
/// Deliberately thin. Membership and capability are the moot's and personae's
/// jobs; this probe proves convergence, not authority.
pub struct CommonsPolicy {
    container: [u8; 32],
}

impl OperationPolicy<CommonsExt> for CommonsPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<CommonsExt>) -> Result<Admission<Self::LogId>, Reject> {
        if operation.header.extensions.container != self.container {
            return Err(Reject::new(
                "wrong-container",
                "operation addresses a different container",
            ));
        }
        if !operation.header.verify() {
            return Err(Reject::new(
                "bad-signature",
                "operation signature is invalid",
            ));
        }
        from_operation(operation)
            .map_err(|err| Reject::new("invalid-commons-batch", err.to_string()))?;
        Ok(Admission::keep(StoreTarget::new(
            Topic::from(self.container),
            COMMONS_LOG,
        )))
    }
}

/// A failure while folding the store back into a graph.
#[derive(Debug, thiserror::Error)]
pub enum MaterializeError {
    #[error(transparent)]
    Store(#[from] muniment::StoreError),
    #[error(transparent)]
    Wire(#[from] WireError),
}

/// Validate and store one operation against `container`'s policy.
///
/// Free-standing so a joined space's `accept` closure can capture a cloned
/// store rather than the whole replica.
pub async fn accept_into<B: Backend + Clone + Send + Sync + 'static>(
    store: &MunimentStore<B, CommonsExt>,
    container: [u8; 32],
    op: &Operation<CommonsExt>,
) -> Result<bool, stickleback::ProcessError> {
    let processor = OperationProcessor::new(store.clone(), CommonsPolicy { container });
    Ok(processor.process(op).await?.inserted())
}

/// Fold every stored batch for `container` into a graph, in the replication
/// layer's canonical order: author, then log, then sequence.
///
/// Two replicas holding the same operations produce the same graph, which is
/// the convergence claim. `resolve` returns a `BTreeMap` keyed by verifying
/// key, so authors already arrive sorted.
pub async fn materialize<B: Backend + Clone + Send + Sync + 'static>(
    store: &MunimentStore<B, CommonsExt>,
    container: [u8; 32],
) -> Result<GraphLog<Container, Relation>, MaterializeError> {
    let by_author: std::collections::BTreeMap<VerifyingKey, Vec<u64>> =
        TopicStore::<Topic, VerifyingKey, u64>::resolve(store, &Topic::from(container)).await?;

    let mut journal: Codicil<CommonsBatch> = Codicil::new();
    for (author, mut logs) in by_author {
        logs.sort_unstable();
        for log_id in logs {
            let entries =
                LogStore::<Operation<CommonsExt>, VerifyingKey, u64, u32, Hash>::get_log_entries(
                    store, &author, &log_id, None, None,
                )
                .await?
                .unwrap_or_default();
            // Entries arrive in sequence order. The second tuple element is
            // the **header** bytes, p2panda's LogSync convention, not the
            // payload; the batch rides in `op.body`. Reading that `Vec<u8>` as
            // a batch decodes header bytes as content, which is why this fold
            // refuses to swallow a decode error: with `.ok()` it silently
            // dropped every batch and produced an empty graph that looked like
            // a sync failure.
            for (op, _header_bytes) in entries {
                journal.append(from_operation(&op)?);
            }
        }
    }
    Ok(GraphLog::replay(journal))
}

/// One member's replica of one shared container.
pub struct Replica<B: Backend + Clone + Send + Sync + 'static> {
    store: MunimentStore<B, CommonsExt>,
    container: [u8; 32],
    writer: WriterId,
    signing_seed: [u8; 32],
    /// This replica's own edits, for minting the next log position and for
    /// local reads before a sync round lands.
    local: GraphLog<Container, Relation>,
    seq: u32,
    backlink: Option<[u8; 32]>,
}

impl<B: Backend + Clone + Send + Sync + 'static> Replica<B> {
    /// A replica writing under `signing_seed`, whose verifying key is also the
    /// chartulary `WriterId` that scopes minted edge ids. One identity, so a
    /// replica cannot mint an edge id another replica could also mint.
    pub fn new(backend: B, container: [u8; 32], signing_seed: [u8; 32]) -> Self {
        let writer = WriterId(
            *SigningKey::from_bytes(&signing_seed)
                .verifying_key()
                .as_bytes(),
        );
        Self {
            store: MunimentStore::new(backend),
            container,
            writer,
            signing_seed,
            local: GraphLog::new().for_writer(writer),
            seq: 0,
            backlink: None,
        }
    }

    /// The store, for `JoinedSpace::join`.
    pub fn sync_store(&self) -> MunimentStore<B, CommonsExt> {
        self.store.clone()
    }

    /// This replica's writer identity.
    pub fn writer(&self) -> WriterId {
        self.writer
    }

    /// Edit the container: apply locally, sign the resulting batch, and store
    /// it. Returns the operation so a caller can publish it on a live lane.
    pub async fn edit(
        &mut self,
        edit: impl FnOnce(&mut GraphLog<Container, Relation>),
    ) -> Result<Operation<CommonsExt>, stickleback::ProcessError> {
        let before = self.local.log().entries().len();
        edit(&mut self.local);
        let batch = self
            .local
            .log()
            .entries()
            .get(before)
            .expect("an edit appends exactly one batch")
            .clone();

        let op = to_operation(
            self.signing_seed,
            self.container,
            &batch,
            self.seq,
            self.backlink,
        );
        self.seq += 1;
        self.backlink = Some(*op.hash.as_bytes());
        self.accept(&op).await?;
        Ok(op)
    }

    /// Validate and store one operation. The `accept` closure a joined space
    /// drains into. Storing only: the graph is a fold, see the module docs.
    pub async fn accept(
        &self,
        op: &Operation<CommonsExt>,
    ) -> Result<bool, stickleback::ProcessError> {
        accept_into(&self.store, self.container, op).await
    }

    /// This replica's view of the container: see [`materialize`].
    pub async fn materialize(&self) -> Result<GraphLog<Container, Relation>, MaterializeError> {
        materialize(&self.store, self.container).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chartulary::taxonomy::{Recognized, RelationClass};
    use chartulary::{Author, EdgeId};
    use identity::{IdentityProvider, InMemoryProvider};
    use muniment::MemoryBackend;
    use std::sync::Arc;
    use std::time::Duration;
    use stickleback::JoinedSpace;
    use transport::{P2pandaTransport, PeerID, sync_overlay_topic};

    const CONTAINER: [u8; 32] = [0xc0; 32];

    fn cites() -> Relation {
        Relation::new(RelationClass::recognized(Recognized::Cites))
    }

    /// Two bound transports tagged with each other on the container's overlay
    /// topic (the two-peer bootstrap mesh and gemot already use).
    async fn two_peers() -> (P2pandaTransport, P2pandaTransport) {
        let alice_provider = Arc::new(InMemoryProvider::from_seed([90; 32]));
        let bob_provider = Arc::new(InMemoryProvider::from_seed([91; 32]));
        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());

        let alice = P2pandaTransport::builder(alice_provider.master_keypair())
            .gossip()
            .bind()
            .await
            .expect("bind alice");
        let bob = P2pandaTransport::builder(bob_provider.master_keypair())
            .gossip()
            .bind()
            .await
            .expect("bind bob");

        let overlay = sync_overlay_topic(CONTAINER);
        alice
            .add_peer(bob.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        alice.set_topics(bob_id, &[overlay]).await.unwrap();
        bob.add_peer(alice.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob.set_topics(alice_id, &[overlay]).await.unwrap();
        (alice, bob)
    }

    /// M3: two members edit one container **while partitioned**, then
    /// reconverge over real LogSync reconciliation.
    ///
    /// This is the first time the M1/M2 merge rules are exercised over a lane
    /// rather than a hand-built journal. Both members mint an edge before they
    /// can see each other, which is exactly the case that collided before
    /// `EdgeId` became `(writer, counter)`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_partitioned_members_converge_on_one_container() {
        let (alice_t, bob_t) = two_peers().await;

        let mut alice = Replica::new(MemoryBackend::new(), CONTAINER, [90; 32]);
        let mut bob = Replica::new(MemoryBackend::new(), CONTAINER, [91; 32]);
        assert_ne!(
            alice.writer(),
            bob.writer(),
            "each replica writes under its own identity"
        );

        // ── Partitioned: neither has joined a space yet, so neither can see
        // the other's edits. Both insert a pair and connect it. ──
        let author = Author::new("ui");
        let a_edge = {
            let a = author.clone();
            let mut minted = None;
            alice
                .edit(|g| {
                    g.insert_node(&a, Container::new("alice-1"));
                })
                .await
                .expect("alice inserts");
            alice
                .edit(|g| {
                    g.insert_node(&a, Container::new("alice-2"));
                })
                .await
                .expect("alice inserts");
            alice
                .edit(|g| {
                    minted = g.connect(&a, &"alice-1".to_string(), &"alice-2".to_string(), cites());
                })
                .await
                .expect("alice connects");
            minted.expect("alice minted an edge")
        };

        let b_edge = {
            let a = author.clone();
            let mut minted = None;
            bob.edit(|g| {
                g.insert_node(&a, Container::new("bob-1"));
            })
            .await
            .expect("bob inserts");
            bob.edit(|g| {
                g.insert_node(&a, Container::new("bob-2"));
            })
            .await
            .expect("bob inserts");
            bob.edit(|g| {
                minted = g.connect(&a, &"bob-1".to_string(), &"bob-2".to_string(), cites());
            })
            .await
            .expect("bob connects");
            minted.expect("bob minted an edge")
        };

        assert_eq!(
            a_edge.counter, b_edge.counter,
            "both minted at counter 0 while partitioned"
        );
        assert_ne!(
            a_edge, b_edge,
            "and the ids still differ, because the writer half separates them"
        );

        // ── Reconnect: join the space on both sides and let LogSync reconcile. ──
        let (a_ep, a_gossip) = alice_t.sync_parts().expect("alice sync parts");
        let a_store = alice.sync_store();
        let _alice_space = JoinedSpace::join::<_, u64, _, _>(
            alice.sync_store(),
            a_ep,
            a_gossip,
            CONTAINER,
            move |op| {
                let store = a_store.clone();
                async move { accept_into(&store, CONTAINER, &op).await.unwrap_or(false) }
            },
        )
        .await
        .expect("alice joins");

        let (b_ep, b_gossip) = bob_t.sync_parts().expect("bob sync parts");
        let b_store = bob.sync_store();
        let _bob_space = JoinedSpace::join::<_, u64, _, _>(
            bob.sync_store(),
            b_ep,
            b_gossip,
            CONTAINER,
            move |op| {
                let store = b_store.clone();
                async move { accept_into(&store, CONTAINER, &op).await.unwrap_or(false) }
            },
        )
        .await
        .expect("bob joins");

        // ── Converge: both graphs hold all four nodes and both edges. ──
        let converged = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let a = alice.materialize().await.unwrap();
                let b = bob.materialize().await.unwrap();
                if a.graph().node_count() == 4 && b.graph().node_count() == 4 {
                    return (a, b);
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
        .await;
        let (a_graph, b_graph) = converged.expect("the two replicas converged");

        for (who, g) in [("alice", &a_graph), ("bob", &b_graph)] {
            assert_eq!(g.graph().node_count(), 4, "{who} sees every node");
            assert_eq!(g.graph().edge_count(), 2, "{who} sees both edges");
            for id in ["alice-1", "alice-2", "bob-1", "bob-2"] {
                assert!(
                    g.graph().key_of(&id.to_string()).is_some(),
                    "{who} is missing {id}"
                );
            }
            // The M1 payoff, over a real lane: two concurrently-minted edges
            // stay separately addressable, so either can be retracted by name.
            let a_key = g.edge_key(a_edge).expect("alice's edge addressable");
            let b_key = g.edge_key(b_edge).expect("bob's edge addressable");
            assert_ne!(a_key, b_key, "{who}: each id names a different edge");
        }
    }

    /// Before blaming a lane: a replica must be able to fold back its own
    /// stored edits. If this fails, `materialize` is wrong, not sync.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_replica_folds_back_its_own_edits() {
        let author = Author::new("ui");
        let mut alice = Replica::new(MemoryBackend::new(), CONTAINER, [90; 32]);
        alice
            .edit(|g| {
                g.insert_node(&author, Container::new("n1"));
            })
            .await
            .expect("insert");
        alice
            .edit(|g| {
                g.insert_node(&author, Container::new("n2"));
            })
            .await
            .expect("insert");

        let store = alice.sync_store();
        let count = store.operation_count().await.expect("count");
        let resolved: std::collections::BTreeMap<VerifyingKey, Vec<u64>> =
            TopicStore::<Topic, VerifyingKey, u64>::resolve(&store, &Topic::from(CONTAINER))
                .await
                .expect("resolve");
        assert_eq!(count, 2, "both operations are stored");
        assert_eq!(resolved.len(), 1, "one author is associated with the topic");

        let folded = alice.materialize().await.expect("materialize");
        assert_eq!(
            folded.graph().node_count(),
            2,
            "the fold reads back what the replica stored"
        );
    }

    /// A container id is the signed address: a batch for one container cannot
    /// be replayed into another.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_operation_for_another_container_is_refused() {
        let mut alice = Replica::new(MemoryBackend::new(), CONTAINER, [90; 32]);
        let op = alice
            .edit(|g| {
                g.insert_node(&Author::new("ui"), Container::new("n"));
            })
            .await
            .expect("alice inserts");

        let other = Replica::new(MemoryBackend::new(), [0xd1; 32], [91; 32]);
        assert!(
            other.accept(&op).await.is_err(),
            "a foreign container's batch is refused before mutation"
        );
        assert_eq!(
            other.materialize().await.unwrap().graph().node_count(),
            0,
            "and nothing was stored"
        );
    }

    /// The edge ids the wire carries survive a CBOR round trip, including the
    /// writer half. A silent loss there would collapse two writers' edges.
    #[test]
    fn a_batch_round_trips_through_the_wire_with_writer_scoped_ids() {
        let author = Author::new("ui");
        let mut g = GraphLog::<Container, Relation>::new().for_writer(WriterId([0xa1; 32]));
        g.insert_node(&author, Container::new("x"));
        g.insert_node(&author, Container::new("y"));
        let minted = g
            .connect(&author, &"x".to_string(), &"y".to_string(), cites())
            .expect("connect");

        let batch = g.log().entries().last().expect("a batch").clone();
        let op = to_operation([0xa1; 32], CONTAINER, &batch, 0, None);
        let decoded = from_operation(&op).expect("round trip");

        let carried = decoded.edits.iter().find_map(|e| match e {
            chartulary::GraphEdit::Connect { id, .. } => Some(*id),
            _ => None,
        });
        assert_eq!(carried, Some(minted));
        assert_eq!(
            carried.map(|id| id.writer),
            Some(WriterId([0xa1; 32])),
            "the writer half survives the wire"
        );
        assert_ne!(
            carried,
            Some(EdgeId::local(0)),
            "and it is not a bare counter"
        );
    }
}
