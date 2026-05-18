//! # Murm
//!
//! Bilateral peer-to-peer comms supercrate for the
//! [`mere`](https://crates.io/crates/mere) browser. One-to-one (and small-
//! group) messaging across pluggable protocols (Cable in Phase 2B; MLS, Tox,
//! and others later).
//!
//! See the workspace's
//! [`MURM_AS_BILATERAL.md`](../../../design_docs/murm_docs/technical_architecture/MURM_AS_BILATERAL.md)
//! for the full architectural specification.
//!
//! ## What Murm owns
//!
//! - **Cabal lifecycle** — opening, joining, leaving cabals; cabal-id
//!   addressing; the `Cabal` handle returned to consumers
//! - **Bilateral identity orchestration** — uses
//!   [`mere_identity::IdentityProvider`] for per-cabal keypair derivation
//!   (Cable spec §2.2 pattern)
//! - **Transport orchestration** — uses [`mere_transport::Transport`] for
//!   stream-level peer connections; Murm is generic over the transport
//!   implementation
//! - **Per-protocol routing** — dispatches conversation operations to
//!   whichever [`murmuring::BilateralProtocol`] backs a given cabal
//! - **Co-op session lifecycle** (Phase 2B) — host-led ephemeral sessions
//!   over bilateral transport
//!
//! ## What Murm does NOT own
//!
//! - Master keypair / OS keychain → [`mere_identity`]
//! - iroh / QUIC / ALPN → [`mere_transport`]
//! - Cable wire protocol, MLS, etc. → [`murmuring`] protocol modules
//! - User-facing chat panel UI → graphshell-side Comms applet (separate)
//! - Many-to-many federation → `moothold`
//!
//! ## Status
//!
//! Pre-1.0. Phase 2A foundation: `Murm` struct + `Cabal`/`CabalId`/`CabalKey`
//! types + dependency wiring. Phase 2B will fill in the Cable concrete
//! protocol and the `Cabal::send`/`subscribe`/`history` API.

#![doc(html_root_url = "https://docs.rs/murm/0.0.1")]
#![warn(missing_docs)]

mod cabal;
mod error;

pub use crate::cabal::{CabalHandle, CabalId, CabalKey};
pub use crate::error::MurmError;

// Re-export key types from the layers we sit on, so consumers don't all
// need direct dependencies on the lower crates.
pub use mere_identity::{Ed25519PublicKey, IdentityProvider};
pub use mere_transport::{Alpn, PeerID, Transport};
pub use murmuring::{BilateralProtocol, ChannelName, InfoEntry, Post, PostId, PostKind};

// Re-export Cable's primary entry points so murm consumers don't need a
// direct dependency on murmuring just to send/receive Cable posts.
pub use murmuring::cable::{decode_post, encode_post, hash_post, sign_post, verify_post};

use std::sync::Arc;

use murmuring::CableEngine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
// AsyncWriteExt is used for write_all/flush/shutdown; AsyncReadExt for read_exact/read_to_end.

/// The Mere bilateral-comms supercrate entry point.
///
/// `Murm` orchestrates identity (via [`IdentityProvider`]), transport (via
/// [`Transport`]), and pluggable bilateral protocols (via
/// [`BilateralProtocol`]).
///
/// ## Generic over transport
///
/// `Murm<T: Transport>` takes the transport as a generic parameter, not a
/// `Box<dyn Transport>`. This avoids object-safety issues with
/// `Transport::Stream` and lets the same code work against an iroh-backed
/// transport in production and an in-memory transport in tests.
///
/// ## Identity is `Arc<dyn IdentityProvider>`
///
/// `IdentityProvider` is object-safe (sync methods only at this stage), so
/// using `Arc<dyn ...>` here is fine and gives flexibility — the same
/// identity backend can be shared with `mere-transport` (for `PeerID`
/// derivation) and other consumers without a generic parameter explosion.
///
/// ## Status
///
/// Phase 2A skeleton. Methods that return `Cabal` currently produce a
/// placeholder; the real `open_cabal` / `host_coop` / `join_coop` flows
/// land in Phase 2B once Cable is implemented.
pub struct Murm<T: Transport> {
    identity: Arc<dyn IdentityProvider>,
    transport: T,
    cable: Arc<CableEngine>,
}

impl<T: Transport> Murm<T> {
    /// Construct a `Murm` with a given identity provider and transport.
    pub fn new(identity: Arc<dyn IdentityProvider>, transport: T) -> Self {
        let cable = Arc::new(CableEngine::new(identity.clone()));
        Self {
            identity,
            transport,
            cable,
        }
    }

    /// The local node's `PeerID`, derived from the identity provider's
    /// master public key.
    pub fn local_peer_id(&self) -> PeerID {
        PeerID::from_public_key(self.identity.master_public_key())
    }

    /// Access the underlying identity provider.
    pub fn identity(&self) -> &Arc<dyn IdentityProvider> {
        &self.identity
    }

    /// Access the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Access the underlying [`CableEngine`].
    ///
    /// Useful for advanced operations not yet exposed on
    /// [`CabalHandle`] (e.g. inspecting all open cabals, custom post
    /// composition).
    pub fn cable(&self) -> &Arc<CableEngine> {
        &self.cable
    }

    /// Open or join a Cable cabal by its secret cabal key.
    ///
    /// Derives the per-cabal Ed25519 keypair (Cable spec §2.2),
    /// computes the public cabal id (BLAKE2b of the key), and creates an
    /// in-memory store for the cabal. Idempotent: opening the same key
    /// twice returns equivalent handles backed by the same underlying
    /// session.
    ///
    /// Returns a [`CabalHandle`] for sending and querying posts.
    pub fn open_cabal(&self, cabal_key: &CabalKey) -> Result<CabalHandle, MurmError> {
        let id_bytes = self.cable.open_cabal(*cabal_key.as_bytes())?;
        let cabal_id = CabalId::new(id_bytes);
        Ok(CabalHandle::new(cabal_id, Arc::clone(&self.cable)))
    }

    /// Compute the per-cabal keypair for a given cabal key.
    ///
    /// Per Cable spec §2.2: `child = BLAKE2b(master_seed || cabal_key)`,
    /// `keypair = Ed25519::from_seed(child)`.
    ///
    /// The returned keypair signs cabal posts authored by this user. The
    /// master secret never leaves the identity provider.
    pub fn derive_cabal_keypair(
        &self,
        cabal_key: &CabalKey,
    ) -> Result<mere_identity::Ed25519Keypair, MurmError> {
        Ok(self.identity.derive_keypair(cabal_key.as_bytes())?)
    }

    /// Open one Cable connection to a peer and push all current posts in
    /// the given cabal to them.
    ///
    /// Connection layout:
    /// - 32 bytes: cabal_id (peers verify they share the same cabal)
    /// - Then: a sequence of `(varint length-prefix, encoded post bytes)`
    ///   tuples, one per post
    /// - Sender drops the stream when done; receiver detects EOF and stops
    ///
    /// Returns the number of posts pushed.
    ///
    /// **Phase 2B scope**: a one-shot snapshot push. Live "send-as-you-post"
    /// broadcast is a future chunk; for now the caller invokes
    /// `push_cabal_to_peer` whenever they want to share state.
    pub async fn push_cabal_to_peer(
        &self,
        peer: PeerID,
        cabal_id: &CabalId,
    ) -> Result<usize, MurmError> {
        let alpn = Alpn::new("mere/cable/v1");
        let mut stream = self.transport.connect(peer, alpn).await?;

        // Send cabal_id header.
        stream
            .write_all(cabal_id.as_bytes())
            .await
            .map_err(|e| MurmError::Backend(format!("write cabal_id: {e}")))?;

        // Send all posts in this cabal.
        let posts = self.cable.all_posts(cabal_id.as_bytes());
        let n = posts.len();
        for post in &posts {
            let bytes = encode_post(post);
            let len_prefix = encode_varint_for_len(bytes.len() as u64);
            stream
                .write_all(&len_prefix)
                .await
                .map_err(|e| MurmError::Backend(format!("write len: {e}")))?;
            stream
                .write_all(&bytes)
                .await
                .map_err(|e| MurmError::Backend(format!("write post: {e}")))?;
        }
        stream
            .flush()
            .await
            .map_err(|e| MurmError::Backend(format!("flush: {e}")))?;

        // Clean half-close + ACK-by-EOF:
        // 1. Shutdown the write half (sends QUIC FIN). Peer's reader
        //    sees EOF; their loop returns.
        // 2. Read the recv side to EOF. Peer's drop (after their loop
        //    returns) closes their send half, sending FIN back to us.
        //    This gates our drop on the peer's completion — without it,
        //    our connection-drop would race with their stream-drain and
        //    truncate the last in-flight QUIC frame(s) on a multi-post
        //    push (observed: 2/3 posts arriving on iroh transport).
        stream
            .shutdown()
            .await
            .map_err(|e| MurmError::Backend(format!("shutdown: {e}")))?;
        let mut ack = Vec::new();
        let _ = stream.read_to_end(&mut ack).await;
        Ok(n)
    }

    /// Accept a single incoming Cable connection and ingest posts from
    /// it into the matching cabal's store.
    ///
    /// Returns the number of posts successfully ingested.
    ///
    /// **Phase 2B scope**: handles one connection per call (no internal
    /// loop). Production deployments will call this in a loop on a
    /// dedicated tokio task.
    ///
    /// Errors if the peer's claimed cabal_id is not currently open in
    /// this Murm, or on any I/O / protocol error.
    pub async fn accept_cable_connection(&self) -> Result<usize, MurmError> {
        let alpn = Alpn::new("mere/cable/v1");
        let stream = self.transport.accept(alpn).await?;
        self.handle_cable_stream(stream).await
    }

    /// Internal: process one fully-established Cable stream.
    async fn handle_cable_stream<S>(&self, mut stream: S) -> Result<usize, MurmError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin,
    {
        // Read cabal_id header.
        let mut cabal_id_bytes = [0u8; 32];
        stream
            .read_exact(&mut cabal_id_bytes)
            .await
            .map_err(|e| MurmError::Backend(format!("read cabal_id: {e}")))?;

        if !self.cable.has_cabal(&cabal_id_bytes) {
            return Err(MurmError::CabalNotFound);
        }

        // Read varint-prefixed posts until EOF.
        let mut count = 0usize;
        loop {
            let len = match read_varint_async(&mut stream).await {
                Ok(n) => n,
                Err(_) => break, // EOF or error → done.
            };
            let mut buf = vec![0u8; len as usize];
            if stream.read_exact(&mut buf).await.is_err() {
                break;
            }
            let post = decode_post(&buf)?;
            self.cable.ingest_post(&cabal_id_bytes, post)?;
            count += 1;
        }
        Ok(count)
    }
}

/// LEB128 varint encode for a length prefix. Internal helper for the
/// snapshot-push wire framing.
fn encode_varint_for_len(mut value: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

/// Read a LEB128 varint from an async reader. Returns `Err` on EOF or
/// read error (caller treats either as "done").
async fn read_varint_async<R: AsyncReadExt + Unpin>(reader: &mut R) -> std::io::Result<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    let mut buf = [0u8; 1];
    for _ in 0..10 {
        reader.read_exact(&mut buf).await?;
        let byte = buf[0];
        value |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "varint overflow",
    ))
}

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

#[cfg(test)]
mod tests {
    use super::*;
    use mere_identity::InMemoryProvider;
    use std::sync::Arc;
    use tokio::io::DuplexStream;

    /// A no-op transport for foundation tests. Connect/accept always
    /// refuse; only `local_peer_id` returns a meaningful value. Phase 2B
    /// will replace this with an in-process loopback transport for full
    /// protocol-roundtrip tests.
    struct StubTransport {
        peer_id: PeerID,
    }

    impl StubTransport {
        fn new(peer_id: PeerID) -> Self {
            Self { peer_id }
        }
    }

    impl Transport for StubTransport {
        type Stream = DuplexStream;

        fn local_peer_id(&self) -> PeerID {
            self.peer_id
        }

        async fn connect(
            &self,
            _peer: PeerID,
            _alpn: Alpn,
        ) -> Result<Self::Stream, mere_transport::TransportError> {
            Err(mere_transport::TransportError::ConnectionRefused)
        }

        async fn accept(
            &self,
            _alpn: Alpn,
        ) -> Result<Self::Stream, mere_transport::TransportError> {
            Err(mere_transport::TransportError::ConnectionRefused)
        }
    }

    fn make_murm() -> Murm<StubTransport> {
        let identity: Arc<dyn IdentityProvider> = Arc::new(InMemoryProvider::from_seed([42; 32]));
        let peer_id = PeerID::from_public_key(identity.master_public_key());
        let transport = StubTransport::new(peer_id);
        Murm::new(identity, transport)
    }

    #[test]
    fn murm_constructs_with_identity_and_transport() {
        let _murm = make_murm();
    }

    #[test]
    fn local_peer_id_matches_master_public_key() {
        let murm = make_murm();
        let expected = PeerID::from_public_key(murm.identity().master_public_key());
        assert_eq!(murm.local_peer_id(), expected);
    }

    #[test]
    fn local_peer_id_matches_transport_peer_id_when_consistent() {
        let murm = make_murm();
        // The stub transport in this test is constructed with the same
        // peer_id as the identity, so they match. (This invariant is
        // enforced by transport-layer setup in production; here we just
        // verify the wiring is correct.)
        assert_eq!(murm.local_peer_id(), murm.transport().local_peer_id());
    }

    #[test]
    fn open_cabal_derives_id_from_key() {
        let murm = make_murm();
        let key = CabalKey::new([42; 32]);
        let cabal = murm.open_cabal(&key).unwrap();
        // The id is BLAKE2b of the key — independent of which Murm
        // instance opened it. Two different Murms with the same cabal_key
        // see the same id.
        let murm2 = make_murm();
        let cabal2 = murm2.open_cabal(&key).unwrap();
        assert_eq!(cabal.id(), cabal2.id());
    }

    #[test]
    fn open_cabal_send_text_history_round_trip() {
        let murm = make_murm();
        let cabal = murm.open_cabal(&CabalKey::new([7; 32])).unwrap();

        let id1 = cabal.send_text_at("session", "first", 1).unwrap();
        let id2 = cabal.send_text_at("session", "second", 2).unwrap();
        assert_ne!(id1, id2);

        let history = cabal.history("session");
        assert_eq!(history.len(), 2);

        // Author of stored posts matches what `author_public_key` reports.
        let expected_author = cabal.author_public_key().unwrap();
        for post in &history {
            assert_eq!(post.author.to_bytes(), expected_author.to_bytes());
        }

        // Channel isolation.
        assert!(cabal.history("links").is_empty());
    }

    #[test]
    fn open_cabal_handle_is_clone_and_send() {
        // Compile-check: CabalHandle should be Clone + Send + Sync, so it
        // can be passed across tasks/threads.
        fn assert_send_sync<T: Send + Sync + Clone + 'static>() {}
        assert_send_sync::<CabalHandle>();
    }

    /// **Transport-level sync demo.** Two Murm instances on a paired
    /// memory transport. Alice posts locally; Bob accepts a Cable
    /// connection and ingests Alice's snapshot. After the exchange,
    /// Bob's history matches Alice's.
    #[tokio::test]
    async fn cable_snapshot_sync_via_transport() {
        use mere_transport::memory::MemoryTransport;

        let alice_provider: Arc<dyn IdentityProvider> =
            Arc::new(mere_identity::InMemoryProvider::from_seed([100; 32]));
        let bob_provider: Arc<dyn IdentityProvider> =
            Arc::new(mere_identity::InMemoryProvider::from_seed([200; 32]));

        let alice_node = PeerID::from_public_key(alice_provider.master_public_key());
        let bob_node = PeerID::from_public_key(bob_provider.master_public_key());

        let (alice_t, bob_t) = MemoryTransport::pair(alice_node, bob_node);

        let alice = Murm::new(alice_provider, alice_t);
        let bob = Murm::new(bob_provider, bob_t);

        let cabal_key = CabalKey::new([0xab; 32]);
        let alice_cabal = alice.open_cabal(&cabal_key).unwrap();
        let bob_cabal = bob.open_cabal(&cabal_key).unwrap();
        assert_eq!(alice_cabal.id(), bob_cabal.id());

        // Alice authors three posts locally.
        alice_cabal.send_text_at("session", "one", 1).unwrap();
        alice_cabal.send_text_at("session", "two", 2).unwrap();
        alice_cabal.send_text_at("session", "three", 3).unwrap();
        assert_eq!(alice_cabal.history("session").len(), 3);
        assert!(bob_cabal.history("session").is_empty());

        // Concurrent push (alice → bob) and accept (bob).
        let cabal_id = *alice_cabal.id();
        let push_fut = alice.push_cabal_to_peer(bob_node, &cabal_id);
        let accept_fut = bob.accept_cable_connection();
        let (push_res, accept_res) = tokio::join!(push_fut, accept_fut);

        let pushed = push_res.expect("push succeeded");
        let ingested = accept_res.expect("accept succeeded");
        assert_eq!(pushed, 3, "alice pushed 3 posts");
        assert_eq!(ingested, 3, "bob ingested 3 posts");

        // Bob's history now mirrors alice's.
        assert_eq!(bob_cabal.history("session").len(), 3);
        for post in bob_cabal.history("session") {
            // All ingested posts pass signature verification (ingest_post
            // enforces this); double-check explicitly.
            assert!(verify_post(&post));
        }
    }

    #[test]
    fn two_murms_on_same_cabal_key_can_exchange_posts_via_ingest() {
        // Alice's murm posts; bob's murm ingests via the post object
        // (simulating transport delivery without yet wiring the actual
        // transport-level sync code).
        let alice_provider: Arc<dyn IdentityProvider> =
            Arc::new(mere_identity::InMemoryProvider::from_seed([10; 32]));
        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let alice = Murm::new(alice_provider, StubTransport::new(alice_id));

        let bob_provider: Arc<dyn IdentityProvider> =
            Arc::new(mere_identity::InMemoryProvider::from_seed([20; 32]));
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());
        let bob = Murm::new(bob_provider, StubTransport::new(bob_id));

        let cabal_key = CabalKey::new([0xab; 32]);
        let alice_cabal = alice.open_cabal(&cabal_key).unwrap();
        let bob_cabal = bob.open_cabal(&cabal_key).unwrap();
        // Same cabal_key → same cabal id (public).
        assert_eq!(alice_cabal.id(), bob_cabal.id());

        // Alice posts. Bob's history is still empty (no sync yet).
        let post_id = alice_cabal.send_text_at("session", "hi bob", 1).unwrap();
        assert_eq!(alice_cabal.history("session").len(), 1);
        assert!(bob_cabal.history("session").is_empty());

        // Simulate transport delivery: bob receives the post object.
        let post = alice_cabal.get_post(&post_id).unwrap();
        let bob_post_id = bob_cabal.ingest_post(post).unwrap();
        assert_eq!(post_id, bob_post_id);

        // Bob's history now contains alice's post; signatures verify
        // (ingest_post checks that).
        assert_eq!(bob_cabal.history("session").len(), 1);
    }

    #[test]
    fn derive_cabal_keypair_uses_identity_provider() {
        let murm = make_murm();
        let cabal_key = CabalKey::new([1; 32]);

        // Two derivations from the same key produce equal public keys
        // (deterministic).
        let kp1 = murm.derive_cabal_keypair(&cabal_key).unwrap();
        let kp2 = murm.derive_cabal_keypair(&cabal_key).unwrap();
        assert_eq!(kp1.public_key().to_bytes(), kp2.public_key().to_bytes());

        // Different cabal key → different derived keypair.
        let other = CabalKey::new([2; 32]);
        let kp3 = murm.derive_cabal_keypair(&other).unwrap();
        assert_ne!(kp1.public_key().to_bytes(), kp3.public_key().to_bytes());
    }

    #[test]
    fn cabal_key_debug_redacts_bytes() {
        let key = CabalKey::new([0xaa; 32]);
        let debug = format!("{:?}", key);
        assert!(debug.contains("redacted"));
        assert!(!debug.contains("aa"));
    }

    // ─────────────────────────────────────────────────────────────────
    // End-to-end integration: Cable post roundtrip between two peers
    // ─────────────────────────────────────────────────────────────────

    use mere_transport::memory::MemoryTransport;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Read a LEB128 varint from an async reader.
    async fn read_varint_async(reader: &mut (impl AsyncReadExt + Unpin)) -> u64 {
        let mut value = 0u64;
        let mut shift = 0;
        let mut buf = [0u8; 1];
        loop {
            reader.read_exact(&mut buf).await.unwrap();
            let byte = buf[0];
            value |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        value
    }

    /// Encode a varint into a Vec.
    fn encode_varint_to_vec(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return out;
            }
            out.push(byte | 0x80);
        }
    }

    /// Full pipeline test: Alice signs a Text post and sends it to Bob over
    /// the in-memory paired transport on the Cable ALPN. Bob reads, decodes,
    /// and verifies the signature. Validates: identity derivation, post
    /// signing, wire encoding, transport delivery, decoding, and signature
    /// verification — all the moving parts of Phase 2B working together.
    #[tokio::test]
    async fn cable_text_post_roundtrips_between_peers() {
        // Setup
        let alice_provider: Arc<dyn IdentityProvider> =
            Arc::new(mere_identity::InMemoryProvider::from_seed([11; 32]));
        let bob_provider: Arc<dyn IdentityProvider> =
            Arc::new(mere_identity::InMemoryProvider::from_seed([22; 32]));

        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());

        let (alice_t, bob_t) = MemoryTransport::pair(alice_id, bob_id);

        // Alice signs a post addressed to a (made-up for this test) cabal key.
        let cabal_key = CabalKey::new([0x42; 32]);
        let alice_kp = alice_provider.derive_keypair(cabal_key.as_bytes()).unwrap();

        let post = sign_post(
            &alice_kp,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "hello, bob — this is alice from a real signed post".to_string(),
                timestamp_ms: 1_700_000_000_000,
            },
        );
        let post_bytes = encode_post(&post);

        // Both peers open the Cable ALPN concurrently — Alice connects, Bob
        // accepts.
        let alpn = Alpn::new("mere/cable/v1");
        let (alice_stream_res, bob_stream_res) =
            tokio::join!(alice_t.connect(bob_id, alpn.clone()), bob_t.accept(alpn));
        let mut alice_stream = alice_stream_res.expect("alice connect failed");
        let mut bob_stream = bob_stream_res.expect("bob accept failed");

        // Wire framing: varint length prefix, then post bytes.
        let len_prefix = encode_varint_to_vec(post_bytes.len() as u64);
        alice_stream.write_all(&len_prefix).await.unwrap();
        alice_stream.write_all(&post_bytes).await.unwrap();
        alice_stream.flush().await.unwrap();

        // Bob reads varint length, then the post bytes.
        let post_len = read_varint_async(&mut bob_stream).await as usize;
        assert_eq!(post_len, post_bytes.len());

        let mut received = vec![0u8; post_len];
        bob_stream.read_exact(&mut received).await.unwrap();
        assert_eq!(received, post_bytes, "transport delivered exact bytes");

        // Bob decodes the post.
        let decoded = decode_post(&received).expect("decode failed");

        // Bob verifies the signature.
        assert!(
            verify_post(&decoded),
            "signature should verify — Alice's keypair signed this"
        );

        // Bob can also confirm the author claim matches Alice's *cabal-derived*
        // public key (via Bob's own identity-provider derivation, since both
        // sides know the cabal key out-of-band).
        let bob_view_of_alice_cabal_pubkey = alice_provider
            .derive_keypair(cabal_key.as_bytes())
            .unwrap()
            .public_key();
        assert_eq!(
            decoded.author.to_bytes(),
            bob_view_of_alice_cabal_pubkey.to_bytes(),
            "author should match the cabal-derived public key"
        );

        // And the content.
        match &decoded.kind {
            PostKind::Text {
                channel,
                text,
                timestamp_ms,
            } => {
                assert_eq!(channel.as_str(), "session");
                assert_eq!(text, "hello, bob — this is alice from a real signed post");
                assert_eq!(*timestamp_ms, 1_700_000_000_000);
            }
            _ => panic!("expected Text post"),
        }
    }

    /// **Real iroh transport.** Same shape as
    /// [`cable_snapshot_sync_via_transport`] (memory-transport version) but
    /// using [`IrohTransport`] over loopback QUIC. Validates that the
    /// generic-over-transport `Murm` works against the production
    /// transport without code changes — only test setup differs (binding
    /// real endpoints, cross-registering EndpointAddrs).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cable_snapshot_sync_via_iroh_transport() {
        use mere_transport::IrohTransport;

        let alice_provider = mere_identity::InMemoryProvider::from_seed([101; 32]);
        let bob_provider = mere_identity::InMemoryProvider::from_seed([202; 32]);

        let alice_kp = alice_provider.master_keypair().clone();
        let bob_kp = bob_provider.master_keypair().clone();

        let cable_alpn = Alpn::new("mere/cable/v1");

        let alice_transport = IrohTransport::bind(&alice_kp, vec![cable_alpn.clone()])
            .await
            .expect("alice bind");
        let bob_transport = IrohTransport::bind(&bob_kp, vec![cable_alpn.clone()])
            .await
            .expect("bob bind");

        // Cross-register so connect-by-PeerID works without DNS.
        alice_transport
            .add_peer(bob_transport.endpoint_addr())
            .expect("alice.add_peer");
        bob_transport
            .add_peer(alice_transport.endpoint_addr())
            .expect("bob.add_peer");

        let bob_node = bob_transport.local_peer_id();

        let alice_provider_arc: Arc<dyn IdentityProvider> = Arc::new(alice_provider);
        let bob_provider_arc: Arc<dyn IdentityProvider> = Arc::new(bob_provider);

        let alice = Murm::new(alice_provider_arc, alice_transport);
        let bob = Arc::new(Murm::new(bob_provider_arc, bob_transport));

        let cabal_key = CabalKey::new([0xcd; 32]);
        let alice_cabal = alice.open_cabal(&cabal_key).unwrap();
        let bob_cabal = bob.open_cabal(&cabal_key).unwrap();
        assert_eq!(alice_cabal.id(), bob_cabal.id());

        // Alice authors three posts.
        alice_cabal.send_text_at("session", "uno", 1).unwrap();
        alice_cabal.send_text_at("session", "dos", 2).unwrap();
        alice_cabal.send_text_at("session", "tres", 3).unwrap();
        assert_eq!(alice_cabal.history("session").len(), 3);
        assert!(bob_cabal.history("session").is_empty());

        // Spawn Bob's accept first so the queue is being drained when
        // Alice's connection arrives. (Push and accept don't deadlock here
        // because push writes the cabal_id immediately after connect,
        // unblocking Bob's underlying accept_bi.)
        let bob_for_accept = Arc::clone(&bob);
        let bob_task = tokio::spawn(async move {
            bob_for_accept
                .accept_cable_connection()
                .await
                .expect("bob accept")
        });

        let cabal_id = *alice_cabal.id();
        let pushed = alice
            .push_cabal_to_peer(bob_node, &cabal_id)
            .await
            .expect("alice push");
        let ingested = bob_task.await.expect("bob task");

        assert_eq!(pushed, 3);
        assert_eq!(ingested, 3);

        let bob_history = bob_cabal.history("session");
        assert_eq!(bob_history.len(), 3);
        for post in bob_history {
            assert!(verify_post(&post));
        }
    }

    /// Same shape but tampering the bytes mid-transit should make Bob's
    /// verification fail. Demonstrates the integrity guarantee end-to-end.
    #[tokio::test]
    async fn tampered_post_in_transit_fails_verification() {
        let alice_provider: Arc<dyn IdentityProvider> =
            Arc::new(mere_identity::InMemoryProvider::from_seed([33; 32]));
        let bob_provider: Arc<dyn IdentityProvider> =
            Arc::new(mere_identity::InMemoryProvider::from_seed([44; 32]));

        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());

        let (alice_t, bob_t) = MemoryTransport::pair(alice_id, bob_id);

        let alice_kp = alice_provider
            .derive_keypair(b"some-cabal-salt-32-bytes-please.")
            .unwrap();
        let post = sign_post(
            &alice_kp,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "original message".to_string(),
                timestamp_ms: 1,
            },
        );
        let mut post_bytes = encode_post(&post);

        // Tamper: flip a byte in the body region (after the 96-byte
        // pubkey+signature prefix). Bob should detect this.
        post_bytes[100] ^= 0x01;

        let alpn = Alpn::new("mere/cable/v1");
        let (alice_stream_res, bob_stream_res) =
            tokio::join!(alice_t.connect(bob_id, alpn.clone()), bob_t.accept(alpn));
        let mut alice_stream = alice_stream_res.unwrap();
        let mut bob_stream = bob_stream_res.unwrap();

        let len_prefix = encode_varint_to_vec(post_bytes.len() as u64);
        alice_stream.write_all(&len_prefix).await.unwrap();
        alice_stream.write_all(&post_bytes).await.unwrap();
        alice_stream.flush().await.unwrap();

        let post_len = read_varint_async(&mut bob_stream).await as usize;
        let mut received = vec![0u8; post_len];
        bob_stream.read_exact(&mut received).await.unwrap();

        let decoded = decode_post(&received).expect("decode succeeds; tamper is in body");
        assert!(
            !verify_post(&decoded),
            "tampered post should fail signature verification"
        );
    }
}
