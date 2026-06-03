//! Cable protocol concrete runtime.
//!
//! [`CableEngine`] is the concrete implementation of the Cable bilateral
//! chat protocol. It owns:
//!
//! - The user's [`identity::IdentityProvider`] (for per-cabal keypair
//!   derivation)
//! - A map of open cabals, keyed by their derived `cabal_id`
//! - Each cabal's per-cabal Ed25519 keypair (for signing posts as this user)
//! - Each cabal's post store (redb; doubles as the LogSync log/topic store)
//!
//! Cabals are opened via [`CableEngine::open_cabal`] (returns the public
//! cabal id derived from the secret cabal key). Posts are composed via
//! [`CableEngine::post_text`] (and similar future helpers per post type).
//!
//! ## Storage
//!
//! Each cabal is backed by a [`PersistentCabalStore`] (redb), which also
//! implements the p2panda `LogStore` / `TopicStore` that LogSync reconciles, so
//! authoring and sync share one store of record. Cabals use redb's in-memory
//! backend for now (ephemeral); the same store type opens on disk for
//! persistence.
//!
//! ## Transport / sync
//!
//! Phase 2B does **not** yet integrate with [`transport::Transport`].
//! `post_text` composes, signs, and stores locally; sync between peers
//! requires the Cable channel-time-range request protocol on top of
//! `transport` streams, which is a future chunk.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use identity::{Ed25519Keypair, Ed25519PublicKey, IdentityProvider};

use p2panda_core::Operation;

use crate::cable::hash::hash_cabal_id;
use crate::cable::hash_post;
use crate::cable::persistent_store::PersistentCabalStore;
use crate::cable::sign::sign_post;
use crate::cable::wire::{operation_to_post, CabalExt};
use crate::{BilateralProtocol, ChannelName, MurmuringError, Post, PostId, PostKind};

/// Cable protocol concrete runtime.
///
/// Construct with [`CableEngine::new`] given an identity provider. Open
/// cabals with [`CableEngine::open_cabal`].
pub struct CableEngine {
    identity: Arc<dyn IdentityProvider>,
    cabals: Mutex<HashMap<[u8; 32], Arc<CabalSession>>>,
}

struct CabalSession {
    /// The per-cabal Ed25519 keypair, derived from
    /// `IdentityProvider::derive_keypair(&cabal_key)`. Used to sign posts
    /// authored by this user in this cabal.
    keypair: Ed25519Keypair,
    /// Per-cabal post store (redb), which doubles as the p2panda `LogStore` /
    /// `TopicStore` that LogSync reconciles. In-memory backend for now (ephemeral
    /// cabals); the same type opens on disk for persistence.
    store: PersistentCabalStore,
    /// This author's per-cabal log position for the *next* operation:
    /// `(next_seq_num, backlink)`. Starts `(0, None)`; each authored post
    /// advances it to `(seq + 1, Some(new_op_id))`, forming a hash-linked
    /// single-author chain (the unit LogSync reconciles). Behind a mutex so
    /// concurrent authoring in the same cabal serializes (no seq/backlink race).
    author_head: Mutex<(u64, Option<PostId>)>,
}

impl CableEngine {
    /// Construct a `CableEngine` backed by the given identity provider.
    pub fn new(identity: Arc<dyn IdentityProvider>) -> Self {
        Self {
            identity,
            cabals: Mutex::new(HashMap::new()),
        }
    }

    /// Open or rejoin a cabal.
    ///
    /// Derives the per-cabal Ed25519 keypair via the identity provider,
    /// computes the cabal id (BLAKE3 of the key), and creates an empty
    /// in-memory store if one doesn't already exist for this id.
    ///
    /// Returns the cabal id (32 bytes). Idempotent: opening the same cabal
    /// key twice returns the same id and reuses the existing session.
    pub fn open_cabal(&self, cabal_key: [u8; 32]) -> Result<[u8; 32], MurmuringError> {
        let id = hash_cabal_id(&cabal_key);
        let mut cabals = self.cabals.lock().unwrap();
        if let std::collections::hash_map::Entry::Vacant(e) = cabals.entry(id) {
            let keypair = self.identity.derive_keypair(&cabal_key)?;
            e.insert(Arc::new(CabalSession {
                keypair,
                store: PersistentCabalStore::in_memory()?,
                author_head: Mutex::new((0, None)),
            }));
        }
        Ok(id)
    }

    /// Close a cabal session. The in-memory store is dropped.
    ///
    /// Returns `true` if a session was open, `false` if not.
    pub fn close_cabal(&self, cabal_id: &[u8; 32]) -> bool {
        self.cabals.lock().unwrap().remove(cabal_id).is_some()
    }

    /// Whether a cabal session is currently open.
    pub fn has_cabal(&self, cabal_id: &[u8; 32]) -> bool {
        self.cabals.lock().unwrap().contains_key(cabal_id)
    }

    /// Look up a session by cabal id.
    fn session(&self, cabal_id: &[u8; 32]) -> Result<Arc<CabalSession>, MurmuringError> {
        let cabals = self.cabals.lock().unwrap();
        cabals
            .get(cabal_id)
            .cloned()
            .ok_or_else(|| MurmuringError::Backend("cabal not open".to_string()))
    }

    /// Compose, sign, store, and return the id of a new text post.
    pub fn post_text(
        &self,
        cabal_id: &[u8; 32],
        channel: &str,
        text: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmuringError> {
        self.post_with_kind(
            cabal_id,
            PostKind::Text {
                channel: ChannelName::new(channel),
                text: text.to_string(),
                timestamp_ms,
            },
        )
    }

    /// Compose, sign, store, and return the id of a `post/topic`.
    pub fn post_topic(
        &self,
        cabal_id: &[u8; 32],
        channel: &str,
        topic: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmuringError> {
        self.post_with_kind(
            cabal_id,
            PostKind::Topic {
                channel: ChannelName::new(channel),
                topic: topic.to_string(),
                timestamp_ms,
            },
        )
    }

    /// Compose, sign, store, and return the id of a `post/join`.
    pub fn post_join(
        &self,
        cabal_id: &[u8; 32],
        channel: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmuringError> {
        self.post_with_kind(
            cabal_id,
            PostKind::Join {
                channel: ChannelName::new(channel),
                timestamp_ms,
            },
        )
    }

    /// Compose, sign, store, and return the id of a `post/leave`.
    pub fn post_leave(
        &self,
        cabal_id: &[u8; 32],
        channel: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmuringError> {
        self.post_with_kind(
            cabal_id,
            PostKind::Leave {
                channel: ChannelName::new(channel),
                timestamp_ms,
            },
        )
    }

    /// Compose, sign, store, and return the id of a `post/info`.
    pub fn post_info(
        &self,
        cabal_id: &[u8; 32],
        entries: Vec<crate::InfoEntry>,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmuringError> {
        self.post_with_kind(
            cabal_id,
            PostKind::Info {
                entries,
                timestamp_ms,
            },
        )
    }

    /// Compose, sign, store, and return the id of a `post/delete`.
    pub fn post_delete(
        &self,
        cabal_id: &[u8; 32],
        posts: Vec<PostId>,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmuringError> {
        self.post_with_kind(
            cabal_id,
            PostKind::Delete {
                posts,
                timestamp_ms,
            },
        )
    }

    /// Internal helper: sign and store an arbitrary `PostKind`.
    fn post_with_kind(
        &self,
        cabal_id: &[u8; 32],
        kind: PostKind,
    ) -> Result<PostId, MurmuringError> {
        let session = self.session(cabal_id)?;
        // Serialize authoring in this cabal so our per-author log chains without
        // a seq/backlink race.
        let mut head = session.author_head.lock().unwrap();
        let (seq_num, backlink) = *head;
        // Cross-author causal `links` are future work; each post is a DAG root
        // for now. The cabal id is signed so posts are self-describing.
        let post = sign_post(&session.keypair, *cabal_id, seq_num, backlink, vec![], kind);
        let post_id = hash_post(&post);
        // Store first; only advance the per-author log head once the post is
        // durably recorded, so a storage failure doesn't burn a seq number.
        session.store.insert(post_id, &post)?;
        *head = (seq_num + 1, Some(post_id));
        Ok(post_id)
    }

    /// Get a single post by id within a cabal.
    pub fn get_post(&self, cabal_id: &[u8; 32], post_id: &PostId) -> Option<Post> {
        self.session(cabal_id).ok()?.store.get(post_id).ok().flatten()
    }

    /// All posts in a channel of a cabal, in author-asserted time order.
    pub fn channel_history(&self, cabal_id: &[u8; 32], channel: &str) -> Vec<Post> {
        let Ok(session) = self.session(cabal_id) else {
            return Vec::new();
        };
        let mut posts = session.store.channel_posts(channel).unwrap_or_default();
        // The redb store returns posts in post-id order; present chat history in
        // author-asserted time order. Stable sort, so equal timestamps keep the
        // deterministic post-id order. (Cross-author causal ordering is a later
        // projection concern — see the sync plan's open questions.)
        posts.sort_by_key(|p| p.kind.timestamp_ms());
        posts
    }

    /// The cabal-derived public key — the `author` field on posts authored
    /// by this engine in this cabal.
    pub fn cabal_author_pubkey(
        &self,
        cabal_id: &[u8; 32],
    ) -> Result<Ed25519PublicKey, MurmuringError> {
        Ok(self.session(cabal_id)?.keypair.public_key())
    }

    /// A `Clone` of this cabal's store, sharing the same underlying redb.
    ///
    /// Handed to `LogSync::builder` so sync reconciles the very store the engine
    /// authors into (the store is the p2panda `LogStore` + `TopicStore`).
    /// `None` if the cabal isn't open.
    pub fn cabal_store(&self, cabal_id: &[u8; 32]) -> Option<PersistentCabalStore> {
        self.session(cabal_id).ok().map(|s| s.store.clone())
    }

    /// Inject a post that arrived from a peer (e.g. via transport sync).
    ///
    /// Verifies the signature, computes the post id, and inserts into the
    /// cabal's store. Returns the computed post id.
    ///
    /// Used by transport-sync code (to land); also useful in tests for
    /// simulating "a post from another peer arrived."
    pub fn ingest_post(&self, cabal_id: &[u8; 32], post: Post) -> Result<PostId, MurmuringError> {
        // Self-describing events: a post must claim the cabal it's ingested
        // into. This rejects a (validly signed) post replayed from another cabal.
        if post.cabal_id != *cabal_id {
            return Err(MurmuringError::CabalMismatch);
        }
        // Per-author log rule (p2panda `validate_header`): a backlink is present
        // iff seq_num > 0. Contiguity (backlink actually matches the prior op,
        // no gaps) is LogSync's job once topic sync lands; here we reject only
        // structurally-impossible log positions.
        if post.backlink.is_some() != (post.seq_num > 0) {
            return Err(MurmuringError::MalformedPost);
        }
        if !crate::cable::sign::verify_post(&post) {
            return Err(MurmuringError::InvalidSignature);
        }
        let session = self.session(cabal_id)?;
        // The store records the op and advances the per-author log frontier.
        let post_id = hash_post(&post);
        session.store.insert(post_id, &post)?;
        Ok(post_id)
    }

    /// Ingest a p2panda [`Operation`] received from a peer over sync (LogSync's
    /// `OperationReceived`). Converts it to a [`Post`] and runs the same checks
    /// as [`ingest_post`](Self::ingest_post) — signature, self-describing cabal
    /// id, log position. Returns the computed post id.
    ///
    /// This is the offline-catch-up counterpart to the gossip ingest path: where
    /// gossip delivers encoded posts, LogSync delivers reconciled operations.
    pub fn ingest_operation(
        &self,
        cabal_id: &[u8; 32],
        op: &Operation<CabalExt>,
    ) -> Result<PostId, MurmuringError> {
        let post = operation_to_post(op)?;
        self.ingest_post(cabal_id, post)
    }
}

impl BilateralProtocol for CableEngine {
    fn name(&self) -> &str {
        "cable"
    }
}

#[cfg(test)]
mod tests {
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
}
