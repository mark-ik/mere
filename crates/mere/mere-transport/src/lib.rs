//! # Mere-Transport
//!
//! Peer transport layer for the [`mere`](https://crates.io/crates/mere)
//! browser workspace. Wraps [iroh](https://www.iroh.computer) for
//! authenticated, encrypted QUIC streams between known peers, and exposes
//! the [`Transport`] trait the rest of the workspace consumes.
//!
//! ## Design
//!
//! - **Identity** comes from [`mere_identity`]: a peer's [`NodeId`] is
//!   derived from its master Ed25519 public key. Transport never holds the
//!   master secret; it consumes the public key for addressing.
//! - **Streams are byte-oriented**: the [`Transport::Stream`] associated
//!   type is `AsyncRead + AsyncWrite`, so consumers can layer their own
//!   framing (Cable's varint-prefixed, MLS's TLS-style, etc.) on top.
//! - **ALPNs are explicit**: each protocol registers its own ALPN string
//!   (`mere/cable/v1`, `mere/coop/v1`, etc.) so multiple protocols can share
//!   one peer connection without ambiguity.
//! - **Generic over implementation**: consumers take `T: Transport` rather
//!   than `Box<dyn Transport>`, so the same code works against real iroh in
//!   production and against an in-memory transport in tests.
//!
//! ## Status
//!
//! Pre-1.0. The trait surface and the in-memory test fixture
//! ([`memory::MemoryTransport`]) shipped in 0.0.1 (Phase 2B).
//! [`iroh_transport::IrohTransport`] is the real iroh-backed implementation
//! (Phase 2C v0).
//!
//! ## Consumers
//!
//! - [`murm`](https://crates.io/crates/murm) — Cable rides on transport
//!   streams (per inherited Cable spec §2.1)
//! - [`moothold`](https://crates.io/crates/moothold) — community/federation
//!   sync (planned)
//! - Future: [`eidetic`](https://crates.io/crates/eidetic) sync, co-op session orchestration

#![doc(html_root_url = "https://docs.rs/mere-transport/0.0.1")]
#![warn(missing_docs)]

mod alpn;
pub mod blobs;
mod error;
pub mod iroh_transport;
pub mod memory;
mod node_id;
mod transport;

pub use crate::alpn::Alpn;
pub use crate::blobs::{BlobError, BlobHash, BlobStore};
pub use crate::error::TransportError;
pub use crate::iroh_transport::{IrohStream, IrohTransport};
pub use crate::node_id::NodeId;
pub use crate::transport::Transport;

// Re-export commonly-used identity types so consumers don't need a direct
// dependency on `mere-identity` for the basic identity-into-transport flow.
pub use mere_identity::{Ed25519PublicKey, IdentityProvider};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

#[cfg(test)]
mod tests {
    use super::*;
    use mere_identity::InMemoryProvider;

    #[test]
    fn alpn_from_str_round_trips() {
        let alpn = Alpn::new("mere/cable/v1");
        assert_eq!(alpn.as_bytes(), b"mere/cable/v1");
        assert_eq!(alpn.len(), 13);
        assert!(!alpn.is_empty());
    }

    #[test]
    fn empty_alpn_reports_empty() {
        let alpn = Alpn::new("");
        assert!(alpn.is_empty());
        assert_eq!(alpn.len(), 0);
    }

    #[test]
    fn alpn_equality_and_hashing() {
        use std::collections::HashSet;
        let a = Alpn::new("mere/cable/v1");
        let b = Alpn::new("mere/cable/v1");
        let c = Alpn::new("mere/coop/v1");
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        assert!(!set.contains(&c));
    }

    #[test]
    fn node_id_round_trips_through_bytes() {
        let provider = InMemoryProvider::from_seed([42; 32]);
        let pk = provider.master_public_key();
        let node_id = NodeId::from_public_key(pk);
        let bytes = node_id.to_bytes();
        let recovered = NodeId::from_bytes(&bytes).unwrap();
        assert_eq!(node_id, recovered);
    }

    #[test]
    fn node_id_carries_public_key_equality() {
        let p1 = InMemoryProvider::from_seed([7; 32]);
        let p2 = InMemoryProvider::from_seed([7; 32]);
        let p3 = InMemoryProvider::from_seed([8; 32]);

        let n1 = NodeId::from_public_key(p1.master_public_key());
        let n2 = NodeId::from_public_key(p2.master_public_key());
        let n3 = NodeId::from_public_key(p3.master_public_key());

        assert_eq!(n1, n2);
        assert_ne!(n1, n3);
    }

    #[test]
    fn node_id_works_as_hashmap_key() {
        use std::collections::HashMap;
        let p1 = InMemoryProvider::from_seed([1; 32]);
        let p2 = InMemoryProvider::from_seed([2; 32]);
        let n1 = NodeId::from_public_key(p1.master_public_key());
        let n2 = NodeId::from_public_key(p2.master_public_key());

        let mut map = HashMap::new();
        map.insert(n1, "peer-one");
        map.insert(n2, "peer-two");

        assert_eq!(map.get(&n1), Some(&"peer-one"));
        assert_eq!(map.get(&n2), Some(&"peer-two"));
    }

    #[test]
    fn node_id_from_implements_from_pubkey() {
        let p = InMemoryProvider::from_seed([13; 32]);
        let pk = p.master_public_key();
        let node_id_via_from: NodeId = pk.into();
        let node_id_via_method = NodeId::from_public_key(pk);
        assert_eq!(node_id_via_from, node_id_via_method);
    }
}
