use super::*;
use identity::{IdentityProvider, InMemoryProvider};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn make_inputs(seed: u8) -> (Ed25519Keypair, PeerID) {
    let provider = InMemoryProvider::from_seed([seed; 32]);
    let kp = provider.master_keypair().clone();
    let peer_id = PeerID::from_public_key(provider.master_public_key());
    (kp, peer_id)
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

    let mut bob_stream = bob.accept(alpn).await.expect("bob accept");
    let mut buf = vec![0u8; payload.len()];
    bob_stream.read_exact(&mut buf).await.expect("bob read");
    assert_eq!(buf, payload);
    bob_stream.write_all(&reply).await.expect("bob write");
    bob_stream.flush().await.expect("bob flush");

    let alice_recv = alice_task.await.expect("alice task");
    assert_eq!(alice_recv, reply);
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
    assert!(!ticket.is_empty(), "the ticket serializes to a non-empty string");
    let learned = bob.add_peer_ticket(&ticket).await.expect("bob parses alice's ticket");
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
            alice_handle.publish(payload.clone()).await.expect("publish");
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

    assert_eq!(received, payload, "bob received exactly what alice published");
}
