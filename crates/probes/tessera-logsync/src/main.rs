//! Tessera's own sync — the convergence proof.
//!
//! `redb-logstore` proved LogSync's offline-catch-up lane runs over a pure-Rust
//! redb store (for murm's `CabalExt` posts). This probe proves the **same
//! substrate carries tessera**: two real p2panda-net peers replicate a moot's
//! signed tessera event log over LogSync, and both fold the synced events into
//! the **same scores**. It is the tessera equivalent of murm's two-peer
//! catch-up test — bounded, because the substrate is already proven; the new
//! thing is that `Operation<TesseraExt>` events converge and the [`Ledger`]
//! projection over them agrees across peers.
//!
//! The store implements only the two traits LogSync needs — [`LogStore`] +
//! [`TopicStore`] over `Operation<TesseraExt>` — like murm's
//! `PersistentCabalStore` (no OperationStore); authoring and persisting received
//! ops go through an inherent [`RedbStore::insert`], which also associates the
//! moot topic from the op's signed extension (the moot id is both the addressing
//! extension and the sync topic, exactly as the live tessera wire bridge).
//!
//! Flow: peer A authors a 3-event tessera log (commit → fulfil → govern) on a
//! moot and stores it; LogSync replicates it to empty peer B; both project their
//! stores with [`Ledger`] and the author's chain-root score matches — and equals
//! the hand-computed value (+10 fulfil, +1 govern = 11).

use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use identity::{IdentityProvider, InMemoryProvider};
use iroh::EndpointAddr;
use moothold::tessera::wire::verify;
use moothold::tessera::{
    from_operation, to_operation, ChainRoot, CommitmentId, Ledger, Scope, TesseraConfig,
    TesseraEvent, TesseraExt,
};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_net::addrs::NodeInfo;
use p2panda_net::{AddressBook, Endpoint, Gossip, LogSync};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use p2panda_sync::protocols::TopicLogSyncEvent;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

/// The moot whose event-DAG both peers sync (the addressing extension *and* the
/// LogSync topic).
const MOOT: [u8; 32] = [0x30; 32];
/// Each author keeps one per-moot tessera log, so the log id is always `0` (the
/// per-space granularity murm's store settled on).
const LOG_ID: u64 = 0;

// ── redb schema (mirrors redb-logstore, over Operation<TesseraExt>) ──────────

/// `hash(32) → StoredOp (CBOR)` — content-addressed op lookups.
const OPS_BY_HASH: TableDefinition<&[u8], &[u8]> = TableDefinition::new("ops_by_hash");
/// `author(32) ++ log_hash(8) ++ seq_be(8) → hash(32)` — per-author log range
/// scans in `seq` order (native redb range query over a fixed-length key).
const OPS_BY_LOG: TableDefinition<&[u8], &[u8]> = TableDefinition::new("ops_by_log");
/// `topic_hash(8) ++ author(32) ++ log_id(CBOR) → ()` — topic → its `(author, log)` pairs.
const TOPICS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("topics");

#[derive(Serialize, Deserialize)]
struct StoredOp {
    header: Header<TesseraExt>,
    body: Option<Vec<u8>>,
    /// 8-byte BLAKE3 prefix of the log id, kept so the `OPS_BY_LOG` key can be
    /// reconstructed from a stored op.
    log_hash: [u8; 8],
}

#[derive(Debug, thiserror::Error)]
enum RedbStoreError {
    #[error("redb: {0}")]
    Redb(String),
    #[error("cbor: {0}")]
    Cbor(String),
    #[error("key: {0}")]
    Key(String),
}

fn redb_err(e: impl std::fmt::Display) -> RedbStoreError {
    RedbStoreError::Redb(e.to_string())
}
fn cbor_err(e: impl std::fmt::Display) -> RedbStoreError {
    RedbStoreError::Cbor(e.to_string())
}

/// A redb-backed store for tessera operations, exposing the LogSync traits.
#[derive(Clone)]
struct RedbStore {
    db: Arc<Database>,
}

impl RedbStore {
    fn in_memory() -> Result<Self, RedbStoreError> {
        let db = Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .map_err(redb_err)?;
        let w = db.begin_write().map_err(redb_err)?;
        {
            w.open_table(OPS_BY_HASH).map_err(redb_err)?;
            w.open_table(OPS_BY_LOG).map_err(redb_err)?;
            w.open_table(TOPICS).map_err(redb_err)?;
        }
        w.commit().map_err(redb_err)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Persist a tessera operation: content row + per-author-log row + topic
    /// association, in one transaction. The moot id in the op's *signed*
    /// extension is the topic, so authoring and receiving share one write path
    /// (no OperationStore). Idempotent on the op hash.
    fn insert(&self, op: &Operation<TesseraExt>) -> Result<bool, RedbStoreError> {
        let author = op.header.verifying_key;
        let lh = log_hash8(&LOG_ID)?;
        let log_key = with_seq(&log_prefix(&author, &lh), op.header.seq_num);
        let stored = StoredOp {
            header: op.header.clone(),
            body: op.body.as_ref().map(|b| b.to_bytes()),
            log_hash: lh,
        };
        let bytes = encode_cbor(&stored).map_err(cbor_err)?;
        let hash = op.hash.as_bytes().to_vec();
        let topic = Topic::from(op.header.extensions.moot_id);
        let topic_key = topics_key(&topic, &author, &LOG_ID)?;

        let write = self.db.begin_write().map_err(redb_err)?;
        let inserted;
        {
            let mut by_hash = write.open_table(OPS_BY_HASH).map_err(redb_err)?;
            let mut by_log = write.open_table(OPS_BY_LOG).map_err(redb_err)?;
            let mut topics = write.open_table(TOPICS).map_err(redb_err)?;
            if by_hash.get(hash.as_slice()).map_err(redb_err)?.is_some() {
                inserted = false;
            } else {
                by_hash
                    .insert(hash.as_slice(), bytes.as_slice())
                    .map_err(redb_err)?;
                by_log
                    .insert(log_key.as_slice(), hash.as_slice())
                    .map_err(redb_err)?;
                topics
                    .insert(topic_key.as_slice(), [].as_slice())
                    .map_err(redb_err)?;
                inserted = true;
            }
        }
        write.commit().map_err(redb_err)?;
        Ok(inserted)
    }

    fn has(&self, hash: &Hash) -> Result<bool, RedbStoreError> {
        let read = self.db.begin_read().map_err(redb_err)?;
        let by_hash = read.open_table(OPS_BY_HASH).map_err(redb_err)?;
        Ok(by_hash
            .get(hash.as_bytes().as_slice())
            .map_err(redb_err)?
            .is_some())
    }
}

// ── key helpers ──────────────────────────────────────────────────────────────

fn log_hash8<L: Serialize>(log_id: &L) -> Result<[u8; 8], RedbStoreError> {
    let bytes = encode_cbor(log_id).map_err(cbor_err)?;
    let h = blake3::hash(&bytes);
    Ok(h.as_bytes()[..8].try_into().unwrap())
}

/// `author(32) ++ log_hash(8)` — the prefix shared by all entries of one log.
fn log_prefix(author: &VerifyingKey, log_hash: &[u8; 8]) -> Vec<u8> {
    let mut k = Vec::with_capacity(40);
    k.extend_from_slice(author.as_bytes());
    k.extend_from_slice(log_hash);
    k
}

fn with_seq(prefix: &[u8], seq: u64) -> Vec<u8> {
    let mut k = Vec::with_capacity(prefix.len() + 8);
    k.extend_from_slice(prefix);
    k.extend_from_slice(&seq.to_be_bytes());
    k
}

fn seq_from_key(key: &[u8]) -> u64 {
    u64::from_be_bytes(key[40..48].try_into().unwrap())
}

fn decode_stored(bytes: &[u8]) -> Result<Operation<TesseraExt>, RedbStoreError> {
    let stored: StoredOp = decode_cbor(bytes).map_err(cbor_err)?;
    let body = stored.body.map(Body::from);
    let hash = stored.header.hash();
    Ok(Operation {
        hash,
        header: stored.header,
        body,
    })
}

fn topics_key(topic: &Topic, author: &VerifyingKey, log_id: &u64) -> Result<Vec<u8>, RedbStoreError> {
    let tp = log_hash8(topic)?; // 8-byte topic prefix
    let mut k = Vec::new();
    k.extend_from_slice(&tp);
    k.extend_from_slice(author.as_bytes());
    k.extend_from_slice(&encode_cbor(log_id).map_err(cbor_err)?);
    Ok(k)
}

// ── LogStore ─────────────────────────────────────────────────────────────────

impl LogStore<Operation<TesseraExt>, VerifyingKey, u64, u64, Hash> for RedbStore {
    type Error = RedbStoreError;

    async fn get_latest_entry(
        &self,
        author: &VerifyingKey,
        log_id: &u64,
    ) -> Result<Option<Operation<TesseraExt>>, Self::Error> {
        let prefix = log_prefix(author, &log_hash8(log_id)?);
        let read = self.db.begin_read().map_err(redb_err)?;
        let by_log = read.open_table(OPS_BY_LOG).map_err(redb_err)?;
        let by_hash = read.open_table(OPS_BY_HASH).map_err(redb_err)?;
        let lo = with_seq(&prefix, 0);
        let hi = with_seq(&prefix, u64::MAX);
        let mut range = by_log
            .range(lo.as_slice()..=hi.as_slice())
            .map_err(redb_err)?;
        match range.next_back() {
            Some(entry) => {
                let (_k, v) = entry.map_err(redb_err)?;
                let hash = v.value().to_vec();
                match by_hash.get(hash.as_slice()).map_err(redb_err)? {
                    Some(op) => Ok(Some(decode_stored(op.value())?)),
                    None => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    async fn get_latest_entry_tx(
        &self,
        author: &VerifyingKey,
        log_id: &u64,
    ) -> Result<Option<Operation<TesseraExt>>, Self::Error> {
        self.get_latest_entry(author, log_id).await
    }

    async fn get_log_heights(
        &self,
        author: &VerifyingKey,
        logs: &[u64],
    ) -> Result<Option<BTreeMap<u64, u64>>, Self::Error> {
        let read = self.db.begin_read().map_err(redb_err)?;
        let by_log = read.open_table(OPS_BY_LOG).map_err(redb_err)?;
        let mut map = BTreeMap::new();
        for log_id in logs {
            let prefix = log_prefix(author, &log_hash8(log_id)?);
            let lo = with_seq(&prefix, 0);
            let hi = with_seq(&prefix, u64::MAX);
            if let Some(entry) = by_log
                .range(lo.as_slice()..=hi.as_slice())
                .map_err(redb_err)?
                .next_back()
            {
                let (k, _v) = entry.map_err(redb_err)?;
                map.insert(*log_id, seq_from_key(k.value()));
            }
        }
        Ok(if map.is_empty() { None } else { Some(map) })
    }

    async fn get_log_size(
        &self,
        author: &VerifyingKey,
        log_id: &u64,
        after: Option<u64>,
        until: Option<u64>,
    ) -> Result<Option<(u64, u64)>, Self::Error> {
        let prefix = log_prefix(author, &log_hash8(log_id)?);
        let lo = with_seq(&prefix, after.map(|a| a + 1).unwrap_or(0));
        let hi = with_seq(&prefix, until.unwrap_or(u64::MAX));
        let read = self.db.begin_read().map_err(redb_err)?;
        let by_log = read.open_table(OPS_BY_LOG).map_err(redb_err)?;
        let by_hash = read.open_table(OPS_BY_HASH).map_err(redb_err)?;
        let mut count = 0u64;
        let mut bytes = 0u64;
        for entry in by_log
            .range(lo.as_slice()..=hi.as_slice())
            .map_err(redb_err)?
        {
            let (_k, v) = entry.map_err(redb_err)?;
            let hash = v.value().to_vec();
            if let Some(op) = by_hash.get(hash.as_slice()).map_err(redb_err)? {
                count += 1;
                bytes += op.value().len() as u64;
            }
        }
        Ok(if count == 0 { None } else { Some((count, bytes)) })
    }

    async fn get_log_entries(
        &self,
        author: &VerifyingKey,
        log_id: &u64,
        after: Option<u64>,
        until: Option<u64>,
    ) -> Result<Option<Vec<(Operation<TesseraExt>, Vec<u8>)>>, Self::Error> {
        let prefix = log_prefix(author, &log_hash8(log_id)?);
        let lo = with_seq(&prefix, after.map(|a| a + 1).unwrap_or(0));
        let hi = with_seq(&prefix, until.unwrap_or(u64::MAX));
        let read = self.db.begin_read().map_err(redb_err)?;
        let by_log = read.open_table(OPS_BY_LOG).map_err(redb_err)?;
        let by_hash = read.open_table(OPS_BY_HASH).map_err(redb_err)?;
        let mut entries = Vec::new();
        for entry in by_log
            .range(lo.as_slice()..=hi.as_slice())
            .map_err(redb_err)?
        {
            let (_k, v) = entry.map_err(redb_err)?;
            let hash = v.value().to_vec();
            if let Some(op) = by_hash.get(hash.as_slice()).map_err(redb_err)? {
                let operation = decode_stored(op.value())?;
                let header_cbor = encode_cbor(&operation.header).map_err(cbor_err)?;
                entries.push((operation, header_cbor));
            }
        }
        Ok(if entries.is_empty() {
            None
        } else {
            Some(entries)
        })
    }

    async fn prune_entries(
        &self,
        _author: &VerifyingKey,
        _log_id: &u64,
        _until: &u64,
    ) -> Result<u64, Self::Error> {
        // No pruning in the probe (the score depends on the whole log).
        Ok(0)
    }
}

// ── TopicStore ───────────────────────────────────────────────────────────────

impl TopicStore<Topic, VerifyingKey, u64> for RedbStore {
    type Error = RedbStoreError;

    async fn associate(
        &self,
        topic: &Topic,
        author: &VerifyingKey,
        data_id: &u64,
    ) -> Result<bool, Self::Error> {
        let key = topics_key(topic, author, data_id)?;
        let write = self.db.begin_write().map_err(redb_err)?;
        let inserted;
        {
            let mut t = write.open_table(TOPICS).map_err(redb_err)?;
            if t.get(key.as_slice()).map_err(redb_err)?.is_some() {
                inserted = false;
            } else {
                t.insert(key.as_slice(), [].as_slice()).map_err(redb_err)?;
                inserted = true;
            }
        }
        write.commit().map_err(redb_err)?;
        Ok(inserted)
    }

    async fn remove(
        &self,
        topic: &Topic,
        author: &VerifyingKey,
        data_id: &u64,
    ) -> Result<bool, Self::Error> {
        let key = topics_key(topic, author, data_id)?;
        let write = self.db.begin_write().map_err(redb_err)?;
        let removed;
        {
            let mut t = write.open_table(TOPICS).map_err(redb_err)?;
            removed = t.remove(key.as_slice()).map_err(redb_err)?.is_some();
        }
        write.commit().map_err(redb_err)?;
        Ok(removed)
    }

    async fn resolve(&self, topic: &Topic) -> Result<BTreeMap<VerifyingKey, Vec<u64>>, Self::Error> {
        let prefix = log_hash8(topic)?; // 8-byte topic prefix
        let read = self.db.begin_read().map_err(redb_err)?;
        let t = read.open_table(TOPICS).map_err(redb_err)?;
        let mut map: BTreeMap<VerifyingKey, Vec<u64>> = BTreeMap::new();
        for entry in t.range(prefix.as_slice()..).map_err(redb_err)? {
            let (k, _v) = entry.map_err(redb_err)?;
            let key = k.value();
            if !key.starts_with(&prefix) {
                break;
            }
            let author_bytes: [u8; 32] = key[8..40]
                .try_into()
                .map_err(|_| RedbStoreError::Key("author".into()))?;
            let author = VerifyingKey::from_bytes(&author_bytes)
                .map_err(|e| RedbStoreError::Key(e.to_string()))?;
            let log_id: u64 = decode_cbor(&key[40..]).map_err(cbor_err)?;
            map.entry(author).or_default().push(log_id);
        }
        Ok(map)
    }
}

// ── projection: a peer's store → per-chain-root tessera scores ───────────────

/// Fold one author's stored tessera log into scores as of `now_ms` — the same
/// projection a moot computes, run over what a peer holds. Reads the log in
/// `seq` order (so the fold is causal), decodes each op to its [`TesseraEvent`],
/// and runs the [`Ledger`].
async fn project_scores(
    store: &RedbStore,
    author: &VerifyingKey,
    now_ms: u64,
) -> HashMap<ChainRoot, i64> {
    let entries = store
        .get_log_entries(author, &LOG_ID, None, None)
        .await
        .expect("read log")
        .unwrap_or_default();
    let events: Vec<TesseraEvent> = entries
        .iter()
        .map(|(op, _)| from_operation(op).expect("decode tessera event").1)
        .collect();
    Ledger::from_events(TesseraConfig::default(), &events).scores(now_ms)
}

// ── two-peer node setup (real p2panda-net over loopback) ─────────────────────

/// A spawned p2panda-net node: a real iroh endpoint + gossip overlay + a
/// `LogSync` protocol bound to this node's redb tessera store.
struct Node {
    address_book: AddressBook,
    endpoint: Endpoint,
    _gossip: Gossip,
    log_sync: LogSync<RedbStore, u64, TesseraExt>,
}

async fn spawn_node(store: RedbStore, signing_key: SigningKey) -> Node {
    let address_book = AddressBook::builder().spawn().await.expect("address book");
    let endpoint = Endpoint::builder(address_book.clone())
        .signing_key(signing_key)
        .spawn()
        .await
        .expect("endpoint");
    let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
        .spawn()
        .await
        .expect("gossip");
    let log_sync = LogSync::builder(store, endpoint.clone(), gossip.clone())
        .spawn()
        .await
        .expect("LogSync over the redb tessera store");
    Node {
        address_book,
        endpoint,
        _gossip: gossip,
        log_sync,
    }
}

/// Mix value p2panda-net's sync manager uses to derive the gossip *overlay*
/// topic from a sync topic (its private `GOSSIP_TOPIC_MIX_VALUE`). LogSync joins
/// gossip on `derive_topic(sync_topic, MIX)`, not the raw sync topic, so peers
/// must be tagged (`set_topics`) with the *derived* topic for the overlay to form.
const GOSSIP_TOPIC_MIX_VALUE: [u8; 32] = [
    253, 6, 251, 217, 173, 228, 215, 244, 130, 181, 150, 142, 220, 244, 49, 219, 35, 94, 163, 197,
    229, 93, 143, 227, 97, 61, 38, 202, 63, 250, 26, 233,
];

/// Replicated verbatim from p2panda-net's private `sync::actors::manager::derive_topic`.
fn derive_topic(topic: Topic, value: impl AsRef<[u8]>) -> Topic {
    Hash::digest([topic.as_bytes(), value.as_ref()].concat()).into()
}

/// This node's dialable loopback [`EndpointAddr`] (wildcard binds rewritten to
/// loopback), mirroring `transport::P2pandaTransport::endpoint_addr`.
async fn endpoint_addr(endpoint: &Endpoint) -> EndpointAddr {
    let iroh_ep = endpoint.endpoint().await.expect("inner iroh endpoint");
    let mut addr = EndpointAddr::new(iroh_ep.id());
    for sock in iroh_ep.bound_sockets() {
        let dial = if sock.ip().is_unspecified() {
            let ip = if sock.is_ipv4() {
                IpAddr::V4(Ipv4Addr::LOCALHOST)
            } else {
                IpAddr::V6(Ipv6Addr::LOCALHOST)
            };
            SocketAddr::new(ip, sock.port())
        } else {
            sock
        };
        addr = addr.with_ip_addr(dial);
    }
    addr
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // The op author is a persona key from the identity vault (real derivation),
    // distinct from the endpoint/network key — exactly as murm authors posts with
    // a derived key while the transport runs on the master key.
    let provider = InMemoryProvider::from_seed([0x0a; 32]);
    let author_kp = provider
        .derive_keypair(b"tessera-peer-a")
        .expect("derive author key");
    let author_root = ChainRoot(author_kp.public_key().to_bytes());
    let author_vk =
        VerifyingKey::from_bytes(&author_kp.public_key().to_bytes()).expect("author verifying key");

    // A 3-event tessera log: pledge a commitment, fulfil it, then participate in
    // governance. Signed as a chained per-author log (seq + backlink), so it is a
    // valid p2panda log LogSync reconciles.
    let cid = CommitmentId([0xc1; 32]);
    let e0 = TesseraEvent::CommitmentMade {
        by: author_root,
        commitment: cid,
        scope: Scope("host/cluster-1".into()),
        cadence_ms: 1_000,
        duration_ms: None,
        at_ms: 1_000,
    };
    let e1 = TesseraEvent::CommitmentFulfilled {
        by: author_root,
        commitment: cid,
        at_ms: 1_050,
    };
    let e2 = TesseraEvent::GovernanceParticipation {
        by: author_root,
        at_ms: 1_100,
    };
    let op0 = to_operation(&author_kp, MOOT, &e0, 0, None);
    let op1 = to_operation(&author_kp, MOOT, &e1, 1, Some(*op0.hash.as_bytes()));
    let op2 = to_operation(&author_kp, MOOT, &e2, 2, Some(*op1.hash.as_bytes()));
    let authored: [&Operation<TesseraExt>; 3] = [&op0, &op1, &op2];
    let now = 5_000u64; // well past the events; the commitment is closed, so no lapse

    // ── 1. local store → projection loop ──
    //
    // Before the network: prove that storing the authored ops and folding them
    // back out of the store yields the expected score (the store round-trips
    // tessera events and the projection reads them in causal order).
    let local = RedbStore::in_memory().expect("local store");
    for op in authored {
        assert!(verify(op), "authored op verifies");
        local.insert(op).unwrap();
    }
    assert!(!local.insert(&op2).unwrap(), "insert is idempotent on the op hash");
    let local_scores = project_scores(&local, &author_vk, now).await;
    assert_eq!(
        local_scores.get(&author_root),
        Some(&11),
        "local projection: +10 fulfil, +1 govern = 11"
    );
    // The store also resolves the moot topic to the author's log (insert
    // associated it from the op's signed extension).
    let resolved = local.resolve(&Topic::from(MOOT)).await.unwrap();
    assert_eq!(resolved.get(&author_vk), Some(&vec![LOG_ID]));
    println!("local store → projection loop: author scores {} as of t={now}", 11);

    // ── 2. two-peer LogSync convergence over the redb tessera store ──
    //
    // Peer A holds the authored 3-event log; peer B starts empty. Once the gossip
    // overlay forms, LogSync auto-initiates (RBSR) and B catches up on A's tessera
    // operations; both then project the SAME author score.
    let alice_store = RedbStore::in_memory().expect("alice store");
    let bob_store = RedbStore::in_memory().expect("bob store");
    for op in authored {
        alice_store.insert(op).unwrap();
    }

    let alice_ep_key = SigningKey::from_bytes(&[0xa1; 32]);
    let bob_ep_key = SigningKey::from_bytes(&[0xb0; 32]);
    let alice_ep_pk = alice_ep_key.verifying_key();
    let bob_ep_pk = bob_ep_key.verifying_key();

    let sync_topic = Topic::from(MOOT);
    let alice = spawn_node(alice_store.clone(), alice_ep_key).await;
    let bob = spawn_node(bob_store.clone(), bob_ep_key).await;

    // Cross-bootstrap: each node learns the other's loopback transport address and
    // that it is interested in the gossip *overlay* topic LogSync joins (the
    // derived topic, not the raw sync topic). Discovery does this in production.
    let overlay_topic = derive_topic(sync_topic, GOSSIP_TOPIC_MIX_VALUE);
    let alice_addr = endpoint_addr(&alice.endpoint).await;
    let bob_addr = endpoint_addr(&bob.endpoint).await;
    alice
        .address_book
        .insert_node_info(NodeInfo::from(bob_addr))
        .await
        .unwrap();
    alice
        .address_book
        .set_topics(bob_ep_pk, [overlay_topic].into_iter())
        .await
        .unwrap();
    bob.address_book
        .insert_node_info(NodeInfo::from(alice_addr))
        .await
        .unwrap();
    bob.address_book
        .set_topics(alice_ep_pk, [overlay_topic].into_iter())
        .await
        .unwrap();

    // Both open a live sync stream on the topic (also joining the gossip overlay,
    // so the neighbour link can form and trigger sync).
    let alice_handle = alice
        .log_sync
        .stream(sync_topic, true)
        .await
        .expect("alice stream");
    let bob_handle = bob
        .log_sync
        .stream(sync_topic, true)
        .await
        .expect("bob stream");

    // Drain Alice's event stream so her side does not stall on backpressure.
    let mut alice_sub = alice_handle.subscribe().await.expect("alice subscription");
    tokio::spawn(async move { while alice_sub.next().await.is_some() {} });

    let mut bob_sub = bob_handle.subscribe().await.expect("bob subscription");

    // Bob receives Alice's three tessera operations via RBSR catch-up. Each is
    // verified (the signature the wire bridge produced), persisted to Bob's store,
    // and counted. Lifecycle events are logged so a stall is diagnosable.
    let caught_up = tokio::time::timeout(Duration::from_secs(30), async {
        let mut received: Vec<Hash> = Vec::new();
        while received.len() < 3 {
            match bob_sub.next().await {
                Some(Ok(from_sync)) => match from_sync.event {
                    TopicLogSyncEvent::OperationReceived { operation, .. } => {
                        let op = *operation;
                        assert!(verify(&op), "a received tessera op verifies");
                        bob_store.insert(&op).unwrap();
                        received.push(op.hash);
                    }
                    TopicLogSyncEvent::Failed { error } => println!("  bob sync FAILED: {error}"),
                    TopicLogSyncEvent::SessionStarted => println!("  bob: SessionStarted"),
                    TopicLogSyncEvent::SyncStarted { .. } => println!("  bob: SyncStarted"),
                    TopicLogSyncEvent::SyncFinished { .. } => println!("  bob: SyncFinished"),
                    TopicLogSyncEvent::LiveModeStarted => println!("  bob: LiveModeStarted"),
                    TopicLogSyncEvent::SessionFinished { .. } => println!("  bob: SessionFinished"),
                },
                Some(Err(e)) => println!("  bob sync error: {e}"),
                None => break,
            }
        }
        received
    })
    .await
    .expect("bob caught up on alice's tessera ops within the timeout");

    assert_eq!(caught_up.len(), 3, "bob received all three tessera operations");
    for op in authored {
        assert!(
            bob_store.has(&op.hash).unwrap(),
            "op {:?} persisted to bob's store",
            op.hash
        );
    }

    // ── 3. the convergence proof: both peers project the SAME score ──
    let alice_scores = project_scores(&alice_store, &author_vk, now).await;
    let bob_scores = project_scores(&bob_store, &author_vk, now).await;
    assert_eq!(
        alice_scores, bob_scores,
        "both peers project identical tessera scores from their stores"
    );
    assert_eq!(
        bob_scores.get(&author_root),
        Some(&11),
        "bob, having only synced ops, computes the same author score as alice"
    );

    println!("\ntwo-peer tessera LogSync convergence succeeded:");
    println!("  alice's 3-event tessera log synced to bob's (empty) redb store over loopback,");
    println!("  discovery-driven (gossip NeighbourUp → auto-initiated RBSR session).");
    println!("  both peers fold the synced events → author score = {} (identical).", 11);

    println!("\n--- tessera-logsync VERDICT ---");
    println!("Tessera networks over the proven substrate:");
    println!("- tessera events ride LogSync as signed Operation<TesseraExt> (wire bridge)");
    println!("- a redb store implements the two traits LogSync needs (LogStore + TopicStore)");
    println!("- two real p2panda-net peers converge: B reconstructs A's score from synced ops");
    println!("=> tessera's reputation log is replicable today; productizing the store into");
    println!("   moothold (mirroring murmuring's PersistentCabalStore) is the next step.");
}
