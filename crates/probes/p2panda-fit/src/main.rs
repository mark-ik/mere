//! S2 spike (the hinge): does p2panda-core's per-author append-only log model
//! carry a MereEvent, including its multi-parent `parents` field?
//!
//! The substrate brief's MereEvent (§4) is a multi-parent causal DAG
//! (`parents: Vec<EventHash>`). p2panda-core's Header is, by its own module
//! docs, "a linked list of operations, where every subsequent operation points
//! to the previous one" via a single `backlink` + `seq_num` (per-author,
//! single-parent). This probe builds two MereEvents as p2panda Operations,
//! carrying all MereEvent fields (space_id / event_type / target_refs /
//! capabilities / parents) as a custom `extensions` payload, signs them, CBOR
//! round-trips them, verifies, and prints a verdict.
//!
//! See design_docs/mere_docs/implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md (S2).

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Timestamp};
use serde::{Deserialize, Serialize};

/// MereEvent fields with no native slot in p2panda's Header, carried as the
/// custom `extensions` payload (the generic `E`). Note `parents`: p2panda has
/// a single `backlink` (previous op in this author's log), not a multi-parent
/// DAG, so MereEvent's multi-parent links must ride here.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct MereExt {
    space_id: String,          // SpaceId::Moot(..) / Personal(..) etc.
    event_type: String,        // "ChatMessage", "GraphAddNode", ...
    target_refs: Vec<String>,  // semantic refs (node ids, thread ids)
    parents: Vec<Hash>,        // multi-parent DAG links (NOT native to p2panda)
    capabilities: Vec<String>, // cap-chain refs (extensions are the documented home for caps)
}

fn build_signed(
    signing_key: &SigningKey,
    payload: &[u8],
    ext: MereExt,
    seq_num: u64,
    backlink: Option<Hash>,
) -> Header<MereExt> {
    let body = Body::new(payload);
    let mut header: Header<MereExt> = Header {
        version: 1,
        verifying_key: signing_key.verifying_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        timestamp: Timestamp::now(),
        seq_num,
        backlink,
        extensions: ext,
    };
    header.sign(signing_key);
    header
}

fn main() {
    let signing_key = SigningKey::generate();

    // op1: a ChatMessage, first in this author's log.
    let h1 = build_signed(
        &signing_key,
        b"hello moot",
        MereExt {
            space_id: "moot:demo".into(),
            event_type: "ChatMessage".into(),
            target_refs: vec!["thread:1".into()],
            parents: vec![],
            capabilities: vec![],
        },
        0,
        None,
    );
    let op1_hash = h1.hash();
    println!("op1 hash       = {op1_hash:?}");
    println!("op1 event_type = {}", h1.extensions.event_type);
    println!("op1 verifies   = {}", h1.verify());

    // op2: a GraphAddNode, second in the SAME author log. Native backlink =
    // op1 (per-author chain). `parents` in extensions carries a multi-parent
    // DAG edge: op1 PLUS a fabricated other-author event.
    let other = Hash::digest(b"another author's event");
    let ext2 = MereExt {
        space_id: "moot:demo".into(),
        event_type: "GraphAddNode".into(),
        target_refs: vec!["node:abc".into()],
        parents: vec![op1_hash, other],
        capabilities: vec!["cap:writer".into()],
    };
    let h2 = build_signed(&signing_key, b"{node:abc}", ext2.clone(), 1, Some(op1_hash));

    // CBOR round-trip the signed header. Extensions + native backlink must
    // survive, and the signature must still verify.
    let bytes = encode_cbor(&h2).expect("encode_cbor");
    let decoded: Header<MereExt> = decode_cbor(&bytes[..]).expect("decode_cbor");
    assert_eq!(decoded.extensions, ext2, "extensions survive CBOR round-trip");
    assert_eq!(decoded.backlink, Some(op1_hash), "native backlink survives");
    assert!(decoded.verify(), "signature verifies after CBOR round-trip");

    // Operation = header + body + operation-id hash.
    let op2 = Operation {
        hash: decoded.hash(),
        header: decoded.clone(),
        body: Some(Body::new(b"{node:abc}")),
    };

    println!("\nheader2 CBOR   = {} bytes", bytes.len());
    println!("op2 id (hash)  = {:?}", op2.hash);
    println!("decoded event  = {}", decoded.extensions.event_type);
    println!(
        "native backlink (per-author log) present = {}",
        decoded.backlink.is_some()
    );
    println!(
        "extension parents (multi-parent DAG) = {} links",
        decoded.extensions.parents.len()
    );
    println!("signature verifies = {}", decoded.verify());

    println!("\n--- S2 VERDICT ---");
    println!("p2panda Header = ONE native backlink + seq_num (per-author linked list).");
    println!("MereEvent multi-parent `parents` has NO native slot; it rides in `extensions`.");
    println!("space_id/event_type/target_refs/capabilities/parents all carry as extensions,");
    println!("survive CBOR + Ed25519 signing; the module docs name 'capabilities' as an");
    println!("intended extension use. => FITS if MereEvent reframes onto per-author-log + refs;");
    println!("a true multi-parent content-hash DAG stays application-level, opaque to p2panda sync.");
}
