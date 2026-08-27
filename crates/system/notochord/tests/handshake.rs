// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! V6/N0: the bounded session handshake, and the attacks it must refuse.
//!
//! Note the two-role shape throughout: the responder passes [`SessionFacts`]
//! (what its carrier observed), while the initiator signs against a
//! [`ProofBinding`] built from its *own* transport identity. The proof
//! verifies only when the two independently derive the same bytes.

use std::collections::BTreeMap;

use notochord::{
    CarrierKind, ChainFault, DenyReason, HandshakeLimits, LocalNetworkPolicy, NetworkId,
    ProfileRef, ProofBinding, RequestedAction, RevocationLedger, ServiceAccess, ServiceRule,
    SessionDecision, SessionFacts, SessionHello, SessionReply, TrafficClass, TrustedRoot, respond,
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
        ServiceRule::new(access, "mere.network", ["connect"], false, None),
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
    binding: &ProofBinding,
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

/// A p2panda-shaped connection, as the responder observes it.
fn carrier_authenticating(initiator: [u8; 32]) -> SessionFacts {
    SessionFacts::authenticated(PROTOCOL, CarrierKind::P2panda, initiator)
}

/// The same connection as the initiator signs it: from its own identity.
fn initiator_binding(own_identity: [u8; 32]) -> ProofBinding {
    ProofBinding::initiator(PROTOCOL, Some(own_identity), None)
}

/// A Reticulum-shaped connection: no carrier identity, but a shared link.
fn reticulum_facts(link: [u8; 16]) -> SessionFacts {
    SessionFacts::new(PROTOCOL, CarrierKind::Reticulum).with_ingress(Some(3), Some(link))
}

fn reticulum_binding(link: [u8; 16]) -> ProofBinding {
    ProofBinding::initiator(PROTOCOL, None, Some(link))
}

fn member_key() -> [u8; 32] {
    member().master_public_key().to_bytes()
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
    let binding = initiator_binding(member_key());
    let facts = carrier_authenticating(member_key());
    let hello = hello_from(&member(), &binding, vec![member_grant_to(member_key())]);
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (reply_bytes, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
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
fn a_valid_certificate_presented_by_the_wrong_carrier_peer_is_rejected() {
    // The p2panda property the plan names: a stranger holds a perfectly good
    // certificate issued to the member, and replays that authority over their
    // own authenticated connection.
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let stranger_key = stranger().master_public_key().to_bytes();

    // The carrier authenticated the stranger, not the member.
    let facts = carrier_authenticating(stranger_key);
    let binding = initiator_binding(stranger_key);
    // The stranger signs honestly, for themself, but presents the member grant.
    let hello = hello_from(&stranger(), &binding, vec![member_grant_to(member_key())]);
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
    assert_eq!(
        reason(&decision),
        DenyReason::Delegation(ChainFault::SubjectMismatch),
        "the presented chain does not name the connecting subject"
    );
}

#[test]
fn a_claimed_subject_cannot_overwrite_a_carrier_authenticated_peer() {
    // N0 receipt. The carrier proved the stranger; the hello claims to be the
    // member and carries the member's grant. The fact must win over the claim.
    let policy = policy(ServiceAccess::Public);
    let ledger = RevocationLedger::new();
    let stranger_key = stranger().master_public_key().to_bytes();
    let facts = carrier_authenticating(stranger_key);
    let binding = initiator_binding(stranger_key);

    let mut hello = hello_from(&stranger(), &binding, Vec::new());
    hello.subject = member_key();
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
    // The forged subject breaks the proof before policy even weighs in, which
    // is the stronger of the two refusals available here.
    assert_eq!(reason(&decision), DenyReason::SessionProofInvalid);
}

#[test]
fn a_hello_claiming_another_identity_fails_its_proof() {
    // The subject field says "member", but the signer is the stranger. The
    // attestation binds the derived key to its own master, so this cannot pass.
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let binding = initiator_binding(member_key());
    let facts = carrier_authenticating(member_key());
    let mut hello = hello_from(&stranger(), &binding, vec![member_grant_to(member_key())]);
    hello.subject = member_key();
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
    assert_eq!(reason(&decision), DenyReason::SessionProofInvalid);
}

#[test]
fn reticulum_admits_a_proved_subject_with_no_carrier_identity() {
    // N0 receipt: best-effort acceptance reports no authenticated initiator,
    // and the session proof alone establishes the subject.
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let link = [0xaa; 16];
    let facts = reticulum_facts(link);
    assert!(facts.authenticated_initiator.is_none());

    let hello = hello_from(
        &member(),
        &reticulum_binding(link),
        vec![member_grant_to(member_key())],
    );
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
    assert!(decision.is_accept());
}

#[test]
fn a_proof_replayed_on_a_different_link_is_rejected() {
    // The transcript binds the shared link, so a hello captured on one link
    // does not verify on another.
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let link = [0xaa; 16];
    let hello = hello_from(
        &member(),
        &reticulum_binding(link),
        vec![member_grant_to(member_key())],
    );
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, admitted) = respond(&policy, &ledger, &bytes, &reticulum_facts(link), NOW_MS, 0);
    assert!(
        admitted.is_accept(),
        "it is good on the link it was minted for"
    );

    let (_, decision) = respond(
        &policy,
        &ledger,
        &bytes,
        &reticulum_facts([0xbb; 16]),
        NOW_MS,
        0,
    );
    assert_eq!(reason(&decision), DenyReason::SessionProofInvalid);
}

#[test]
fn a_differing_local_interface_does_not_break_the_proof() {
    // N0 receipt, and the deliberate asymmetry: the link is shared and signed,
    // but the interface id is local to whoever assigned it. Binding it would
    // reject every honest session.
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let link = [0xaa; 16];
    let hello = hello_from(
        &member(),
        &reticulum_binding(link),
        vec![member_grant_to(member_key())],
    );
    let bytes = hello.encode(&policy.limits).expect("encode");

    // The responder saw the same link over a wildly different interface number.
    let responder_facts =
        SessionFacts::new(PROTOCOL, CarrierKind::Reticulum).with_ingress(Some(9_999), Some(link));
    let (_, decision) = respond(&policy, &ledger, &bytes, &responder_facts, NOW_MS, 0);
    assert!(
        decision.is_accept(),
        "the two ends need not agree on a purely local interface number"
    );
}

#[test]
fn a_tampered_hello_field_fails_its_proof() {
    let policy = policy(ServiceAccess::MemberOnly);
    let ledger = RevocationLedger::new();
    let binding = initiator_binding(member_key());
    let facts = carrier_authenticating(member_key());
    let mut hello = hello_from(&member(), &binding, vec![member_grant_to(member_key())]);
    hello.action.path = "/services/secret".into();
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
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
    let binding = initiator_binding(member_key());
    let facts = carrier_authenticating(member_key());
    let hello = hello_from(&member(), &binding, vec![member_grant_to(member_key())]);
    let bytes = hello
        .encode(&HandshakeLimits::default())
        .expect("encode at the default bound");
    assert!(
        bytes.len() > 32,
        "the fixture must exceed the tightened bound"
    );

    let (_, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
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
    let binding = initiator_binding(member_key());
    let facts = carrier_authenticating(member_key());
    let chain = vec![member_grant_to(member_key()); 6];
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

    let (_, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
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
        &carrier_authenticating(member_key()),
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
    let binding = initiator_binding(member_key());
    let facts = carrier_authenticating(member_key());
    let hello = hello_from(&member(), &binding, Vec::new());
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
    assert!(decision.is_accept());
}

#[test]
fn a_good_proof_does_not_open_a_service_the_owner_has_not_offered() {
    let policy = policy(ServiceAccess::Disabled);
    let ledger = RevocationLedger::new();
    let binding = initiator_binding(member_key());
    let facts = carrier_authenticating(member_key());
    let hello = hello_from(&member(), &binding, vec![member_grant_to(member_key())]);
    let bytes = hello.encode(&policy.limits).expect("encode");

    let (_, decision) = respond(&policy, &ledger, &bytes, &facts, NOW_MS, 0);
    assert_eq!(reason(&decision), DenyReason::ServiceNotOffered);
}
