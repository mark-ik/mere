//! Cabal post wire encoding — p2panda-core Operations (BLAKE3, CBOR, Ed25519).
//!
//! A murm "post" is the logical chat event; on the wire it is a p2panda-core
//! operation (a signed [`Header<CabalExt>`] plus an optional body), the same
//! event type the rest of the substrate (p2panda-net sync, the cabal-space
//! model) is built on. This is the §12 "Cable on our stack" promotion: the
//! shared-secret bilateral trust model, but BLAKE3 / CBOR / p2panda operations
//! throughout, not the literal Cable varint wire.
//!
//! ## Mapping
//!
//! A [`Post`] maps to an operation as:
//!
//! - `author`    ↔ `Header::verifying_key`
//! - `signature` ↔ `Header::signature` (Ed25519 over the canonical header)
//! - `links`     ↔ `CabalExt::parents` (app-level causal refs; see Probe 1 —
//!   p2panda's own per-author `seq_num`/`backlink` log is left unused for now,
//!   multi-parent causality rides as application data)
//! - `kind`      ↔ `CabalExt::{channel, post_type, timestamp_ms, info, deletes}`
//!   plus the operation `Body` (text / topic bytes; empty for the others)
//!
//! The post id ([`crate::cable::hash`]) stays the plain BLAKE3-256 of these
//! canonical CBOR bytes.
//!
//! The construction is deterministic in `(author, links, kind)`, so signing
//! ([`crate::cable::sign`]), verification, and encoding all reconstruct
//! byte-identical headers — content addressing is stable across peers.

use identity::{Ed25519PublicKey, Ed25519Signature};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, Signature, Timestamp, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{ChannelName, InfoEntry, MurmuringError, Post, PostId, PostKind};

/// Cabal-event metadata carried in the signed p2panda `Header` extension.
///
/// Everything needed to reconstruct a [`PostKind`] except the free content
/// (text / topic), which rides in the operation `Body` and is bound into the
/// signature via the header's `payload_hash`.
///
/// Public because a cabal post *is* an `Operation<CabalExt>`: the sync wiring
/// (in `murm`) receives these operations from p2panda-net's LogSync and converts
/// them back to [`Post`]s via [`operation_to_post`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CabalExt {
    /// The cabal (space) this event belongs to: `BLAKE3(cabal_key)`. Signed, so
    /// the event is self-describing and can't be replayed into another cabal.
    pub cabal_id: [u8; 32],
    /// Channel name (`""` for channel-less kinds: info, delete).
    pub channel: String,
    /// Cable `post_type` discriminator (0..=5).
    pub post_type: u64,
    /// Author-asserted millisecond timestamp.
    pub timestamp_ms: u64,
    /// Causal-DAG predecessors — the post's `links`.
    pub parents: Vec<[u8; 32]>,
    /// `post/info` entries (only populated for `post_type == 2`).
    pub info: Vec<(String, Vec<u8>)>,
    /// `post/delete` targets (only populated for `post_type == 1`).
    pub deletes: Vec<[u8; 32]>,
}

/// The canonical wire form: a signed header plus its body bytes, CBOR-encoded
/// as one blob.
#[derive(Serialize, Deserialize)]
struct WirePost {
    header: Header<CabalExt>,
    body: Vec<u8>,
}

// ── identity ed25519 ↔ p2panda-core ed25519 ──────────────────────────────────
// Both are ed25519-dalek underneath; convert through canonical bytes.

fn to_p2_vk(author: &Ed25519PublicKey) -> Result<VerifyingKey, MurmuringError> {
    VerifyingKey::from_bytes(&author.to_bytes()).map_err(|_| MurmuringError::MalformedPost)
}

fn from_p2_vk(vk: &VerifyingKey) -> Result<Ed25519PublicKey, MurmuringError> {
    Ed25519PublicKey::from_bytes(vk.as_bytes()).map_err(|_| MurmuringError::MalformedPost)
}

pub(crate) fn to_p2_sig(sig: &Ed25519Signature) -> Signature {
    Signature::from_bytes(&sig.to_bytes())
}

pub(crate) fn from_p2_sig(sig: &Signature) -> Ed25519Signature {
    Ed25519Signature::from_bytes(&sig.to_bytes())
}

/// Split a [`PostKind`] (+ cabal id, links) into the header extension and body.
fn decompose(cabal_id: [u8; 32], links: &[PostId], kind: &PostKind) -> (CabalExt, Vec<u8>) {
    let parents: Vec<[u8; 32]> = links.iter().map(|l| *l.as_bytes()).collect();
    let mut ext = CabalExt {
        cabal_id,
        channel: String::new(),
        post_type: kind.post_type(),
        timestamp_ms: kind.timestamp_ms(),
        parents,
        info: Vec::new(),
        deletes: Vec::new(),
    };
    let body = match kind {
        PostKind::Text { channel, text, .. } => {
            ext.channel = channel.as_str().to_string();
            text.as_bytes().to_vec()
        }
        PostKind::Topic { channel, topic, .. } => {
            ext.channel = channel.as_str().to_string();
            topic.as_bytes().to_vec()
        }
        PostKind::Join { channel, .. } | PostKind::Leave { channel, .. } => {
            ext.channel = channel.as_str().to_string();
            Vec::new()
        }
        PostKind::Info { entries, .. } => {
            ext.info = entries
                .iter()
                .map(|e| (e.key.clone(), e.value.clone()))
                .collect();
            Vec::new()
        }
        PostKind::Delete { posts, .. } => {
            ext.deletes = posts.iter().map(|p| *p.as_bytes()).collect();
            Vec::new()
        }
    };
    (ext, body)
}

/// Rebuild a [`PostKind`] from a header extension and body bytes.
fn recompose(ext: &CabalExt, body: &[u8]) -> Result<PostKind, MurmuringError> {
    let timestamp_ms = ext.timestamp_ms;
    let text_body = || String::from_utf8(body.to_vec()).map_err(|_| MurmuringError::MalformedPost);
    match ext.post_type {
        0 => Ok(PostKind::Text {
            channel: ChannelName::new(ext.channel.clone()),
            text: text_body()?,
            timestamp_ms,
        }),
        1 => Ok(PostKind::Delete {
            posts: ext.deletes.iter().map(|d| PostId::new(*d)).collect(),
            timestamp_ms,
        }),
        2 => Ok(PostKind::Info {
            entries: ext
                .info
                .iter()
                .map(|(k, v)| InfoEntry {
                    key: k.clone(),
                    value: v.clone(),
                })
                .collect(),
            timestamp_ms,
        }),
        3 => Ok(PostKind::Topic {
            channel: ChannelName::new(ext.channel.clone()),
            topic: text_body()?,
            timestamp_ms,
        }),
        4 => Ok(PostKind::Join {
            channel: ChannelName::new(ext.channel.clone()),
            timestamp_ms,
        }),
        5 => Ok(PostKind::Leave {
            channel: ChannelName::new(ext.channel.clone()),
            timestamp_ms,
        }),
        _ => Err(MurmuringError::MalformedPost),
    }
}

/// Build the unsigned p2panda header (+ body bytes) for a post's logical fields.
///
/// Deterministic in `(author, cabal_id, seq_num, backlink, links, kind)`. Shared
/// by signing, verification, and encoding so all three reconstruct
/// byte-identical headers. `seq_num`/`backlink` are the per-author log position
/// (the p2panda log rule: `backlink.is_some()` iff `seq_num > 0`).
pub(crate) fn unsigned_header(
    author: VerifyingKey,
    cabal_id: [u8; 32],
    seq_num: u64,
    backlink: Option<PostId>,
    links: &[PostId],
    kind: &PostKind,
) -> (Header<CabalExt>, Vec<u8>) {
    let (extensions, body_bytes) = decompose(cabal_id, links, kind);
    let body = Body::new(&body_bytes);
    // Per the operation format, `payload_hash` is present iff the body is
    // non-empty (channel-less and empty-text posts carry no body).
    let (payload_size, payload_hash) = if body_bytes.is_empty() {
        (0, None)
    } else {
        (body.size(), Some(body.hash()))
    };
    let header = Header {
        version: 1,
        verifying_key: author,
        signature: None,
        payload_size,
        payload_hash,
        timestamp: Timestamp::from(kind.timestamp_ms()),
        seq_num,
        backlink: backlink.map(|id| Hash::from(*id.as_bytes())),
        extensions,
    };
    (header, body_bytes)
}

/// Rebuild the *signed* header (+ body bytes) for an existing [`Post`].
fn signed_header(post: &Post) -> Result<(Header<CabalExt>, Vec<u8>), MurmuringError> {
    let (mut header, body) = unsigned_header(
        to_p2_vk(&post.author)?,
        post.cabal_id,
        post.seq_num,
        post.backlink,
        &post.links,
        &post.kind,
    );
    header.signature = Some(to_p2_sig(&post.signature));
    Ok((header, body))
}

/// The operation id of a post: its signed-header hash (`Header::hash()`), the
/// p2panda operation identity used for content addressing, backlinks, and sync.
/// Equal to what `Header::hash()` returned at authoring time (the header is
/// rebuilt deterministically from the post's fields).
pub fn operation_id(post: &Post) -> PostId {
    match signed_header(post) {
        Ok((header, _body)) => PostId::new(*header.hash().as_bytes()),
        // Unreachable for a well-formed post: `author` is always a valid key.
        Err(_) => PostId::new([0u8; 32]),
    }
}

/// Reconstruct the full p2panda [`Operation`] for a stored [`Post`].
///
/// The inverse of [`decode_post`] at the operation layer: it rebuilds the signed
/// `Header<CabalExt>` and body so a stored post can be served to p2panda-net's
/// LogSync, which reconciles `Operation`s, not `Post`s. `body` is `Some` iff the
/// post carries a payload (text/topic), matching the header's `payload_hash`.
pub fn post_to_operation(post: &Post) -> Result<Operation<CabalExt>, MurmuringError> {
    let (header, body_bytes) = signed_header(post)?;
    let hash = header.hash();
    let body = if body_bytes.is_empty() {
        None
    } else {
        Some(Body::new(&body_bytes))
    };
    Ok(Operation { hash, header, body })
}

/// Reconstruct a [`Post`] from a received p2panda [`Operation`].
///
/// The inverse of [`post_to_operation`]: rebuilds the logical post from the
/// operation's signed header (`author`, `signature`, `seq_num`, `backlink`,
/// `links`) and its `CabalExt` extension + body (the `PostKind`). Used by the
/// sync drain to land operations LogSync delivers. Returns
/// [`MurmuringError::MalformedPost`] for a missing signature, an unknown
/// `post_type`, invalid key bytes, or non-UTF-8 text/topic content.
///
/// This does **not** verify the signature; the ingest path
/// ([`crate::cable::CableEngine::ingest_post`]) does, alongside the cabal-id and
/// log-position checks.
pub fn operation_to_post(op: &Operation<CabalExt>) -> Result<Post, MurmuringError> {
    let header = &op.header;
    let author = from_p2_vk(&header.verifying_key)?;
    let signature = header
        .signature
        .as_ref()
        .map(from_p2_sig)
        .ok_or(MurmuringError::MalformedPost)?;
    let links = header
        .extensions
        .parents
        .iter()
        .map(|p| PostId::new(*p))
        .collect();
    let body_bytes = op.body.as_ref().map(|b| b.to_bytes()).unwrap_or_default();
    let kind = recompose(&header.extensions, &body_bytes)?;
    let backlink = header.backlink.map(|h| PostId::new(*h.as_bytes()));
    Ok(Post {
        author,
        cabal_id: header.extensions.cabal_id,
        seq_num: header.seq_num,
        backlink,
        links,
        kind,
        signature,
    })
}

/// Encode a [`Post`] to its canonical wire bytes (a CBOR p2panda operation).
pub fn encode_post(post: &Post) -> Vec<u8> {
    // A `Post` whose `author` is not a valid ed25519 point can never have been
    // produced by signing or decoding; fall back to empty rather than panic.
    let Ok((header, body)) = signed_header(post) else {
        return Vec::new();
    };
    encode_cbor(&WirePost { header, body }).expect("CBOR encoding of a post never fails")
}

/// Decode a [`Post`] from canonical wire bytes.
///
/// Returns [`MurmuringError::MalformedPost`] for any structural problem
/// (CBOR error, missing signature, unknown `post_type`, invalid public-key
/// bytes, non-UTF-8 in a text/topic field).
pub fn decode_post(bytes: &[u8]) -> Result<Post, MurmuringError> {
    let wire: WirePost = decode_cbor(bytes).map_err(|_| MurmuringError::MalformedPost)?;
    let author = from_p2_vk(&wire.header.verifying_key)?;
    let signature = wire
        .header
        .signature
        .as_ref()
        .map(from_p2_sig)
        .ok_or(MurmuringError::MalformedPost)?;
    let links = wire
        .header
        .extensions
        .parents
        .iter()
        .map(|p| PostId::new(*p))
        .collect();
    let kind = recompose(&wire.header.extensions, &wire.body)?;
    let backlink = wire.header.backlink.map(|h| PostId::new(*h.as_bytes()));
    Ok(Post {
        author,
        cabal_id: wire.header.extensions.cabal_id,
        seq_num: wire.header.seq_num,
        backlink,
        links,
        kind,
        signature,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cable::sign::sign_post;
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};

    fn keypair() -> Ed25519Keypair {
        InMemoryProvider::from_seed([5; 32])
            .derive_keypair(b"wire-test")
            .unwrap()
    }

    /// The cabal id used by the round-trip helper.
    const TEST_CABAL: [u8; 32] = [0xca; 32];

    /// Round-trip a signed post: encode → decode → re-encode yields identical
    /// bytes, and the decoded post still verifies. (`Post` doesn't implement
    /// `PartialEq` — the canonical bytes are the equality test.)
    fn assert_round_trip(kind: PostKind, links: Vec<PostId>) {
        let post = sign_post(&keypair(), TEST_CABAL, 0, None, links, kind);
        let encoded = encode_post(&post);
        let decoded = decode_post(&encoded).expect("decode failed");
        assert_eq!(
            encoded,
            encode_post(&decoded),
            "post round-trip changed bytes"
        );
        assert!(
            crate::cable::sign::verify_post(&decoded),
            "decoded post failed verification"
        );
    }

    #[test]
    fn round_trip_text() {
        assert_round_trip(
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "hello, cabal".to_string(),
                timestamp_ms: 1_700_000_000_000,
            },
            vec![],
        );
    }

    #[test]
    fn round_trip_text_with_links() {
        assert_round_trip(
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "follow-up".to_string(),
                timestamp_ms: 1,
            },
            vec![PostId::new([7; 32]), PostId::new([8; 32])],
        );
    }

    #[test]
    fn round_trip_delete_multiple() {
        assert_round_trip(
            PostKind::Delete {
                posts: vec![
                    PostId::new([3; 32]),
                    PostId::new([4; 32]),
                    PostId::new([5; 32]),
                ],
                timestamp_ms: 42,
            },
            vec![PostId::new([1; 32])],
        );
    }

    #[test]
    fn round_trip_info_multiple_entries() {
        assert_round_trip(
            PostKind::Info {
                entries: vec![
                    InfoEntry::name("alice"),
                    InfoEntry::new("accept-role", vec![1u8]),
                    InfoEntry::new("custom-key", vec![0xde, 0xad, 0xbe, 0xef]),
                ],
                timestamp_ms: 0,
            },
            vec![],
        );
    }

    #[test]
    fn round_trip_info_empty() {
        assert_round_trip(
            PostKind::Info {
                entries: vec![],
                timestamp_ms: 0,
            },
            vec![],
        );
    }

    #[test]
    fn round_trip_topic_empty_clears() {
        assert_round_trip(
            PostKind::Topic {
                channel: ChannelName::new("session"),
                topic: String::new(),
                timestamp_ms: 0,
            },
            vec![],
        );
    }

    #[test]
    fn round_trip_join_and_leave() {
        assert_round_trip(
            PostKind::Join {
                channel: ChannelName::new("session"),
                timestamp_ms: 1,
            },
            vec![],
        );
        assert_round_trip(
            PostKind::Leave {
                channel: ChannelName::new("session"),
                timestamp_ms: 1,
            },
            vec![],
        );
    }

    #[test]
    fn round_trip_text_with_unicode() {
        assert_round_trip(
            PostKind::Text {
                channel: ChannelName::new("国際"),
                text: "こんにちは 🌐".to_string(),
                timestamp_ms: 1,
            },
            vec![],
        );
    }

    #[test]
    fn garbage_bytes_are_malformed() {
        assert!(matches!(
            decode_post(&[0xff, 0x00, 0x13, 0x37]),
            Err(MurmuringError::MalformedPost)
        ));
        assert!(matches!(decode_post(&[]), Err(MurmuringError::MalformedPost)));
    }

    #[test]
    fn decoded_post_carries_the_signing_author_and_cabal() {
        let kp = keypair();
        let post = sign_post(
            &kp,
            [0x42; 32],
            0,
            None,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "x".to_string(),
                timestamp_ms: 1,
            },
        );
        let decoded = decode_post(&encode_post(&post)).unwrap();
        assert_eq!(decoded.author.to_bytes(), kp.public_key().to_bytes());
        assert_eq!(
            decoded.cabal_id,
            [0x42; 32],
            "cabal id survives the round trip"
        );
    }

    #[test]
    fn operations_pass_p2panda_log_validation() {
        // The strongest check: our per-author chain satisfies p2panda's *own*
        // validators, so p2panda-net LogSync will accept it as a real log.
        use p2panda_core::operation::{validate_backlink, validate_header};

        let kp = keypair();
        let op0 = sign_post(
            &kp,
            TEST_CABAL,
            0,
            None,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "first".to_string(),
                timestamp_ms: 1,
            },
        );
        let op1 = sign_post(
            &kp,
            TEST_CABAL,
            1,
            Some(operation_id(&op0)),
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "second".to_string(),
                timestamp_ms: 2,
            },
        );

        let (h0, _) = signed_header(&op0).expect("rebuild op0 header");
        let (h1, _) = signed_header(&op1).expect("rebuild op1 header");

        assert!(validate_header(&h0).is_ok(), "root op is a valid header");
        assert!(validate_header(&h1).is_ok(), "second op is a valid header");
        assert!(
            validate_backlink(&h0, &h1).is_ok(),
            "op1 chains onto op0 per p2panda's backlink rule (seq+1, backlink == op0 header hash)"
        );
    }

    #[test]
    fn operation_to_post_inverts_post_to_operation() {
        // A post → operation → post round-trip is byte-identical and still
        // verifies. This is the path the sync drain takes when LogSync delivers
        // an `OperationReceived` (operation back into a post to ingest).
        let post = sign_post(
            &keypair(),
            TEST_CABAL,
            1,
            Some(PostId::new([9; 32])),
            vec![PostId::new([7; 32])],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "round trip via operation".to_string(),
                timestamp_ms: 5,
            },
        );
        let op = post_to_operation(&post).expect("post -> operation");
        // The operation id is the post's operation id (signed-header hash).
        assert_eq!(op.hash.as_bytes(), operation_id(&post).as_bytes());

        let back = operation_to_post(&op).expect("operation -> post");
        assert_eq!(
            encode_post(&post),
            encode_post(&back),
            "post survives the operation round-trip"
        );
        assert!(
            crate::cable::sign::verify_post(&back),
            "reconstructed post verifies"
        );
    }

    #[test]
    fn operation_to_post_handles_bodyless_posts() {
        // Channel-less, payload-less kinds (info/delete/join/leave) carry no
        // operation body; the round-trip must still reconstruct them.
        let post = sign_post(
            &keypair(),
            TEST_CABAL,
            0,
            None,
            vec![],
            PostKind::Info {
                entries: vec![InfoEntry::name("alice")],
                timestamp_ms: 0,
            },
        );
        let op = post_to_operation(&post).unwrap();
        assert!(op.body.is_none(), "a bodyless post yields an op with no body");
        let back = operation_to_post(&op).unwrap();
        assert_eq!(encode_post(&post), encode_post(&back));
    }
}
