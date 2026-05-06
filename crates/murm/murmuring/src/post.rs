//! Cable-shaped post types, post IDs, and channel names.
//!
//! These types model the Cable wire-protocol post shapes from the upstream
//! [Cable spec](https://github.com/cabal-club/cable). The in-memory
//! representation here matches the wire format precisely so that
//! `cable::wire::{encode_post, decode_post}` can round-trip without lossy
//! conversion.

use mere_identity::{Ed25519PublicKey, Ed25519Signature};

use crate::MurmuringError;

/// Maximum channel name length in bytes. Picked to fit in a single varint
/// byte and to match common chat-protocol limits. The Cable spec is silent
/// on this but Cable implementations conventionally enforce a limit; 255
/// is more than generous for channel-naming use.
pub const MAX_CHANNEL_NAME_BYTES: usize = 255;

/// A post identifier — the BLAKE2b-256 hash of a post's canonical wire
/// encoding (with Cable-specific salt and personalization parameters; see
/// [`crate::cable::hash`]).
///
/// Per Cable's content-addressing model, posts are identified by their
/// hash; references to posts (in subsequent posts' `links` field, or in
/// `post/delete` payloads) carry these 32-byte hashes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct PostId(pub [u8; 32]);

impl PostId {
    /// Construct from raw bytes.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// View as bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A channel name within a cabal.
///
/// Per Cable spec §2.5, posts are organized into named channels. Co-op
/// session minichat uses `"session"`; named cabals may use multiple
/// channels (`"links"`, `"notes"`, etc.).
///
/// The wire encoding is a varint length prefix followed by UTF-8 bytes.
///
/// ## Validation
///
/// - Non-empty
/// - At most [`MAX_CHANNEL_NAME_BYTES`] bytes (UTF-8 length, not
///   codepoint count)
/// - No ASCII control characters (`< 0x20` or `0x7F`) — these are
///   typically problematic in chat-style names and the Cable spec doesn't
///   require their support
///
/// [`ChannelName::new`] is permissive (always succeeds; the channel name
/// may be invalid but tests of older Murm versions still compile).
/// [`ChannelName::try_new`] validates and returns
/// [`MurmuringError::MissingField`] on failure.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ChannelName(String);

impl ChannelName {
    /// Construct a channel name without validation.
    ///
    /// Useful for tests and for round-tripping channel names from peers
    /// (we accept what we receive even if it doesn't pass our local
    /// validation — the peer might use slightly different rules). For
    /// user-facing input, prefer [`ChannelName::try_new`].
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Construct a channel name, validating against the rules in the
    /// type-level docs.
    ///
    /// Returns [`MurmuringError::MissingField`] (with a description of
    /// the failing rule) if the name is invalid.
    pub fn try_new(name: impl Into<String>) -> Result<Self, MurmuringError> {
        let name = name.into();
        if name.is_empty() {
            return Err(MurmuringError::MissingField("channel name (empty)"));
        }
        if name.len() > MAX_CHANNEL_NAME_BYTES {
            return Err(MurmuringError::MissingField("channel name (too long)"));
        }
        if name
            .chars()
            .any(|c| (c as u32) < 0x20 || (c as u32) == 0x7F)
        {
            return Err(MurmuringError::MissingField(
                "channel name (control character)",
            ));
        }
        Ok(Self(name))
    }

    /// Check whether a string is a valid channel name without
    /// constructing.
    pub fn is_valid_name(s: &str) -> bool {
        !s.is_empty()
            && s.len() <= MAX_CHANNEL_NAME_BYTES
            && !s.chars().any(|c| (c as u32) < 0x20 || (c as u32) == 0x7F)
    }

    /// View as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A single key-value entry in a `post/info` payload.
///
/// Per Cable spec §post/info, info posts carry a *list* of key-value pairs
/// rather than a fixed set of fields. Standard keys include:
///
/// - `"name"` — UTF-8 display name (defaults to hex-encoded public key)
/// - `"accept-role"` — varint (`0` or `1`; default `1`)
///
/// Custom keys are permitted; receivers ignore unknown keys.
///
/// **Validation note**: per the spec, keys are UTF-8 strings (1–128
/// codepoints), values are bytes (max 4096). The types here are
/// permissive; encode-time validation lands in a later phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InfoEntry {
    /// Key name (UTF-8).
    pub key: String,
    /// Value bytes (interpretation key-specific).
    pub value: Vec<u8>,
}

impl InfoEntry {
    /// Construct an entry from a key and value.
    pub fn new(key: impl Into<String>, value: impl Into<Vec<u8>>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }

    /// Construct a standard `name` entry from a display-name string.
    pub fn name(name: impl AsRef<str>) -> Self {
        Self {
            key: "name".to_string(),
            value: name.as_ref().as_bytes().to_vec(),
        }
    }
}

/// A signed Cable post.
///
/// Posts are content-addressed via their `PostId` (BLAKE2b-256 hash of the
/// canonical wire encoding) and form a causal DAG via their `links` field.
/// The author signs everything *after* the signature field in the canonical
/// encoding (i.e., `links`, `post_type`, `timestamp`, and the type-specific
/// body).
#[derive(Clone, Debug)]
pub struct Post {
    /// The author's public key.
    pub author: Ed25519PublicKey,
    /// Causal-DAG predecessors (BLAKE2b-256 hashes of preceding posts).
    pub links: Vec<PostId>,
    /// The post's payload kind (also implicitly carries `post_type` and
    /// `timestamp_ms`).
    pub kind: PostKind,
    /// The author's signature over the canonical encoding starting at the
    /// `num_links` field.
    pub signature: Ed25519Signature,
}

/// The payload variant of a [`Post`].
///
/// Carries the `post_type` discriminator and `timestamp_ms` together with
/// the type-specific body, mirroring the Cable wire format. The
/// integer-valued `post_type` per Cable spec is:
///
/// - `0` = [`PostKind::Text`]
/// - `1` = [`PostKind::Delete`]
/// - `2` = [`PostKind::Info`]
/// - `3` = [`PostKind::Topic`]
/// - `4` = [`PostKind::Join`]
/// - `5` = [`PostKind::Leave`]
///
/// (See [`PostKind::post_type`] for programmatic access.)
#[derive(Clone, Debug)]
pub enum PostKind {
    /// `post_type = 0`. A chat text message.
    Text {
        /// Channel this post belongs to.
        channel: ChannelName,
        /// UTF-8 text body. Cable spec caps at 4096 bytes.
        text: String,
        /// Author-asserted millisecond Unix timestamp.
        timestamp_ms: u64,
    },
    /// `post_type = 1`. Author-initiated deletion of one or more earlier
    /// posts. The author can only delete posts they themselves authored
    /// (receivers enforce).
    Delete {
        /// The posts to delete (referenced by hash).
        posts: Vec<PostId>,
        /// Author-asserted timestamp.
        timestamp_ms: u64,
    },
    /// `post_type = 2`. Sets or replaces the author's metadata. Each new
    /// `Info` post completely replaces prior metadata for that author.
    Info {
        /// Key-value entries (e.g. `name`, `accept-role`).
        entries: Vec<InfoEntry>,
        /// Author-asserted timestamp.
        timestamp_ms: u64,
    },
    /// `post_type = 3`. Sets a channel topic. Empty `topic` clears.
    Topic {
        /// Channel whose topic is being set.
        channel: ChannelName,
        /// New topic text (UTF-8, 0–512 codepoints per spec).
        topic: String,
        /// Author-asserted timestamp.
        timestamp_ms: u64,
    },
    /// `post_type = 4`. Author entering a channel.
    Join {
        /// Channel being joined.
        channel: ChannelName,
        /// Author-asserted timestamp.
        timestamp_ms: u64,
    },
    /// `post_type = 5`. Author leaving a channel.
    Leave {
        /// Channel being left.
        channel: ChannelName,
        /// Author-asserted timestamp.
        timestamp_ms: u64,
    },
}

impl PostKind {
    /// The author-asserted timestamp for this post kind.
    pub fn timestamp_ms(&self) -> u64 {
        match self {
            PostKind::Text { timestamp_ms, .. }
            | PostKind::Delete { timestamp_ms, .. }
            | PostKind::Info { timestamp_ms, .. }
            | PostKind::Topic { timestamp_ms, .. }
            | PostKind::Join { timestamp_ms, .. }
            | PostKind::Leave { timestamp_ms, .. } => *timestamp_ms,
        }
    }

    /// The channel this post belongs to, if any. Channel-less variants
    /// ([`PostKind::Info`], [`PostKind::Delete`]) return `None`.
    pub fn channel(&self) -> Option<&ChannelName> {
        match self {
            PostKind::Text { channel, .. }
            | PostKind::Topic { channel, .. }
            | PostKind::Join { channel, .. }
            | PostKind::Leave { channel, .. } => Some(channel),
            PostKind::Info { .. } | PostKind::Delete { .. } => None,
        }
    }

    /// The Cable wire `post_type` discriminator (0–5).
    pub fn post_type(&self) -> u64 {
        match self {
            PostKind::Text { .. } => 0,
            PostKind::Delete { .. } => 1,
            PostKind::Info { .. } => 2,
            PostKind::Topic { .. } => 3,
            PostKind::Join { .. } => 4,
            PostKind::Leave { .. } => 5,
        }
    }

    /// A short discriminant string for logging / diagnostics.
    pub fn kind_name(&self) -> &'static str {
        match self {
            PostKind::Text { .. } => "text",
            PostKind::Delete { .. } => "delete",
            PostKind::Info { .. } => "info",
            PostKind::Topic { .. } => "topic",
            PostKind::Join { .. } => "join",
            PostKind::Leave { .. } => "leave",
        }
    }
}
