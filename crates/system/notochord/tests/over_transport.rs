//! V6 done-condition: an owner rule admits or rejects a real connection
//! **before** the application above ever sees the stream.
//!
//! Runs over a `tokio::io::duplex` pair, which is the same stream shape
//! `MemoryTransport` hands back from `accept`, so the sequence exercised here
//! is exactly what a Murm acceptance adapter performs: accept the session,
//! run the handshake on the raw stream, and only then hand it upward.

#![cfg(feature = "tokio")]

use std::collections::BTreeMap;

use notochord::{
    CarrierKind, DenyReason, LocalNetworkPolicy, NetworkId, ProfileRef, ProofBinding,
    RequestedAction, RevocationLedger, ServiceAccess, ServiceRule, SessionDecision, SessionFacts,
    SessionHello, TrafficClass, TrustedRoot, accept_session, initiate_session,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use personae::{IdentityProvider, InMemoryProvider};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT_AUTHORITY: [u8; 32] = [7; 32];
const MURM: &str = "/services/murm";
const PROTOCOL: &[u8] = b"mere/murm/v1";
const NOW_MS: u64 = 50;
/// What the service sends once a session is admitted.
const APPLICATION_BYTES: &[u8] = b"murm-application-payload";

fn root() -> InMemoryProvider {
    InMemoryProvider::from_seed([1; 32])
}

fn member() -> InMemoryProvider {
    InMemoryProvider::from_seed([4; 32])
}

fn grant_to(subject: [u8; 32]) -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        &root(),
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            root().master_public_key().to_bytes(),
            subject,
            CapabilityScope {
                domain: "mere.network".into(),
                resource: NETWORK.0.to_vec(),
                path_prefix: MURM.into(),
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
        MURM.to_string(),
        ServiceRule {
            access,
            require_transport_identity: false,
            max_sessions: None,
        },
    )]);
    policy
}

fn hello(binding: &ProofBinding, delegations: Vec<SignedDelegationCertificate>) -> SessionHello {
    SessionHello::issue(
        &member(),
        NETWORK,
        ProfileRef {
            id: "mere.base".into(),
            revision: 2,
        },
        RequestedAction {
            domain: "mere.network".into(),
            path: MURM.into(),
            action: "connect".into(),
        },
        TrafficClass::Interactive,
        [42; 32],
        binding,
        delegations,
    )
    .expect("issue hello")
}

/// Drive both halves over one duplex pair. Returns the responder decision and
/// whatever the initiator managed to read from the application afterwards.
async fn exchange(access: ServiceAccess) -> (SessionDecision, Vec<u8>) {
    let policy = policy(access);
    let ledger = RevocationLedger::new();
    let subject = member().master_public_key().to_bytes();
    // Responder view (facts) and initiator view (binding) of one connection.
    let facts = SessionFacts::authenticated(PROTOCOL, CarrierKind::Memory, subject);
    let binding = ProofBinding::initiator(PROTOCOL, Some(subject), None);
    let (mut client, mut server) = tokio::io::duplex(4096);

    let server_policy = policy.clone();
    let server_facts = facts.clone();
    let responder = tokio::spawn(async move {
        let decision = accept_session(
            &mut server,
            &server_policy,
            &ledger,
            &server_facts,
            NOW_MS,
            0,
        )
        .await
        .expect("responder handshake");
        // The service layer sits here: it only ever receives an admitted
        // stream, so a refusal must never reach it.
        if decision.is_accept() {
            server
                .write_all(APPLICATION_BYTES)
                .await
                .expect("service write");
            server.flush().await.expect("flush");
        }
        drop(server);
        decision
    });

    let reply = initiate_session(
        &mut client,
        &hello(&binding, vec![grant_to(subject)]),
        &policy.limits,
    )
    .await
    .expect("initiator handshake");

    let mut application = Vec::new();
    client
        .read_to_end(&mut application)
        .await
        .expect("read application bytes");
    let decision = responder.await.expect("responder task");
    assert_eq!(
        reply.is_accept(),
        decision.is_accept(),
        "both ends agree on the outcome"
    );
    (decision, application)
}

#[tokio::test]
async fn an_authorized_session_is_admitted_and_then_carries_application_bytes() {
    let (decision, application) = exchange(ServiceAccess::MemberOnly).await;
    assert!(decision.is_accept());
    assert_eq!(
        application, APPLICATION_BYTES,
        "the service speaks only after admission"
    );
}

#[tokio::test]
async fn a_refused_session_never_reaches_the_application() {
    let (decision, application) = exchange(ServiceAccess::Disabled).await;
    match decision {
        SessionDecision::Deny { reason } => assert_eq!(reason, DenyReason::ServiceNotOffered),
        SessionDecision::Accept { .. } => panic!("the owner did not offer this service"),
    }
    assert!(
        application.is_empty(),
        "not one application byte crosses a refused session"
    );
}

/// N2 groundwork: the shape both service carriers share. An admitted session
/// hands back the principal and the carrier's facts alongside the stream, and
/// a refusal consumes the stream instead of returning one.
#[tokio::test]
async fn admit_session_yields_the_principal_or_consumes_the_stream() {
    use notochord::admit_session;

    for (access, expect_admit) in [
        (ServiceAccess::MemberOnly, true),
        (ServiceAccess::Disabled, false),
    ] {
        let policy = policy(access);
        let ledger = RevocationLedger::new();
        let subject = member().master_public_key().to_bytes();
        let facts = SessionFacts::authenticated(PROTOCOL, CarrierKind::Memory, subject);
        let binding = ProofBinding::initiator(PROTOCOL, Some(subject), None);
        let hello = hello(&binding, vec![grant_to(subject)]);
        let expected_claims = hello.claims();
        let (mut client, server) = tokio::io::duplex(4096);

        let server_policy = policy.clone();
        let server_facts = facts.clone();
        let responder = tokio::spawn(async move {
            admit_session(server, &server_policy, &ledger, &server_facts, NOW_MS, 0)
                .await
                .expect("responder handshake")
        });

        let reply = initiate_session(&mut client, &hello, &policy.limits)
            .await
            .expect("initiator handshake");
        let outcome = responder.await.expect("responder task");

        assert_eq!(reply.is_accept(), expect_admit, "both ends agree");
        match outcome {
            Ok(session) => {
                assert!(expect_admit);
                assert_eq!(session.principal.subject, subject);
                assert_eq!(
                    session.facts, facts,
                    "the carrier's facts travel with the admitted session"
                );
                assert_eq!(
                    session.claims, expected_claims,
                    "the verified claims travel with the admitted session for later revocation checks"
                );
                assert_eq!(session.principal.action.path, MURM);
            }
            Err(reason) => {
                assert!(!expect_admit);
                assert_eq!(reason, DenyReason::ServiceNotOffered);
                // The stream was consumed by the refusal, so the client sees
                // the end of it rather than waiting on a service that will
                // never speak.
                let mut rest = Vec::new();
                tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    client.read_to_end(&mut rest),
                )
                .await
                .expect("a finished refusal must not leave the peer hanging")
                .expect("read to end");
                assert!(rest.is_empty(), "a refusal sends nothing after its reply");
            }
        }
    }
}
