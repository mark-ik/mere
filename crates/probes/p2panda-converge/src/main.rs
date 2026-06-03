//! Probe 1: concurrent multi-author convergence through p2panda LogSync, and
//! what the per-author-log model guarantees about cross-author `parents` refs.
//!
//! Outcome (recorded in the spike plan, Probe 1):
//!
//! 1. **Convergence works and is p2panda's tested guarantee.** Its own
//!    `e2e_log_sync` test (read from p2panda-net 0.6.1 source) drives two
//!    authors' logs to converge via LogSync (each receives the other's
//!    operations: SyncStarted -> OperationReceived -> SyncFinished ->
//!    LiveModeStarted). This probe reproduces the full *public* setup path
//!    (spawn, author MereEvents into a log, associate the log with a topic,
//!    subscribe) and confirms it works.
//!
//! 2. **The public sync API is discovery-driven, not manually initiated.**
//!    `SyncHandle::initiate_session` is `#[cfg(test)]` (in-crate only); out of
//!    crate, sessions start from discovery (mDNS / random-walk / relay
//!    bootstrap). `TestNode` hardcodes *passive* mDNS, so two in-process nodes
//!    will not find each other without real discovery wiring. So a moot's
//!    members discover each other, then LogSync converges the topic's
//!    associated logs.
//!
//! 3. **The decisive finding: causal completeness is association-policy
//!    dependent, not an automatic DAG property.** LogSync syncs the author-logs
//!    ASSOCIATED with a topic (`associate(topic, {author: [log_ids]})`) and
//!    treats the operation body / extensions (where `MereEvent.parents` live)
//!    as opaque. A cross-author `parents` ref resolves only if that author's log
//!    was also associated and synced under the topic. Dropping the native
//!    multi-parent DAG (where the sync layer walks parents to guarantee causal
//!    completeness) means we must guarantee it ourselves: the moot associates
//!    every member log, and any ref outside the associated set needs app-level
//!    backfill. Keep `parents` as application-level data (the substrate brief's
//!    stance), now with the mechanism named.
//!
//! See design_docs/mere_docs/implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md (Probe 1).

use std::collections::HashMap;

use p2panda_core::Topic;
use p2panda_net::test_utils::TestNode;
use serde::{Deserialize, Serialize};

/// A MereEvent carried in an operation body. `parents` are the multi-parent DAG
/// refs the substrate brief wants; here they ride opaquely in the payload.
#[derive(Debug, Serialize, Deserialize)]
struct MereEvent {
    event_type: String,
    parents: Vec<String>,
    text: String,
}

const EVENTS_PER_AUTHOR: usize = 3;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let topic: Topic = [0u8; 32].into();
    let log_id: u64 = 0;

    // Two nodes on loopback. Alice is seeded with Bob's node_info.
    let mut bob = TestNode::spawn([11; 32], None).await;
    let mut alice = TestNode::spawn([10; 32], Some(bob.node_info())).await;

    // Each author independently writes MereEvents into their own log; event N
    // names event N-1 as a parent (an intra-author causal edge). This is the
    // full public authoring path.
    let alice_hashes = author_events(&mut alice, "alice", log_id).await;
    let bob_hashes = author_events(&mut bob, "bob", log_id).await;
    println!("alice authored {} events, bob authored {}", alice_hashes.len(), bob_hashes.len());

    // Each associates their own log with the shared topic (this is what tells
    // LogSync the log belongs to the topic).
    alice
        .client
        .associate(&topic, &HashMap::from([(alice.client_id(), vec![log_id])]))
        .await;
    bob.client
        .associate(&topic, &HashMap::from([(bob.client_id(), vec![log_id])]))
        .await;

    // Both can subscribe to the topic stream (the public receive path).
    let alice_handle = alice.log_sync.stream(topic, true).await?;
    let _alice_sub = alice_handle.subscribe().await?;
    let bob_handle = bob.log_sync.stream(topic, true).await?;
    let _bob_sub = bob_handle.subscribe().await?;
    println!("both nodes subscribed to the topic stream (public setup path OK)");

    println!("\n--- Probe 1 VERDICT ---");
    println!("Public setup path works: spawn, author MereEvents-in-bodies (with parents),");
    println!("associate logs with a topic, subscribe. Convergence itself is p2panda's tested");
    println!("guarantee (e2e_log_sync, read from source): two authors' associated logs sync,");
    println!("each receiving the other's operations.");
    println!();
    println!("Constraint: the PUBLIC sync API is discovery-driven; initiate_session is");
    println!("#[cfg(test)]. A moot's members discover each other, then LogSync converges.");
    println!();
    println!("Decisive: LogSync converges the author-logs ASSOCIATED with a topic and treats");
    println!("`parents` as opaque payload. Cross-author causal completeness is therefore a");
    println!("function of topic-association policy, NOT an automatic multi-parent-DAG property.");
    println!("=> Keep `parents` application-level; the moot must associate every member log so");
    println!("   refs resolve; refs outside the associated set need app-level backfill.");
    println!();
    println!("Side-finding feeding Probe 2: a moot maps to a flat topic + per-author logs.");
    println!("Hierarchical graph-cluster namespaces do NOT map onto p2panda's flat topics,");
    println!("so the spatial namespace layer wants Willow/Mere-native, not p2panda topics.");

    Ok(())
}

/// Author `EVENTS_PER_AUTHOR` MereEvents into one log; each names the previous
/// op's hash as a parent. Returns the op hashes (as debug strings).
async fn author_events(node: &mut TestNode, who: &str, log_id: u64) -> Vec<String> {
    let mut hashes = Vec::new();
    let mut prev_parent: Vec<String> = Vec::new();
    for i in 0..EVENTS_PER_AUTHOR {
        let event = MereEvent {
            event_type: "ChatMessage".to_string(),
            parents: prev_parent.clone(),
            text: format!("{who} #{i}"),
        };
        let body = serde_json::to_vec(&event).expect("serialize MereEvent");
        let (header, _bytes, _body) = node.client.create_operation(&body, log_id).await;
        let hash = format!("{:?}", header.hash());
        prev_parent = vec![hash.clone()];
        hashes.push(hash);
    }
    hashes
}
