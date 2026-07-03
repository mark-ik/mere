//! p2panda-net sync traits over the redb post store.
//!
//! [`PersistentCabalStore`] stores cabal posts; p2panda-net's LogSync needs the
//! same data exposed as a *log* it can reconcile. This module implements the two
//! traits LogSync requires — [`LogStore`] (per-author append-only logs, for
//! offering/serving entries) and [`TopicStore`] (which logs belong to a topic) —
//! directly over that store, so there is **one store of record and one write
//! path**: a post is the canonical operation, served either as a [`Post`] (the
//! view) or an [`Operation`] (for sync). No second operation store, no
//! double-write.
//!
//! ## Index
//!
//! The post store already keys posts by id and by channel. Sync additionally
//! needs ordered per-author-log access, so [`index_post_in_txn`] (called inside
//! [`PersistentCabalStore::insert`]'s transaction) maintains two sibling tables:
//!
//! - [`OPS_BY_LOG`]: `author(32) ++ log_id_be(8) ++ seq_be(8)` → `post_id(32)`.
//!   Fixed-length key, so a log is a native redb range scan in `seq` order.
//! - [`LOG_TOPICS`]: `topic(32) ++ author(32) ++ log_id_be(8)` → `()`. Resolves a
//!   topic (a cabal id) to the `(author, log)` pairs it contains.
//!
//! ## Log model
//!
//! Each store holds exactly one cabal, and each author keeps a single per-cabal
//! log, so the log id is always [`LOG_ID`] (`0`) — matching the per-space
//! granularity the [sync plan][plan] settled on. The topic is the cabal id.
//!
//! ## Async-over-sync
//!
//! The trait methods are `async` but redb is synchronous, so they do blocking
//! redb work inline (as the [redb-logstore probe][plan] did). Logs are small in
//! practice; if a hot path ever blocks the runtime, wrap the redb calls in
//! `spawn_blocking` (the `db` is an `Arc`, cheap to clone into a blocking task).
//!
//! [plan]: https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-06-02_logsync_sync_as_projection_plan.md

use std::collections::BTreeMap;

use p2panda_core::cbor::encode_cbor;
use p2panda_core::{Hash, Operation, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use redb::{ReadableTable, TableDefinition, WriteTransaction};

use crate::cable::persistent_store::{POSTS, PersistentCabalStore};
use crate::cable::wire::{CabalExt, post_to_operation};
use crate::{MurmuringError, Post, PostId};

/// `author(32) ++ log_id_be(8) ++ seq_be(8)` → `post_id(32)` — per-author log
/// entries in `seq` order (native redb range scan).
pub(crate) const OPS_BY_LOG: TableDefinition<&[u8], &[u8]> = TableDefinition::new("ops_by_log");
/// `topic(32) ++ author(32) ++ log_id_be(8)` → `()` — topic → its `(author, log)` pairs.
pub(crate) const LOG_TOPICS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("log_topics");

/// The single per-cabal log id (each store holds one cabal; each author keeps
/// one log in it).
pub(crate) const LOG_ID: u64 = 0;

fn be(e: impl std::fmt::Display) -> MurmuringError {
    MurmuringError::Backend(e.to_string())
}

// ── key helpers ──────────────────────────────────────────────────────────────

/// `author(32) ++ log_id_be(8)` — the prefix shared by all entries of one log.
fn log_prefix(author: &[u8; 32], log_id: u64) -> Vec<u8> {
    let mut k = Vec::with_capacity(40);
    k.extend_from_slice(author);
    k.extend_from_slice(&log_id.to_be_bytes());
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

fn topics_key(topic: &[u8; 32], author: &[u8; 32], log_id: u64) -> Vec<u8> {
    let mut k = Vec::with_capacity(72);
    k.extend_from_slice(topic);
    k.extend_from_slice(author);
    k.extend_from_slice(&log_id.to_be_bytes());
    k
}

// ── write path (called from PersistentCabalStore::insert) ────────────────────

/// Create the sync index tables. Called once from [`PersistentCabalStore::open`]
/// so they exist on disk before the first insert.
pub(crate) fn create_index_tables(txn: &WriteTransaction) -> Result<(), MurmuringError> {
    txn.open_table(OPS_BY_LOG).map_err(be)?;
    txn.open_table(LOG_TOPICS).map_err(be)?;
    Ok(())
}

/// Index a newly-inserted post into the per-author log and topic tables, inside
/// the caller's write transaction (so post bytes + indexes commit atomically).
///
/// Idempotent at the row level: re-indexing the same `(author, seq)` overwrites
/// with the same `post_id` (content addressing means same id ⇒ same bytes), and
/// the topic row is a set membership. `insert` only calls this on first insert,
/// so it normally runs once per post.
pub(crate) fn index_post_in_txn(
    txn: &WriteTransaction,
    post_id: &PostId,
    post: &Post,
) -> Result<(), MurmuringError> {
    let author = post.author.to_bytes();
    let log_key = with_seq(&log_prefix(&author, LOG_ID), post.seq_num);
    let topic_key = topics_key(&post.cabal_id, &author, LOG_ID);
    {
        let mut by_log = txn.open_table(OPS_BY_LOG).map_err(be)?;
        by_log
            .insert(log_key.as_slice(), post_id.as_bytes().as_slice())
            .map_err(be)?;
    }
    {
        let mut topics = txn.open_table(LOG_TOPICS).map_err(be)?;
        topics
            .insert(topic_key.as_slice(), [].as_slice())
            .map_err(be)?;
    }
    Ok(())
}

// ── read path: a stored post id → its Operation ──────────────────────────────

impl PersistentCabalStore {
    /// Load the [`Operation`] for a stored post id (via the `POSTS` table), or
    /// `None` if absent. Shared by the `LogStore` read methods.
    fn operation_by_id(
        &self,
        read: &redb::ReadTransaction,
        post_id: &[u8; 32],
    ) -> Result<Option<Operation<CabalExt>>, MurmuringError> {
        let posts = read.open_table(POSTS).map_err(be)?;
        match posts.get(post_id).map_err(be)? {
            None => Ok(None),
            Some(v) => {
                let post = crate::cable::wire::decode_post(v.value())?;
                Ok(Some(post_to_operation(&post)?))
            }
        }
    }
}

// ── LogStore ─────────────────────────────────────────────────────────────────

impl LogStore<Operation<CabalExt>, VerifyingKey, u64, u64, Hash> for PersistentCabalStore {
    type Error = MurmuringError;

    async fn get_latest_entry(
        &self,
        author: &VerifyingKey,
        log_id: &u64,
    ) -> Result<Option<Operation<CabalExt>>, Self::Error> {
        let prefix = log_prefix(author.as_bytes(), *log_id);
        let read = self.db.begin_read().map_err(be)?;
        let by_log = read.open_table(OPS_BY_LOG).map_err(be)?;
        let lo = with_seq(&prefix, 0);
        let hi = with_seq(&prefix, u64::MAX);
        let mut range = by_log.range(lo.as_slice()..=hi.as_slice()).map_err(be)?;
        match range.next_back() {
            None => Ok(None),
            Some(entry) => {
                let (_k, v) = entry.map_err(be)?;
                let pid: [u8; 32] = v
                    .value()
                    .try_into()
                    .map_err(|_| MurmuringError::MalformedPost)?;
                self.operation_by_id(&read, &pid)
            }
        }
    }

    async fn get_latest_entry_tx(
        &self,
        author: &VerifyingKey,
        log_id: &u64,
    ) -> Result<Option<Operation<CabalExt>>, Self::Error> {
        // redb opens its own read transaction per call; the `_tx` contract
        // (caller holds the transaction) is a SqliteStore concern.
        self.get_latest_entry(author, log_id).await
    }

    async fn get_log_heights(
        &self,
        author: &VerifyingKey,
        logs: &[u64],
    ) -> Result<Option<BTreeMap<u64, u64>>, Self::Error> {
        let read = self.db.begin_read().map_err(be)?;
        let by_log = read.open_table(OPS_BY_LOG).map_err(be)?;
        let mut map = BTreeMap::new();
        for log_id in logs {
            let prefix = log_prefix(author.as_bytes(), *log_id);
            let lo = with_seq(&prefix, 0);
            let hi = with_seq(&prefix, u64::MAX);
            if let Some(entry) = by_log
                .range(lo.as_slice()..=hi.as_slice())
                .map_err(be)?
                .next_back()
            {
                let (k, _v) = entry.map_err(be)?;
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
        let prefix = log_prefix(author.as_bytes(), *log_id);
        let lo = with_seq(&prefix, after.map(|a| a + 1).unwrap_or(0));
        let hi = with_seq(&prefix, until.unwrap_or(u64::MAX));
        let read = self.db.begin_read().map_err(be)?;
        let by_log = read.open_table(OPS_BY_LOG).map_err(be)?;
        let posts = read.open_table(POSTS).map_err(be)?;
        let mut count = 0u64;
        let mut bytes = 0u64;
        for entry in by_log.range(lo.as_slice()..=hi.as_slice()).map_err(be)? {
            let (_k, v) = entry.map_err(be)?;
            let pid: [u8; 32] = v
                .value()
                .try_into()
                .map_err(|_| MurmuringError::MalformedPost)?;
            if let Some(p) = posts.get(&pid).map_err(be)? {
                count += 1;
                // Approximate stored size with the canonical wire-byte length;
                // the exact header/payload split is not load-bearing for sync.
                bytes += p.value().len() as u64;
            }
        }
        Ok(if count == 0 {
            None
        } else {
            Some((count, bytes))
        })
    }

    async fn get_log_entries(
        &self,
        author: &VerifyingKey,
        log_id: &u64,
        after: Option<u64>,
        until: Option<u64>,
    ) -> Result<Option<Vec<(Operation<CabalExt>, Vec<u8>)>>, Self::Error> {
        let prefix = log_prefix(author.as_bytes(), *log_id);
        let lo = with_seq(&prefix, after.map(|a| a + 1).unwrap_or(0));
        let hi = with_seq(&prefix, until.unwrap_or(u64::MAX));
        let read = self.db.begin_read().map_err(be)?;
        let by_log = read.open_table(OPS_BY_LOG).map_err(be)?;
        let mut entries = Vec::new();
        // Collect ids first so the POSTS lookups don't borrow the range iterator.
        let mut ids = Vec::new();
        for entry in by_log.range(lo.as_slice()..=hi.as_slice()).map_err(be)? {
            let (_k, v) = entry.map_err(be)?;
            let pid: [u8; 32] = v
                .value()
                .try_into()
                .map_err(|_| MurmuringError::MalformedPost)?;
            ids.push(pid);
        }
        for pid in ids {
            if let Some(operation) = self.operation_by_id(&read, &pid)? {
                let header_cbor = encode_cbor(&operation.header).map_err(be)?;
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
        // Pruning the log is an open question (the sync plan couples it to
        // lapse-and-revive durability), and murm posts bundle header + payload
        // in one wire blob, so p2panda's payload-only prune does not map here.
        // No-op for v1: nothing is dropped.
        Ok(0)
    }
}

// ── TopicStore ───────────────────────────────────────────────────────────────

impl TopicStore<Topic, VerifyingKey, u64> for PersistentCabalStore {
    type Error = MurmuringError;

    async fn associate(
        &self,
        topic: &Topic,
        author: &VerifyingKey,
        data_id: &u64,
    ) -> Result<bool, Self::Error> {
        let key = topics_key(topic.as_bytes(), author.as_bytes(), *data_id);
        let txn = self.db.begin_write().map_err(be)?;
        let inserted;
        {
            let mut t = txn.open_table(LOG_TOPICS).map_err(be)?;
            if t.get(key.as_slice()).map_err(be)?.is_some() {
                inserted = false;
            } else {
                t.insert(key.as_slice(), [].as_slice()).map_err(be)?;
                inserted = true;
            }
        }
        txn.commit().map_err(be)?;
        Ok(inserted)
    }

    async fn remove(
        &self,
        topic: &Topic,
        author: &VerifyingKey,
        data_id: &u64,
    ) -> Result<bool, Self::Error> {
        let key = topics_key(topic.as_bytes(), author.as_bytes(), *data_id);
        let txn = self.db.begin_write().map_err(be)?;
        let removed;
        {
            let mut t = txn.open_table(LOG_TOPICS).map_err(be)?;
            removed = t.remove(key.as_slice()).map_err(be)?.is_some();
        }
        txn.commit().map_err(be)?;
        Ok(removed)
    }

    async fn resolve(
        &self,
        topic: &Topic,
    ) -> Result<BTreeMap<VerifyingKey, Vec<u64>>, Self::Error> {
        let prefix = topic.as_bytes();
        let read = self.db.begin_read().map_err(be)?;
        let t = read.open_table(LOG_TOPICS).map_err(be)?;
        let mut map: BTreeMap<VerifyingKey, Vec<u64>> = BTreeMap::new();
        for entry in t.range(prefix.as_slice()..).map_err(be)? {
            let (k, _v) = entry.map_err(be)?;
            let key = k.value();
            if !key.starts_with(prefix.as_slice()) {
                break;
            }
            let author_bytes: [u8; 32] = key[32..64]
                .try_into()
                .map_err(|_| MurmuringError::Backend("topic key author".into()))?;
            let author = VerifyingKey::from_bytes(&author_bytes)
                .map_err(|e| MurmuringError::Backend(e.to_string()))?;
            let log_id = u64::from_be_bytes(
                key[64..72]
                    .try_into()
                    .map_err(|_| MurmuringError::Backend("topic key log_id".into()))?,
            );
            map.entry(author).or_default().push(log_id);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cable::hash_post;
    use crate::cable::sign::sign_post;
    use crate::{ChannelName, PostKind};
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
    use tempfile::tempdir;

    const CABAL: [u8; 32] = [0xcb; 32];

    fn keypair(seed: u8) -> Ed25519Keypair {
        InMemoryProvider::from_seed([seed; 32])
            .derive_keypair(b"log-store-test")
            .unwrap()
    }

    fn text(kp: &Ed25519Keypair, seq: u64, backlink: Option<PostId>, body: &str) -> (PostId, Post) {
        let post = sign_post(
            kp,
            CABAL,
            seq,
            backlink,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: body.to_string(),
                timestamp_ms: seq + 1,
            },
        );
        (hash_post(&post), post)
    }

    fn store() -> (tempfile::TempDir, PersistentCabalStore) {
        let dir = tempdir().unwrap();
        let store = PersistentCabalStore::open(dir.path().join("cabal.redb")).unwrap();
        (dir, store)
    }

    /// Insert a 3-op chained log, then read it back through the `LogStore` API.
    #[tokio::test]
    async fn log_entries_round_trip_as_operations() {
        let (_dir, store) = store();
        let kp = keypair(1);
        let author = VerifyingKey::from_bytes(&kp.public_key().to_bytes()).unwrap();

        let (id0, p0) = text(&kp, 0, None, "first");
        let (id1, p1) = text(&kp, 1, Some(id0), "second");
        let (id2, p2) = text(&kp, 2, Some(id1), "third");
        store.insert(id0, &p0).unwrap();
        store.insert(id1, &p1).unwrap();
        store.insert(id2, &p2).unwrap();

        // Latest entry is op2, and its operation id is the signed-header hash.
        let latest = store
            .get_latest_entry(&author, &LOG_ID)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.hash.as_bytes(), id2.as_bytes());

        // Height is 2 (seq of the latest entry).
        let heights = store
            .get_log_heights(&author, &[LOG_ID])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(heights.get(&LOG_ID), Some(&2));

        // All three entries, in seq order.
        let all = store
            .get_log_entries(&author, &LOG_ID, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].0.hash.as_bytes(), id0.as_bytes());
        assert_eq!(all[2].0.hash.as_bytes(), id2.as_bytes());

        // Ranged: after seq 0 (exclusive) yields op1, op2.
        let ranged = store
            .get_log_entries(&author, &LOG_ID, Some(0), None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ranged.len(), 2);
        assert_eq!(ranged[0].0.hash.as_bytes(), id1.as_bytes());

        let (count, bytes) = store
            .get_log_size(&author, &LOG_ID, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 3);
        assert!(bytes > 0);
    }

    /// The reconstructed operations satisfy p2panda's own log validators, so
    /// LogSync will accept what this store serves.
    #[tokio::test]
    async fn served_operations_pass_p2panda_validation() {
        use p2panda_core::operation::{validate_backlink, validate_header};

        let (_dir, store) = store();
        let kp = keypair(2);
        let author = VerifyingKey::from_bytes(&kp.public_key().to_bytes()).unwrap();
        let (id0, p0) = text(&kp, 0, None, "a");
        let (id1, p1) = text(&kp, 1, Some(id0), "b");
        store.insert(id0, &p0).unwrap();
        store.insert(id1, &p1).unwrap();

        let entries = store
            .get_log_entries(&author, &LOG_ID, None, None)
            .await
            .unwrap()
            .unwrap();
        let h0 = &entries[0].0.header;
        let h1 = &entries[1].0.header;
        assert!(validate_header(h0).is_ok());
        assert!(validate_header(h1).is_ok());
        assert!(validate_backlink(h0, h1).is_ok(), "served chain validates");
    }

    /// A topic (cabal id) resolves to the authors whose posts are in the store.
    #[tokio::test]
    async fn topic_resolves_to_authors_logs() {
        let (_dir, store) = store();
        let topic = Topic::from(CABAL);

        let alice = keypair(3);
        let bob = keypair(4);
        let alice_vk = VerifyingKey::from_bytes(&alice.public_key().to_bytes()).unwrap();
        let bob_vk = VerifyingKey::from_bytes(&bob.public_key().to_bytes()).unwrap();

        let (ida, pa) = text(&alice, 0, None, "from alice");
        let (idb, pb) = text(&bob, 0, None, "from bob");
        store.insert(ida, &pa).unwrap();
        store.insert(idb, &pb).unwrap();

        // Insert alone associated both authors' logs under the cabal topic.
        let resolved = store.resolve(&topic).await.unwrap();
        assert_eq!(resolved.get(&alice_vk), Some(&vec![LOG_ID]));
        assert_eq!(resolved.get(&bob_vk), Some(&vec![LOG_ID]));

        // A foreign topic resolves to nothing.
        let empty = store.resolve(&Topic::from([0x00; 32])).await.unwrap();
        assert!(empty.is_empty());
    }

    /// Explicit `associate` / `remove` round-trip (idempotent associate).
    #[tokio::test]
    async fn associate_and_remove_topic() {
        let (_dir, store) = store();
        let topic = Topic::from([0x42; 32]);
        let kp = keypair(5);
        let vk = VerifyingKey::from_bytes(&kp.public_key().to_bytes()).unwrap();

        assert!(store.associate(&topic, &vk, &LOG_ID).await.unwrap());
        // Idempotent: associating again reports "already present".
        assert!(!store.associate(&topic, &vk, &LOG_ID).await.unwrap());
        assert_eq!(
            store.resolve(&topic).await.unwrap().get(&vk),
            Some(&vec![LOG_ID])
        );

        assert!(store.remove(&topic, &vk, &LOG_ID).await.unwrap());
        assert!(store.resolve(&topic).await.unwrap().is_empty());
    }
}
