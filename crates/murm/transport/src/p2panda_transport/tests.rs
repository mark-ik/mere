use super::*;
use crate::TransportKind;
use identity::{IdentityProvider, InMemoryProvider};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn make_inputs(seed: u8) -> (Ed25519Keypair, PeerID) {
    let provider = InMemoryProvider::from_seed([seed; 32]);
    let kp = provider.master_keypair().clone();
    let peer_id = PeerID::from_public_key(provider.master_public_key());
    (kp, peer_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_seed_constructor_preserves_peer_identity() {
    let (keypair, expected_peer) = make_inputs(41);
    let transport = P2pandaTransport::bind_seed(keypair.to_seed(), vec![Alpn::new("mere/test/v1")])
        .await
        .expect("bind from external identity seed");

    assert_eq!(transport.local_peer_id(), expected_peer);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paired_p2panda_transports_round_trip_bytes() {
    let alpn = Alpn::new("mere/test/v1");
    let (alice_kp, alice_id) = make_inputs(1);
    let (bob_kp, bob_id) = make_inputs(2);

    let alice = P2pandaTransport::bind(&alice_kp, vec![alpn.clone()])
        .await
        .expect("bind alice");
    let bob = P2pandaTransport::bind(&bob_kp, vec![alpn.clone()])
        .await
        .expect("bind bob");

    alice
        .add_peer(bob.endpoint_addr().await.unwrap())
        .await
        .expect("alice.add_peer");
    bob.add_peer(alice.endpoint_addr().await.unwrap())
        .await
        .expect("bob.add_peer");

    assert_eq!(alice.local_peer_id(), alice_id);
    assert_eq!(bob.local_peer_id(), bob_id);

    let payload = b"hello over p2panda-net".to_vec();
    let reply = b"hi back".to_vec();

    let alice_task = {
        let alpn = alpn.clone();
        let payload = payload.clone();
        let reply_len = reply.len();
        tokio::spawn(async move {
            let mut s = alice.connect(bob_id, alpn).await.expect("connect");
            s.write_all(&payload).await.expect("alice write");
            s.flush().await.expect("alice flush");
            let mut buf = vec![0u8; reply_len];
            s.read_exact(&mut buf).await.expect("alice read");
            buf
        })
    };

    let accepted = bob.accept(alpn.clone()).await.expect("bob accept");

    // p2panda authenticates its connections, so the accepted session reports
    // the real initiator (plan D4/V4) — not a claim read off the wire.
    assert_eq!(
        accepted.peer,
        Some(alice_id),
        "p2panda must report the authenticated initiator"
    );
    assert_ne!(
        accepted.peer,
        Some(bob_id),
        "the reported peer must be the initiator, not the acceptor"
    );
    assert!(accepted.is_transport_authenticated());
    assert_eq!(accepted.protocol, alpn);
    assert_eq!(accepted.ingress.transport, TransportKind::P2panda);

    let mut bob_stream = accepted.into_stream();
    let mut buf = vec![0u8; payload.len()];
    bob_stream.read_exact(&mut buf).await.expect("bob read");
    assert_eq!(buf, payload);
    bob_stream.write_all(&reply).await.expect("bob write");
    bob_stream.flush().await.expect("bob flush");

    let alice_recv = alice_task.await.expect("alice task");
    assert_eq!(alice_recv, reply);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_keeps_a_small_final_frame_alive_until_acknowledged() {
    let alpn = Alpn::new("mere/final-frame/v1");
    let (alice_kp, alice_id) = make_inputs(11);
    let (bob_kp, bob_id) = make_inputs(12);
    let alice = P2pandaTransport::bind(&alice_kp, vec![alpn.clone()])
        .await
        .expect("bind alice");
    let bob = P2pandaTransport::bind(&bob_kp, vec![alpn.clone()])
        .await
        .expect("bind bob");
    alice
        .add_peer(bob.endpoint_addr().await.unwrap())
        .await
        .expect("alice.add_peer");
    bob.add_peer(alice.endpoint_addr().await.unwrap())
        .await
        .expect("bob.add_peer");
    let (server_finished_tx, server_finished_rx) = tokio::sync::oneshot::channel();

    let alice_task = tokio::spawn(async move {
        let mut stream = alice.connect(bob_id, alpn).await.expect("connect");
        stream.write_all(b"request").await.expect("write request");
        stream.flush().await.expect("flush request");
        let mut final_frame = [0; 4];
        stream
            .read_exact(&mut final_frame)
            .await
            .expect("read final frame after responder shutdown");
        let _ = server_finished_rx.await;
        final_frame
    });

    let mut session = bob
        .accept(Alpn::new("mere/final-frame/v1"))
        .await
        .expect("accept");
    assert_eq!(session.peer, Some(alice_id));
    let mut request = [0; 7];
    session
        .stream
        .read_exact(&mut request)
        .await
        .expect("read request");
    assert_eq!(&request, b"request");
    session
        .stream
        .write_all(b"deny")
        .await
        .expect("write final frame");
    session.stream.shutdown().await.expect("finish final frame");
    let _ = server_finished_tx.send(());
    drop(session);

    let final_frame = alice_task.await.expect("alice task");
    assert_eq!(&final_frame, b"deny");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ticket_round_trips_identity_and_registers_the_peer() {
    let (alice_kp, alice_id) = make_inputs(30);
    let (bob_kp, _) = make_inputs(31);
    let alice = P2pandaTransport::bind(&alice_kp, vec![Alpn::new("mere/test/v1")])
        .await
        .expect("bind alice");
    let bob = P2pandaTransport::bind(&bob_kp, vec![Alpn::new("mere/test/v1")])
        .await
        .expect("bind bob");

    // Alice shares a ticket string; bob parses it, which carries alice's
    // identity and registers her transport info for dialing.
    let ticket = alice.ticket().await.expect("alice ticket");
    assert!(
        !ticket.is_empty(),
        "the ticket serializes to a non-empty string"
    );
    let learned = bob
        .add_peer_ticket(&ticket)
        .await
        .expect("bob parses alice's ticket");
    assert_eq!(learned, alice_id, "the ticket round-trips alice's PeerID");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unregistered_alpn_accept_returns_error() {
    let (kp, _) = make_inputs(7);
    let t = P2pandaTransport::bind(&kp, vec![Alpn::new("mere/test/v1")])
        .await
        .expect("bind");
    let err = t
        .accept(Alpn::new("mere/other/v1"))
        .await
        .expect_err("must error");
    assert!(matches!(err, TransportError::AlpnNotRegistered));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binds_with_mdns_discovery() {
    // The discovery capability wires up: the mDNS service spawns off the
    // same endpoint without error. (End-to-end LAN discovery is multi-host
    // and not deterministically testable on a single machine.)
    let (kp, _) = make_inputs(9);
    let t = P2pandaTransport::builder(&kp)
        .alpns(vec![Alpn::new("mere/cable/v1")])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .expect("bind with mdns");
    assert!(t.endpoint_addr().await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binds_with_random_walk_discovery() {
    // The random-walk discovery service spawns off the same endpoint and
    // its walker actors stay alive on the held handle. (End-to-end
    // discovery needs a populated bootstrap set + reachable peers, which is
    // a multi-host deployment concern, not single-machine-deterministic.)
    let (kp, _) = make_inputs(11);
    let t = P2pandaTransport::builder(&kp)
        .alpns(vec![Alpn::new("mere/cable/v1")])
        .discovery()
        .bind()
        .await
        .expect("bind with random-walk discovery");
    assert!(t.endpoint_addr().await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binds_with_mdns_and_random_walk_together() {
    // LAN (mDNS) and internet (random-walk) discovery coexist on one
    // endpoint; both handles are held.
    let (kp, _) = make_inputs(12);
    let cfg = DiscoveryConfig {
        random_walkers_count: 4,
        ..DiscoveryConfig::default()
    };
    let t = P2pandaTransport::builder(&kp)
        .alpns(vec![Alpn::new("mere/cable/v1")])
        .mdns(MdnsDiscoveryMode::Active)
        .discovery_config(cfg)
        .bind()
        .await
        .expect("bind with mdns + random-walk");
    assert!(t.endpoint_addr().await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gossip_propagates_ops_between_subscribed_peers() {
    use tokio_stream::StreamExt;

    let topic = [0x5e; 32];
    let (alice_kp, alice_id) = make_inputs(20);
    let (bob_kp, bob_id) = make_inputs(21);

    let alice = P2pandaTransport::builder(&alice_kp)
        .gossip()
        .bind()
        .await
        .expect("bind alice");
    let bob = P2pandaTransport::builder(&bob_kp)
        .gossip()
        .bind()
        .await
        .expect("bind bob");

    // Explicit bootstrap: cross-register transport addresses and tag the
    // topic so gossip can form the overlay. (Discovery does this in
    // production; here we seed it deterministically.)
    alice
        .add_peer(bob.endpoint_addr().await.unwrap())
        .await
        .unwrap();
    alice.set_topics(bob_id, &[topic]).await.unwrap();
    bob.add_peer(alice.endpoint_addr().await.unwrap())
        .await
        .unwrap();
    bob.set_topics(alice_id, &[topic]).await.unwrap();

    // Both subscribe to the space topic.
    let alice_handle = alice.subscribe(topic).await.expect("alice subscribe");
    let bob_handle = bob.subscribe(topic).await.expect("bob subscribe");
    let mut bob_rx = bob_handle.subscribe();

    // Publish until bob receives (the gossip overlay needs a moment to form
    // the neighbour link), bounded by a timeout. This is a real convergence
    // proof: bytes Alice broadcasts reach Bob over the gossip overlay.
    let payload = b"a synced cabal operation".to_vec();
    let received = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            alice_handle
                .publish(payload.clone())
                .await
                .expect("publish");
            tokio::select! {
                msg = bob_rx.next() => {
                    if let Some(Ok(bytes)) = msg
                        && bytes == payload
                    {
                        break bytes;
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {}
            }
        }
    })
    .await
    .expect("bob received the gossip-broadcast operation within the timeout");

    assert_eq!(
        received, payload,
        "bob received exactly what alice published"
    );
}

/// A known address is not a live path, and the peer directory must say which
/// it has.
///
/// This is the distinction that hid a dead link for hours on 2026-08-03: a
/// firewall dropped every inbound packet to a device while its address stayed
/// in the address book, so the host reported the peer as reachable and looked
/// healthy while nothing replicated. `reachable` answers "do we know where it
/// lives"; `connected` answers "are we talking to it".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_peer_directory_separates_a_known_address_from_a_live_path() {
    use tokio_stream::StreamExt;

    let topic = [0x5b; 32];
    let (alice_kp, alice_id) = make_inputs(90);
    let (bob_kp, bob_id) = make_inputs(91);

    let alice = P2pandaTransport::builder(&alice_kp)
        .gossip()
        .bind()
        .await
        .expect("bind alice");
    let bob = P2pandaTransport::builder(&bob_kp)
        .gossip()
        .bind()
        .await
        .expect("bind bob");

    // Alice learns where Bob lives, and does not speak to him. This is the
    // state a paired-but-unreachable device sits in, and the state that used
    // to be indistinguishable from a working peer.
    alice
        .add_peer(bob.endpoint_addr().await.unwrap())
        .await
        .unwrap();
    alice.set_topics(bob_id, &[topic]).await.unwrap();

    let known = alice
        .peers_for_topic(topic)
        .await
        .expect("directory")
        .into_iter()
        .find(|peer| peer.peer == bob_id)
        .expect("a peer with a registered address is in the directory");
    assert!(known.reachable, "his address is known");
    assert!(
        !known.connected,
        "knowing an address is not a live path: nothing has been sent yet"
    );

    // Now actually talk to him, over the gossip overlay rather than a
    // hand-rolled ALPN: a dial to a protocol the peer does not serve is
    // refused, which produces no path and would prove nothing.
    bob.add_peer(alice.endpoint_addr().await.unwrap())
        .await
        .unwrap();
    bob.set_topics(alice_id, &[topic]).await.unwrap();
    let alice_handle = alice.subscribe(topic).await.expect("alice subscribe");
    let bob_handle = bob.subscribe(topic).await.expect("bob subscribe");
    let mut bob_rx = bob_handle.subscribe();
    let payload = b"traffic that forms a real path".to_vec();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            alice_handle.publish(payload.clone()).await.expect("publish");
            tokio::select! {
                msg = bob_rx.next() => {
                    if let Some(Ok(bytes)) = msg && bytes == payload { break }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {}
            }
        }
    })
    .await
    .expect("the overlay carried a message, so a path exists");

    let connected = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let directory = alice.peers_for_topic(topic).await.expect("directory");
            if let Some(peer) = directory.iter().find(|peer| peer.peer == bob_id)
                && peer.connected
            {
                break true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        connected,
        "a peer the endpoint holds an active path to must report connected"
    );

    // The cached-address rung rides on this: a connected peer's current
    // address set serializes to a ticket that round-trips back into the
    // address book and names the same peer. This is what a device persists
    // and reseeds after a restart.
    let ticket = alice
        .peer_ticket(bob_id)
        .await
        .expect("ticket query")
        .expect("a connected peer has addresses to serialize");
    let parsed = alice
        .add_peer_ticket(&ticket)
        .await
        .expect("the cached hint must round-trip through the ticket codec");
    assert_eq!(parsed, bob_id, "the ticket names the peer it was cached for");
}
