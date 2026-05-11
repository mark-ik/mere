/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Iroh [`BlobFetcher`] companion crate for [`eidetic`].
//!
//! Implements `eidetic::BlobFetcher` for `BlobSource::Iroh { ticket }` by
//! parsing the ticket as `"<node-id-hex>/<blob-hash-hex>"`, fetching the
//! blob from the named peer through [`mere_transport`]'s `BlobStore` /
//! `IrohTransport`, and returning the bytes.
//!
//! Returns `Ok(None)` for any other source kind so
//! [`eidetic::manifest::resolve_blob`] can fall through.
//!
//! ## Hash verification
//!
//! `iroh-blobs` already does BLAKE3 verification natively as part of its
//! transfer protocol — fetching a blob by hash and getting bytes whose
//! hash matches is the protocol's basic guarantee. `eidetic::resolve_blob`
//! also re-verifies against the manifest's `content_hash` for
//! defense-in-depth, so two independent BLAKE3 checks happen on every
//! successful iroh fetch.
//!
//! ## Ticket format
//!
//! `"<node-id-hex>/<blob-hash-hex>"` — 64 hex chars, slash, 64 hex chars.
//! Picked for simplicity over iroh-blobs' opaque base32 `BlobTicket`
//! format; both encode the same `(NodeId, Hash)` pair.

#![doc(html_root_url = "https://docs.rs/eidetic-iroh-fetcher/0.0.1")]

use async_trait::async_trait;
use eidetic::{BlobFetcher, BlobSource, Error, Result};
use mere_transport::{BlobHash, BlobStore, IrohTransport, NodeId};
use std::sync::Arc;

/// Iroh-only [`BlobFetcher`].
///
/// Holds shared references to a [`BlobStore`] and [`IrohTransport`] —
/// typically the host's already-running iroh stack. Construction does
/// not start a new iroh node; the references must already be alive.
pub struct IrohFetcher {
    blobs: Arc<BlobStore>,
    transport: Arc<IrohTransport>,
}

impl IrohFetcher {
    /// Construct from shared references to an existing [`BlobStore`] and
    /// [`IrohTransport`].
    pub fn new(blobs: Arc<BlobStore>, transport: Arc<IrohTransport>) -> Self {
        Self { blobs, transport }
    }

    async fn fetch_iroh(&self, ticket: &str) -> Result<Vec<u8>> {
        let (node_id, blob_hash) = parse_ticket(ticket)?;

        // Fetch persists into the local blob store.
        self.blobs
            .fetch_from(&self.transport, node_id, blob_hash)
            .await
            .map_err(|e| Error::new(format!("iroh fetch {ticket}: {e:?}")))?;

        // Then read the bytes out.
        let bytes = self
            .blobs
            .get_bytes(blob_hash)
            .await
            .map_err(|e| Error::new(format!("iroh get_bytes {ticket}: {e:?}")))?;
        Ok(bytes.to_vec())
    }
}

#[async_trait(?Send)]
impl BlobFetcher for IrohFetcher {
    async fn fetch(&mut self, source: &BlobSource) -> Result<Option<Vec<u8>>> {
        match source {
            BlobSource::Iroh { ticket } => self.fetch_iroh(ticket).await.map(Some),
            _ => Ok(None),
        }
    }
}

/// Parse a `"<node-id-hex>/<blob-hash-hex>"` ticket into its parts.
fn parse_ticket(ticket: &str) -> Result<(NodeId, BlobHash)> {
    let (node_part, hash_part) = ticket
        .split_once('/')
        .ok_or_else(|| Error::new(format!("iroh ticket missing '/': {ticket}")))?;
    if node_part.len() != 64 {
        return Err(Error::new(format!(
            "iroh ticket node-id length {} != 64 hex chars",
            node_part.len()
        )));
    }
    if hash_part.len() != 64 {
        return Err(Error::new(format!(
            "iroh ticket blob-hash length {} != 64 hex chars",
            hash_part.len()
        )));
    }
    let node_bytes =
        decode_hex_32(node_part).map_err(|e| Error::new(format!("node-id hex: {e}")))?;
    let hash_bytes =
        decode_hex_32(hash_part).map_err(|e| Error::new(format!("blob-hash hex: {e}")))?;

    let node_id = NodeId::from_bytes(&node_bytes)
        .map_err(|e| Error::new(format!("invalid node-id: {e:?}")))?;
    let blob_hash = BlobHash::from_bytes(hash_bytes);
    Ok((node_id, blob_hash))
}

fn decode_hex_32(hex: &str) -> std::result::Result<[u8; 32], String> {
    let mut out = [0u8; 32];
    for (idx, pair) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(pair).map_err(|_| "non-utf8 hex".to_string())?;
        out[idx] = u8::from_str_radix(s, 16).map_err(|_| format!("bad hex byte: {s}"))?;
    }
    Ok(out)
}

/// Build a `"<node-id-hex>/<blob-hash-hex>"` ticket string from a
/// [`NodeId`] and [`BlobHash`]. Convenience for producers that want to
/// declare iroh sources in a manifest.
pub fn build_ticket(node_id: NodeId, blob_hash: BlobHash) -> String {
    let mut out = String::with_capacity(129);
    for byte in node_id.to_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out.push('/');
    for byte in blob_hash.to_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use mere_identity::{IdentityProvider, InMemoryProvider};

    #[test]
    fn build_then_parse_ticket_round_trips() {
        let node_bytes = [0xAB; 32];
        let hash_bytes = [0xCD; 32];
        // Use a real node-id seed so from_bytes succeeds (32 random bytes
        // aren't always a valid ed25519 public key).
        let provider = InMemoryProvider::from_seed(node_bytes);
        let node_id = NodeId::from_public_key(provider.master_public_key());
        let blob_hash = BlobHash::from_bytes(hash_bytes);

        let ticket = build_ticket(node_id, blob_hash);
        assert_eq!(ticket.len(), 129); // 64 + 1 + 64
        assert!(ticket.contains('/'));

        let (parsed_node, parsed_hash) = parse_ticket(&ticket).unwrap();
        assert_eq!(parsed_node, node_id);
        assert_eq!(parsed_hash, blob_hash);
    }

    #[test]
    fn parse_ticket_rejects_missing_slash() {
        let result = parse_ticket(&"a".repeat(128));
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("missing '/'"));
    }

    #[test]
    fn parse_ticket_rejects_wrong_length() {
        let short = format!("{}/{}", "a".repeat(60), "b".repeat(64));
        let result = parse_ticket(&short);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("node-id length"));

        let short_hash = format!("{}/{}", "a".repeat(64), "b".repeat(60));
        let result = parse_ticket(&short_hash);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("blob-hash length"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn fetch_returns_bytes_from_remote_peer() {
        // Two iroh nodes wired up via mere-transport. Alice has a blob;
        // the IrohFetcher (running on Bob's stack) pulls it through the
        // BlobFetcher trait surface.
        let alice_provider = InMemoryProvider::from_seed([10; 32]);
        let bob_provider = InMemoryProvider::from_seed([20; 32]);

        let alice_kp = alice_provider.master_keypair().clone();
        let bob_kp = bob_provider.master_keypair().clone();

        let alice_blobs = Arc::new(BlobStore::new());
        let bob_blobs = Arc::new(BlobStore::new());

        let alice_transport = Arc::new(
            IrohTransport::bind_with_blobs(&alice_kp, vec![], Some(&alice_blobs))
                .await
                .expect("alice bind"),
        );
        let bob_transport = Arc::new(
            IrohTransport::bind_with_blobs(&bob_kp, vec![], Some(&bob_blobs))
                .await
                .expect("bob bind"),
        );

        alice_transport
            .add_peer(bob_transport.endpoint_addr())
            .unwrap();
        bob_transport
            .add_peer(alice_transport.endpoint_addr())
            .unwrap();

        let alice_node_id = NodeId::from_public_key(alice_provider.master_public_key());

        // Alice puts a blob.
        let payload = Bytes::from_static(b"this blob lives on alice's machine");
        let blob_hash = alice_blobs.put_bytes(payload.clone()).await.unwrap();

        // Bob's IrohFetcher pulls it.
        let ticket = build_ticket(alice_node_id, blob_hash);
        let mut fetcher = IrohFetcher::new(bob_blobs.clone(), bob_transport.clone());
        let result = fetcher
            .fetch(&BlobSource::Iroh { ticket })
            .await
            .expect("fetch ok");
        let bytes = result.expect("Some");
        assert_eq!(bytes.as_slice(), payload.as_ref());
    }

    #[tokio::test]
    async fn falls_through_for_non_iroh_sources() {
        // Construct without spinning up real iroh — non-iroh sources
        // never call into the transport.
        let provider = InMemoryProvider::from_seed([42; 32]);
        let kp = provider.master_keypair().clone();
        let blobs = Arc::new(BlobStore::new());
        let transport = Arc::new(
            IrohTransport::bind_with_blobs(&kp, vec![], Some(&blobs))
                .await
                .unwrap(),
        );
        let mut fetcher = IrohFetcher::new(blobs, transport);

        assert!(matches!(
            fetcher
                .fetch(&BlobSource::Local {
                    key: "blob:abc".to_string(),
                })
                .await,
            Ok(None)
        ));
        assert!(matches!(
            fetcher
                .fetch(&BlobSource::Https {
                    url: "https://example.test/x".to_string(),
                })
                .await,
            Ok(None)
        ));
        assert!(matches!(
            fetcher
                .fetch(&BlobSource::LocalOnlyRef {
                    local_ref: "host:/x".to_string(),
                })
                .await,
            Ok(None)
        ));
    }
}
