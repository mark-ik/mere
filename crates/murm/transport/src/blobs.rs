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

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use bytes::Bytes;
use iroh_blobs::store::GcConfig;
use iroh_blobs::store::fs::{FsStore, options::Options as FsOptions};
use iroh_blobs::store::mem::{MemStore, Options as MemOptions};
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

/// Mutable peer admission for one blob-serving endpoint.
///
/// The iroh-blobs protocol addresses content by hash but does not decide who
/// may ask for it. A domain host supplies that decision from its own durable
/// authority facts and may refresh the set as pairing or group authority
/// changes. An empty authorizer denies every remote peer.
#[derive(Clone, Debug, Default)]
pub struct BlobPeerAuthorizer {
    peers: Arc<RwLock<BTreeSet<[u8; 32]>>>,
}

impl BlobPeerAuthorizer {
    /// Admit the supplied transport-authenticated peer identities.
    pub fn from_peers(peers: impl IntoIterator<Item = [u8; 32]>) -> Self {
        Self {
            peers: Arc::new(RwLock::new(peers.into_iter().collect())),
        }
    }

    /// Whether this transport-authenticated peer may use the blob protocol.
    pub fn allows(&self, peer: &[u8; 32]) -> bool {
        self.peers
            .read()
            .map(|peers| peers.contains(peer))
            .unwrap_or(false)
    }

    /// Admit a peer immediately. Returns whether the set changed.
    pub fn allow(&self, peer: [u8; 32]) -> bool {
        self.peers
            .write()
            .map(|mut peers| peers.insert(peer))
            .unwrap_or(false)
    }

    /// Withdraw a peer immediately. Returns whether the set changed.
    pub fn deny(&self, peer: &[u8; 32]) -> bool {
        self.peers
            .write()
            .map(|mut peers| peers.remove(peer))
            .unwrap_or(false)
    }

    /// Replace the effective peer set from a newly materialized authority
    /// view. Returns whether the set changed.
    pub fn replace(&self, peers: impl IntoIterator<Item = [u8; 32]>) -> bool {
        let next = peers.into_iter().collect::<BTreeSet<_>>();
        self.peers
            .write()
            .map(|mut current| {
                if *current == next {
                    return false;
                }
                *current = next;
                true
            })
            .unwrap_or(false)
    }

    /// Current admitted peers, sorted for deterministic receipts.
    pub fn peers(&self) -> Vec<[u8; 32]> {
        self.peers
            .read()
            .map(|peers| peers.iter().copied().collect())
            .unwrap_or_default()
    }
}

/// Opaque domain scope for blob custody and read authority.
///
/// A Knot space id is one such scope. The transport deliberately does not
/// interpret the bytes or infer membership from them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct BlobScope([u8; 32]);

impl BlobScope {
    /// Name a stable domain-owned scope.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Raw scope identity for receipts and durable projections.
    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Clone, Debug, Default)]
struct BlobScopeAuthority {
    readers: BTreeSet<[u8; 32]>,
    retained: BTreeSet<BlobHash>,
}

/// Mutable serving-side authorization for scoped blob reads.
///
/// A read is allowed only when the authenticated peer is a reader of the
/// named scope and that exact hash is retained by the scope. Keeping custody
/// facts here, separate from the physical store, permits one hash to be held
/// once and authorized through several scopes without widening either scope.
#[derive(Clone, Debug, Default)]
pub struct BlobReadAuthorizer {
    scopes: Arc<RwLock<BTreeMap<BlobScope, BlobScopeAuthority>>>,
}

impl BlobReadAuthorizer {
    /// Create an empty authorizer. Empty authority denies every read.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a retained hash to one custody scope.
    pub fn retain(&self, scope: BlobScope, hash: BlobHash) -> bool {
        self.scopes
            .write()
            .map(|mut scopes| scopes.entry(scope).or_default().retained.insert(hash))
            .unwrap_or(false)
    }

    /// Withdraw a custody binding without deleting the physical bytes.
    pub fn release(&self, scope: BlobScope, hash: &BlobHash) -> bool {
        self.scopes
            .write()
            .map(|mut scopes| {
                scopes
                    .get_mut(&scope)
                    .map(|authority| authority.retained.remove(hash))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Admit a transport-authenticated reader to one scope.
    pub fn allow_reader(&self, scope: BlobScope, peer: [u8; 32]) -> bool {
        self.scopes
            .write()
            .map(|mut scopes| scopes.entry(scope).or_default().readers.insert(peer))
            .unwrap_or(false)
    }

    /// Withdraw a reader from one scope immediately.
    pub fn deny_reader(&self, scope: BlobScope, peer: &[u8; 32]) -> bool {
        self.scopes
            .write()
            .map(|mut scopes| {
                scopes
                    .get_mut(&scope)
                    .map(|authority| authority.readers.remove(peer))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Replace one scope's readers from a newly materialized authority view.
    pub fn replace_readers(
        &self,
        scope: BlobScope,
        peers: impl IntoIterator<Item = [u8; 32]>,
    ) -> bool {
        let next = peers.into_iter().collect::<BTreeSet<_>>();
        self.scopes
            .write()
            .map(|mut scopes| {
                let authority = scopes.entry(scope).or_default();
                if authority.readers == next {
                    return false;
                }
                authority.readers = next;
                true
            })
            .unwrap_or(false)
    }

    /// Evaluate the exact serving decision `(scope, peer, hash, read)`.
    pub fn allows(&self, scope: BlobScope, peer: &[u8; 32], hash: BlobHash) -> bool {
        self.scopes
            .read()
            .ok()
            .and_then(|scopes| scopes.get(&scope).cloned())
            .map(|authority| authority.readers.contains(peer) && authority.retained.contains(&hash))
            .unwrap_or(false)
    }

    /// Current readers for deterministic authority receipts.
    pub fn readers(&self, scope: BlobScope) -> Vec<[u8; 32]> {
        self.scopes
            .read()
            .ok()
            .and_then(|scopes| scopes.get(&scope).cloned())
            .map(|authority| authority.readers.into_iter().collect())
            .unwrap_or_default()
    }

    /// Scopes currently retaining a hash, sorted for deterministic receipts.
    pub fn retaining_scopes(&self, hash: BlobHash) -> Vec<BlobScope> {
        self.scopes
            .read()
            .map(|scopes| {
                scopes
                    .iter()
                    .filter_map(|(scope, authority)| {
                        authority.retained.contains(&hash).then_some(*scope)
                    })
                    .collect()
            })
            .unwrap_or_default()
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

    /// Construct an in-memory store whose untagged bytes are collected on the
    /// configured interval.
    ///
    /// Named tags are logical custody claims. Callers that want retention to
    /// release physical bytes use this constructor, store through
    /// [`put_bytes_named`](Self::put_bytes_named), and later delete that tag.
    pub fn new_collecting(interval: Duration) -> Self {
        Self {
            store: Backing::Memory(MemStore::new_with_opts(MemOptions {
                gc_config: Some(GcConfig {
                    interval,
                    add_protected: None,
                }),
            })),
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
        let store = FsStore::load(root).await.map_err(|e| {
            BlobError::Backend(format!("open blob store at {}: {e:?}", root.display()))
        })?;
        Ok(Self {
            store: Backing::File(store),
        })
    }

    /// Open a persistent store whose untagged bytes are collected on the
    /// configured interval.
    pub async fn open_collecting(
        root: impl AsRef<Path>,
        interval: Duration,
    ) -> Result<Self, BlobError> {
        let root = root.as_ref();
        std::fs::create_dir_all(root)
            .map_err(|e| BlobError::Backend(format!("create blob root: {e}")))?;
        let mut options = FsOptions::new(root);
        options.gc = Some(GcConfig {
            interval,
            add_protected: None,
        });
        let store = FsStore::load_with_opts(root.join("blobs.db"), options)
            .await
            .map_err(|e| {
                BlobError::Backend(format!(
                    "open collecting blob store at {}: {e:?}",
                    root.display()
                ))
            })?;
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

    /// Put bytes under a caller-owned stable tag.
    ///
    /// Unlike [`put_bytes`](Self::put_bytes), this does not create an anonymous
    /// permanent tag. Deleting `tag` releases this caller's custody while tags
    /// owned by another mesh or subsystem keep shared content alive.
    #[tracing::instrument(level = "debug", skip(self, bytes, tag))]
    pub async fn put_bytes_named(
        &self,
        bytes: impl Into<Bytes>,
        tag: impl AsRef<[u8]>,
    ) -> Result<BlobHash, BlobError> {
        let bytes: Bytes = bytes.into();
        let byte_count = bytes.len();
        let hash = self
            .store()
            .blobs()
            .add_bytes(bytes)
            .with_named_tag(tag)
            .await
            .map_err(|e| BlobError::Backend(format!("add_bytes named: {e:?}")))?
            .hash;
        let hash = BlobHash(hash);
        tracing::debug!(byte_count, ?hash, "named blob stored");
        Ok(hash)
    }

    /// Add or replace a stable custody tag for bytes already present.
    pub async fn pin(&self, tag: impl AsRef<[u8]>, hash: BlobHash) -> Result<(), BlobError> {
        if !self.has(hash).await? {
            return Err(BlobError::NotFound(hash));
        }
        self.store()
            .tags()
            .set(tag, hash.0)
            .await
            .map_err(|e| BlobError::Backend(format!("set blob tag: {e:?}")))
    }

    /// Delete one logical custody tag.
    ///
    /// Returns whether the tag existed. Physical bytes remain while any other
    /// tag names the same hash and are removed on the store's next configured
    /// garbage-collection pass once no owner remains.
    pub async fn release(&self, tag: impl AsRef<[u8]>) -> Result<bool, BlobError> {
        let removed = self
            .store()
            .tags()
            .delete(tag)
            .await
            .map_err(|e| BlobError::Backend(format!("delete blob tag: {e:?}")))?;
        Ok(removed != 0)
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

    /// Read at most `length` bytes starting at `offset`, with the blob's total
    /// length.
    ///
    /// For serving a blob in pieces to something that cannot take it whole.
    /// Reading through [`get_bytes`](Self::get_bytes) once per piece is
    /// quadratic in the blob's size, which a large transfer notices; this
    /// seeks instead.
    ///
    /// Returns the total length alongside the bytes so a caller can tell a
    /// short read at the end of a blob from a truncated one without asking
    /// again. An `offset` at or past the end yields no bytes rather than an
    /// error, so a caller stepping to the end stops rather than failing.
    ///
    /// A partially fetched blob reads as [`BlobError::NotFound`]. Its later
    /// bytes are absent and its content has not been verified as a whole, so
    /// serving a prefix would hand out bytes nothing has vouched for.
    #[tracing::instrument(level = "debug", skip(self), fields(?hash))]
    pub async fn read_range(
        &self,
        hash: BlobHash,
        offset: u64,
        length: usize,
    ) -> Result<(Bytes, u64), BlobError> {
        use iroh_blobs::api::blobs::BlobStatus;
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        // One call for both questions: is it wholly here, and how big is it.
        // The reader cannot seek from the end, so the size has to come from
        // the store rather than from the stream.
        let status = self
            .store()
            .blobs()
            .status(hash.0)
            .await
            .map_err(|error| BlobError::Backend(format!("read_range status: {error:?}")))?;
        let BlobStatus::Complete { size: total } = status else {
            return Err(BlobError::NotFound(hash));
        };
        if offset >= total {
            return Ok((Bytes::new(), total));
        }
        let mut reader = self.store().blobs().reader(hash.0);
        reader
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|error| BlobError::Backend(format!("read_range seek to {offset}: {error}")))?;
        let want = length.min((total - offset) as usize);
        let mut buffer = vec![0u8; want];
        reader
            .read_exact(&mut buffer)
            .await
            .map_err(|error| BlobError::Backend(format!("read_range read {want}: {error}")))?;
        Ok((Bytes::from(buffer), total))
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

    /// Fetch a blob and retain it under a caller-owned stable tag.
    pub async fn fetch_from_named(
        &self,
        transport: &crate::P2pandaTransport,
        peer: crate::PeerID,
        hash: BlobHash,
        tag: impl AsRef<[u8]>,
    ) -> Result<(), BlobError> {
        self.fetch_from(transport, peer, hash).await?;
        self.pin(tag, hash).await
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
    async fn named_custody_releases_only_its_own_claim() {
        let store = BlobStore::new_collecting(Duration::from_millis(10));
        let hash = store
            .put_bytes_named(Bytes::from_static(b"shared bytes"), b"mesh/a")
            .await
            .unwrap();
        store
            .put_bytes_named(Bytes::from_static(b"shared bytes"), b"eidetic/a")
            .await
            .unwrap();

        assert!(store.release(b"mesh/a").await.unwrap());
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(
            store.has(hash).await.unwrap(),
            "another subsystem's tag keeps shared content alive"
        );

        assert!(store.release(b"eidetic/a").await.unwrap());
        for _ in 0..50 {
            if !store.has(hash).await.unwrap() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("untagged bytes were not garbage-collected");
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

    /// The resident-host backing applies the same named-custody rule as the
    /// in-memory proof store. This covers the constructor Distillery will use
    /// when it gains a process entry point.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn disk_backed_collecting_store_reclaims_released_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("blobs");
        let store = BlobStore::open_collecting(&root, Duration::from_millis(10))
            .await
            .expect("open collecting store");
        let hash = store
            .put_bytes_named(
                Bytes::from_static(b"settled resident bytes"),
                b"mesh/resident",
            )
            .await
            .expect("put named bytes");

        assert!(store.release(b"mesh/resident").await.expect("release"));
        for _ in 0..100 {
            if !store.has(hash).await.expect("has") {
                store.shutdown().await.expect("shutdown");
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        store.shutdown().await.expect("shutdown");
        panic!("the disk-backed collecting store kept released bytes");
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

    #[test]
    fn scoped_authority_requires_reader_and_custody_for_the_exact_hash() {
        let authority = BlobReadAuthorizer::new();
        let space_a = BlobScope::new([0xa1; 32]);
        let space_b = BlobScope::new([0xb1; 32]);
        let peer = [0x31; 32];
        let private_a = BlobHash::from_bytes([0x01; 32]);
        let shared = BlobHash::from_bytes([0x02; 32]);

        authority.allow_reader(space_a, peer);
        authority.allow_reader(space_b, peer);
        authority.retain(space_a, private_a);
        authority.retain(space_a, shared);
        authority.retain(space_b, shared);

        assert!(authority.allows(space_a, &peer, private_a));
        assert!(!authority.allows(space_b, &peer, private_a));
        assert!(authority.allows(space_a, &peer, shared));
        assert!(authority.allows(space_b, &peer, shared));
        assert_eq!(authority.retaining_scopes(shared), vec![space_a, space_b]);

        assert!(authority.release(space_a, &private_a));
        assert!(!authority.allows(space_a, &peer, private_a));
        assert!(authority.deny_reader(space_b, &peer));
        assert!(!authority.allows(space_b, &peer, shared));
        assert!(authority.allows(space_a, &peer, shared));
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn blob_protocol_refuses_a_transport_authenticated_unadmitted_peer() {
        use crate::{P2pandaTransport, PeerID};
        use identity::{IdentityProvider, InMemoryProvider};

        let alice = InMemoryProvider::from_seed([31; 32]);
        let bob = InMemoryProvider::from_seed([32; 32]);
        let charlie = InMemoryProvider::from_seed([33; 32]);
        let alice_blobs = BlobStore::new();
        let bob_blobs = BlobStore::new();
        let charlie_blobs = BlobStore::new();
        let bob_id = PeerID::from_public_key(bob.master_public_key());
        let alice_id = PeerID::from_public_key(alice.master_public_key());
        let access = BlobPeerAuthorizer::from_peers([bob_id.to_bytes()]);

        let alice_transport = P2pandaTransport::bind_with_authorized_blobs(
            alice.master_keypair(),
            vec![],
            &alice_blobs,
            access,
        )
        .await
        .unwrap();
        let bob_transport =
            P2pandaTransport::bind_with_blobs(bob.master_keypair(), vec![], Some(&bob_blobs))
                .await
                .unwrap();
        let charlie_transport = P2pandaTransport::bind_with_blobs(
            charlie.master_keypair(),
            vec![],
            Some(&charlie_blobs),
        )
        .await
        .unwrap();
        let alice_addr = alice_transport.endpoint_addr().await.unwrap();
        bob_transport.add_peer(alice_addr.clone()).await.unwrap();
        charlie_transport.add_peer(alice_addr).await.unwrap();

        let payload = Bytes::from_static(b"paired evidence");
        let hash = alice_blobs.put_bytes(payload.clone()).await.unwrap();
        bob_blobs
            .fetch_from(&bob_transport, alice_id, hash)
            .await
            .unwrap();
        assert_eq!(bob_blobs.get_bytes(hash).await.unwrap(), payload);

        let refused = charlie_blobs
            .fetch_from(&charlie_transport, alice_id, hash)
            .await;
        assert!(refused.is_err());
        assert!(!charlie_blobs.has(hash).await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn scoped_blob_serving_refuses_cross_scope_reads_and_live_revocation() {
        use crate::{P2pandaTransport, PeerID};
        use identity::{IdentityProvider, InMemoryProvider};

        let alice_a = InMemoryProvider::from_seed([41; 32]);
        let alice_b = InMemoryProvider::from_seed([42; 32]);
        let bob = InMemoryProvider::from_seed([43; 32]);
        let charlie = InMemoryProvider::from_seed([44; 32]);
        let owner_blobs = BlobStore::new();
        let bob_blobs = BlobStore::new();
        let charlie_blobs = BlobStore::new();
        let authority = BlobReadAuthorizer::new();
        let space_a = BlobScope::new([0xa2; 32]);
        let space_b = BlobScope::new([0xb2; 32]);
        let bob_id = PeerID::from_public_key(bob.master_public_key());
        let charlie_id = PeerID::from_public_key(charlie.master_public_key());
        let alice_a_id = PeerID::from_public_key(alice_a.master_public_key());
        let alice_b_id = PeerID::from_public_key(alice_b.master_public_key());
        for peer in [bob_id.to_bytes(), charlie_id.to_bytes()] {
            authority.allow_reader(space_a, peer);
            authority.allow_reader(space_b, peer);
        }

        let a_only = owner_blobs
            .put_bytes(Bytes::from_static(b"space a only"))
            .await
            .unwrap();
        let b_only = owner_blobs
            .put_bytes(Bytes::from_static(b"space b only"))
            .await
            .unwrap();
        let shared = owner_blobs
            .put_bytes(Bytes::from_static(b"one physical shared blob"))
            .await
            .unwrap();
        let revoked = owner_blobs
            .put_bytes(Bytes::from_static(b"still held after revocation"))
            .await
            .unwrap();
        authority.retain(space_a, a_only);
        authority.retain(space_a, shared);
        authority.retain(space_a, revoked);
        authority.retain(space_b, b_only);
        authority.retain(space_b, shared);

        let host_a = P2pandaTransport::builder(alice_a.master_keypair())
            .scoped_blobs(&owner_blobs, space_a, authority.clone())
            .bind()
            .await
            .unwrap();
        let host_b = P2pandaTransport::builder(alice_b.master_keypair())
            .scoped_blobs(&owner_blobs, space_b, authority.clone())
            .bind()
            .await
            .unwrap();
        let bob_transport =
            P2pandaTransport::bind_with_blobs(bob.master_keypair(), vec![], Some(&bob_blobs))
                .await
                .unwrap();
        let charlie_transport = P2pandaTransport::bind_with_blobs(
            charlie.master_keypair(),
            vec![],
            Some(&charlie_blobs),
        )
        .await
        .unwrap();
        let host_a_addr = host_a.endpoint_addr().await.unwrap();
        let host_b_addr = host_b.endpoint_addr().await.unwrap();
        bob_transport.add_peer(host_a_addr.clone()).await.unwrap();
        bob_transport.add_peer(host_b_addr.clone()).await.unwrap();
        charlie_transport.add_peer(host_a_addr).await.unwrap();
        charlie_transport.add_peer(host_b_addr).await.unwrap();

        bob_blobs
            .fetch_from(&bob_transport, alice_a_id, a_only)
            .await
            .expect("space A reader fetches A-only hash");
        let cross_scope = bob_blobs
            .fetch_from(&bob_transport, alice_a_id, b_only)
            .await;
        assert!(cross_scope.is_err());
        assert!(!bob_blobs.has(b_only).await.unwrap());

        bob_blobs
            .fetch_from(&bob_transport, alice_a_id, shared)
            .await
            .expect("shared hash reads through scope A");
        charlie_blobs
            .fetch_from(&charlie_transport, alice_b_id, shared)
            .await
            .expect("same stored hash reads through scope B");
        assert_eq!(authority.retaining_scopes(shared), vec![space_a, space_b]);

        assert!(authority.allows(space_a, &bob_id.to_bytes(), revoked));
        assert!(authority.release(space_a, &revoked));
        let after_release = bob_blobs
            .fetch_from(&bob_transport, alice_a_id, revoked)
            .await;
        assert!(after_release.is_err());
        assert!(!bob_blobs.has(revoked).await.unwrap());
        assert!(owner_blobs.has(revoked).await.unwrap());

        authority.retain(space_a, revoked);
        assert!(authority.deny_reader(space_a, &charlie_id.to_bytes()));
        let after_unpair = charlie_blobs
            .fetch_from(&charlie_transport, alice_a_id, revoked)
            .await;
        assert!(after_unpair.is_err());
        assert!(!charlie_blobs.has(revoked).await.unwrap());
        assert!(owner_blobs.has(revoked).await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn read_range_walks_a_blob_without_rereading_it() {
        let blobs = BlobStore::new();
        let payload: Vec<u8> = (0..10_000u32).map(|index| (index % 251) as u8).collect();
        let hash = blobs.put_bytes(payload.clone()).await.unwrap();

        // Walk it in pieces the way a chunked reader does.
        let mut assembled = Vec::new();
        loop {
            let (piece, total) = blobs
                .read_range(hash, assembled.len() as u64, 3_000)
                .await
                .unwrap();
            assert_eq!(total, payload.len() as u64);
            if piece.is_empty() {
                break;
            }
            assembled.extend_from_slice(&piece);
        }
        assert_eq!(assembled, payload);

        // A short tail is reported as itself, not padded or refused.
        let (tail, total) = blobs.read_range(hash, 9_990, 500).await.unwrap();
        assert_eq!(tail.len(), 10);
        assert_eq!(total, payload.len() as u64);

        // Past the end stops a walker rather than failing it.
        let (past, _) = blobs.read_range(hash, 10_000, 64).await.unwrap();
        assert!(past.is_empty());

        // A blob that is not held is a distinct, nameable error.
        let absent = BlobHash::from_bytes([0x9f; 32]);
        assert!(matches!(
            blobs.read_range(absent, 0, 8).await,
            Err(BlobError::NotFound(_))
        ));
    }
}
