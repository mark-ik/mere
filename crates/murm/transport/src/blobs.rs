//! Content-addressed blob store, backed by `iroh-blobs`.
//!
//! Phase 2C v0 per
//! [`mere/design_docs/mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](../../../../../../design_docs/mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md)
//! §2.1 — `iroh-blobs` is a sibling iroh primitive that lives in
//! `transport`. Consumers (murm Cable attachments, gemot engram
//! payloads, eidetic large artifacts) put bytes and get a stable BLAKE3
//! [`BlobHash`]; future network transfer (one peer fetches a hash from
//! another) lands with the first concrete consumer.
//!
//! ## Scope
//!
//! - Local store, in memory ([`BlobStore::new`]) or on disk
//!   ([`BlobStore::open`]). Both back the same `iroh_blobs` `Store` API, so
//!   every operation below behaves identically either way.
//! - `put_bytes(...) -> BlobHash`, `get_bytes(hash) -> Bytes`, `has(hash) -> bool`.
//! - **Network transfer**: [`BlobStore::fetch_from`] downloads a blob
//!   from a peer via [`P2pandaTransport`](crate::P2pandaTransport). The peer
//!   must have been constructed with
//!   [`P2pandaTransport::bind_with_blobs`](crate::P2pandaTransport::bind_with_blobs)
//!   so its router serves the iroh-blobs protocol.
//!
//! ## Choosing a backing
//!
//! Memory is right for a process whose blobs are already durable elsewhere,
//! and for tests. Disk is right for a resident host, where the store IS the
//! durable copy: a transfer interrupted by a restart resumes against bytes
//! already on disk instead of refetching from the peer, and a device can
//! still serve a blob to a sibling after the process that received it exited.

use std::path::Path;

use bytes::Bytes;
use iroh_blobs::store::fs::FsStore;
use iroh_blobs::store::mem::MemStore;
use iroh_blobs::{Hash, api::Store};
use thiserror::Error;

/// A content-addressed blob hash (BLAKE3-256).
///
/// Wraps `iroh_blobs::Hash`. We expose this as our own newtype rather
/// than re-exporting iroh's type so that consumers depend on the
/// `transport` API surface, not on a specific iroh-blobs version
/// directly.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct BlobHash(Hash);

impl BlobHash {
    /// Construct from raw 32-byte BLAKE3 digest.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Hash::from(bytes))
    }

    /// 32-byte BLAKE3 digest.
    pub fn to_bytes(&self) -> [u8; 32] {
        *self.0.as_bytes()
    }

    /// Borrow as a slice.
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl From<Hash> for BlobHash {
    fn from(h: Hash) -> Self {
        Self(h)
    }
}

impl From<BlobHash> for Hash {
    fn from(h: BlobHash) -> Self {
        h.0
    }
}

/// Errors raised by [`BlobStore`] operations.
#[derive(Debug, Error)]
pub enum BlobError {
    /// The requested blob was not found in the local store.
    #[error("blob not found: {0:?}")]
    NotFound(BlobHash),

    /// Underlying iroh-blobs error.
    #[error("blob store backend: {0}")]
    Backend(String),
}

/// Which store holds the bytes. Both variants deref to the same
/// `iroh_blobs` `Store`, so this choice never reaches the operations.
enum Backing {
    Memory(MemStore),
    File(FsStore),
}

/// Local content-addressed blob store.
///
/// Wraps an `iroh_blobs` store, in memory or on disk. Holds bytes; returns
/// stable BLAKE3 hashes.
pub struct BlobStore {
    store: Backing,
}

impl BlobStore {
    /// Construct a new in-memory blob store. Bytes live as long as the
    /// process does.
    pub fn new() -> Self {
        Self {
            store: Backing::Memory(MemStore::new()),
        }
    }

    /// Open (or create) a blob store rooted at `root` on disk.
    ///
    /// Survives process restart, which is what makes this the resident
    /// host's choice: an interrupted transfer resumes against the bytes
    /// already written rather than refetching them.
    pub async fn open(root: impl AsRef<Path>) -> Result<Self, BlobError> {
        let root = root.as_ref();
        if let Some(parent) = root.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BlobError::Backend(format!("create blob root: {e}")))?;
        }
        let store = FsStore::load(root)
            .await
            .map_err(|e| BlobError::Backend(format!("open blob store at {}: {e:?}", root.display())))?;
        Ok(Self {
            store: Backing::File(store),
        })
    }

    /// Whether this store survives a restart.
    pub fn is_persistent(&self) -> bool {
        matches!(self.store, Backing::File(_))
    }

    /// Borrow the underlying `iroh_blobs` `Store` API. Exposed for
    /// advanced uses (network handlers, batch ops) the simple
    /// put/get/has facade doesn't cover.
    ///
    /// Most consumers should use the safer `put_bytes` / `get_bytes`
    /// helpers above.
    pub fn store(&self) -> &Store {
        match &self.store {
            Backing::Memory(store) => store,
            Backing::File(store) => store,
        }
    }

    /// Flush pending metadata to disk. A no-op worth calling on a memory
    /// store, so a caller never has to branch on the backing.
    ///
    /// The resident host calls this after staging a transfer's bytes, so a
    /// crash between "fetched" and "applied" leaves the bytes findable.
    pub async fn flush(&self) -> Result<(), BlobError> {
        match &self.store {
            Backing::Memory(_) => Ok(()),
            Backing::File(store) => store
                .sync_db()
                .await
                .map_err(|e| BlobError::Backend(format!("sync blob db: {e:?}"))),
        }
    }

    /// Shut the store down cleanly, flushing first. Only meaningful for a
    /// disk-backed store.
    pub async fn shutdown(&self) -> Result<(), BlobError> {
        match &self.store {
            Backing::Memory(_) => Ok(()),
            Backing::File(store) => {
                store
                    .sync_db()
                    .await
                    .map_err(|e| BlobError::Backend(format!("sync blob db: {e:?}")))?;
                store
                    .shutdown()
                    .await
                    .map_err(|e| BlobError::Backend(format!("shutdown blob store: {e:?}")))
            }
        }
    }

    /// Put bytes into the store and return the BLAKE3 hash.
    #[tracing::instrument(level = "debug", skip(self, bytes))]
    pub async fn put_bytes(&self, bytes: impl Into<Bytes>) -> Result<BlobHash, BlobError> {
        let bytes: Bytes = bytes.into();
        let byte_count = bytes.len();
        let tag = self
            .store()
            .blobs()
            .add_bytes(bytes)
            .with_tag()
            .await
            .map_err(|e| BlobError::Backend(format!("add_bytes: {e:?}")))?;
        let hash = BlobHash(tag.hash);
        tracing::debug!(byte_count, ?hash, "blob stored");
        Ok(hash)
    }

    /// Read all bytes for a hash. Errors if the blob is not in the
    /// local store.
    #[tracing::instrument(level = "debug", skip(self), fields(?hash))]
    pub async fn get_bytes(&self, hash: BlobHash) -> Result<Bytes, BlobError> {
        // Existence check first → returns the right error variant for
        // a missing blob, instead of the generic backend error
        // get_bytes would surface.
        if !self.has(hash).await? {
            return Err(BlobError::NotFound(hash));
        }
        self.store()
            .blobs()
            .get_bytes(hash.0)
            .await
            .map_err(|e| BlobError::Backend(format!("get_bytes: {e:?}")))
    }

    /// Whether the given hash is present in the local store.
    #[tracing::instrument(level = "debug", skip(self), fields(?hash))]
    pub async fn has(&self, hash: BlobHash) -> Result<bool, BlobError> {
        self.store()
            .blobs()
            .has(hash.0)
            .await
            .map_err(|e| BlobError::Backend(format!("has: {e:?}")))
    }

    /// Fetch a blob from a remote peer over iroh.
    ///
    /// The peer must have been constructed via
    /// [`P2pandaTransport::bind_with_blobs`](crate::P2pandaTransport::bind_with_blobs)
    /// so its p2panda-net endpoint serves the iroh-blobs protocol against a
    /// store that holds the blob.
    ///
    /// On success the bytes are persisted into this local store; you
    /// can then call [`get_bytes`](Self::get_bytes) on the same hash.
    #[tracing::instrument(
        level = "debug",
        skip(self, transport),
        fields(?peer, ?hash),
    )]
    pub async fn fetch_from(
        &self,
        transport: &crate::P2pandaTransport,
        peer: crate::PeerID,
        hash: BlobHash,
    ) -> Result<(), BlobError> {
        let conn = transport
            .connect_raw(peer, iroh_blobs::ALPN)
            .await
            .map_err(|e| BlobError::Backend(format!("connect blobs: {e}")))?;
        self.store()
            .remote()
            .fetch(conn, hash.0)
            .complete()
            .await
            .map_err(|e| BlobError::Backend(format!("fetch: {e:?}")))?;
        Ok(())
    }
}

impl Default for BlobStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_then_get_returns_same_bytes() {
        let store = BlobStore::new();
        let payload = Bytes::from_static(b"hello, blob world");
        let hash = store.put_bytes(payload.clone()).await.expect("put");
        let got = store.get_bytes(hash).await.expect("get");
        assert_eq!(got, payload);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_is_deterministic_by_content() {
        let store = BlobStore::new();
        let h1 = store
            .put_bytes(Bytes::from_static(b"deterministic content"))
            .await
            .unwrap();
        let h2 = store
            .put_bytes(Bytes::from_static(b"deterministic content"))
            .await
            .unwrap();
        assert_eq!(h1, h2, "same bytes → same hash");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn different_content_yields_different_hash() {
        let store = BlobStore::new();
        let h1 = store.put_bytes(Bytes::from_static(b"one")).await.unwrap();
        let h2 = store.put_bytes(Bytes::from_static(b"two")).await.unwrap();
        assert_ne!(h1, h2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn has_reports_presence() {
        let store = BlobStore::new();
        let payload = Bytes::from_static(b"existence test");
        let hash = store.put_bytes(payload).await.unwrap();
        assert!(store.has(hash).await.unwrap());

        let other = BlobHash::from_bytes([0xff; 32]);
        assert!(!store.has(other).await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_missing_blob_returns_not_found() {
        let store = BlobStore::new();
        let other = BlobHash::from_bytes([0xee; 32]);
        let err = store.get_bytes(other).await.expect_err("must error");
        assert!(matches!(err, BlobError::NotFound(h) if h == other));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn empty_blob_round_trips() {
        let store = BlobStore::new();
        let hash = store.put_bytes(Bytes::new()).await.unwrap();
        let got = store.get_bytes(hash).await.unwrap();
        assert!(got.is_empty());
    }

    /// The reason the disk backing exists: bytes outlive the process that
    /// wrote them. Dropping the store and reopening the same root is the
    /// closest a test gets to a host restart.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn disk_backed_blobs_survive_a_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("blobs");
        let payload = Bytes::from_static(b"this must outlive the process");

        let hash = {
            let store = BlobStore::open(&root).await.expect("open");
            assert!(store.is_persistent());
            let hash = store.put_bytes(payload.clone()).await.expect("put");
            store.shutdown().await.expect("shutdown");
            hash
        };

        let reopened = BlobStore::open(&root).await.expect("reopen");
        assert!(
            reopened.has(hash).await.expect("has"),
            "a restart must not lose staged bytes"
        );
        assert_eq!(reopened.get_bytes(hash).await.expect("get"), payload);
    }

    /// A memory store answers the durability verbs rather than making every
    /// caller branch on the backing first.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn memory_store_is_not_persistent_but_still_flushes() {
        let store = BlobStore::new();
        assert!(!store.is_persistent());
        store.flush().await.expect("flush is a no-op");
        store.shutdown().await.expect("shutdown is a no-op");
    }

    #[test]
    fn blob_hash_round_trips_through_bytes() {
        let bytes = [0x42; 32];
        let h = BlobHash::from_bytes(bytes);
        assert_eq!(h.to_bytes(), bytes);
        assert_eq!(h.as_bytes(), &bytes);
    }

    /// End-to-end p2p blob fetch over real iroh.
    ///
    /// Alice puts a blob into her local store. Bob's P2pandaTransport is
    /// bound with his own (initially empty) BlobStore. Bob calls
    /// `fetch_from` to pull the blob from Alice, then reads it locally.
    /// Validates the p2panda-net-served iroh-blobs ALPN end-to-end.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn fetch_from_remote_peer_round_trips_blob() {
        use crate::P2pandaTransport;
        use identity::{IdentityProvider, InMemoryProvider};

        let alice_provider = InMemoryProvider::from_seed([10; 32]);
        let bob_provider = InMemoryProvider::from_seed([20; 32]);

        let alice_kp = alice_provider.master_keypair().clone();
        let bob_kp = bob_provider.master_keypair().clone();

        let alice_blobs = BlobStore::new();
        let bob_blobs = BlobStore::new();

        let alice_transport =
            P2pandaTransport::bind_with_blobs(&alice_kp, vec![], Some(&alice_blobs))
                .await
                .expect("alice bind");
        let bob_transport = P2pandaTransport::bind_with_blobs(&bob_kp, vec![], Some(&bob_blobs))
            .await
            .expect("bob bind");

        // Cross-register addresses.
        alice_transport
            .add_peer(bob_transport.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob_transport
            .add_peer(alice_transport.endpoint_addr().await.unwrap())
            .await
            .unwrap();

        let alice_peer_id = crate::PeerID::from_public_key(alice_provider.master_public_key());

        // Alice puts.
        let payload = Bytes::from_static(b"this blob lives on alice's machine");
        let hash = alice_blobs.put_bytes(payload.clone()).await.unwrap();

        // Bob doesn't have it yet.
        assert!(!bob_blobs.has(hash).await.unwrap());

        // Bob fetches from alice.
        bob_blobs
            .fetch_from(&bob_transport, alice_peer_id, hash)
            .await
            .expect("fetch");

        // Bob now has it; bytes match.
        assert!(bob_blobs.has(hash).await.unwrap());
        let got = bob_blobs.get_bytes(hash).await.unwrap();
        assert_eq!(got, payload);
    }
}
