//! In-memory store for Cable cabal posts.
//!
//! Phase 2B Mode A (ephemeral) backing — posts live in memory and are lost
//! on drop. The persistent Mode B store (fjall + redb + rkyv per the
//! migration plan) is murm-level Phase 2C work.
//!
//! ## Concurrency
//!
//! `InMemoryCabalStore` is `Send + Sync` via interior `Mutex`. All public
//! methods take `&self` and lock briefly. No async/await is used inside
//! locked sections, so a `std::sync::Mutex` is sufficient (no
//! `tokio::sync::Mutex` needed).
//!
//! ## Indexing
//!
//! - Primary: `posts: HashMap<PostId, Post>` — content-addressed lookup
//! - Secondary: `by_channel: HashMap<String, Vec<PostId>>` — preserves
//!   insertion order within a channel for now (causal-DAG topo sort is
//!   future work)

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{Post, PostId};

/// An in-memory cabal post store.
///
/// Thread-safe via an interior `Mutex`. Holds posts indexed by `PostId`
/// with a secondary channel-name index.
#[derive(Default)]
pub struct InMemoryCabalStore {
    inner: Mutex<StoreInner>,
}

#[derive(Default)]
struct StoreInner {
    posts: HashMap<PostId, Post>,
    by_channel: HashMap<String, Vec<PostId>>,
    /// Per-author log frontier: author public-key bytes → (highest `seq_num`
    /// seen, that operation's id). This is what LogSync advertises and requests
    /// per author. (Frontier, not contiguous head: gap-detection/back-fill is
    /// LogSync's job once topic sync is wired.)
    heads: HashMap<[u8; 32], (u64, PostId)>,
}

impl InMemoryCabalStore {
    /// Construct an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a post into the store.
    ///
    /// Returns `true` if the post was new, `false` if a post with the same
    /// `PostId` was already present (in which case the store is unchanged
    /// — content addressing means same id implies same bytes).
    pub fn insert(&self, post_id: PostId, post: Post) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.posts.contains_key(&post_id) {
            return false;
        }
        if let Some(channel) = post.kind.channel() {
            inner
                .by_channel
                .entry(channel.as_str().to_string())
                .or_default()
                .push(post_id);
        }
        // Advance this author's log frontier (highest seq_num seen).
        let author = post.author.to_bytes();
        let advance = inner
            .heads
            .get(&author)
            .map(|&(seq, _)| post.seq_num >= seq)
            .unwrap_or(true);
        if advance {
            inner.heads.insert(author, (post.seq_num, post_id));
        }
        inner.posts.insert(post_id, post);
        true
    }

    /// Look up a post by id.
    pub fn get(&self, post_id: &PostId) -> Option<Post> {
        let inner = self.inner.lock().unwrap();
        inner.posts.get(post_id).cloned()
    }

    /// All posts in a channel, in insertion order.
    pub fn channel_posts(&self, channel: &str) -> Vec<Post> {
        let inner = self.inner.lock().unwrap();
        match inner.by_channel.get(channel) {
            None => Vec::new(),
            Some(ids) => ids
                .iter()
                .filter_map(|id| inner.posts.get(id).cloned())
                .collect(),
        }
    }

    /// Total post count.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().posts.len()
    }

    /// Whether the store has no posts.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().posts.is_empty()
    }

    /// All posts in the store, in id-iteration order (HashMap order — not
    /// stable across runs).
    ///
    /// Used by transport-level sync: sender enumerates and pushes all
    /// known posts to the connecting peer. Order is irrelevant because
    /// the receiver content-addresses each post independently and ingests
    /// idempotently.
    pub fn all_posts(&self) -> Vec<Post> {
        let inner = self.inner.lock().unwrap();
        inner.posts.values().cloned().collect()
    }

    /// The per-author log frontier: the highest `seq_num` seen from `author` in
    /// this cabal and that operation's id, or `None` if we hold nothing from
    /// them. Used by sync to advertise our heads and request what we're missing.
    pub fn author_head(&self, author: &[u8; 32]) -> Option<(u64, PostId)> {
        self.inner.lock().unwrap().heads.get(author).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cable::hash_post;
    use crate::cable::sign::sign_post;
    use crate::cable::wire::encode_post;
    use crate::{ChannelName, PostKind};
    use identity::{IdentityProvider, InMemoryProvider};

    fn make_text(channel: &str, text: &str, timestamp_ms: u64) -> (PostId, Post) {
        let kp = InMemoryProvider::from_seed([7; 32])
            .derive_keypair(b"test")
            .unwrap();
        let post = sign_post(
            &kp,
            [0x07; 32],
            0,
            None,
            vec![],
            PostKind::Text {
                channel: ChannelName::new(channel),
                text: text.to_string(),
                timestamp_ms,
            },
        );
        let id = hash_post(&post);
        (id, post)
    }

    #[test]
    fn empty_store_returns_empty() {
        let store = InMemoryCabalStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.channel_posts("session").is_empty());
        // Note: Post does not implement PartialEq (Ed25519Signature doesn't),
        // so we check via `is_none()` rather than `assert_eq!(.., None)`.
        assert!(store.get(&PostId::new([0; 32])).is_none());
    }

    #[test]
    fn insert_and_get() {
        let store = InMemoryCabalStore::new();
        let (id, post) = make_text("session", "hi", 1);
        assert!(store.insert(id, post.clone()));
        assert_eq!(store.len(), 1);
        let got = store.get(&id).unwrap();
        // Compare via re-encoding (Post doesn't impl PartialEq).
        assert_eq!(encode_post(&got), encode_post(&post));
    }

    #[test]
    fn duplicate_insert_returns_false_and_does_not_overwrite() {
        let store = InMemoryCabalStore::new();
        let (id, post) = make_text("session", "first", 1);
        assert!(store.insert(id, post.clone()));
        assert!(!store.insert(id, post));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn channel_posts_returns_in_insertion_order() {
        let store = InMemoryCabalStore::new();
        let (id1, post1) = make_text("session", "one", 1);
        let (id2, post2) = make_text("session", "two", 2);
        let (id3, post3) = make_text("session", "three", 3);
        store.insert(id1, post1);
        store.insert(id2, post2);
        store.insert(id3, post3);

        let channel = store.channel_posts("session");
        assert_eq!(channel.len(), 3);
        // Verify order via timestamp_ms (which we used 1/2/3 for).
        assert_eq!(channel[0].kind.timestamp_ms(), 1);
        assert_eq!(channel[1].kind.timestamp_ms(), 2);
        assert_eq!(channel[2].kind.timestamp_ms(), 3);
    }

    #[test]
    fn different_channels_indexed_separately() {
        let store = InMemoryCabalStore::new();
        let (id_s, post_s) = make_text("session", "s", 1);
        let (id_l, post_l) = make_text("links", "l", 1);
        store.insert(id_s, post_s);
        store.insert(id_l, post_l);

        assert_eq!(store.channel_posts("session").len(), 1);
        assert_eq!(store.channel_posts("links").len(), 1);
        assert!(store.channel_posts("notes").is_empty());
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn author_head_tracks_the_log_frontier() {
        let store = InMemoryCabalStore::new();
        let kp = InMemoryProvider::from_seed([7; 32])
            .derive_keypair(b"test")
            .unwrap();
        let author = kp.public_key().to_bytes();
        assert!(store.author_head(&author).is_none());

        let op0 = sign_post(
            &kp,
            [0x07; 32],
            0,
            None,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("s"),
                text: "a".to_string(),
                timestamp_ms: 1,
            },
        );
        let id0 = hash_post(&op0);
        assert!(store.insert(id0, op0));
        assert_eq!(store.author_head(&author), Some((0, id0)));

        let op1 = sign_post(
            &kp,
            [0x07; 32],
            1,
            Some(id0),
            vec![],
            PostKind::Text {
                channel: ChannelName::new("s"),
                text: "b".to_string(),
                timestamp_ms: 2,
            },
        );
        let id1 = hash_post(&op1);
        assert!(store.insert(id1, op1.clone()));
        assert_eq!(store.author_head(&author), Some((1, id1)));

        // Re-inserting an op we already hold is an idempotent no-op and does
        // not regress the frontier.
        assert!(!store.insert(id1, op1));
        assert_eq!(store.author_head(&author), Some((1, id1)));
    }
}
