use std::time::{SystemTime, UNIX_EPOCH};

use notochord::{
    AdmittedPrincipal, AdmittedSession, CarrierKind, HandshakeLimits, NetworkId, ProfileRef,
    ProofBinding, RequestedAction, RevocationLedger, SessionClaims, SessionFacts, SessionHello,
    TrafficClass, TrustedRoot,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
    SignedDelegationCertificate, SignedDelegationRevocation,
};
use personae::{IdentityProvider, InMemoryProvider, PersonaId, SealedRecordStorage};
use tempfile::{TempDir, tempdir};

use super::*;
use crate::otp::{OtpItemStore, OtpReleaseParticipantProof};

const ROOT_AUTHORITY: [u8; 32] = [0x31; 32];
const NETWORK: [u8; 32] = [0x47; 32];
const RFC4226_SECRET_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

fn owner() -> InMemoryProvider {
    InMemoryProvider::from_seed([0x11; 32])
}

fn participant() -> InMemoryProvider {
    InMemoryProvider::from_seed([0x22; 32])
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn grant(path: String, expires_at_ms: Option<u64>) -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        &owner(),
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            owner().master_public_key().to_bytes(),
            participant().master_public_key().to_bytes(),
            CapabilityScope {
                domain: OTP_RELEASE_DOMAIN.into(),
                resource: NETWORK.to_vec(),
                path_prefix: path,
                actions: [OTP_RELEASE_ACTION.to_string()].into_iter().collect(),
            },
            0,
            0,
            expires_at_ms,
            0,
            [0x51; 32],
        ),
    )
    .unwrap()
}

fn admitted(
    persona: PersonaId,
    item: OtpItemId,
    session_id: [u8; 32],
    certificate: SignedDelegationCertificate,
) -> AdmittedSession<Vec<u8>> {
    let action = RequestedAction {
        domain: OTP_RELEASE_DOMAIN.into(),
        path: otp_item_path(persona, item),
        action: OTP_RELEASE_ACTION.into(),
    };
    let subject = participant().master_public_key().to_bytes();
    AdmittedSession {
        stream: Vec::<u8>::new(),
        principal: AdmittedPrincipal {
            subject,
            class: TrafficClass::Interactive,
            session_id,
            action: action.clone(),
        },
        claims: SessionClaims {
            wire_version: 1,
            network: NetworkId(NETWORK),
            profile: ProfileRef {
                id: "mere.base".into(),
                revision: 1,
            },
            action,
            class: TrafficClass::Interactive,
            subject,
            delegations: vec![certificate],
        },
        facts: SessionFacts::authenticated(
            b"mere/castellan-otp-test/v1".to_vec(),
            CarrierKind::Memory,
            subject,
        ),
        limits: HandshakeLimits::default(),
    }
}

struct Fixture {
    _dir: TempDir,
    gate: OtpReleaseGate,
    persona: PersonaId,
    item: OtpItemId,
}

fn fixture() -> Fixture {
    let dir = tempdir().unwrap();
    let persona = PersonaId::new();
    let items = OtpItemStore::new(
        SealedRecordStorage::open_with_key(dir.path(), [0x62; 32]),
        persona,
    );
    let item = items
        .import_otpauth_uri(&format!(
            "otpauth://totp/Merely:mark?secret={RFC4226_SECRET_BASE32}&issuer=Merely&digits=8"
        ))
        .unwrap();
    Fixture {
        _dir: dir,
        gate: OtpReleaseGate::new(items),
        persona,
        item: item.id,
    }
}

fn open(
    fixture: &Fixture,
    session_id: [u8; 32],
    certificate: SignedDelegationCertificate,
    ledger: &RevocationLedger,
    at_ms: u64,
) -> OtpAdmittedSession<Vec<u8>> {
    OtpAdmittedSession::from_admitted(
        admitted(fixture.persona, fixture.item, session_id, certificate),
        fixture.gate.clone(),
        ledger,
        at_ms,
    )
    .unwrap()
}

fn revoke(ledger: &mut RevocationLedger, certificate: &SignedDelegationCertificate, at_ms: u64) {
    let statement = SignedDelegationRevocation::issue(
        &owner(),
        DelegationRevocation::new(
            certificate.certificate.id(),
            owner().master_public_key().to_bytes(),
            certificate.certificate.scope.clone(),
            at_ms,
            [0x71; 32],
        ),
    )
    .unwrap();
    assert!(ledger.fold(&statement));
}

#[test]
fn policy_requires_member_authority_and_transport_identity() {
    let policy = otp_release_policy(
        NetworkId(NETWORK),
        Vec::new(),
        vec![ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        }],
        Some(3),
    );
    let rule = policy.services.get(OTP_RELEASE_SERVICE).unwrap();

    assert_eq!(rule.access, notochord::ServiceAccess::MemberOnly);
    assert!(rule.require_transport_identity);
    assert_eq!(rule.domain, OTP_RELEASE_DOMAIN);
    assert_eq!(rule.actions, [OTP_RELEASE_ACTION.to_string()].into());
    assert_eq!(rule.max_sessions, Some(3));
}

#[test]
fn a_signed_notochord_admission_flows_into_the_release_adapter() {
    let fixture = fixture();
    let at_ms = now_ms();
    let certificate = grant(otp_item_path(fixture.persona, fixture.item), None);
    let subject = participant().master_public_key().to_bytes();
    let protocol = b"mere/castellan-otp-test/v1";
    let facts = SessionFacts::authenticated(protocol.to_vec(), CarrierKind::Memory, subject);
    let binding = ProofBinding::initiator(protocol.to_vec(), Some(subject), None);
    let action = RequestedAction {
        domain: OTP_RELEASE_DOMAIN.into(),
        path: otp_item_path(fixture.persona, fixture.item),
        action: OTP_RELEASE_ACTION.into(),
    };
    let profile = ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    };
    let hello = SessionHello::issue(
        &participant(),
        NetworkId(NETWORK),
        profile.clone(),
        action,
        TrafficClass::Interactive,
        [0x91; 32],
        &binding,
        vec![certificate],
    )
    .unwrap();
    let policy = otp_release_policy(
        NetworkId(NETWORK),
        vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: owner().master_public_key().to_bytes(),
        }],
        vec![profile],
        Some(3),
    );
    let ledger = RevocationLedger::default();
    let (_, principal) = notochord::admit(
        &policy,
        &ledger,
        &hello.encode(&policy.limits).unwrap(),
        &facts,
        at_ms,
        0,
    );
    let admitted = AdmittedSession {
        stream: Vec::<u8>::new(),
        principal: principal.unwrap(),
        claims: hello.claims(),
        facts,
        limits: policy.limits,
    };

    let mut session =
        OtpAdmittedSession::from_admitted(admitted, fixture.gate.clone(), &ledger, at_ms).unwrap();
    let request = session.petition(&ledger, at_ms).unwrap();

    assert_eq!(
        request.participant.proof(),
        OtpReleaseParticipantProof::AdmittedSession
    );
    assert_eq!(session.persona(), fixture.persona);
    assert_eq!(session.item(), fixture.item);
}

#[test]
fn petition_identity_comes_from_the_admitted_transcript() {
    let fixture = fixture();
    let at_ms = now_ms();
    let certificate = grant(otp_item_path(fixture.persona, fixture.item), None);
    let mut session = open(
        &fixture,
        [0x81; 32],
        certificate,
        &RevocationLedger::default(),
        at_ms,
    );

    let request = session
        .petition(&RevocationLedger::default(), at_ms)
        .unwrap();

    assert!(matches!(
        fixture.gate.approve(request.id),
        Err(OtpReleaseError::SessionBoundApprovalRequired(id)) if id == request.id
    ));
    assert_eq!(
        request.participant.proof(),
        OtpReleaseParticipantProof::AdmittedSession
    );
    assert_eq!(
        request.participant.principal(),
        format!(
            "personae:ed25519:{}",
            hex(&participant().master_public_key().to_bytes())
        )
    );
    assert!(!format!("{request:?}").contains(&hex(&[0x81; 32])));
}

#[test]
fn approval_is_opaque_until_the_same_carrier_begins_delivery() {
    let fixture = fixture();
    let at_ms = now_ms();
    let ledger = RevocationLedger::default();
    let mut session = open(
        &fixture,
        [0x82; 32],
        grant(otp_item_path(fixture.persona, fixture.item), None),
        &ledger,
        at_ms,
    );
    let request = session.petition(&ledger, at_ms).unwrap();
    let approved = session.approve(request.id, &ledger, at_ms).unwrap();
    assert!(!format!("{approved:?}").contains("94287082"));

    let mut delivery = session.begin_delivery(approved, &ledger, at_ms).unwrap();
    assert_eq!(delivery.request(), &request);
    let (carrier, tile) = delivery.parts();
    let code = tile.code_at_unix_time(at_ms / 1_000).unwrap();
    carrier.extend_from_slice(code.as_bytes());

    assert_eq!(carrier.len(), 8);
    assert!(carrier.iter().all(u8::is_ascii_digit));
}

#[test]
fn another_session_cannot_take_an_approved_value() {
    let fixture = fixture();
    let at_ms = now_ms();
    let ledger = RevocationLedger::default();
    let mut left = open(
        &fixture,
        [0x83; 32],
        grant(otp_item_path(fixture.persona, fixture.item), None),
        &ledger,
        at_ms,
    );
    let mut right = open(
        &fixture,
        [0x84; 32],
        grant(otp_item_path(fixture.persona, fixture.item), None),
        &ledger,
        at_ms,
    );
    let request = left.petition(&ledger, at_ms).unwrap();
    let approved = left.approve(request.id, &ledger, at_ms).unwrap();

    assert!(matches!(
        right.begin_delivery(approved, &ledger, at_ms),
        Err(OtpAdmittedReleaseError::ForeignApproval)
    ));
}

#[test]
fn revocation_is_rechecked_before_approval_and_delivery() {
    let fixture = fixture();
    let at_ms = now_ms();
    let mut ledger = RevocationLedger::default();
    let certificate = grant(otp_item_path(fixture.persona, fixture.item), None);
    let mut session = open(&fixture, [0x85; 32], certificate.clone(), &ledger, at_ms);
    let request = session.petition(&ledger, at_ms).unwrap();
    revoke(&mut ledger, &certificate, at_ms);

    assert!(matches!(
        session.approve(request.id, &ledger, at_ms),
        Err(OtpAdmittedReleaseError::AuthorityLapsed(
            AuthorityLapse::Revoked
        ))
    ));

    let mut ledger = RevocationLedger::default();
    let certificate = grant(otp_item_path(fixture.persona, fixture.item), None);
    let mut session = open(&fixture, [0x86; 32], certificate.clone(), &ledger, at_ms);
    let request = session.petition(&ledger, at_ms).unwrap();
    let approved = session.approve(request.id, &ledger, at_ms).unwrap();
    revoke(&mut ledger, &certificate, at_ms);

    assert!(matches!(
        session.begin_delivery(approved, &ledger, at_ms),
        Err(OtpAdmittedReleaseError::AuthorityLapsed(
            AuthorityLapse::Revoked
        ))
    ));
}

#[test]
fn expiry_and_wrong_scope_are_refused_at_the_adapter_boundary() {
    let fixture = fixture();
    let at_ms = now_ms();
    let expired = grant(otp_item_path(fixture.persona, fixture.item), Some(at_ms));
    let result = OtpAdmittedSession::from_admitted(
        admitted(fixture.persona, fixture.item, [0x87; 32], expired),
        fixture.gate.clone(),
        &RevocationLedger::default(),
        at_ms + 1,
    );
    assert!(matches!(
        result,
        Err(OtpAdmittedReleaseError::AuthorityLapsed(
            AuthorityLapse::Expired { at_ms: deadline }
        )) if deadline == at_ms
    ));

    let neighbour = OtpItemId::from_uuid(uuid::Uuid::new_v4());
    let wrong_scope = grant(otp_item_path(fixture.persona, neighbour), None);
    let result = OtpAdmittedSession::from_admitted(
        admitted(fixture.persona, fixture.item, [0x88; 32], wrong_scope),
        fixture.gate.clone(),
        &RevocationLedger::default(),
        at_ms,
    );
    assert!(matches!(
        result,
        Err(OtpAdmittedReleaseError::ActionNotCovered)
    ));

    let other_persona = PersonaId::new();
    let wrong_persona = grant(otp_item_path(other_persona, fixture.item), None);
    let result = OtpAdmittedSession::from_admitted(
        admitted(other_persona, fixture.item, [0x8a; 32], wrong_persona),
        fixture.gate.clone(),
        &RevocationLedger::default(),
        at_ms,
    );
    assert!(matches!(
        result,
        Err(OtpAdmittedReleaseError::ActionNotServed)
    ));

    let alias = format!(
        "{OTP_RELEASE_SERVICE}/{}/{item}",
        fixture.persona.as_uuid().to_string().to_uppercase(),
        item = fixture.item
    );
    let mut aliased = admitted(
        fixture.persona,
        fixture.item,
        [0x8b; 32],
        grant(alias.clone(), None),
    );
    aliased.principal.action.path = alias.clone();
    aliased.claims.action.path = alias;
    let result = OtpAdmittedSession::from_admitted(
        aliased,
        fixture.gate.clone(),
        &RevocationLedger::default(),
        at_ms,
    );
    assert!(matches!(
        result,
        Err(OtpAdmittedReleaseError::ActionNotServed)
    ));
}

#[test]
fn dropping_a_session_cancels_its_unresolved_petitions() {
    let fixture = fixture();
    let at_ms = now_ms();
    let ledger = RevocationLedger::default();
    let mut session = open(
        &fixture,
        [0x89; 32],
        grant(otp_item_path(fixture.persona, fixture.item), None),
        &ledger,
        at_ms,
    );
    session.petition(&ledger, at_ms).unwrap();
    assert_eq!(fixture.gate.pending().unwrap().len(), 1);

    drop(session);

    assert!(fixture.gate.pending().unwrap().is_empty());
}
