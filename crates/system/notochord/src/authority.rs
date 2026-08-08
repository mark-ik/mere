//! Retained authority for an already-admitted session.
//!
//! Admission proves a session hello once. A service that outlives that
//! handshake must retain the verified delegation chain so it can observe a
//! later revocation or expiry without decoding application bytes again.

use personae::delegation::SignedDelegationCertificate;

use crate::{AdmittedPrincipal, AdmittedSession, RevocationLedger};

/// Why an admitted authority no longer holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityLapse {
    /// The chain expired. The deadline, rather than the observation time, is
    /// retained so logs say when authority ended.
    Expired { at_ms: u64 },
    /// A certificate in the chain was revoked by its issuer.
    Revoked,
}

/// The local, verified authority retained after Notochord admission.
///
/// This type is deliberately non-serializable. It is a conclusion about one
/// authenticated connection, not a credential another process may present.
#[derive(Clone, Debug)]
pub struct RetainedAuthority {
    principal: AdmittedPrincipal,
    chain: Vec<SignedDelegationCertificate>,
}

impl RetainedAuthority {
    /// Retain the verified chain carried by an admitted carrier session.
    pub fn from_admitted<S>(session: &AdmittedSession<S>) -> Self {
        Self::new(
            session.principal.clone(),
            session.claims.delegations.clone(),
        )
    }

    /// Retain an already-verified admission conclusion and delegation chain.
    ///
    /// This supports sans-I/O callers that own the verified hello themselves.
    /// Carrier paths should prefer [`Self::from_admitted`].
    pub fn new(principal: AdmittedPrincipal, chain: Vec<SignedDelegationCertificate>) -> Self {
        Self { principal, chain }
    }

    /// The principal admitted for this connection.
    pub fn principal(&self) -> &AdmittedPrincipal {
        &self.principal
    }

    /// The earliest chain expiry, if the chain is time-bounded.
    pub fn deadline_ms(&self) -> Option<u64> {
        self.chain
            .iter()
            .filter_map(|signed| signed.certificate.expires_at_ms)
            .min()
    }

    /// Whether authority has lapsed under the current revocation ledger.
    ///
    /// Revocation outranks expiry when both apply, because the owner's active
    /// withdrawal is the useful and security-relevant conclusion.
    pub fn lapse(&self, ledger: &RevocationLedger, now_ms: u64) -> Option<AuthorityLapse> {
        if self
            .chain
            .iter()
            .any(|signed| ledger.revokes(&signed.certificate))
        {
            return Some(AuthorityLapse::Revoked);
        }
        match self.deadline_ms() {
            Some(deadline) if now_ms > deadline => {
                Some(AuthorityLapse::Expired { at_ms: deadline })
            }
            _ => None,
        }
    }

    /// Whether the leaf grant covers one application-owned path and action.
    ///
    /// Callers must check [`Self::lapse`] against their current revocation
    /// ledger before disclosing anything. Expiry is also included here through
    /// the Personae certificate scope rule.
    pub fn covers(&self, path: &str, action: &str, at_ms: u64) -> bool {
        self.chain
            .last()
            .is_some_and(|leaf| leaf.certificate.covers(path, action, at_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NetworkId, ProfileRef, RequestedAction, SessionClaims, TrafficClass};
    use personae::IdentityProvider;
    use personae::InMemoryProvider;
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
        SignedDelegationCertificate, SignedDelegationRevocation,
    };

    const ROOT_AUTHORITY: [u8; 32] = [7; 32];
    const NETWORK: [u8; 32] = [3; 32];
    const NOW_MS: u64 = 50;
    const EXPIRY_MS: u64 = 100;

    fn owner() -> InMemoryProvider {
        InMemoryProvider::from_seed([1; 32])
    }

    fn viewer() -> InMemoryProvider {
        InMemoryProvider::from_seed([4; 32])
    }

    fn grant(path_prefix: &str) -> SignedDelegationCertificate {
        SignedDelegationCertificate::issue(
            &owner(),
            DelegationCertificate::new(
                DelegationParent::Root(ROOT_AUTHORITY),
                owner().master_public_key().to_bytes(),
                viewer().master_public_key().to_bytes(),
                CapabilityScope {
                    domain: "mere.knot".into(),
                    resource: NETWORK.to_vec(),
                    path_prefix: path_prefix.into(),
                    actions: ["read".to_string()].into_iter().collect(),
                },
                5,
                10,
                Some(EXPIRY_MS),
                1,
                [1; 32],
            ),
        )
        .expect("issue certificate")
    }

    fn principal() -> AdmittedPrincipal {
        AdmittedPrincipal {
            subject: viewer().master_public_key().to_bytes(),
            class: TrafficClass::Interactive,
            session_id: [21; 32],
            action: RequestedAction {
                domain: "mere.knot".into(),
                path: "/services/knot-publish".into(),
                action: "read".into(),
            },
        }
    }

    fn retained(path: &str) -> RetainedAuthority {
        RetainedAuthority::new(principal(), vec![grant(path)])
    }

    fn revoked_ledger(certificate: &SignedDelegationCertificate) -> RevocationLedger {
        let statement = SignedDelegationRevocation::issue(
            &owner(),
            DelegationRevocation::new(
                certificate.certificate.id(),
                owner().master_public_key().to_bytes(),
                certificate.certificate.scope.clone(),
                NOW_MS,
                [2; 32],
            ),
        )
        .expect("issue revocation");
        let mut ledger = RevocationLedger::new();
        assert!(ledger.fold(&statement));
        ledger
    }

    #[test]
    fn retained_authority_rechecks_expiry_and_revocation() {
        let certificate = grant("/publications/a");
        let authority = RetainedAuthority::new(principal(), vec![certificate.clone()]);

        assert_eq!(authority.deadline_ms(), Some(EXPIRY_MS));
        assert_eq!(
            authority.lapse(&RevocationLedger::default(), EXPIRY_MS),
            None
        );
        assert_eq!(
            authority.lapse(&RevocationLedger::default(), EXPIRY_MS + 1),
            Some(AuthorityLapse::Expired { at_ms: EXPIRY_MS })
        );
        assert_eq!(
            authority.lapse(&revoked_ledger(&certificate), EXPIRY_MS + 1),
            Some(AuthorityLapse::Revoked),
            "revocation must outrank a coincident expiry"
        );
    }

    #[test]
    fn a_one_path_grant_does_not_cover_a_neighbour() {
        let authority = retained("/publications/a");
        assert!(authority.covers("/publications/a", "read", NOW_MS));
        assert!(!authority.covers("/publications/a-private", "read", NOW_MS));
        assert!(!authority.covers("/publications/b", "read", NOW_MS));
    }

    #[test]
    fn retained_authority_can_be_taken_from_an_admitted_session() {
        let certificate = grant("/publications/a");
        let admitted = AdmittedSession {
            stream: (),
            principal: principal(),
            claims: SessionClaims {
                wire_version: 1,
                network: NetworkId(NETWORK),
                profile: ProfileRef {
                    id: "mere.base".into(),
                    revision: 1,
                },
                action: principal().action,
                class: TrafficClass::Interactive,
                subject: viewer().master_public_key().to_bytes(),
                delegations: vec![certificate.clone()],
            },
            facts: crate::SessionFacts::new(b"mere/knot-publish/v1", crate::CarrierKind::Memory),
            limits: crate::HandshakeLimits::default(),
        };

        let authority = RetainedAuthority::from_admitted(&admitted);
        assert_eq!(authority.principal(), &admitted.principal);
        assert_eq!(
            authority.lapse(&revoked_ledger(&certificate), NOW_MS),
            Some(AuthorityLapse::Revoked)
        );
    }
}
