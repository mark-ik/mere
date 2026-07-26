//! The V5 policy matrix: deterministic admission decisions over real
//! personae delegation statements, exercising the crate's public surface.

use std::collections::BTreeMap;

use network_policy::{
    CarrierKind, ChainFault, DenyReason, HandshakeLimits, LocalNetworkPolicy, NetworkId,
    ProfileRef, RequestedAction, RevocationLedger, ServiceAccess, ServiceRule, SessionClaims,
    SessionDecision, SessionFacts, TrafficClass, TrustedRoot,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
    SignedDelegationCertificate, SignedDelegationRevocation,
};
use personae::{IdentityProvider, InMemoryProvider};

const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT_AUTHORITY: [u8; 32] = [7; 32];
const MURM: &str = "/services/murm";
const NOW_MS: u64 = 50;

fn root() -> InMemoryProvider {
    InMemoryProvider::from_seed([1; 32])
}

fn intermediate() -> InMemoryProvider {
    InMemoryProvider::from_seed([2; 32])
}

fn member() -> InMemoryProvider {
    InMemoryProvider::from_seed([4; 32])
}

fn scope(path: &str) -> CapabilityScope {
    CapabilityScope {
        domain: "mere.network".into(),
        resource: NETWORK.0.to_vec(),
        path_prefix: path.into(),
        actions: ["connect".to_string()].into_iter().collect(),
    }
}

fn grant(
    issuer: &InMemoryProvider,
    parent: DelegationParent,
    subject: [u8; 32],
    path: &str,
    expires_at_ms: Option<u64>,
    remaining_depth: u16,
    nonce: u8,
) -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        issuer,
        DelegationCertificate::new(
            parent,
            issuer.master_public_key().to_bytes(),
            subject,
            scope(path),
            5,
            10,
            expires_at_ms,
            remaining_depth,
            [nonce; 32],
        ),
    )
    .expect("issue certificate")
}

/// One certificate straight from the root authority to the member.
fn member_grant() -> SignedDelegationCertificate {
    grant(
        &root(),
        DelegationParent::Root(ROOT_AUTHORITY),
        member().master_public_key().to_bytes(),
        MURM,
        Some(100),
        1,
        1,
    )
}

/// Root grants `/services` to the intermediate, which narrows to the member.
fn two_step_chain() -> Vec<SignedDelegationCertificate> {
    let mid = intermediate();
    let parent = grant(
        &root(),
        DelegationParent::Root(ROOT_AUTHORITY),
        mid.master_public_key().to_bytes(),
        "/services",
        Some(100),
        2,
        2,
    );
    let child = grant(
        &mid,
        DelegationParent::Certificate(parent.certificate.id()),
        member().master_public_key().to_bytes(),
        MURM,
        Some(90),
        1,
        3,
    );
    vec![parent, child]
}

fn policy_with(rule: ServiceRule) -> LocalNetworkPolicy {
    let mut policy = LocalNetworkPolicy::closed(NETWORK);
    policy.accepted_profiles = vec![ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }];
    policy.trusted_roots = vec![TrustedRoot {
        authority: ROOT_AUTHORITY,
        issuer: root().master_public_key().to_bytes(),
    }];
    policy.services = BTreeMap::from([(MURM.to_string(), rule)]);
    policy
}

fn member_only_rule() -> ServiceRule {
    ServiceRule {
        access: ServiceAccess::MemberOnly,
        require_transport_identity: false,
        max_sessions: None,
    }
}

/// Carrier facts for an authenticating carrier that proved the member.
fn facts() -> SessionFacts {
    SessionFacts::authenticated(
        b"mere/murm/v1".to_vec(),
        CarrierKind::Memory,
        member().master_public_key().to_bytes(),
    )
}

fn request(delegations: Vec<SignedDelegationCertificate>) -> SessionClaims {
    SessionClaims {
        wire_version: 1,
        network: NETWORK,
        profile: ProfileRef {
            id: "mere.base".into(),
            revision: 2,
        },
        action: RequestedAction {
            domain: "mere.network".into(),
            path: MURM.into(),
            action: "connect".into(),
        },
        class: TrafficClass::Interactive,
        subject: member().master_public_key().to_bytes(),
        delegations,
    }
}

fn denial(decision: SessionDecision) -> DenyReason {
    match decision {
        SessionDecision::Deny { reason } => reason,
        SessionDecision::Accept { .. } => panic!("expected a denial"),
    }
}

#[test]
fn public_service_admits_while_transit_stays_disabled() {
    let policy = policy_with(ServiceRule {
        access: ServiceAccess::Public,
        require_transport_identity: false,
        max_sessions: None,
    });
    let ledger = RevocationLedger::new();
    let decision = policy.evaluate(&facts(), &request(Vec::new()), &ledger, NOW_MS, 0);
    assert!(
        decision.is_accept(),
        "public service admits without authority"
    );
    assert!(
        !policy.permits_transit(),
        "the transit axis stays independent"
    );
}

#[test]
fn transit_enabled_while_the_service_stays_private() {
    let mut policy = policy_with(ServiceRule {
        access: ServiceAccess::Disabled,
        require_transport_identity: false,
        max_sessions: None,
    });
    policy.transit.enabled = true;
    let ledger = RevocationLedger::new();
    assert!(policy.permits_transit());
    assert_eq!(
        denial(policy.evaluate(&facts(), &request(Vec::new()), &ledger, NOW_MS, 0)),
        DenyReason::ServiceNotOffered,
        "offering transit must not open the service"
    );
}

#[test]
fn member_only_service_admits_a_valid_chain_and_refuses_none() {
    let policy = policy_with(member_only_rule());
    let ledger = RevocationLedger::new();
    assert!(
        policy
            .evaluate(&facts(), &request(vec![member_grant()]), &ledger, NOW_MS, 0)
            .is_accept()
    );
    assert_eq!(
        denial(policy.evaluate(&facts(), &request(Vec::new()), &ledger, NOW_MS, 0)),
        DenyReason::Delegation(ChainFault::Empty)
    );
}

#[test]
fn missing_transport_identity_where_one_is_required() {
    let policy = policy_with(ServiceRule {
        access: ServiceAccess::Public,
        require_transport_identity: true,
        max_sessions: None,
    });
    let ledger = RevocationLedger::new();
    let anonymous_carrier = SessionFacts::new(b"mere/murm/v1".to_vec(), CarrierKind::Reticulum);
    assert_eq!(
        denial(policy.evaluate(&anonymous_carrier, &request(Vec::new()), &ledger, NOW_MS, 0)),
        DenyReason::TransportIdentityRequired
    );
}

#[test]
fn an_expired_certificate_is_refused() {
    let policy = policy_with(member_only_rule());
    let ledger = RevocationLedger::new();
    let expired = grant(
        &root(),
        DelegationParent::Root(ROOT_AUTHORITY),
        member().master_public_key().to_bytes(),
        MURM,
        Some(30),
        1,
        4,
    );
    assert_eq!(
        denial(policy.evaluate(&facts(), &request(vec![expired]), &ledger, NOW_MS, 0)),
        DenyReason::Delegation(ChainFault::Expired)
    );
}

#[test]
fn revoking_a_parent_cascades_to_its_child() {
    let policy = policy_with(member_only_rule());
    let chain = two_step_chain();
    let mut ledger = RevocationLedger::new();

    // The chain is good until the root withdraws the intermediate's grant.
    assert!(
        policy
            .evaluate(&facts(), &request(chain.clone()), &ledger, NOW_MS, 0)
            .is_accept()
    );
    let statement = SignedDelegationRevocation::issue(
        &root(),
        DelegationRevocation::new(
            chain[0].certificate.id(),
            root().master_public_key().to_bytes(),
            scope("/services"),
            20,
            [5; 32],
        ),
    )
    .expect("issue revocation");
    assert!(ledger.fold(&statement));
    assert_eq!(
        denial(policy.evaluate(&facts(), &request(chain), &ledger, NOW_MS, 0)),
        DenyReason::Delegation(ChainFault::Revoked)
    );
}

#[test]
fn a_widened_child_scope_is_refused() {
    let policy = policy_with(member_only_rule());
    let ledger = RevocationLedger::new();
    let mid = intermediate();
    let parent = grant(
        &root(),
        DelegationParent::Root(ROOT_AUTHORITY),
        mid.master_public_key().to_bytes(),
        "/services",
        Some(100),
        2,
        6,
    );
    let widened = grant(
        &mid,
        DelegationParent::Certificate(parent.certificate.id()),
        member().master_public_key().to_bytes(),
        "/",
        Some(90),
        1,
        7,
    );
    assert_eq!(
        denial(policy.evaluate(
            &facts(),
            &request(vec![parent, widened]),
            &ledger,
            NOW_MS,
            0
        )),
        DenyReason::Delegation(ChainFault::NotAttenuated)
    );
}

#[test]
fn excessive_delegation_depth_is_refused() {
    let mut policy = policy_with(member_only_rule());
    policy.limits = HandshakeLimits {
        max_delegation_depth: 1,
        ..HandshakeLimits::default()
    };
    let ledger = RevocationLedger::new();
    assert_eq!(
        denial(policy.evaluate(&facts(), &request(two_step_chain()), &ledger, NOW_MS, 0)),
        DenyReason::Delegation(ChainFault::DepthExceeded)
    );
}

#[test]
fn an_incompatible_profile_is_refused_before_authority_is_read() {
    let policy = policy_with(member_only_rule());
    let ledger = RevocationLedger::new();
    let mut exotic = request(vec![member_grant()]);
    exotic.profile = ProfileRef {
        id: "exotic.mesh".into(),
        revision: 9,
    };
    assert_eq!(
        denial(policy.evaluate(&facts(), &exotic, &ledger, NOW_MS, 0)),
        DenyReason::ProfileNotAccepted
    );
}

#[test]
fn capacity_refuses_after_otherwise_valid_authority() {
    let policy = policy_with(ServiceRule {
        max_sessions: Some(1),
        ..member_only_rule()
    });
    let ledger = RevocationLedger::new();
    let admitted = request(vec![member_grant()]);
    assert!(
        policy
            .evaluate(&facts(), &admitted, &ledger, NOW_MS, 0)
            .is_accept()
    );
    assert_eq!(
        denial(policy.evaluate(&facts(), &admitted, &ledger, NOW_MS, 1)),
        DenyReason::CapacityExhausted,
        "the same valid authority is refused only for capacity"
    );
}

#[test]
fn a_subject_that_is_not_the_authenticated_peer_is_refused() {
    // D6, at the policy layer: holding valid authority does not let you use it
    // over a connection the transport proved belongs to someone else.
    let policy = policy_with(member_only_rule());
    let ledger = RevocationLedger::new();
    // The carrier proved somebody else; the claim cannot talk its way past it.
    let stranger_carrier =
        SessionFacts::authenticated(b"mere/murm/v1".to_vec(), CarrierKind::Memory, [0xcd; 32]);
    assert_eq!(
        denial(policy.evaluate(
            &stranger_carrier,
            &request(vec![member_grant()]),
            &ledger,
            NOW_MS,
            0
        )),
        DenyReason::SubjectNotTransportPeer
    );
}

#[test]
fn decisions_are_deterministic_over_identical_inputs() {
    let policy = policy_with(member_only_rule());
    let ledger = RevocationLedger::new();
    let admitted = request(vec![member_grant()]);
    let first = policy.evaluate(&facts(), &admitted, &ledger, NOW_MS, 0);
    for _ in 0..8 {
        assert_eq!(
            first,
            policy.evaluate(&facts(), &admitted, &ledger, NOW_MS, 0)
        );
    }
}
