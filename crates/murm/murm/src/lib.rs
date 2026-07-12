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
//!   [`identity::IdentityProvider`] for per-cabal keypair derivation
//!   (Cable spec §2.2 pattern)
//! - **Transport orchestration** — uses [`transport::Transport`] for
//!   stream-level peer connections; Murm is generic over the transport
//!   implementation
//! - **Per-protocol routing** — dispatches conversation operations to
//!   whichever [`murmuring::BilateralProtocol`] backs a given cabal
//! - **Co-op session lifecycle** (Phase 2B) — host-led ephemeral sessions
//!   over bilateral transport
//!
//! ## What Murm does NOT own
//!
//! - Master keypair / OS keychain → [`identity`]
//! - iroh / QUIC / ALPN → [`transport`]
//! - Cable wire protocol, MLS, etc. → [`murmuring`] protocol modules
//! - User-facing chat panel UI → graphshell-side Comms applet (separate)
//! - Many-to-many federation → `moothold`
//!
//! ## Status
//!
//! Pre-1.0. Phase 2B: the Cable concrete protocol is in place, and a cabal has a
//! real **send** ([`CabalHandle::send_text`] and siblings), **history**
//! ([`CabalHandle::history`]), and live **subscribe** ([`CabalHandle::subscribe`])
//! API. With a [`P2pandaTransport`], [`SyncedCabal`] adds the two sync lanes
//! (live gossip + LogSync catch-up) on top of the same surface.

#![doc(html_root_url = "https://docs.rs/murm/0.0.1")]
#![warn(missing_docs)]

mod cabal;
mod error;
mod gossip_sync;

pub use crate::cabal::{CabalHandle, CabalId, CabalKey, CabalMembership};
pub use crate::error::MurmError;
pub use crate::gossip_sync::SyncedCabal;

// Re-export key types from the layers we sit on, so consumers don't all
// need direct dependencies on the lower crates.
pub use identity::{Ed25519PublicKey, IdentityProvider};
pub use murmuring::{BilateralProtocol, ChannelName, InfoEntry, Post, PostId, PostKind};
pub use transport::{Alpn, PeerID, Transport};

// Re-export Cable's primary entry points so murm consumers don't need a
// direct dependency on murmuring just to send/receive Cable posts.
pub use murmuring::cable::{decode_post, encode_post, hash_post, sign_post, verify_post};

use std::sync::Arc;

use murmuring::CableEngine;

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
/// identity backend can be shared with `transport` (for `PeerID`
/// derivation) and other consumers without a generic parameter explosion.
///
/// ## Status
///
/// Phase 2B. [`open_cabal`](Murm::open_cabal) returns a working
/// [`CabalHandle`] (send / history / subscribe); over a [`P2pandaTransport`],
/// [`subscribe_cabal`](Murm::subscribe_cabal) returns a [`SyncedCabal`] that
/// also runs the gossip + LogSync lanes. The host-led co-op session flows
/// (`host_coop` / `join_coop`) are still ahead.
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
    /// computes the public cabal id (BLAKE3 of the key), and creates an
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
    /// Per Cable spec §2.2: `child = BLAKE3(master_seed || cabal_key)`,
    /// `keypair = Ed25519::from_seed(child)`.
    ///
    /// The returned keypair signs cabal posts authored by this user. The
    /// master secret never leaves the identity provider.
    pub fn derive_cabal_keypair(
        &self,
        cabal_key: &CabalKey,
    ) -> Result<identity::Ed25519Keypair, MurmError> {
        Ok(self.identity.derive_keypair(cabal_key.as_bytes())?)
    }
}

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

#[cfg(test)]
mod tests {
    use super::*;
    use identity::InMemoryProvider;
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
        ) -> Result<Self::Stream, transport::TransportError> {
            Err(transport::TransportError::ConnectionRefused)
        }

        async fn accept(&self, _alpn: Alpn) -> Result<Self::Stream, transport::TransportError> {
            Err(transport::TransportError::ConnectionRefused)
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
        // The id is BLAKE3 of the key — independent of which Murm
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

    #[test]
    fn two_murms_on_same_cabal_key_can_exchange_posts_via_ingest() {
        // Alice's murm posts; bob's murm ingests via the post object
        // (simulating transport delivery without yet wiring the actual
        // transport-level sync code).
        let alice_provider: Arc<dyn IdentityProvider> =
            Arc::new(identity::InMemoryProvider::from_seed([10; 32]));
        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let alice = Murm::new(alice_provider, StubTransport::new(alice_id));

        let bob_provider: Arc<dyn IdentityProvider> =
            Arc::new(identity::InMemoryProvider::from_seed([20; 32]));
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
    fn cabal_subscribe_emits_authored_and_ingested_posts() {
        // Two peers on the same cabal. Bob subscribes, then sees both a post he
        // authors locally and a post he ingests from alice — each once.
        let alice_provider: Arc<dyn IdentityProvider> =
            Arc::new(identity::InMemoryProvider::from_seed([60; 32]));
        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let alice = Murm::new(alice_provider, StubTransport::new(alice_id));

        let bob_provider: Arc<dyn IdentityProvider> =
            Arc::new(identity::InMemoryProvider::from_seed([61; 32]));
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());
        let bob = Murm::new(bob_provider, StubTransport::new(bob_id));

        let key = CabalKey::new([0xcd; 32]);
        let alice_cabal = alice.open_cabal(&key).unwrap();
        let bob_cabal = bob.open_cabal(&key).unwrap();

        let mut bob_rx = bob_cabal.subscribe().unwrap();

        // Bob authors locally → his subscriber sees it.
        let local_id = bob_cabal.send_text_at("session", "bob local", 1).unwrap();
        let got_local = bob_rx.try_recv().expect("local authored post emitted");
        assert_eq!(hash_post(&got_local), local_id);

        // Alice authors; bob ingests the post object → subscriber sees it.
        let remote_id = alice_cabal
            .send_text_at("session", "from alice", 2)
            .unwrap();
        let post = alice_cabal.get_post(&remote_id).unwrap();
        bob_cabal.ingest_post(post).unwrap();
        let got_remote = bob_rx.try_recv().expect("ingested post emitted");
        assert_eq!(hash_post(&got_remote), remote_id);

        assert!(bob_rx.try_recv().is_err(), "nothing else is pending");
    }

    #[test]
    fn cabal_membership_folds_signed_join_leave_by_author_sequence() {
        let alice_provider: Arc<dyn IdentityProvider> =
            Arc::new(identity::InMemoryProvider::from_seed([70; 32]));
        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let alice = Murm::new(alice_provider, StubTransport::new(alice_id));

        let bob_provider: Arc<dyn IdentityProvider> =
            Arc::new(identity::InMemoryProvider::from_seed([71; 32]));
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());
        let bob = Murm::new(bob_provider, StubTransport::new(bob_id));

        let key = CabalKey::new([0xce; 32]);
        let alice_cabal = alice.open_cabal(&key).unwrap();
        let bob_cabal = bob.open_cabal(&key).unwrap();
        let alice_author = alice_cabal.author_public_key().unwrap().to_bytes();
        let bob_author = bob_cabal.author_public_key().unwrap().to_bytes();

        let alice_join = alice_cabal.send_join_at("secrets", 30).unwrap();
        let bob_join = bob_cabal.send_join_at("secrets", 10).unwrap();
        bob_cabal
            .ingest_post(alice_cabal.get_post(&alice_join).unwrap())
            .unwrap();
        alice_cabal
            .ingest_post(bob_cabal.get_post(&bob_join).unwrap())
            .unwrap();

        let alice_view = alice_cabal.membership("secrets");
        let bob_view = bob_cabal.membership("secrets");
        assert_eq!(alice_view, bob_view);
        assert!(alice_view.contains(&alice_author));
        assert!(alice_view.contains(&bob_author));
        let joined_revision = alice_view.revision;

        // Alice's later per-author operation wins even with an older asserted
        // timestamp; membership never uses wall-clock last-writer-wins.
        let alice_leave = alice_cabal.send_leave_at("secrets", 1).unwrap();
        bob_cabal
            .ingest_post(alice_cabal.get_post(&alice_leave).unwrap())
            .unwrap();
        let left = bob_cabal.membership("secrets");
        assert!(!left.contains(&alice_author));
        assert!(left.contains(&bob_author));
        assert_ne!(left.revision, joined_revision);
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

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use transport::memory::MemoryTransport;

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
            Arc::new(identity::InMemoryProvider::from_seed([11; 32]));
        let bob_provider: Arc<dyn IdentityProvider> =
            Arc::new(identity::InMemoryProvider::from_seed([22; 32]));

        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());

        let (alice_t, bob_t) = MemoryTransport::pair(alice_id, bob_id);

        // Alice signs a post addressed to a (made-up for this test) cabal.
        let cabal_key = CabalKey::new([0x42; 32]);
        let cabal_id = [0x42; 32]; // opaque cabal id; this test exercises the wire
        let alice_kp = alice_provider.derive_keypair(cabal_key.as_bytes()).unwrap();

        let post = sign_post(
            &alice_kp,
            cabal_id,
            0,
            None,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "hello, bob, this is alice from a real signed post".to_string(),
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
                assert_eq!(text, "hello, bob, this is alice from a real signed post");
                assert_eq!(*timestamp_ms, 1_700_000_000_000);
            }
            _ => panic!("expected Text post"),
        }
    }

    /// Same shape but tampering the bytes mid-transit should make Bob's
    /// verification fail. Demonstrates the integrity guarantee end-to-end.
    #[tokio::test]
    async fn tampered_post_in_transit_fails_verification() {
        let alice_provider: Arc<dyn IdentityProvider> =
            Arc::new(identity::InMemoryProvider::from_seed([33; 32]));
        let bob_provider: Arc<dyn IdentityProvider> =
            Arc::new(identity::InMemoryProvider::from_seed([44; 32]));

        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());

        let (alice_t, bob_t) = MemoryTransport::pair(alice_id, bob_id);

        let alice_kp = alice_provider
            .derive_keypair(b"some-cabal-salt-32-bytes-please.")
            .unwrap();
        let post = sign_post(
            &alice_kp,
            [0x42; 32],
            0,
            None,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "original message".to_string(),
                timestamp_ms: 1,
            },
        );
        let mut post_bytes = encode_post(&post);

        // Tamper: flip a byte partway through the encoded operation. Bob must
        // reject it, whether that surfaces as a decode failure or a bad
        // signature.
        let mid = post_bytes.len() / 2;
        post_bytes[mid] ^= 0x01;

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

        let detected = match decode_post(&received) {
            Ok(post) => !verify_post(&post),
            Err(_) => true,
        };
        assert!(
            detected,
            "tampered post must be rejected (decode failure or bad signature)"
        );
    }
}
