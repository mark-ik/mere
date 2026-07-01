//! Bilateral-lane tests for [`ReticulumTransport`] over TCP loopback.

use std::time::Duration;

use identity::Ed25519Keypair;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{ReticulumInterface, ReticulumTransport};
use crate::{Alpn, PeerID, Transport};

/// A distinct-per-test loopback port. Reticulum's TCP interface takes a string
/// address and does not expose an ephemeral-port readback, so tests pin a port.
fn iface_server(port: u16) -> ReticulumInterface {
    ReticulumInterface::TcpServer {
        bind: format!("127.0.0.1:{port}").parse().unwrap(),
    }
}

fn iface_client(port: u16) -> ReticulumInterface {
    ReticulumInterface::TcpClient {
        addr: format!("127.0.0.1:{port}").parse().unwrap(),
    }
}

#[tokio::test]
async fn derived_identity_is_stable_and_peer_id_matches() {
    let alpn = Alpn::new("mere/cable/v1");
    let kp = Ed25519Keypair::from_seed([7u8; 32]);
    let expected = PeerID::from_public_key(kp.public_key());

    let a = ReticulumTransport::builder(&kp)
        .alpns(vec![alpn.clone()])
        .bind()
        .await
        .expect("bind a");
    let b = ReticulumTransport::builder(&kp)
        .alpns(vec![alpn])
        .bind()
        .await
        .expect("bind b");

    // PeerID is the master public key, independent of the transport.
    assert_eq!(a.local_peer_id(), expected);
    // Same master seed => same Reticulum destination address across instances.
    assert_eq!(a.local_peer_id(), b.local_peer_id());
}

#[tokio::test]
async fn accept_unregistered_alpn_errors() {
    let kp = Ed25519Keypair::from_seed([9u8; 32]);
    let t = ReticulumTransport::builder(&kp)
        .alpns(vec![Alpn::new("mere/cable/v1")])
        .bind()
        .await
        .expect("bind");

    let err = t.accept(Alpn::new("mere/coop/v1")).await.unwrap_err();
    assert!(matches!(err, crate::TransportError::AlpnNotRegistered));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bilateral_round_trip_over_tcp_loopback() {
    let alpn = Alpn::new("mere/cable/v1");
    let port = 47_811u16;

    let server_kp = Ed25519Keypair::from_seed([1u8; 32]);
    let client_kp = Ed25519Keypair::from_seed([2u8; 32]);
    let server_peer = PeerID::from_public_key(server_kp.public_key());

    let server = ReticulumTransport::builder(&server_kp)
        .alpns(vec![alpn.clone()])
        .interfaces(vec![iface_server(port)])
        .announce_interval(Duration::from_millis(500))
        .bind()
        .await
        .expect("bind server");

    let client = ReticulumTransport::builder(&client_kp)
        .alpns(vec![alpn.clone()])
        .interfaces(vec![iface_client(port)])
        .announce_interval(Duration::from_millis(500))
        .connect_timeout(Duration::from_secs(20))
        .bind()
        .await
        .expect("bind client");

    // Server accepts an inbound link and echoes a reply.
    let alpn_server = alpn.clone();
    let accept = tokio::spawn(async move {
        let mut stream = tokio::time::timeout(Duration::from_secs(25), server.accept(alpn_server))
            .await
            .expect("accept timed out")
            .expect("accept failed");
        let mut buf = [0u8; 5];
        stream.read_exact(&mut buf).await.expect("server read");
        assert_eq!(&buf, b"hello");
        stream.write_all(b"world").await.expect("server write");
        stream.flush().await.expect("server flush");
        // Keep the server (and its link) alive long enough for the reply to land.
        tokio::time::sleep(Duration::from_millis(500)).await;
    });

    // Client connects to the server and exchanges one message each way.
    let mut stream = client
        .connect(server_peer, alpn.clone())
        .await
        .expect("client connect");
    stream.write_all(b"hello").await.expect("client write");
    stream.flush().await.expect("client flush");

    let mut reply = [0u8; 5];
    tokio::time::timeout(Duration::from_secs(15), stream.read_exact(&mut reply))
        .await
        .expect("client read timed out")
        .expect("client read");
    assert_eq!(&reply, b"world");

    accept.await.expect("accept task");
}
