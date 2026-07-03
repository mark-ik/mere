use super::*;
use crate::cable::sign::verify_post;
use identity::InMemoryProvider;

fn engine_with_seed(seed: [u8; 32]) -> CableEngine {
    let provider: Arc<dyn IdentityProvider> = Arc::new(InMemoryProvider::from_seed(seed));
    CableEngine::new(provider)
}

#[test]
fn open_cabal_returns_stable_id() {
    let engine = engine_with_seed([1; 32]);
    let key = [42u8; 32];
    let id1 = engine.open_cabal(key).unwrap();
    let id2 = engine.open_cabal(key).unwrap();
    assert_eq!(id1, id2);
    assert!(engine.has_cabal(&id1));
}

#[test]
fn different_cabal_keys_give_different_ids() {
    let engine = engine_with_seed([1; 32]);
    let id1 = engine.open_cabal([1; 32]).unwrap();
    let id2 = engine.open_cabal([2; 32]).unwrap();
    assert_ne!(id1, id2);
}

#[test]
fn cabal_author_pubkey_is_stable() {
    let engine = engine_with_seed([1; 32]);
    let id = engine.open_cabal([42; 32]).unwrap();
    let pk1 = engine.cabal_author_pubkey(&id).unwrap();
    let pk2 = engine.cabal_author_pubkey(&id).unwrap();
    assert_eq!(pk1.to_bytes(), pk2.to_bytes());
}

#[test]
fn cabal_author_pubkey_matches_identity_derivation() {
    // Manually derive: the engine's pubkey for cabal X should match the
    // identity provider's derive_keypair(cabal_key).public_key().
    let provider: Arc<dyn IdentityProvider> = Arc::new(InMemoryProvider::from_seed([99; 32]));
    let engine = CableEngine::new(provider.clone());
    let cabal_key = [7u8; 32];
    let id = engine.open_cabal(cabal_key).unwrap();

    let engine_pk = engine.cabal_author_pubkey(&id).unwrap();
    let manual_pk = provider.derive_keypair(&cabal_key).unwrap().public_key();
    assert_eq!(engine_pk.to_bytes(), manual_pk.to_bytes());
}

#[test]
fn post_text_signs_and_stores() {
    let engine = engine_with_seed([1; 32]);
    let cabal_id = engine.open_cabal([42; 32]).unwrap();

    let post_id = engine
        .post_text(&cabal_id, "session", "hello", 1_700_000_000_000)
        .unwrap();

    let post = engine.get_post(&cabal_id, &post_id).expect("post stored");

    // Signature verifies.
    assert!(verify_post(&post));

    // Author matches the cabal-derived pubkey.
    let expected_pk = engine.cabal_author_pubkey(&cabal_id).unwrap();
    assert_eq!(post.author.to_bytes(), expected_pk.to_bytes());

    // Content matches what we sent.
    if let PostKind::Text {
        channel,
        text,
        timestamp_ms,
    } = &post.kind
    {
        assert_eq!(channel.as_str(), "session");
        assert_eq!(text, "hello");
        assert_eq!(*timestamp_ms, 1_700_000_000_000);
    } else {
        panic!("expected Text post");
    }
}

#[test]
fn channel_history_returns_inserted_posts_in_order() {
    let engine = engine_with_seed([1; 32]);
    let cabal_id = engine.open_cabal([42; 32]).unwrap();

    for i in 0..5 {
        engine
            .post_text(&cabal_id, "session", &format!("msg-{i}"), i as u64)
            .unwrap();
    }

    let history = engine.channel_history(&cabal_id, "session");
    assert_eq!(history.len(), 5);
    for (i, post) in history.iter().enumerate() {
        assert_eq!(post.kind.timestamp_ms(), i as u64);
    }
}

#[test]
fn channel_isolation() {
    let engine = engine_with_seed([1; 32]);
    let cabal_id = engine.open_cabal([42; 32]).unwrap();

    engine.post_text(&cabal_id, "session", "s1", 1).unwrap();
    engine.post_text(&cabal_id, "session", "s2", 2).unwrap();
    engine.post_text(&cabal_id, "links", "l1", 3).unwrap();

    assert_eq!(engine.channel_history(&cabal_id, "session").len(), 2);
    assert_eq!(engine.channel_history(&cabal_id, "links").len(), 1);
    assert_eq!(engine.channel_history(&cabal_id, "notes").len(), 0);
}

#[test]
fn cabal_isolation() {
    let engine = engine_with_seed([1; 32]);
    let cabal_a = engine.open_cabal([1; 32]).unwrap();
    let cabal_b = engine.open_cabal([2; 32]).unwrap();

    engine
        .post_text(&cabal_a, "session", "in cabal a", 1)
        .unwrap();
    engine
        .post_text(&cabal_b, "session", "in cabal b", 2)
        .unwrap();

    let a_posts = engine.channel_history(&cabal_a, "session");
    let b_posts = engine.channel_history(&cabal_b, "session");

    assert_eq!(a_posts.len(), 1);
    assert_eq!(b_posts.len(), 1);
    if let PostKind::Text { text: a_text, .. } = &a_posts[0].kind {
        assert_eq!(a_text, "in cabal a");
    }
    if let PostKind::Text { text: b_text, .. } = &b_posts[0].kind {
        assert_eq!(b_text, "in cabal b");
    }
}

#[test]
fn close_cabal_drops_session() {
    let engine = engine_with_seed([1; 32]);
    let id = engine.open_cabal([42; 32]).unwrap();
    engine.post_text(&id, "session", "hi", 1).unwrap();

    assert!(engine.has_cabal(&id));
    assert!(engine.close_cabal(&id));
    assert!(!engine.has_cabal(&id));

    // Re-opening creates a fresh session (history lost).
    let id2 = engine.open_cabal([42; 32]).unwrap();
    assert_eq!(id, id2);
    assert!(engine.channel_history(&id2, "session").is_empty());
}

#[test]
fn ingest_post_verifies_signature() {
    // Alice's engine; Bob's engine. Alice posts; Bob ingests via the
    // raw post object (simulating transport delivery).
    let alice_engine = engine_with_seed([10; 32]);
    let bob_engine = engine_with_seed([20; 32]);

    let cabal_key = [42u8; 32];
    let alice_cabal = alice_engine.open_cabal(cabal_key).unwrap();
    let bob_cabal = bob_engine.open_cabal(cabal_key).unwrap();
    // Same cabal_key → same cabal_id even though engines differ.
    assert_eq!(alice_cabal, bob_cabal);

    // Alice posts.
    let post_id = alice_engine
        .post_text(&alice_cabal, "session", "from alice", 1)
        .unwrap();
    let post = alice_engine.get_post(&alice_cabal, &post_id).unwrap();

    // Bob ingests the post; computed id should match.
    let bob_post_id = bob_engine.ingest_post(&bob_cabal, post.clone()).unwrap();
    assert_eq!(post_id, bob_post_id);

    // Bob's history now contains the post.
    let bob_history = bob_engine.channel_history(&bob_cabal, "session");
    assert_eq!(bob_history.len(), 1);
}

#[test]
fn ingest_post_rejects_invalid_signature() {
    let alice_engine = engine_with_seed([10; 32]);
    let bob_engine = engine_with_seed([20; 32]);

    let cabal_key = [42u8; 32];
    let alice_cabal = alice_engine.open_cabal(cabal_key).unwrap();
    let bob_cabal = bob_engine.open_cabal(cabal_key).unwrap();

    let post_id = alice_engine
        .post_text(&alice_cabal, "session", "ok", 1)
        .unwrap();
    let mut post = alice_engine.get_post(&alice_cabal, &post_id).unwrap();

    // Tamper with the body.
    if let PostKind::Text { ref mut text, .. } = post.kind {
        *text = "tampered".to_string();
    }

    let result = bob_engine.ingest_post(&bob_cabal, post);
    assert!(matches!(result, Err(MurmuringError::InvalidSignature)));
    assert!(bob_engine.channel_history(&bob_cabal, "session").is_empty());
}

#[test]
fn ingest_operation_lands_a_received_operation() {
    // The LogSync drain path: alice authors, the post is served as an
    // operation, bob ingests the *operation* (not the encoded post) and his
    // history converges. Same effect as `ingest_post`, different wire form.
    let alice = engine_with_seed([10; 32]);
    let bob = engine_with_seed([20; 32]);
    let cabal_key = [42u8; 32];
    let alice_cabal = alice.open_cabal(cabal_key).unwrap();
    let bob_cabal = bob.open_cabal(cabal_key).unwrap();

    let post_id = alice
        .post_text(&alice_cabal, "session", "via operation", 1)
        .unwrap();
    let post = alice.get_post(&alice_cabal, &post_id).unwrap();
    let op = crate::cable::wire::post_to_operation(&post).unwrap();

    let bob_post_id = bob.ingest_operation(&bob_cabal, &op).unwrap();
    assert_eq!(post_id, bob_post_id, "operation id matches the post id");
    assert_eq!(bob.channel_history(&bob_cabal, "session").len(), 1);
}

#[test]
fn ingest_operation_rejects_foreign_cabal() {
    // The same self-describing-cabal guard applies to operations.
    let engine = engine_with_seed([10; 32]);
    let cabal_a = engine.open_cabal([1; 32]).unwrap();
    let cabal_b = engine.open_cabal([2; 32]).unwrap();
    let post_id = engine.post_text(&cabal_a, "session", "in a", 1).unwrap();
    let post = engine.get_post(&cabal_a, &post_id).unwrap();
    let op = crate::cable::wire::post_to_operation(&post).unwrap();
    assert!(matches!(
        engine.ingest_operation(&cabal_b, &op),
        Err(MurmuringError::CabalMismatch)
    ));
}

#[test]
fn ingest_post_rejects_foreign_cabal() {
    // A validly-signed post authored in cabal A can't be ingested into
    // cabal B: its self-describing cabal_id won't match.
    let engine = engine_with_seed([10; 32]);
    let cabal_a = engine.open_cabal([1; 32]).unwrap();
    let cabal_b = engine.open_cabal([2; 32]).unwrap();

    let post_id = engine.post_text(&cabal_a, "session", "in a", 1).unwrap();
    let post = engine.get_post(&cabal_a, &post_id).unwrap();

    let result = engine.ingest_post(&cabal_b, post);
    assert!(matches!(result, Err(MurmuringError::CabalMismatch)));
    assert!(engine.channel_history(&cabal_b, "session").is_empty());
}

#[test]
fn authoring_forms_a_per_author_log_chain() {
    // Sequential posts in a cabal form a hash-linked single-author log:
    // seq 0,1,2 with each backlink pointing at the previous op's id.
    let engine = engine_with_seed([1; 32]);
    let cabal = engine.open_cabal([42; 32]).unwrap();

    let id0 = engine.post_text(&cabal, "session", "a", 1).unwrap();
    let id1 = engine.post_text(&cabal, "session", "b", 2).unwrap();
    let id2 = engine.post_text(&cabal, "session", "c", 3).unwrap();

    let p0 = engine.get_post(&cabal, &id0).unwrap();
    let p1 = engine.get_post(&cabal, &id1).unwrap();
    let p2 = engine.get_post(&cabal, &id2).unwrap();

    assert_eq!((p0.seq_num, p0.backlink), (0, None));
    assert_eq!((p1.seq_num, p1.backlink), (1, Some(id0)));
    assert_eq!((p2.seq_num, p2.backlink), (2, Some(id1)));

    for p in [&p0, &p1, &p2] {
        assert!(crate::cable::sign::verify_post(p));
    }
}

#[test]
fn engine_implements_bilateral_protocol() {
    let engine = engine_with_seed([1; 32]);
    let p: &dyn BilateralProtocol = &engine;
    assert_eq!(p.name(), "cable");
}

#[test]
fn post_text_on_unopened_cabal_errors() {
    let engine = engine_with_seed([1; 32]);
    let fake_id = [0xff; 32];
    let result = engine.post_text(&fake_id, "session", "hi", 1);
    assert!(matches!(result, Err(MurmuringError::Backend(_))));
}

#[test]
fn all_six_post_helpers_round_trip_through_store() {
    use crate::InfoEntry;

    let engine = engine_with_seed([1; 32]);
    let cabal_id = engine.open_cabal([42; 32]).unwrap();

    // Each post helper composes, signs, stores. Verify retrieval.
    let id_text = engine.post_text(&cabal_id, "session", "txt", 1).unwrap();
    let id_topic = engine.post_topic(&cabal_id, "session", "topic", 2).unwrap();
    let id_join = engine.post_join(&cabal_id, "session", 3).unwrap();
    let id_leave = engine.post_leave(&cabal_id, "session", 4).unwrap();
    let id_info = engine
        .post_info(&cabal_id, vec![InfoEntry::name("alice")], 5)
        .unwrap();
    let id_delete = engine.post_delete(&cabal_id, vec![id_text], 6).unwrap();

    let ids = [id_text, id_topic, id_join, id_leave, id_info, id_delete];
    // All ids are distinct (different content → different hash).
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(ids[i], ids[j], "ids[{i}] and ids[{j}] should differ");
        }
    }

    // All retrievable.
    for id in ids {
        assert!(engine.get_post(&cabal_id, &id).is_some());
    }

    // Channel-scoped posts in session: text, topic, join, leave (4)
    // Info and Delete are channel-less.
    let session_history = engine.channel_history(&cabal_id, "session");
    assert_eq!(session_history.len(), 4);
}

#[test]
fn subscribe_emits_locally_authored_posts() {
    let engine = engine_with_seed([1; 32]);
    let cabal_id = engine.open_cabal([42; 32]).unwrap();
    let mut rx = engine.subscribe(&cabal_id).unwrap();

    let id = engine.post_text(&cabal_id, "session", "hello", 1).unwrap();

    let got = rx
        .try_recv()
        .expect("subscriber receives the authored post");
    assert_eq!(hash_post(&got), id);
    assert!(rx.try_recv().is_err(), "nothing else is pending");
}

#[test]
fn subscribe_only_delivers_posts_authored_after_it_subscribes() {
    let engine = engine_with_seed([2; 32]);
    let cabal_id = engine.open_cabal([7; 32]).unwrap();
    // Authored before anyone subscribes — the backlog is not replayed.
    engine.post_text(&cabal_id, "session", "before", 1).unwrap();

    let mut rx = engine.subscribe(&cabal_id).unwrap();
    let after = engine.post_text(&cabal_id, "session", "after", 2).unwrap();

    let got = rx.try_recv().expect("the post-subscribe post arrives");
    assert_eq!(hash_post(&got), after);
    assert!(rx.try_recv().is_err());
}

#[test]
fn subscribe_emits_an_ingested_post_once_across_duplicate_ingest() {
    // Alice authors; Bob ingests the same post twice (as gossip + LogSync would
    // both deliver it). Bob's subscriber sees it once, gated on first insert.
    let alice = engine_with_seed([10; 32]);
    let cabal_key = [0xcd; 32];
    let alice_cabal = alice.open_cabal(cabal_key).unwrap();
    let id = alice
        .post_text(&alice_cabal, "session", "hi bob", 1)
        .unwrap();
    let post = alice.get_post(&alice_cabal, &id).unwrap();

    let bob = engine_with_seed([11; 32]);
    let bob_cabal = bob.open_cabal(cabal_key).unwrap();
    assert_eq!(bob_cabal, alice_cabal, "same key → same cabal id");
    let mut rx = bob.subscribe(&bob_cabal).unwrap();

    bob.ingest_post(&bob_cabal, post.clone()).unwrap();
    bob.ingest_post(&bob_cabal, post).unwrap(); // duplicate: no-op insert

    let got = rx.try_recv().expect("ingested post is emitted");
    assert_eq!(hash_post(&got), id);
    assert!(
        rx.try_recv().is_err(),
        "the duplicate ingest does not re-emit"
    );
}
