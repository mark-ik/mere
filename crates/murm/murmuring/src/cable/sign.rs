//! Cable post signing and verification.
//!
//! Per Cable spec, the post signature is an Ed25519 signature over the
//! "signed region" of the canonical wire encoding: every byte after the
//! signature field itself, i.e.,
//!
//! ```text
//! num_links | links | post_type | timestamp | body
//! ```
//!
//! [`sign_post`] constructs a signed [`Post`] from a keypair, links, and
//! payload kind. [`verify_post`] checks that an existing `Post`'s signature
//! is valid against its `author` field.

use mere_identity::Ed25519Keypair;

use crate::cable::wire::encode_signed_region;
use crate::{Post, PostId, PostKind};

/// Sign a new [`Post`] with the given keypair.
///
/// The post's `author` field is set to the keypair's public key; the
/// `signature` field is set to the Ed25519 signature over the canonical
/// signed-region bytes.
pub fn sign_post(keypair: &Ed25519Keypair, links: Vec<PostId>, kind: PostKind) -> Post {
    let mut signed_region = Vec::new();
    encode_signed_region(&mut signed_region, &links, &kind);
    let signature = keypair.sign(&signed_region);
    Post {
        author: keypair.public_key(),
        links,
        kind,
        signature,
    }
}

/// Verify a post's signature.
///
/// Returns `true` iff the signature is a valid Ed25519 signature by
/// `post.author` over the canonical signed-region bytes computed from
/// `post.links` and `post.kind`. A `false` return covers signature
/// invalidity, mismatched author, or a tampered body.
pub fn verify_post(post: &Post) -> bool {
    let mut signed_region = Vec::new();
    encode_signed_region(&mut signed_region, &post.links, &post.kind);
    post.author.verify(&signed_region, &post.signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cable::wire::{decode_post, encode_post};
    use crate::{ChannelName, InfoEntry, PostKind};
    use mere_identity::{IdentityProvider, InMemoryProvider};

    fn keypair() -> Ed25519Keypair {
        InMemoryProvider::from_seed([99; 32])
            .derive_keypair(b"signing-test")
            .unwrap()
    }

    #[test]
    fn sign_then_verify_text() {
        let kp = keypair();
        let post = sign_post(
            &kp,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "hello signed cabal".to_string(),
                timestamp_ms: 1_700_000_000_000,
            },
        );
        assert!(verify_post(&post));
        assert_eq!(post.author.to_bytes(), kp.public_key().to_bytes());
    }

    #[test]
    fn sign_then_verify_all_post_types() {
        let kp = keypair();

        let cases: Vec<PostKind> = vec![
            PostKind::Text {
                channel: ChannelName::new("c"),
                text: "x".to_string(),
                timestamp_ms: 1,
            },
            PostKind::Delete {
                posts: vec![PostId::new([1; 32]), PostId::new([2; 32])],
                timestamp_ms: 2,
            },
            PostKind::Info {
                entries: vec![InfoEntry::name("alice")],
                timestamp_ms: 3,
            },
            PostKind::Topic {
                channel: ChannelName::new("c"),
                topic: "t".to_string(),
                timestamp_ms: 4,
            },
            PostKind::Join {
                channel: ChannelName::new("c"),
                timestamp_ms: 5,
            },
            PostKind::Leave {
                channel: ChannelName::new("c"),
                timestamp_ms: 6,
            },
        ];

        for kind in cases {
            let kind_name = kind.kind_name();
            let post = sign_post(&kp, vec![], kind);
            assert!(verify_post(&post), "verify failed for {kind_name}");
        }
    }

    #[test]
    fn verify_rejects_tampered_text() {
        let kp = keypair();
        let mut post = sign_post(
            &kp,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "original".to_string(),
                timestamp_ms: 1,
            },
        );

        // Mutate the text — signature should no longer verify.
        if let PostKind::Text { ref mut text, .. } = post.kind {
            *text = "tampered".to_string();
        }
        assert!(!verify_post(&post));
    }

    #[test]
    fn verify_rejects_tampered_timestamp() {
        let kp = keypair();
        let mut post = sign_post(
            &kp,
            vec![],
            PostKind::Join {
                channel: ChannelName::new("c"),
                timestamp_ms: 100,
            },
        );

        if let PostKind::Join {
            ref mut timestamp_ms,
            ..
        } = post.kind
        {
            *timestamp_ms = 200;
        }
        assert!(!verify_post(&post));
    }

    #[test]
    fn verify_rejects_swapped_author() {
        let kp_alice = keypair();
        let kp_bob = InMemoryProvider::from_seed([0; 32])
            .derive_keypair(b"signing-test")
            .unwrap();

        let mut post = sign_post(
            &kp_alice,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("c"),
                text: "from alice".to_string(),
                timestamp_ms: 1,
            },
        );

        // Replace the author claim with bob's public key — signature was
        // alice's, so verification should fail.
        post.author = kp_bob.public_key();
        assert!(!verify_post(&post));
    }

    #[test]
    fn signed_post_round_trips_through_wire_encoding() {
        let kp = keypair();
        let post = sign_post(
            &kp,
            vec![PostId::new([7; 32])],
            PostKind::Text {
                channel: ChannelName::new("session"),
                text: "round trip me".to_string(),
                timestamp_ms: 1_700_000_000_000,
            },
        );

        // Encode → decode → re-verify.
        let encoded = encode_post(&post);
        let decoded = decode_post(&encoded).expect("decode failed");
        assert!(
            verify_post(&decoded),
            "decoded post failed signature verification"
        );

        // And re-encoding the decoded post yields the same bytes.
        let re_encoded = encode_post(&decoded);
        assert_eq!(encoded, re_encoded);
    }

    #[test]
    fn deterministic_signing_for_same_input() {
        // Ed25519 signatures are deterministic per RFC 8032: signing the
        // same message with the same keypair yields the same signature.
        // We rely on this for content-addressing stability.
        let kp = keypair();
        let post1 = sign_post(
            &kp,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("c"),
                text: "x".to_string(),
                timestamp_ms: 1,
            },
        );
        let post2 = sign_post(
            &kp,
            vec![],
            PostKind::Text {
                channel: ChannelName::new("c"),
                text: "x".to_string(),
                timestamp_ms: 1,
            },
        );
        assert_eq!(
            post1.signature.to_bytes(),
            post2.signature.to_bytes(),
            "Ed25519 signing is supposed to be deterministic"
        );
    }

    #[test]
    fn different_links_yield_different_signatures() {
        let kp = keypair();
        let kind = || PostKind::Text {
            channel: ChannelName::new("c"),
            text: "x".to_string(),
            timestamp_ms: 1,
        };
        let post_no_links = sign_post(&kp, vec![], kind());
        let post_with_links = sign_post(&kp, vec![PostId::new([1; 32])], kind());

        // Signatures should differ because the signed region differs (links
        // are part of it).
        assert_ne!(
            post_no_links.signature.to_bytes(),
            post_with_links.signature.to_bytes()
        );

        // And both verify against their own posts.
        assert!(verify_post(&post_no_links));
        assert!(verify_post(&post_with_links));
    }
}
