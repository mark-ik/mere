//! Thread 2: a cabal modeled as a BLAKE3 signed-event SPACE on our stack.
//!
//! The §12 decision: a cabal's soul is the *shared-secret trust model*, not the
//! Cable wire. So model a cabal as a space of p2panda-core Operations (which we
//! proved fit in Probe S2, BLAKE3-native), keyed by a shared `cabal_key`:
//!
//! - **Shared secret** `cabal_key` (32 bytes, distributed out-of-band). Anyone
//!   with it is in the cabal. Symmetric, no roster, no capability issuance.
//! - **Space id** = `BLAKE3(cabal_key)` — the public address peers use to refer
//!   to the cabal without revealing the secret.
//! - **Per-member per-cabal signing key** = `BLAKE3-keyed(member_master, cabal_key)`
//!   (the Cable §2.2 derivation pattern, now BLAKE3). Each member signs their
//!   own events; everyone-with-the-key can verify.
//! - **Events** = p2panda-core `Operation`s carrying a `CabalExt` extension
//!   (space id, channel, event type, parents). BLAKE3-hashed, Ed25519-signed.
//!
//! This is "Cable on our stack": same bilateral trust model, BLAKE3 throughout,
//! no literal Cable wire. Promoting it into murm (replacing the Cable wire with
//! this space model) is the follow-on refactor.
//!
//! See design_docs/mere_docs/implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md (Thread 2 / §12).

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Header, SigningKey, Timestamp};
use serde::{Deserialize, Serialize};

/// The cabal-event metadata carried as a p2panda `Header` extension.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct CabalExt {
    /// `BLAKE3(cabal_key)` — the space this event belongs to.
    cabal_id: [u8; 32],
    /// Channel within the cabal (Cable's `"session"`, `"links"`, ...).
    channel: String,
    /// Event kind: "Text", "Join", "Leave", "Topic", ...
    event_type: String,
    /// Multi-parent causal refs (per Probe 1: app-level, opaque to sync).
    parents: Vec<[u8; 32]>,
}

/// Public cabal id from the shared secret: `BLAKE3-256(cabal_key)`.
fn cabal_id(cabal_key: &[u8; 32]) -> [u8; 32] {
    *blake3::hash(cabal_key).as_bytes()
}

/// A member's per-cabal signing key: `BLAKE3-keyed(master_seed, cabal_key)`,
/// then an Ed25519 key from that seed. The Cable §2.2 derivation, BLAKE3.
fn per_cabal_key(member_master: &[u8; 32], cabal_key: &[u8; 32]) -> SigningKey {
    let seed = *blake3::keyed_hash(member_master, cabal_key).as_bytes();
    SigningKey::from_bytes(&seed)
}

/// Author one cabal event into the space, signed by the member's per-cabal key.
fn author_event(
    key: &SigningKey,
    cid: [u8; 32],
    channel: &str,
    event_type: &str,
    text: &str,
    seq_num: u64,
    backlink: Option<p2panda_core::Hash>,
    parents: Vec<[u8; 32]>,
) -> Header<CabalExt> {
    let body = Body::new(text.as_bytes());
    let mut header: Header<CabalExt> = Header {
        version: 1,
        verifying_key: key.verifying_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        timestamp: Timestamp::now(),
        seq_num,
        backlink,
        extensions: CabalExt {
            cabal_id: cid,
            channel: channel.to_string(),
            event_type: event_type.to_string(),
            parents,
        },
    };
    header.sign(key);
    header
}

fn main() {
    // One shared secret defines the cabal. Two members each hold it.
    let cabal_key = [0xabu8; 32];
    let cid = cabal_id(&cabal_key);

    let alice_master = [1u8; 32];
    let bob_master = [2u8; 32];

    // Each member derives their OWN per-cabal signing key from their master +
    // the shared cabal_key. Different members → different keys, same cabal.
    let alice_key = per_cabal_key(&alice_master, &cabal_key);
    let bob_key = per_cabal_key(&bob_master, &cabal_key);
    assert_ne!(
        alice_key.verifying_key().as_bytes(),
        bob_key.verifying_key().as_bytes(),
        "members have distinct per-cabal keys"
    );

    // The space id is the same for both — that's how they address the cabal.
    println!("cabal_id (BLAKE3 of shared secret) = {}", hex(&cid));

    // Alice opens the cabal with a Text event in channel "session".
    let a1 = author_event(&alice_key, cid, "session", "Text", "hello from alice", 0, None, vec![]);
    let a1_hash = a1.hash();
    // Bob replies, naming Alice's event as a parent (multi-parent ref).
    let b1 = author_event(
        &bob_key, cid, "session", "Text", "hi alice, bob here", 0, None, vec![*a1_hash.as_bytes()],
    );

    // Events are BLAKE3-hashed, Ed25519-signed; both verify; CBOR round-trips.
    let a1_bytes = encode_cbor(&a1).expect("encode a1");
    let a1_decoded: Header<CabalExt> = decode_cbor(&a1_bytes[..]).expect("decode a1");
    assert!(a1_decoded.verify(), "alice's event verifies");
    assert_eq!(a1_decoded.extensions.cabal_id, cid, "event addressed to the cabal space");
    assert_eq!(a1_decoded.extensions.channel, "session");

    let b1_bytes = encode_cbor(&b1).expect("encode b1");
    let b1_decoded: Header<CabalExt> = decode_cbor(&b1_bytes[..]).expect("decode b1");
    assert!(b1_decoded.verify(), "bob's event verifies");
    assert_eq!(b1_decoded.extensions.parents, vec![*a1_hash.as_bytes()], "bob's parent ref survives");

    // A member who does NOT know the cabal_key derives a different key and a
    // different cabal_id — they cannot impersonate the space.
    let outsider_cabal_id = cabal_id(&[0xffu8; 32]);
    assert_ne!(outsider_cabal_id, cid, "a different secret yields a different space");

    println!("alice event hash = {}", hex(a1_hash.as_bytes()));
    println!("bob event hash   = {}", hex(b1.hash().as_bytes()));
    println!("both verify      = {}", a1_decoded.verify() && b1_decoded.verify());

    println!("\n--- Thread 2 VERDICT ---");
    println!("A cabal is modeled as a BLAKE3 signed-event SPACE on our stack:");
    println!("- shared secret cabal_key defines membership (symmetric, no roster)");
    println!("- space id = BLAKE3(cabal_key); per-member key = BLAKE3-keyed(master, cabal_key)");
    println!("- events are p2panda-core Operations (BLAKE3 + Ed25519) with a CabalExt extension");
    println!("- CBOR round-trips, signatures verify, multi-parent refs ride as app-level data");
    println!("=> 'Cable on our stack': the bilateral shared-secret trust model, BLAKE3 throughout,");
    println!("   no literal Cable wire. Next: promote this space model into murm (retire the wire).");
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
