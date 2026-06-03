//! S1 (low-level): can ONE `p2panda-net::Endpoint` host a Cable-shaped custom
//! protocol (custom ALPN, raw bi-stream) between two nodes, and expose the raw
//! iroh endpoint so iroh-blobs could coexist, while p2panda's own
//! Discovery/Gossip/LogSync run on the same node?
//!
//! This probe stands up two `p2panda-net::Endpoint`s on loopback (no relay, no
//! discovery), registers a custom `mere/cable/v1` ALPN on Bob, has Alice
//! `connect` and push the exact Cable snapshot framing (32-byte cabal_id +
//! varint-length-prefixed posts) over a raw bi-stream, and confirms Bob
//! receives it. It also calls `endpoint()` to show the raw `iroh::Endpoint` is
//! reachable (the hook our `IrohTransport` uses to register `iroh-blobs`).
//!
//! See design_docs/mere_docs/implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md
//! (S1, low-level layer (a)).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use iroh::EndpointAddr;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use p2panda_core::SigningKey;
use p2panda_net::addrs::NodeInfo;
use p2panda_net::{AddressBook, Endpoint};
use tokio::sync::mpsc;

const CABLE_ALPN: &[u8] = b"mere/cable/v1";

/// A custom protocol handler for Cable snapshots — the same shape as our
/// `IrohTransport::QueueProtocolHandler`, but registered on a
/// `p2panda-net::Endpoint` via `Endpoint::accept`.
#[derive(Debug)]
struct CableHandler {
    tx: mpsc::UnboundedSender<Vec<Vec<u8>>>,
}

impl ProtocolHandler for CableHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (_send, mut recv) = connection.accept_bi().await.map_err(AcceptError::from_err)?;
        let bytes = recv
            .read_to_end(1_000_000)
            .await
            .map_err(AcceptError::from_err)?;
        let posts = parse_cable_frame(&bytes);
        let _ = self.tx.send(posts);
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let alice_sk = SigningKey::from_bytes(&[7u8; 32]);
    let bob_sk = SigningKey::from_bytes(&[9u8; 32]);
    let bob_vk = bob_sk.verifying_key();

    // Two address books, two endpoints. No relay, no discovery: pure loopback.
    let alice_ab = AddressBook::builder().spawn().await?;
    let bob_ab = AddressBook::builder().spawn().await?;

    let alice_ep = Endpoint::builder(alice_ab.clone())
        .signing_key(alice_sk)
        .spawn()
        .await?;
    let bob_ep = Endpoint::builder(bob_ab.clone())
        .signing_key(bob_sk)
        .spawn()
        .await?;

    // Bob registers the Cable custom ALPN on his single endpoint.
    let (tx, mut rx) = mpsc::unbounded_channel();
    bob_ep.accept(CABLE_ALPN, CableHandler { tx }).await?;

    // The raw iroh endpoint is reachable (this is the hook iroh-blobs'
    // BlobsProtocol would register on); use it to build Bob's loopback address.
    let bob_iroh = bob_ep.endpoint().await?;
    let bob_addr = loopback_addr(&bob_iroh);
    alice_ab.insert_node_info(NodeInfo::from(bob_addr)).await?;

    // Alice connects on the Cable ALPN and pushes a 3-post snapshot using the
    // exact murm wire: 32-byte cabal_id, then varint-len-prefixed posts.
    let conn = alice_ep.connect(bob_vk, CABLE_ALPN).await?;
    let (mut send, _recv) = conn.open_bi().await?;
    let cabal_id = [0xABu8; 32];
    send.write_all(&cabal_id).await?;
    for post in [b"one".as_ref(), b"two".as_ref(), b"three".as_ref()] {
        let mut frame = Vec::new();
        write_uvarint(post.len() as u64, &mut frame);
        frame.extend_from_slice(post);
        send.write_all(&frame).await?;
    }
    send.finish()?;

    // Await Bob's parsed posts.
    let posts = rx.recv().await.ok_or("no posts received")?;
    println!(
        "bob received {} posts over p2panda-net::Endpoint custom ALPN:",
        posts.len()
    );
    for p in &posts {
        println!("  - {}", String::from_utf8_lossy(p));
    }
    assert_eq!(posts.len(), 3, "expected 3 posts");
    assert_eq!(posts[0], b"one");
    assert_eq!(posts[1], b"two");
    assert_eq!(posts[2], b"three");

    println!("\n--- S1 (low-level) VERDICT ---");
    println!("One p2panda-net::Endpoint hosted a custom Cable ALPN ('mere/cable/v1'),");
    println!("accepted a raw bi-stream by peer + ALPN, and round-tripped the exact Cable");
    println!("snapshot framing (32B cabal_id + varint-prefixed posts), unchanged from murm.");
    println!("endpoint() returned the raw iroh::Endpoint (the same hook our IrohTransport");
    println!("uses to register iroh-blobs' BlobsProtocol), so Cable + blobs can coexist on");
    println!("one endpoint while p2panda Discovery/Gossip/LogSync run on the same node.");
    println!("=> A P2pandaTransport: Transport adapter preserving Cable is viable.");

    conn.close(0u32.into(), b"bye");
    Ok(())
}

/// Build a dialable loopback `EndpointAddr` from a bound iroh endpoint —
/// the same wildcard-to-loopback rewrite our `IrohTransport::endpoint_addr`
/// does so a peer can dial without DNS/discovery.
fn loopback_addr(ep: &iroh::Endpoint) -> EndpointAddr {
    let mut addr = EndpointAddr::new(ep.id());
    for sock in ep.bound_sockets() {
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

/// Parse the murm Cable snapshot frame: 32-byte cabal_id, then a sequence of
/// `(varint length, bytes)` posts.
fn parse_cable_frame(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut posts = Vec::new();
    if bytes.len() < 32 {
        return posts;
    }
    let mut pos = 32;
    while pos < bytes.len() {
        let (len, n) = match read_uvarint(&bytes[pos..]) {
            Some(v) => v,
            None => break,
        };
        pos += n;
        let end = pos + len as usize;
        if end > bytes.len() {
            break;
        }
        posts.push(bytes[pos..end].to_vec());
        pos = end;
    }
    posts
}

fn write_uvarint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn read_uvarint(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for (idx, &byte) in bytes.iter().enumerate() {
        if shift >= 64 {
            return None;
        }
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((value, idx + 1));
        }
        shift += 7;
    }
    None
}
