//! Persistent cabal post store, redb-backed.
//!
//! [`PersistentCabalStore`] stores Cable posts in a single redb file.
//! Posts are persisted as their canonical wire bytes
//! ([`crate::cable::wire::encode_post`]) — no separate serialization format
//! is layered on top. Decoding on read uses
//! [`crate::cable::wire::decode_post`].
//!
//! Storing wire bytes directly is a deliberate choice:
//!
//! - One serialization format used everywhere: protocol wire == on-disk
//!   format. No drift, no separate schema to maintain.
//! - Round-trip via `encode_post → decode_post` is already test-covered.
//! - Local-only metadata (read/unread flags, etc.) can be added later as
//!   sibling tables without disturbing the post-bytes table.
//!
//! ## Schema
//!
//! Two tables:
//!
//! - `posts`: `[u8; 32]` (post id) → `&[u8]` (wire bytes)
//! - `channel_posts`: multimap `&str` (channel name) → `&[u8; 32]` (post id)
//!
//! Both writes happen atomically inside a single transaction in
//! [`PersistentCabalStore::insert`].
//!
//! ## Iteration order
//!
//! Channel-history iteration returns posts in **post-id-sorted order**
//! (redb's natural multimap iteration), not insertion order. This differs
//! from [`crate::cable::store::InMemoryCabalStore`], which preserves
//! insertion order. Tests that compare across stores must compare *sets*
//! of posts, not ordered sequences. Phase 2C may add a separate
//! timestamp-ordered index when consumers need it.

use std::path::Path;
use std::sync::Arc;

use redb::{
    Database, MultimapTableDefinition, ReadableTable, ReadableTableMetadata, TableDefinition,
};

use crate::cable::wire::{decode_post, encode_post};
use crate::{MurmuringError, Post, PostId};

const POSTS: TableDefinition<&[u8; 32], &[u8]> = TableDefinition::new("posts");
const CHANNEL_POSTS: MultimapTableDefinition<&str, &[u8; 32]> =
    MultimapTableDefinition::new("channel_posts");

/// A redb-backed persistent post store for a Cable cabal.
///
/// Each instance corresponds to one `.redb` file holding one cabal's posts.
/// Per the migration plan §5.1, the host (`murm`) is responsible for
/// choosing the per-cabal directory layout under the user data dir.
pub struct PersistentCabalStore {
    db: Arc<Database>,
}

impl PersistentCabalStore {
    /// Open or create a redb file at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MurmuringError> {
        let db = Database::create(path).map_err(|e| MurmuringError::Backend(e.to_string()))?;
        // Touch the tables once so they exist on disk even if no posts are
        // inserted before the first read.
        let txn = db
            .begin_write()
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        {
            let _ = txn
                .open_table(POSTS)
                .map_err(|e| MurmuringError::Backend(e.to_string()))?;
            let _ = txn
                .open_multimap_table(CHANNEL_POSTS)
                .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Insert a post.
    ///
    /// Returns `Ok(true)` on first insert of a given `post_id`, `Ok(false)`
    /// if the post was already present (content addressing means same id
    /// implies same bytes; we don't overwrite). Errors are storage-layer
    /// (redb) failures.
    pub fn insert(&self, post_id: PostId, post: &Post) -> Result<bool, MurmuringError> {
        let id_bytes: [u8; 32] = *post_id.as_bytes();
        let wire_bytes = encode_post(post);
        let channel_name = post.kind.channel().map(|c| c.as_str().to_string());

        let txn = self
            .db
            .begin_write()
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        let inserted: bool;
        {
            let mut posts_tbl = txn
                .open_table(POSTS)
                .map_err(|e| MurmuringError::Backend(e.to_string()))?;

            let already = posts_tbl
                .get(&id_bytes)
                .map_err(|e| MurmuringError::Backend(e.to_string()))?
                .is_some();
            if already {
                inserted = false;
            } else {
                posts_tbl
                    .insert(&id_bytes, wire_bytes.as_slice())
                    .map_err(|e| MurmuringError::Backend(e.to_string()))?;
                inserted = true;

                if let Some(channel) = &channel_name {
                    let mut chan_tbl = txn
                        .open_multimap_table(CHANNEL_POSTS)
                        .map_err(|e| MurmuringError::Backend(e.to_string()))?;
                    chan_tbl
                        .insert(channel.as_str(), &id_bytes)
                        .map_err(|e| MurmuringError::Backend(e.to_string()))?;
                }
            }
        }
        txn.commit()
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        Ok(inserted)
    }

    /// Look up a post by id.
    pub fn get(&self, post_id: &PostId) -> Result<Option<Post>, MurmuringError> {
        let id_bytes: [u8; 32] = *post_id.as_bytes();
        let txn = self
            .db
            .begin_read()
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        let table = txn
            .open_table(POSTS)
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        let entry = table
            .get(&id_bytes)
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        match entry {
            None => Ok(None),
            Some(value) => {
                let bytes = value.value();
                let post = decode_post(bytes)?;
                Ok(Some(post))
            }
        }
    }

    /// All posts in a channel.
    ///
    /// Returns posts in **post-id-sorted order** (redb multimap natural
    /// order), not insertion order. See module-level docs.
    pub fn channel_posts(&self, channel: &str) -> Result<Vec<Post>, MurmuringError> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        let chan_tbl = txn
            .open_multimap_table(CHANNEL_POSTS)
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        let posts_tbl = txn
            .open_table(POSTS)
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;

        let entries = chan_tbl
            .get(channel)
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;

        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| MurmuringError::Backend(e.to_string()))?;
            let id_bytes = entry.value();
            if let Some(value) = posts_tbl
                .get(&id_bytes)
                .map_err(|e| MurmuringError::Backend(e.to_string()))?
            {
                let bytes = value.value();
                out.push(decode_post(bytes)?);
            }
        }
        Ok(out)
    }

    /// Total post count.
    pub fn len(&self) -> Result<usize, MurmuringError> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        let table = txn
            .open_table(POSTS)
            .map_err(|e| MurmuringError::Backend(e.to_string()))?;
        Ok(table
            .len()
            .map_err(|e| MurmuringError::Backend(e.to_string()))? as usize)
    }

    /// Whether the store has no posts.
    pub fn is_empty(&self) -> Result<bool, MurmuringError> {
        Ok(self.len()? == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cable::hash::hash_post_bytes;
    use crate::cable::sign::sign_post;
    use crate::{ChannelName, PostKind};
    use identity::{IdentityProvider, InMemoryProvider};
    use tempfile::tempdir;

    fn make_text(channel: &str, text: &str, timestamp_ms: u64) -> (PostId, Post) {
        let kp = InMemoryProvider::from_seed([7; 32])
            .derive_keypair(b"persistent-test")
            .unwrap();
        let post = sign_post(
            &kp,
            vec![],
            PostKind::Text {
                channel: ChannelName::new(channel),
                text: text.to_string(),
                timestamp_ms,
            },
        );
        let bytes = encode_post(&post);
        let id = hash_post_bytes(&bytes);
        (id, post)
    }

    #[test]
    fn open_create_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cabal.redb");
        let store = PersistentCabalStore::open(&path).unwrap();
        assert!(store.is_empty().unwrap());
        assert_eq!(store.len().unwrap(), 0);
        assert!(store.channel_posts("session").unwrap().is_empty());
    }

    #[test]
    fn insert_then_get_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cabal.redb");
        let store = PersistentCabalStore::open(&path).unwrap();

        let (id, post) = make_text("session", "hello persistence", 1);
        assert!(store.insert(id, &post).unwrap());
        assert_eq!(store.len().unwrap(), 1);

        let got = store.get(&id).unwrap().expect("post stored");
        assert_eq!(encode_post(&got), encode_post(&post));
    }

    #[test]
    fn duplicate_insert_returns_false_and_doesnt_overwrite() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cabal.redb");
        let store = PersistentCabalStore::open(&path).unwrap();

        let (id, post) = make_text("session", "first", 1);
        assert!(store.insert(id, &post).unwrap());
        assert!(!store.insert(id, &post).unwrap());
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn channel_posts_returns_inserted_posts() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cabal.redb");
        let store = PersistentCabalStore::open(&path).unwrap();

        let (id1, post1) = make_text("session", "one", 1);
        let (id2, post2) = make_text("session", "two", 2);
        let (id3, post3) = make_text("links", "elsewhere", 3);
        store.insert(id1, &post1).unwrap();
        store.insert(id2, &post2).unwrap();
        store.insert(id3, &post3).unwrap();

        let session = store.channel_posts("session").unwrap();
        assert_eq!(session.len(), 2);
        let links = store.channel_posts("links").unwrap();
        assert_eq!(links.len(), 1);
        let notes = store.channel_posts("notes").unwrap();
        assert!(notes.is_empty());
    }

    #[test]
    fn persistence_across_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cabal.redb");

        let (id, post) = make_text("session", "persists across reopen", 42);

        // Write and drop.
        {
            let store = PersistentCabalStore::open(&path).unwrap();
            store.insert(id, &post).unwrap();
            assert_eq!(store.len().unwrap(), 1);
        }

        // Reopen — post should still be there.
        let store = PersistentCabalStore::open(&path).unwrap();
        assert_eq!(store.len().unwrap(), 1);
        let got = store.get(&id).unwrap().expect("post survived reopen");
        if let PostKind::Text { text, .. } = &got.kind {
            assert_eq!(text, "persists across reopen");
        } else {
            panic!("expected Text post");
        }
        // Channel index also persisted.
        assert_eq!(store.channel_posts("session").unwrap().len(), 1);
    }

    #[test]
    fn many_inserts_persist() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cabal.redb");

        // Insert 100 posts in one session.
        let mut ids = Vec::new();
        {
            let store = PersistentCabalStore::open(&path).unwrap();
            for i in 0..100 {
                let (id, post) = make_text("session", &format!("msg-{i}"), i as u64);
                store.insert(id, &post).unwrap();
                ids.push(id);
            }
            assert_eq!(store.len().unwrap(), 100);
        }

        // Reopen — every one is still retrievable by id.
        let store = PersistentCabalStore::open(&path).unwrap();
        assert_eq!(store.len().unwrap(), 100);
        for id in &ids {
            assert!(store.get(id).unwrap().is_some());
        }
        // And all 100 are in the session channel index.
        assert_eq!(store.channel_posts("session").unwrap().len(), 100);
    }

    #[test]
    fn get_unknown_post_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cabal.redb");
        let store = PersistentCabalStore::open(&path).unwrap();
        assert!(store.get(&PostId::new([0xff; 32])).unwrap().is_none());
    }

    #[test]
    fn info_post_has_no_channel_index() {
        // Posts without a channel (Info, Delete) should still insert
        // into the posts table but not appear in any channel.
        let dir = tempdir().unwrap();
        let path = dir.path().join("cabal.redb");
        let store = PersistentCabalStore::open(&path).unwrap();

        let kp = InMemoryProvider::from_seed([7; 32])
            .derive_keypair(b"info-test")
            .unwrap();
        let post = sign_post(
            &kp,
            vec![],
            PostKind::Info {
                entries: vec![crate::InfoEntry::name("alice")],
                timestamp_ms: 1,
            },
        );
        let id = hash_post_bytes(&encode_post(&post));
        store.insert(id, &post).unwrap();

        // Retrievable by id.
        assert!(store.get(&id).unwrap().is_some());
        assert_eq!(store.len().unwrap(), 1);

        // But not under any channel.
        assert!(store.channel_posts("session").unwrap().is_empty());
    }
}
