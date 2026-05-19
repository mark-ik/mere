//! Cable post wire encoding/decoding.
//!
//! Implements the canonical byte-level format defined in the upstream
//! [Cable spec](https://github.com/cabal-club/cable).
//!
//! ## Post layout
//!
//! ```text
//! public_key (32 bytes)
//! signature  (64 bytes)            ← signs everything that follows
//! num_links  (varint)
//! links      (32 × num_links bytes)
//! post_type  (varint)              ← 0..=5
//! timestamp  (varint)              ← millis since UNIX epoch
//! body                             ← post-type specific
//! ```
//!
//! ## Body layouts (selected by `post_type`)
//!
//! - `0` text:  channel_len + channel + text_len + text
//! - `1` delete: num_deletions + (32 bytes each)
//! - `2` info:   num_keypairs + (key_len + key + value_len + value)*
//! - `3` topic:  channel_len + channel + topic_len + topic
//! - `4` join:   channel_len + channel
//! - `5` leave:  channel_len + channel
//!
//! ## Status
//!
//! Encode and decode are implemented for all 6 post types. Round-trip
//! tested. **Signature semantics are not yet implemented** here; the
//! `signature` field is stored and serialized as an opaque 64-byte value.
//! Signing/verification (computing the signature over the post-prefix
//! bytes, etc.) lands in `cable::sign` (next chunk).

use std::io::{Cursor, Read};

use identity::{Ed25519PublicKey, Ed25519Signature};

use crate::cable::varint::{read_varint, write_varint};
use crate::{ChannelName, InfoEntry, MurmuringError, Post, PostId, PostKind};

// ─────────────────────────────────────────────────────────────────────────
// Encode
// ─────────────────────────────────────────────────────────────────────────

/// Encode a [`Post`] to its canonical Cable wire bytes.
pub fn encode_post(post: &Post) -> Vec<u8> {
    let mut out = Vec::with_capacity(128 + post.links.len() * 32);

    // public_key (32 bytes)
    out.extend_from_slice(&post.author.to_bytes());

    // signature (64 bytes)
    out.extend_from_slice(&post.signature.to_bytes());

    // Everything after the signature: num_links | links | post_type |
    // timestamp | body. This region is also the signing input.
    encode_signed_region(&mut out, &post.links, &post.kind);

    out
}

/// Encode the "signed region" of a post — every byte after the signature
/// field in the canonical wire layout. This is what Ed25519 signing/
/// verification covers.
///
/// Layout: `num_links | links | post_type | timestamp | body`.
pub(crate) fn encode_signed_region(out: &mut Vec<u8>, links: &[PostId], kind: &PostKind) {
    write_varint(out, links.len() as u64).expect("Vec write never fails");
    for link in links {
        out.extend_from_slice(link.as_bytes());
    }
    write_varint(out, kind.post_type()).expect("Vec write never fails");
    write_varint(out, kind.timestamp_ms()).expect("Vec write never fails");
    encode_body(out, kind);
}

pub(crate) fn encode_body(out: &mut Vec<u8>, kind: &PostKind) {
    match kind {
        PostKind::Text { channel, text, .. } => {
            write_lp_str(out, channel.as_str());
            write_lp_bytes(out, text.as_bytes());
        }
        PostKind::Delete { posts, .. } => {
            write_varint(out, posts.len() as u64).expect("infallible");
            for hash in posts {
                out.extend_from_slice(hash.as_bytes());
            }
        }
        PostKind::Info { entries, .. } => {
            write_varint(out, entries.len() as u64).expect("infallible");
            for entry in entries {
                write_lp_str(out, &entry.key);
                write_lp_bytes(out, &entry.value);
            }
        }
        PostKind::Topic { channel, topic, .. } => {
            write_lp_str(out, channel.as_str());
            write_lp_bytes(out, topic.as_bytes());
        }
        PostKind::Join { channel, .. } => {
            write_lp_str(out, channel.as_str());
        }
        PostKind::Leave { channel, .. } => {
            write_lp_str(out, channel.as_str());
        }
    }
}

/// Write a length-prefixed UTF-8 string: varint length, then UTF-8 bytes.
fn write_lp_str(out: &mut Vec<u8>, s: &str) {
    write_lp_bytes(out, s.as_bytes());
}

/// Write a length-prefixed byte slice: varint length, then bytes.
fn write_lp_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    write_varint(out, bytes.len() as u64).expect("Vec write never fails");
    out.extend_from_slice(bytes);
}

// ─────────────────────────────────────────────────────────────────────────
// Decode
// ─────────────────────────────────────────────────────────────────────────

/// Decode a [`Post`] from canonical Cable wire bytes.
///
/// Returns [`MurmuringError::MalformedPost`] for any structural problem
/// (truncation, varint overflow, unknown post_type, invalid public-key
/// bytes, non-UTF-8 in fields that the spec defines as UTF-8).
pub fn decode_post(bytes: &[u8]) -> Result<Post, MurmuringError> {
    let mut cursor = Cursor::new(bytes);

    // public_key
    let mut pk = [0u8; 32];
    cursor
        .read_exact(&mut pk)
        .map_err(|_| MurmuringError::MalformedPost)?;
    let author = Ed25519PublicKey::from_bytes(&pk)?;

    // signature
    let mut sig = [0u8; 64];
    cursor
        .read_exact(&mut sig)
        .map_err(|_| MurmuringError::MalformedPost)?;
    let signature = Ed25519Signature::from_bytes(&sig);

    // num_links + links
    let num_links = read_varint(&mut cursor)?;
    let mut links = Vec::with_capacity(num_links as usize);
    for _ in 0..num_links {
        let mut hash = [0u8; 32];
        cursor
            .read_exact(&mut hash)
            .map_err(|_| MurmuringError::MalformedPost)?;
        links.push(PostId::new(hash));
    }

    // post_type, timestamp, body
    let post_type = read_varint(&mut cursor)?;
    let timestamp_ms = read_varint(&mut cursor)?;
    let kind = decode_body(&mut cursor, post_type, timestamp_ms)?;

    Ok(Post {
        author,
        links,
        kind,
        signature,
    })
}

fn decode_body<R: Read>(
    cursor: &mut R,
    post_type: u64,
    timestamp_ms: u64,
) -> Result<PostKind, MurmuringError> {
    match post_type {
        0 => {
            let channel = ChannelName::new(read_lp_str(cursor)?);
            let text = read_lp_str(cursor)?;
            Ok(PostKind::Text {
                channel,
                text,
                timestamp_ms,
            })
        }
        1 => {
            let n = read_varint(cursor)?;
            let mut posts = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let mut hash = [0u8; 32];
                cursor
                    .read_exact(&mut hash)
                    .map_err(|_| MurmuringError::MalformedPost)?;
                posts.push(PostId::new(hash));
            }
            Ok(PostKind::Delete {
                posts,
                timestamp_ms,
            })
        }
        2 => {
            let n = read_varint(cursor)?;
            let mut entries = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let key = read_lp_str(cursor)?;
                let value = read_lp_bytes(cursor)?;
                entries.push(InfoEntry { key, value });
            }
            Ok(PostKind::Info {
                entries,
                timestamp_ms,
            })
        }
        3 => {
            let channel = ChannelName::new(read_lp_str(cursor)?);
            let topic = read_lp_str(cursor)?;
            Ok(PostKind::Topic {
                channel,
                topic,
                timestamp_ms,
            })
        }
        4 => {
            let channel = ChannelName::new(read_lp_str(cursor)?);
            Ok(PostKind::Join {
                channel,
                timestamp_ms,
            })
        }
        5 => {
            let channel = ChannelName::new(read_lp_str(cursor)?);
            Ok(PostKind::Leave {
                channel,
                timestamp_ms,
            })
        }
        _ => Err(MurmuringError::MalformedPost),
    }
}

/// Read a length-prefixed byte vector.
fn read_lp_bytes<R: Read>(cursor: &mut R) -> Result<Vec<u8>, MurmuringError> {
    let len = read_varint(cursor)?;
    let mut buf = vec![0u8; len as usize];
    cursor
        .read_exact(&mut buf)
        .map_err(|_| MurmuringError::MalformedPost)?;
    Ok(buf)
}

/// Read a length-prefixed UTF-8 string.
fn read_lp_str<R: Read>(cursor: &mut R) -> Result<String, MurmuringError> {
    let bytes = read_lp_bytes(cursor)?;
    String::from_utf8(bytes).map_err(|_| MurmuringError::MalformedPost)
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};

    /// Construct a signature value for testing — content of the signature
    /// is irrelevant for wire round-trip tests; we just need *some* valid
    /// `Ed25519Signature`.
    fn dummy_signature() -> Ed25519Signature {
        let kp = InMemoryProvider::from_seed([5; 32])
            .derive_keypair(b"test")
            .unwrap();
        kp.sign(b"placeholder")
    }

    fn dummy_author() -> Ed25519PublicKey {
        InMemoryProvider::from_seed([5; 32]).master_public_key()
    }

    fn dummy_keypair() -> Ed25519Keypair {
        InMemoryProvider::from_seed([5; 32])
            .derive_keypair(b"test")
            .unwrap()
    }

    fn make_post(kind: PostKind, links: Vec<PostId>) -> Post {
        Post {
            author: dummy_author(),
            links,
            kind,
            signature: dummy_signature(),
        }
    }

    fn assert_round_trip(post: &Post) {
        let encoded = encode_post(post);
        let decoded = decode_post(&encoded).expect("decode failed");
        // Compare via re-encoding (Post doesn't impl PartialEq because
        // Ed25519Signature doesn't; round-tripped bytes are the canonical
        // equality test).
        let re_encoded = encode_post(&decoded);
        assert_eq!(encoded, re_encoded, "post round-trip changed bytes");
    }

    #[test]
    fn round_trip_text() {
        let post = make_post(
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "hello, cabal".to_string(),
                timestamp_ms: 1_700_000_000_000,
            },
            vec![],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_text_with_links() {
        let post = make_post(
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "follow-up".to_string(),
                timestamp_ms: 1,
            },
            vec![PostId::new([7; 32]), PostId::new([8; 32])],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_delete_single() {
        let post = make_post(
            PostKind::Delete {
                posts: vec![PostId::new([3; 32])],
                timestamp_ms: 42,
            },
            vec![],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_delete_multiple() {
        let post = make_post(
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
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_info_with_name() {
        let post = make_post(
            PostKind::Info {
                entries: vec![InfoEntry::name("alice")],
                timestamp_ms: 0,
            },
            vec![],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_info_multiple_entries() {
        let post = make_post(
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
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_info_empty() {
        let post = make_post(
            PostKind::Info {
                entries: vec![],
                timestamp_ms: 0,
            },
            vec![],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_topic() {
        let post = make_post(
            PostKind::Topic {
                channel: ChannelName::new("links"),
                topic: "share interesting URLs here".to_string(),
                timestamp_ms: 100,
            },
            vec![],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_topic_empty_clears() {
        let post = make_post(
            PostKind::Topic {
                channel: ChannelName::new("session"),
                topic: String::new(),
                timestamp_ms: 0,
            },
            vec![],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_join() {
        let post = make_post(
            PostKind::Join {
                channel: ChannelName::new("session"),
                timestamp_ms: 1,
            },
            vec![],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_leave() {
        let post = make_post(
            PostKind::Leave {
                channel: ChannelName::new("session"),
                timestamp_ms: 1,
            },
            vec![],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn round_trip_text_with_unicode() {
        let post = make_post(
            PostKind::Text {
                channel: ChannelName::new("国際"),
                text: "こんにちは 🌐".to_string(),
                timestamp_ms: 1,
            },
            vec![],
        );
        assert_round_trip(&post);
    }

    #[test]
    fn truncated_post_is_malformed() {
        // 32 bytes (just public_key) — no room for signature.
        let bytes = vec![0xaau8; 32];
        // public_key bytes need to be a valid Ed25519 point; 0xaa repeated
        // is not (and would fail at the public-key parse step). Use a
        // valid public key for this test.
        let valid_pk = dummy_author().to_bytes().to_vec();
        let result = decode_post(&valid_pk);
        assert!(matches!(result, Err(MurmuringError::MalformedPost)));
        // (the 0xaa version would fail at PublicKey parse, also surfacing
        // as an error — verifying we don't panic)
        let _ = decode_post(&bytes);
    }

    #[test]
    fn unknown_post_type_is_malformed() {
        // Build a minimal post with post_type = 99 (unknown).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&dummy_author().to_bytes()); // pub key
        bytes.extend_from_slice(&dummy_signature().to_bytes()); // sig
        write_varint(&mut bytes, 0).unwrap(); // num_links
        write_varint(&mut bytes, 99).unwrap(); // post_type (unknown)
        write_varint(&mut bytes, 0).unwrap(); // timestamp

        let result = decode_post(&bytes);
        assert!(matches!(result, Err(MurmuringError::MalformedPost)));
    }

    #[test]
    fn invalid_utf8_in_text_is_malformed() {
        // Build a post/text with invalid UTF-8 in the text field. The
        // header parses fine; failure should surface during text decoding.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&dummy_author().to_bytes());
        bytes.extend_from_slice(&dummy_signature().to_bytes());
        write_varint(&mut bytes, 0).unwrap(); // num_links
        write_varint(&mut bytes, 0).unwrap(); // post_type = text
        write_varint(&mut bytes, 0).unwrap(); // timestamp

        // channel
        write_varint(&mut bytes, 1).unwrap();
        bytes.push(b'c');

        // text — invalid UTF-8 (0xff is never a valid UTF-8 leading byte)
        write_varint(&mut bytes, 1).unwrap();
        bytes.push(0xff);

        let result = decode_post(&bytes);
        assert!(matches!(result, Err(MurmuringError::MalformedPost)));
    }

    #[test]
    fn header_byte_layout_is_correct() {
        // Verify the exact byte order: pubkey (32) | signature (64) |
        // num_links (varint) | links | post_type | timestamp | body.
        let kp = dummy_keypair();
        let post = Post {
            author: kp.public_key(),
            links: vec![],
            kind: PostKind::Join {
                channel: ChannelName::new("c"),
                timestamp_ms: 7,
            },
            signature: kp.sign(b"x"), // dummy, not validated by encode/decode
        };

        let encoded = encode_post(&post);

        // First 32 bytes = public key
        assert_eq!(&encoded[..32], &kp.public_key().to_bytes()[..]);
        // Next 64 bytes = signature
        assert_eq!(&encoded[32..96], &post.signature.to_bytes()[..]);
        // Next byte should be num_links (0 → single 0x00 byte)
        assert_eq!(encoded[96], 0);
        // Next: post_type (4 = join, single 0x04 byte)
        assert_eq!(encoded[97], 4);
        // Then varint timestamp (7 → single 0x07 byte)
        assert_eq!(encoded[98], 7);
        // Then channel: len = 1 (varint 0x01), then "c" (0x63)
        assert_eq!(encoded[99], 1);
        assert_eq!(encoded[100], b'c');
        assert_eq!(encoded.len(), 101);
    }
}
