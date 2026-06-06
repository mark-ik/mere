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
use tokio::sync::broadcast;

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

/// Per-cabal capacity of the live-`subscribe` broadcast buffer. A consumer that
/// falls more than this many posts behind gets a `Lagged` error and should
/// reconcile from [`CableEngine::channel_history`]; 256 is generous for
/// interactive use.
const EVENT_CHANNEL_CAPACITY: usize = 256;

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
    /// Live-`subscribe` fan-out. Every post that lands *for the first time* —
    /// authored locally, gossiped, or caught up via LogSync — is broadcast here
    /// once (gated on the store's first-insert, so a post arriving on two lanes
    /// emits once). Subscribers see posts stored after they subscribe; the
    /// backlog is [`channel_history`](CableEngine::channel_history).
    events: broadcast::Sender<Post>,
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
                events: broadcast::channel(EVENT_CHANNEL_CAPACITY).0,
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
        let inserted = session.store.insert(post_id, &post)?;
        *head = (seq_num + 1, Some(post_id));
        drop(head);
        // Fan out to live subscribers. Gated on first insert (a re-authored id is
        // a no-op) so each post emits once; ignore "no subscribers".
        if inserted {
            let _ = session.events.send(post);
        }
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
        let inserted = session.store.insert(post_id, &post)?;
        // Fan out to live subscribers, once. A post arriving on both the gossip
        // and LogSync lanes inserts once, so it emits once.
        if inserted {
            let _ = session.events.send(post);
        }
        Ok(post_id)
    }

    /// Subscribe to posts as they land in a cabal.
    ///
    /// The returned receiver yields each post stored *after* it subscribes —
    /// authored locally, gossiped by a peer, or caught up via LogSync — exactly
    /// once (a post arriving on two lanes lands, and emits, once). It does not
    /// replay the backlog; pair it with
    /// [`channel_history`](Self::channel_history) for what came before. A slow
    /// consumer that overruns the buffer gets a `Lagged` error and should
    /// reconcile from history. Errors only if the cabal isn't open.
    pub fn subscribe(
        &self,
        cabal_id: &[u8; 32],
    ) -> Result<broadcast::Receiver<Post>, MurmuringError> {
        Ok(self.session(cabal_id)?.events.subscribe())
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
mod tests;
