//! Cabal-level types for bilateral comms.
//!
//! A *cabal* in Murm terminology is a long-lived bilateral or small-group
//! conversation — Cable's primary unit of organization. A cabal has:
//!
//! - A 32-byte symmetric **cabal key** ([`CabalKey`], shared secret,
//!   distributed out-of-band among members)
//! - A **cabal id** ([`CabalId`], public identifier, derived from the key
//!   via BLAKE3 — used for addressing and discovery without revealing the
//!   secret)
//! - One or more **named channels** (per Cable spec §2.5: `"session"`,
//!   `"links"`, etc.)
//! - A persistent or ephemeral post store (Phase 2B uses in-memory)
//!
//! Per Cable spec §2.2, the user's per-cabal Ed25519 keypair is derived
//! from `BLAKE3(master_secret || cabal_key)` — that derivation lives in
//! [`identity::IdentityProvider::derive_keypair`].

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use murmuring::CableEngine;

use crate::error::MurmError;
use crate::{Post, PostId};

/// Current wall-clock time as milliseconds since UNIX epoch. Returns 0 if
/// the system clock is before the epoch (effectively unreachable on
/// real systems).
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A cabal's symmetric secret key (32 bytes).
///
/// Distributed out-of-band to members (QR code, invite link, mDNS, etc.).
/// Knowledge of the key is what makes you a cabal member at the protocol
/// level.
///
/// **Security note**: cabal keys are secrets in transit and at rest. Any
/// local persistence should be keychain-protected.
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct CabalKey(pub [u8; 32]);

impl CabalKey {
    /// Construct from raw bytes.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// View as bytes.
    ///
    /// **Use cautiously**: this exposes the secret cabal key. Prefer
    /// derived values ([`CabalId`], the per-cabal keypair via
    /// `derive_keypair`) over raw key bytes wherever possible.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

// Don't auto-derive Debug — it would print the secret bytes. A redacted
// manual impl is safer.
impl std::fmt::Debug for CabalKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CabalKey(<redacted>)")
    }
}

/// A cabal's public identifier (`BLAKE3(cabal_key)`).
///
/// Suitable as a routing identifier without revealing the secret. Two
/// peers who hold the same cabal key compute the same `CabalId`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct CabalId(pub [u8; 32]);

impl CabalId {
    /// Construct from raw bytes.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// View as bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Handle to an open cabal.
///
/// Returned by [`crate::Murm::open_cabal`]. Holds the cabal's public id and
/// a shared reference to the underlying [`CableEngine`] that backs the
/// cabal's storage and signing. `CabalHandle` is `Send + Sync + 'static`
/// (no lifetime tied to `Murm`), so you can clone it across tasks.
#[derive(Clone)]
pub struct CabalHandle {
    cabal_id: CabalId,
    engine: Arc<CableEngine>,
}

impl CabalHandle {
    /// Construct a cabal handle. Internal — `Murm::open_cabal` creates these.
    pub(crate) fn new(cabal_id: CabalId, engine: Arc<CableEngine>) -> Self {
        Self { cabal_id, engine }
    }

    /// The cabal's public identifier.
    pub fn id(&self) -> &CabalId {
        &self.cabal_id
    }

    /// The author public key this user posts under in this cabal —
    /// derived from `(master_secret, cabal_key)` per Cable spec §2.2.
    pub fn author_public_key(&self) -> Result<identity::Ed25519PublicKey, MurmError> {
        Ok(self.engine.cabal_author_pubkey(self.cabal_id.as_bytes())?)
    }

    /// Compose, sign, and store a `post/text` message at the current wall
    /// clock.
    pub fn send_text(&self, channel: &str, text: &str) -> Result<PostId, MurmError> {
        self.send_text_at(channel, text, now_ms())
    }

    /// Compose, sign, and store a `post/text` message with an explicit
    /// timestamp (useful for tests and replays).
    pub fn send_text_at(
        &self,
        channel: &str,
        text: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        Ok(self
            .engine
            .post_text(self.cabal_id.as_bytes(), channel, text, timestamp_ms)?)
    }

    /// Compose, sign, and store a `post/topic` at the current wall clock.
    pub fn send_topic(&self, channel: &str, topic: &str) -> Result<PostId, MurmError> {
        self.send_topic_at(channel, topic, now_ms())
    }

    /// Compose, sign, and store a `post/topic` with an explicit timestamp.
    pub fn send_topic_at(
        &self,
        channel: &str,
        topic: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        Ok(self
            .engine
            .post_topic(self.cabal_id.as_bytes(), channel, topic, timestamp_ms)?)
    }

    /// Compose, sign, and store a `post/join` at the current wall clock.
    pub fn send_join(&self, channel: &str) -> Result<PostId, MurmError> {
        self.send_join_at(channel, now_ms())
    }

    /// Compose, sign, and store a `post/join` with an explicit timestamp.
    pub fn send_join_at(&self, channel: &str, timestamp_ms: u64) -> Result<PostId, MurmError> {
        Ok(self
            .engine
            .post_join(self.cabal_id.as_bytes(), channel, timestamp_ms)?)
    }

    /// Compose, sign, and store a `post/leave` at the current wall clock.
    pub fn send_leave(&self, channel: &str) -> Result<PostId, MurmError> {
        self.send_leave_at(channel, now_ms())
    }

    /// Compose, sign, and store a `post/leave` with an explicit timestamp.
    pub fn send_leave_at(&self, channel: &str, timestamp_ms: u64) -> Result<PostId, MurmError> {
        Ok(self
            .engine
            .post_leave(self.cabal_id.as_bytes(), channel, timestamp_ms)?)
    }

    /// Compose, sign, and store a `post/info` at the current wall clock.
    pub fn send_info(&self, entries: Vec<murmuring::InfoEntry>) -> Result<PostId, MurmError> {
        self.send_info_at(entries, now_ms())
    }

    /// Compose, sign, and store a `post/info` with an explicit timestamp.
    pub fn send_info_at(
        &self,
        entries: Vec<murmuring::InfoEntry>,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        Ok(self
            .engine
            .post_info(self.cabal_id.as_bytes(), entries, timestamp_ms)?)
    }

    /// Compose, sign, and store a `post/delete` at the current wall clock.
    pub fn send_delete(&self, posts: Vec<PostId>) -> Result<PostId, MurmError> {
        self.send_delete_at(posts, now_ms())
    }

    /// Compose, sign, and store a `post/delete` with an explicit timestamp.
    pub fn send_delete_at(
        &self,
        posts: Vec<PostId>,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        Ok(self
            .engine
            .post_delete(self.cabal_id.as_bytes(), posts, timestamp_ms)?)
    }

    /// Look up a post by id.
    pub fn get_post(&self, post_id: &PostId) -> Option<Post> {
        self.engine.get_post(self.cabal_id.as_bytes(), post_id)
    }

    /// All posts in a channel, in insertion order.
    pub fn history(&self, channel: &str) -> Vec<Post> {
        self.engine
            .channel_history(self.cabal_id.as_bytes(), channel)
    }

    /// Subscribe to posts as they land in this cabal.
    ///
    /// The receiver yields each post stored *after* it subscribes — authored
    /// locally (here or via a [`SyncedCabal`](crate::SyncedCabal)), gossiped by a
    /// peer, or caught up via LogSync — once each. It does not replay the
    /// backlog; load it with [`history`](Self::history) first, then drive the live
    /// view from this stream (dedup by [`PostId`] is cheap, posts being
    /// content-addressed). On a `Lagged` error the consumer should re-read
    /// `history` to reconcile.
    pub fn subscribe(&self) -> Result<tokio::sync::broadcast::Receiver<Post>, MurmError> {
        Ok(self.engine.subscribe(self.cabal_id.as_bytes())?)
    }

    /// Inject a post arrived from a peer (e.g. via transport sync).
    ///
    /// Verifies the signature, computes the post id, and stores it.
    /// Returns the computed post id on success; rejects with
    /// [`MurmError::Protocol`] if the signature is invalid.
    pub fn ingest_post(&self, post: Post) -> Result<PostId, MurmError> {
        Ok(self.engine.ingest_post(self.cabal_id.as_bytes(), post)?)
    }
}
