//! V6: the bounded session handshake, and the attacks it must refuse.

use std::collections::BTreeMap;

use network_policy::{
    ChainFault, DenyReason, HandshakeLimits, LocalNetworkPolicy, NetworkId, ProfileRef,
    RequestedAction, RevocationLedger, ServiceAccess, ServiceRule, SessionBinding, SessionDecision,
    SessionHello, SessionReply, TrafficClass, TrustedRoot, respond,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use personae::{IdentityProvider, InMemoryProvider};

const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT_AUTHORITY: [u8; 32] = [7; 32];
const MURM: &str = "/services/murm";
const PROTOCOL: &[u8] = b"mere/murm/v1";
const NOW_MS: u64 = 50;

fn root() -> InMemoryProvider {
    InMemoryProvider::from_seed([1; 32])
}

fn member() -> InMemoryProvider {
    InMemoryProvider::from_seed([4; 32])
}

fn stranger() -> InMemoryProvider {
    InMemoryProvider::from_seed([11; 32])
}

fn member_grant_to(subject: [u8; 32]) -> SignedDelegationCertificate {
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

fn action() -> RequestedAction {
    RequestedAction {
        domain: "mere.network".into(),
        path: MURM.into(),
        action: "connect".into(),
    }
}

fn hello_from(
    provider: &InMemoryProvider,
    binding: &SessionBinding,
    delegations: Vec<SignedDelegationCertificate>,
) -> SessionHello {
    SessionHello::issue(
        provider,
        NETWORK,
        ProfileRef {
            id: "mere.base".into(),
            revision: 2,
        },
        action(),
        TrafficClass::Interactive,
        [42; 32],
        binding,
        delegations,
    )
    .expect("issue hello")
}

/// A p2panda-shaped connection: the transport proved the member identity.
fn authenticated_binding() -> SessionBinding {
    SessionBinding::authenticated(PROTOCOL, member().master_public_key().to_bytes())
}

/// A Reticulum-shaped connection: no transport identity, but a link to bind to.
fn reticulum_binding(link: [u8; 16]) -> SessionBinding {
    SessionBinding {
        protocol: PROTOCOL.to_vec(),
        transport_peer: None,
        interface: Some(3),
        link: Some(link),
    }
}

fn reason(decision: &SessionDecision) -> DenyReason {
    match decision {
        SessionDecision::Deny { reason } => reason.clone(),
        SessionDecision::Accept { .. } => panic!("expected a denial"),
    }
}

#[test]
fn an_authorized_hello_is_admitted_and_the_reply_decodes() {
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let binding = authenticated_binding();
    let subject = member().master_public_key().to_bytes();
    let hello = hello_from(&member(), &binding, vec![member_grant_to(subject)]);
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (reply_bytes, decision) = respond(&policy, &ledger, &bytes, &binding, NOW_MS, 0);
    assert!(decision.is_accept(), "a valid member hello is admitted");

    let reply = SessionReply::decode(&reply_bytes, &policy.limits).expect("decode reply");
    match reply {
        SessionReply::Accept {
            session_id,
            class,
            profile_revision,
            ..
        } => {
            assert_eq!(class, TrafficClass::Interactive);
            assert_eq!(
                profile_revision, 1,
                "the responder answers with its own revision"
            );
            assert_eq!(
                session_id,
                hello.session_id(&binding),
                "both sides derive the same session id from the bound transcript"
            );
        }
        SessionReply::Reject { reason } => panic!("unexpected rejection: {reason}"),
    }
}

#[test]
fn a_valid_certificate_presented_by_the_wrong_transport_peer_is_rejected() {
    // The p2panda property the plan names: a stranger holds a perfectly good
    // certificate issued to the member, and replays that authority over their
    // own authenticated connection.
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let subject = member().master_public_key().to_bytes();

    // The connection is authenticated as the stranger, not the member.
    let binding =
        SessionBinding::authenticated(PROTOCOL, stranger().master_public_key().to_bytes());
    // The stranger signs honestly, for themself, but presents the member grant.
    let hello = hello_from(&stranger(), &binding, vec![member_grant_to(subject)]);
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &binding, NOW_MS, 0);
    assert_eq!(
        reason(&decision),
        DenyReason::Delegation(ChainFault::SubjectMismatch),
        "the presented chain does not name the connecting subject"
    );
}

#[test]
fn a_hello_claiming_another_identity_fails_its_proof() {
    // The subject field says "member", but the signer is the stranger. The
    // attestation binds the derived key to its own master, so this cannot pass.
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let binding = authenticated_binding();
    let subject = member().master_public_key().to_bytes();
    let mut hello = hello_from(&stranger(), &binding, vec![member_grant_to(subject)]);
    hello.subject = subject;
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &binding, NOW_MS, 0);
    assert_eq!(reason(&decision), DenyReason::SessionProofInvalid);
}

#[test]
fn a_proof_replayed_on_a_different_link_is_rejected() {
    // The Reticulum property: the transcript binds the link, so a hello
    // captured on one link does not verify on another.
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let subject = member().master_public_key().to_bytes();
    let original = reticulum_binding([0xaa; 16]);
    let hello = hello_from(&member(), &original, vec![member_grant_to(subject)]);
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, admitted) = respond(&policy, &ledger, &bytes, &original, NOW_MS, 0);
    assert!(
        admitted.is_accept(),
        "it is good on the link it was minted for"
    );

    let replayed = reticulum_binding([0xbb; 16]);
    let (_, decision) = respond(&policy, &ledger, &bytes, &replayed, NOW_MS, 0);
    assert_eq!(reason(&decision), DenyReason::SessionProofInvalid);
}

#[test]
fn a_tampered_hello_field_fails_its_proof() {
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let binding = authenticated_binding();
    let subject = member().master_public_key().to_bytes();
    let mut hello = hello_from(&member(), &binding, vec![member_grant_to(subject)]);
    hello.action.path = "/services/secret".into();
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &binding, NOW_MS, 0);
    assert_eq!(reason(&decision), DenyReason::SessionProofInvalid);
}

#[test]
fn an_oversized_hello_is_refused_without_being_parsed() {
    let mut policy = policy(ServiceAccess::MemberOnly);
    policy.limits = HandshakeLimits {
        max_hello_bytes: 32,
        ..HandshakeLimits::default()
    };
    let ledger = RevocationLedger::new();
    let binding = authenticated_binding();
    let subject = member().master_public_key().to_bytes();
    let hello = hello_from(&member(), &binding, vec![member_grant_to(subject)]);
    let bytes = hello
        .encode(&HandshakeLimits::default())
        .expect("encode at the default bound");
    assert!(
        bytes.len() > 32,
        "the fixture must exceed the tightened bound"
    );

    let (_, decision) = respond(&policy, &ledger, &bytes, &binding, NOW_MS, 0);
    assert_eq!(reason(&decision), DenyReason::MalformedHello);
}

#[test]
fn too_many_certificates_are_refused_by_both_sides() {
    // Roomy byte budget on both sides so the count is the only bound in play.
    let mut policy = policy(ServiceAccess::MemberOnly);
    policy.limits = HandshakeLimits {
        max_hello_bytes: 8_192,
        max_certificates: 4,
        ..HandshakeLimits::default()
    };
    let ledger = RevocationLedger::new();
    let binding = authenticated_binding();
    let subject = member().master_public_key().to_bytes();
    let chain = vec![member_grant_to(subject); 6];
    let hello = hello_from(&member(), &binding, chain);

    assert!(
        hello.encode(&policy.limits).is_err(),
        "the initiator refuses to send it"
    );

    let bytes = hello
        .encode(&HandshakeLimits {
            max_hello_bytes: 8_192,
            max_certificates: 8,
            ..HandshakeLimits::default()
        })
        .expect("encode with room");
    assert!(
        bytes.len() <= policy.limits.max_hello_bytes as usize,
        "the frame must fit the responder byte budget so only the count can fail"
    );
    let (_, decision) = respond(&policy, &ledger, &bytes, &binding, NOW_MS, 0);
    assert_eq!(
        reason(&decision),
        DenyReason::MalformedHello,
        "and the responder refuses to read it"
    );
}

#[test]
fn garbage_bytes_are_refused_cleanly() {
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let (reply_bytes, decision) = respond(
        &policy,
        &ledger,
        &[0xff; 64],
        &authenticated_binding(),
        NOW_MS,
        0,
    );
    assert_eq!(reason(&decision), DenyReason::MalformedHello);
    assert!(
        SessionReply::decode(&reply_bytes, &policy.limits).is_ok(),
        "a refusal is still a well-formed reply the caller can write"
    );
}

#[test]
fn a_public_service_admits_a_hello_carrying_no_authority() {
    let policy = policy(ServiceAccess::Public);
    let ledger = RevocationLedger::new();
    let binding = authenticated_binding();
    let hello = hello_from(&member(), &binding, Vec::new());
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &binding, NOW_MS, 0);
    assert!(decision.is_accept());
}

#[test]
fn a_good_proof_does_not_open_a_service_the_owner_has_not_offered() {
    let policy = policy(ServiceAccess::Disabled);
    let ledger = RevocationLedger::new();
    let binding = authenticated_binding();
    let subject = member().master_public_key().to_bytes();
    let hello = hello_from(&member(), &binding, vec![member_grant_to(subject)]);
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &binding, NOW_MS, 0);
    assert_eq!(reason(&decision), DenyReason::ServiceNotOffered);
}
