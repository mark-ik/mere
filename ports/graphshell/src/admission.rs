//! Graphshell's session admission: `network-policy` under the Graphshell
//! action triple.
//!
//! G5c. The carrier's handshake is the **sole** admission step (G5a.1): it
//! binds a Personae subject, its attested signer, its delegation chain, the
//! requested action, and a nonce to *this* connection, and the application
//! stream is withheld until that verifies. This module supplies the two things
//! `network-policy` cannot know — what Graphshell's service is called, and how
//! a Graphshell host asks — and returns the conclusion as an
//! [`AdmittedPrincipal`].
//!
//! Nothing here travels in `graphshell-protocol`. Admission happens *below*
//! the application protocol, which is why `SessionOpen` negotiates version and
//! capabilities only and carries no principal of its own.
//!
//! ## The triple
//!
//! `mere.graphshell` / `/services/projection` / `connect`. The action
//! vocabulary in `network-policy` is deliberately open
//! (`RequestedAction { domain, path, action }`), so adding Graphshell is a new
//! triple rather than a change to that crate — and a delegation minted for
//! Murm's service does not admit a Graphshell session, because the scope's
//! `domain` and `path_prefix` are part of what the chain has to cover.

use network_policy::{
    AdmittedPrincipal, CarrierKind, DenyReason, HandshakeError, LocalNetworkPolicy, NetworkId,
    ProfileRef, ProofBinding, RequestedAction, RevocationLedger, SessionFacts, SessionHello,
    TrafficClass, admit,
};
use personae::IdentityProvider;
use personae::delegation::SignedDelegationCertificate;

/// The application domain owning Graphshell's action vocabulary.
pub const GRAPHSHELL_DOMAIN: &str = "mere.graphshell";

/// The service path a projection session runs under.
pub const PROJECTION_SERVICE: &str = "/services/projection";

/// Opening a projection session.
pub const CONNECT_ACTION: &str = "connect";

/// The ALPN-ish protocol label a Graphshell session is accepted for. It is part
/// of the signed transcript, so a proof minted for another protocol on the same
/// connection does not verify here.
pub const PROJECTION_PROTOCOL: &[u8] = b"mere/graphshell/v1";

/// Graphshell's action triple.
pub fn connect_action() -> RequestedAction {
    RequestedAction {
        domain: GRAPHSHELL_DOMAIN.to_string(),
        path: PROJECTION_SERVICE.to_string(),
        action: CONNECT_ACTION.to_string(),
    }
}

/// Build and sign the client's hello for `binding`.
///
/// Returns the [`SessionHello`] rather than encoded bytes, so it composes with
/// `network_policy::initiate_session` (which writes the frame and reads the
/// reply) instead of standing up a second encode path beside it.
///
/// `binding` must describe the connection this hello will actually travel on;
/// the responder rebuilds it from what *it* observed, so a captured hello is
/// useless on a different link.
pub fn open_session<P: IdentityProvider>(
    identity: &P,
    network: NetworkId,
    profile: ProfileRef,
    class: TrafficClass,
    nonce: [u8; 32],
    binding: &ProofBinding,
    delegations: Vec<SignedDelegationCertificate>,
) -> Result<SessionHello, HandshakeError> {
    SessionHello::issue(
        identity,
        network,
        profile,
        connect_action(),
        class,
        nonce,
        binding,
        delegations,
    )
}

/// Admit (or refuse) an incoming Graphshell session.
///
/// Returns the reply frame to write in both cases — a refusal is still a
/// well-formed reply — and the principal when the session is admitted.
///
/// The action is checked, not assumed: a chain that admits some *other* service
/// is refused here, so a peer cannot reach projections with a grant minted for
/// something else.
pub fn admit_session(
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    hello_bytes: &[u8],
    facts: &SessionFacts,
    now_ms: u64,
    active_sessions: u32,
) -> (Vec<u8>, Result<AdmittedPrincipal, DenyReason>) {
    let (reply, outcome) = admit(policy, ledger, hello_bytes, facts, now_ms, active_sessions);
    let outcome = outcome.and_then(|principal| {
        if principal.action == connect_action() {
            Ok(principal)
        } else {
            // Defence in depth: the policy already evaluates the requested
            // action, so reaching here means a hello was admitted for a
            // different service on a Graphshell listener. Refuse rather than
            // serve projections on someone else's grant.
            //
            // `ActionNotOffered`, not `ActionNotCovered`: the authority did
            // not fall short — it was fine for what it asked. This listener
            // simply does not serve that action, which is a different fact and
            // deserves a different word to whoever reads the refusal.
            Err(DenyReason::ActionNotOffered)
        }
    });
    (reply, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use network_policy::{ServiceAccess, ServiceRule, TrustedRoot};
    use personae::InMemoryProvider;
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
    };
    use std::collections::BTreeMap;

    const NETWORK: NetworkId = NetworkId([3; 32]);
    const ROOT_AUTHORITY: [u8; 32] = [7; 32];
    const NOW_MS: u64 = 50;

    fn owner() -> InMemoryProvider {
        InMemoryProvider::from_seed([1; 32])
    }

    fn viewer() -> InMemoryProvider {
        InMemoryProvider::from_seed([4; 32])
    }

    fn stranger() -> InMemoryProvider {
        InMemoryProvider::from_seed([11; 32])
    }

    fn profile_ref() -> ProfileRef {
        ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        }
    }

    /// A grant from the owner letting `subject` open projection sessions.
    fn projection_grant(
        subject: [u8; 32],
        path: &str,
        domain: &str,
    ) -> SignedDelegationCertificate {
        SignedDelegationCertificate::issue(
            &owner(),
            DelegationCertificate::new(
                DelegationParent::Root(ROOT_AUTHORITY),
                owner().master_public_key().to_bytes(),
                subject,
                CapabilityScope {
                    domain: domain.into(),
                    resource: NETWORK.0.to_vec(),
                    path_prefix: path.into(),
                    actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
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

    fn policy() -> LocalNetworkPolicy {
        let mut policy = LocalNetworkPolicy::closed(NETWORK);
        policy.accepted_profiles = vec![profile_ref()];
        policy.trusted_roots = vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: owner().master_public_key().to_bytes(),
        }];
        policy.services = BTreeMap::from([(
            PROJECTION_SERVICE.to_string(),
            ServiceRule {
                access: ServiceAccess::MemberOnly,
                require_transport_identity: false,
                max_sessions: None,
            },
        )]);
        policy
    }

    /// What the initiator signs against: an unauthenticated carrier here, so
    /// only the protocol is bound.
    fn binding() -> ProofBinding {
        ProofBinding::initiator(PROJECTION_PROTOCOL, None, None)
    }

    /// What the responder observed for that same connection.
    fn facts() -> SessionFacts {
        SessionFacts::new(PROJECTION_PROTOCOL, CarrierKind::P2panda)
    }

    fn hello_from<P: IdentityProvider>(
        identity: &P,
        binding: &ProofBinding,
        delegations: Vec<SignedDelegationCertificate>,
    ) -> Vec<u8> {
        open_session(
            identity,
            NETWORK,
            profile_ref(),
            TrafficClass::Interactive,
            [5; 32],
            binding,
            delegations,
        )
        .expect("issue hello")
        .encode(&LocalNetworkPolicy::closed(NETWORK).limits.clamped())
        .expect("encode hello")
    }

    #[test]
    fn a_granted_viewer_is_admitted_and_named() {
        let viewer = viewer();
        let subject = viewer.master_public_key().to_bytes();
        let hello = hello_from(
            &viewer,
            &binding(),
            vec![projection_grant(
                subject,
                PROJECTION_SERVICE,
                GRAPHSHELL_DOMAIN,
            )],
        );

        let (_, outcome) = admit_session(
            &policy(),
            &RevocationLedger::default(),
            &hello,
            &facts(),
            NOW_MS,
            0,
        );
        let principal = outcome.expect("a granted viewer opens a projection session");
        assert_eq!(
            principal.subject, subject,
            "the admitted principal is the peer, established by the handshake"
        );
        assert_eq!(principal.action, connect_action());
    }

    #[test]
    fn an_ungranted_stranger_is_refused() {
        let hello = hello_from(&stranger(), &binding(), Vec::new());
        let (_, outcome) = admit_session(
            &policy(),
            &RevocationLedger::default(),
            &hello,
            &facts(),
            NOW_MS,
            0,
        );
        assert!(
            outcome.is_err(),
            "no chain, no session: MemberOnly means a delegation is required"
        );
    }

    #[test]
    fn a_grant_for_another_service_does_not_open_projections() {
        // The whole point of a triple: authority is per-service, so a peer
        // admitted to Murm cannot reach Graphshell's projections with it.
        let viewer = viewer();
        let subject = viewer.master_public_key().to_bytes();
        let hello = hello_from(
            &viewer,
            &binding(),
            vec![projection_grant(subject, "/services/murm", "mere.network")],
        );
        let (_, outcome) = admit_session(
            &policy(),
            &RevocationLedger::default(),
            &hello,
            &facts(),
            NOW_MS,
            0,
        );
        assert!(
            outcome.is_err(),
            "a Murm grant is not a Graphshell grant, even for the same subject"
        );
    }

    #[test]
    fn a_captured_hello_does_not_open_a_different_connection() {
        // The transcript binds the connection, so replaying a valid hello onto
        // another one fails: this is what makes admission per-connection rather
        // than per-credential.
        let viewer = viewer();
        let subject = viewer.master_public_key().to_bytes();
        let hello = hello_from(
            &viewer,
            &binding(),
            vec![projection_grant(
                subject,
                PROJECTION_SERVICE,
                GRAPHSHELL_DOMAIN,
            )],
        );

        let elsewhere =
            SessionFacts::authenticated(PROJECTION_PROTOCOL, CarrierKind::P2panda, [42; 32]);
        let (_, outcome) = admit_session(
            &policy(),
            &RevocationLedger::default(),
            &hello,
            &elsewhere,
            NOW_MS,
            0,
        );
        assert!(
            outcome.is_err(),
            "a proof minted for one connection is worthless on another"
        );
    }
}
