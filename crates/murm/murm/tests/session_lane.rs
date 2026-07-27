//! Notochord N2, Murm half: the owner's rule decides a real Murm connection
//! before the engine sees a byte.
//!
//! This is the low-power plan's V6 done-condition made concrete. The lane
//! carries the same posts the gossip lane does; what is new is that a peer has
//! to get past the owner's policy first, and a refused one leaves no trace in
//! the conversation.

#![cfg(feature = "session-lane")]

use std::collections::BTreeMap;
use std::sync::Arc;

use identity::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use identity::{IdentityProvider, InMemoryProvider};
use murm::{
    Admission, CabalKey, ConversationEngine, Post, SessionOutcome, lane_binding, push_posts,
    serve_session,
};
use notochord::{
    CarrierKind, DenyReason, LocalNetworkPolicy, NetworkId, ProfileRef, RequestedAction,
    RevocationLedger, ServiceAccess, ServiceRule, SessionFacts, SessionHello, TrafficClass,
    TrustedRoot,
};

const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT_AUTHORITY: [u8; 32] = [7; 32];
const MURM_SERVICE: &str = "/services/murm";
const PROTOCOL: &[u8] = b"mere/murm/v1";
const NOW_MS: u64 = 50;

fn root() -> InMemoryProvider {
    InMemoryProvider::from_seed([1; 32])
}

fn member() -> InMemoryProvider {
    InMemoryProvider::from_seed([4; 32])
}

fn member_key() -> [u8; 32] {
    member().master_public_key().to_bytes()
}

/// A grant for some other service's action triple.
///
/// Mirrors Graphshell's (`mere.graphshell` / `/services/projection`), spelled
/// out rather than imported: murm must not depend on a port, and the point of
/// the test is that a perfectly valid grant for *someone else's* service is
/// worthless here.
fn foreign_grant() -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        &root(),
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            root().master_public_key().to_bytes(),
            member_key(),
            CapabilityScope {
                domain: "mere.graphshell".into(),
                resource: NETWORK.0.to_vec(),
                path_prefix: "/services/projection".into(),
                actions: ["connect".to_string()].into_iter().collect(),
            },
            5,
            10,
            Some(100),
            1,
            [2; 32],
        ),
    )
    .expect("issue certificate")
}

fn grant() -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        &root(),
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            root().master_public_key().to_bytes(),
            member_key(),
            CapabilityScope {
                domain: "mere.network".into(),
                resource: NETWORK.0.to_vec(),
                path_prefix: MURM_SERVICE.into(),
                actions: ["connect".to_string()].into_iter().collect(),
            },
            5,
            10,
            Some(100),
            1,
            [1; 32],
        ),
    )
    .expect("issue certificate")
}

fn policy(access: ServiceAccess) -> LocalNetworkPolicy {
    let mut policy = LocalNetworkPolicy::closed(NETWORK);
    policy.accepted_profiles = vec![ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }];
    policy.trusted_roots = vec![TrustedRoot {
        authority: ROOT_AUTHORITY,
        issuer: root().master_public_key().to_bytes(),
    }];
    policy.services = BTreeMap::from([(
        MURM_SERVICE.to_string(),
        ServiceRule {
            access,
            require_transport_identity: false,
            max_sessions: None,
        },
    )]);
    policy
}

fn hello() -> SessionHello {
    hello_with(vec![grant()])
}

fn hello_with(delegations: Vec<SignedDelegationCertificate>) -> SessionHello {
    SessionHello::issue(
        &member(),
        NETWORK,
        ProfileRef {
            id: "mere.base".into(),
            revision: 2,
        },
        RequestedAction {
            domain: "mere.network".into(),
            path: MURM_SERVICE.into(),
            action: "connect".into(),
        },
        TrafficClass::Interactive,
        [42; 32],
        &lane_binding(PROTOCOL, member_key()),
        delegations,
    )
    .expect("issue hello")
}

/// A server engine holding one cabal, and a properly signed post for it.
///
/// The post is authored in the *sender's* engine so it carries a real
/// signature and the self-describing cabal id; the server ingests the encoded
/// form, exactly as it would off the gossip lane.
async fn fixtures() -> (Arc<ConversationEngine>, [u8; 32], Post) {
    let key = CabalKey::new([9u8; 32]);

    let sender = Arc::new(ConversationEngine::new(Arc::new(member())));
    let cabal_id = sender.open(*key.as_bytes()).await.expect("sender opens");
    let post_id = sender
        .post_text(&cabal_id, "general", "over the mesh", 1_000)
        .await
        .expect("author a post");
    let post = sender
        .get_post(&cabal_id, &post_id)
        .expect("the authored post");

    let server = Arc::new(ConversationEngine::new(Arc::new(
        InMemoryProvider::from_seed([8; 32]),
    )));
    server.open(*key.as_bytes()).await.expect("server opens");
    (server, cabal_id, post)
}

/// Run one session end to end, returning the server's outcome and how many
/// posts the conversation actually holds afterwards.
async fn run(access: ServiceAccess) -> (Result<SessionOutcome, DenyReason>, usize) {
    run_with(access, hello()).await
}

async fn run_with(
    access: ServiceAccess,
    hello: SessionHello,
) -> (Result<SessionOutcome, DenyReason>, usize) {
    let (engine, cabal_id, post) = fixtures().await;
    let policy = policy(access);
    let ledger = RevocationLedger::new();
    let facts = SessionFacts::authenticated(PROTOCOL, CarrierKind::Memory, member_key());
    let (client, server) = tokio::io::duplex(64 * 1024);

    let server_engine = Arc::clone(&engine);
    let server_policy = policy.clone();
    let responder = tokio::spawn(async move {
        serve_session(
            server,
            &server_engine,
            cabal_id,
            Admission {
                policy: &server_policy,
                ledger: &ledger,
                facts: &facts,
                now_ms: NOW_MS,
                active_sessions: 0,
            },
        )
        .await
        .expect("serve session")
    });

    let _ = push_posts(client, &hello, &policy, std::slice::from_ref(&post))
        .await
        .expect("push posts");
    let outcome = responder.await.expect("responder task");
    let held = engine.history(&cabal_id, "general").len();
    (outcome, held)
}

#[tokio::test]
async fn an_admitted_peer_reaches_the_conversation() {
    let (outcome, held) = run(ServiceAccess::MemberOnly).await;
    let outcome = outcome.expect("a member with a valid grant is admitted");
    assert_eq!(outcome.posts_ingested, 1);
    assert_eq!(outcome.posts_rejected, 0);
    assert_eq!(
        outcome.principal.subject,
        member_key(),
        "traffic is attributed to the subject the proof established"
    );
    assert_eq!(held, 1, "the post landed in the cabal");
}

#[tokio::test]
async fn a_refused_peer_never_reaches_the_engine() {
    // The V6 done-condition: the owner has not offered this service, so the
    // connection is refused before Murm receives application bytes.
    let (outcome, held) = run(ServiceAccess::Disabled).await;
    match outcome {
        Err(reason) => assert_eq!(reason, DenyReason::ServiceNotOffered),
        Ok(_) => panic!("the owner did not offer this service"),
    }
    assert_eq!(
        held, 0,
        "not one post crosses into the conversation on a refused session"
    );
}

#[tokio::test]
async fn a_grant_for_another_service_does_not_open_murm() {
    // Half of the Notochord promotion gate: "a Graphshell projection grant
    // cannot open Murm". The chain is valid, signed by a root this node
    // trusts, and issued to the very peer that is connecting — it simply is
    // not a grant for *this* service, and that is the whole of the objection.
    let (outcome, held) =
        run_with(ServiceAccess::MemberOnly, hello_with(vec![foreign_grant()])).await;
    match outcome {
        Err(reason) => assert_eq!(
            reason,
            DenyReason::ActionNotCovered,
            "a valid grant for another domain is still not authority here"
        ),
        Ok(_) => panic!("a projection grant must not open a cabal"),
    }
    assert_eq!(held, 0, "and nothing it sent reaches the conversation");
}
