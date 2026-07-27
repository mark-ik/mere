//! G5d: Graphshell's projection carrier.
//!
//! The accept path that runs *before* a single `SessionOpen` byte is read. It
//! is the second of Notochord N2's two service carriers (the first is Murm's
//! real accept path), and it deliberately owns none of the machinery it uses:
//!
//! - the carrier facts come from [`transport::AcceptedSession::into_session`],
//!   the one audited adapter (N1), so this port never hand-builds a
//!   [`SessionFacts`](network_policy::SessionFacts);
//! - the framing and the admitted conclusion come from
//!   [`network_policy::admit_session`], so a refusal is finished rather than
//!   flushed and a refused stream cannot reach an application;
//! - the delegation grammar stays in Personae, and the owner's rules stay in
//!   the policy the host supplies.
//!
//! What is Graphshell's, and lives here: the ALPN, the service path the owner
//! offers, and which admitted actions this service serves.
//!
//! ## The transport must outlive the session
//!
//! [`accept_projection_session`] borrows its transport and never takes
//! ownership. That is load-bearing rather than stylistic. Both carriers drain
//! buffered writes when a *stream* is dropped, but by two unrelated
//! mechanisms sharing one escape hatch:
//!
//! - p2panda: quinn's `Drop for SendStream` finishes the stream, except when
//!   the connection is already errored, which is exactly what dropping the
//!   endpoint underneath it produces;
//! - Reticulum: the outbound relay reads its duplex to EOF, unless the
//!   endpoint has been torn down and the relay task aborted.
//!
//! So a short-lived accept task owning its transport would discard the reply
//! it had already written, on either arm, whenever it returned promptly.
//! Taking `&T` makes that unspellable here.
//!
//! ## Why the action check is not a second admission
//!
//! Admission answers "who is this, and what did they ask for". It cannot
//! answer "and is that something projections serve", because
//! `LocalNetworkPolicy` has no action vocabulary to answer it with; see
//! [`crate::admission::serves_action`]. Splitting it this way is the ownership
//! the Notochord plan sets out: services authorize operations after admission
//! under their own action vocabulary.

use network_policy::{
    AdmittedSession, DenyReason, IoHandshakeError, LocalNetworkPolicy, NetworkId, ProfileRef,
    RevocationLedger, ServiceAccess, ServiceRule, TrustedRoot, admit_session,
};
use tokio::io::AsyncWriteExt;
use transport::{Alpn, Transport, TransportError};

use crate::admission::{PROJECTION_PROTOCOL, PROJECTION_SERVICE, serves_action};

/// The ALPN a projection session is accepted for.
///
/// The same bytes are the protocol in the signed transcript, so a proof minted
/// for another protocol on the same connection does not verify here.
pub fn projection_alpn() -> Alpn {
    Alpn::from_bytes(PROJECTION_PROTOCOL)
}

/// A policy offering exactly one service: Graphshell projections.
///
/// A convenience for a host that serves projections and nothing else; a host
/// with more services builds its own and inserts the same rule. The value is
/// that the service path and its default posture are stated once.
///
/// `MemberOnly` is the default on purpose. A projection session hands out a
/// live view of a graph, so it wants a delegation chain rather than an open
/// door.
pub fn projection_policy(
    network: NetworkId,
    trusted_roots: Vec<TrustedRoot>,
    accepted_profiles: Vec<ProfileRef>,
    max_sessions: Option<u32>,
) -> LocalNetworkPolicy {
    let mut policy = LocalNetworkPolicy::closed(network);
    policy.trusted_roots = trusted_roots;
    policy.accepted_profiles = accepted_profiles;
    policy.services.insert(
        PROJECTION_SERVICE.to_string(),
        ServiceRule {
            access: ServiceAccess::MemberOnly,
            require_transport_identity: false,
            max_sessions,
        },
    );
    policy
}

/// Why a projection session was not served.
///
/// Split because the two mean different things to whoever reads the log: one
/// is the owner's policy or the peer's authority falling short, the other is a
/// peer who was genuinely admitted asking this service for something it does
/// not do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionRefusal {
    /// Admission refused the session, and the peer has been told why.
    NotAdmitted(DenyReason),
    /// Admitted, but for an action projections do not serve.
    ///
    /// Carries that action for the log. The peer sees the session close
    /// rather than a handshake denial, because the handshake did not deny it:
    /// this service did.
    ActionNotServed(String),
}

/// Errors that stop the accept path before it reaches a decision.
#[derive(Debug, thiserror::Error)]
pub enum ProjectionAcceptError {
    /// The carrier could not accept a session.
    #[error("projection carrier accept failed: {0}")]
    Carrier(#[from] TransportError),
    /// The handshake could not complete on the accepted stream.
    #[error(transparent)]
    Handshake(#[from] IoHandshakeError),
}

/// Accept one projection session, admitted and action-checked.
///
/// On `Ok(Ok(session))` the stream has cleared admission *and* carries an
/// action this service serves, so it is ready for `SessionOpen`. Nothing in
/// `graphshell-protocol` restates any of it: the protocol negotiates version
/// and capabilities only, because the principal was settled here.
///
/// `transport` is borrowed; see the module docs on why owning it would lose
/// the reply.
pub async fn accept_projection_session<T: Transport>(
    transport: &T,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    now_ms: u64,
    active_sessions: u32,
) -> Result<Result<AdmittedSession<T::Stream>, ProjectionRefusal>, ProjectionAcceptError> {
    let accepted = transport.accept(projection_alpn()).await?;
    // N1's adapter, not a local copy: every fact below is read off the
    // acceptance record before any application byte is read.
    let (stream, facts) = accepted.into_session();

    let admitted = admit_session(stream, policy, ledger, &facts, now_ms, active_sessions).await?;
    let mut session = match admitted {
        Ok(session) => session,
        Err(reason) => return Ok(Err(ProjectionRefusal::NotAdmitted(reason))),
    };

    if !serves_action(&session.principal) {
        let action = session.principal.action.action.clone();
        // Finish rather than drop, for the same reason a refused handshake is
        // finished. Both arms happen to drain on drop today, through two Drop
        // impls nobody chose; saying it explicitly is the difference between a
        // guarantee and an accident.
        let _ = session.stream.shutdown().await;
        return Ok(Err(ProjectionRefusal::ActionNotServed(action)));
    }

    Ok(Ok(session))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, connect_action, open_session};
    use network_policy::{RequestedAction, SessionHello, TrafficClass, initiate_session};
    use personae::IdentityProvider;
    use personae::InMemoryProvider;
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
    };
    use transport::memory::MemoryTransport;
    use transport::{PeerID, initiator_binding};

    const NETWORK: NetworkId = NetworkId([3; 32]);
    const ROOT_AUTHORITY: [u8; 32] = [7; 32];
    const NOW_MS: u64 = 50;

    fn owner() -> InMemoryProvider {
        InMemoryProvider::from_seed([1; 32])
    }

    fn viewer() -> InMemoryProvider {
        InMemoryProvider::from_seed([4; 32])
    }

    fn profile_ref() -> ProfileRef {
        ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        }
    }

    /// A grant from the owner letting `subject` perform `action` at the
    /// projection service.
    fn grant(subject: [u8; 32], action: &str) -> SignedDelegationCertificate {
        SignedDelegationCertificate::issue(
            &owner(),
            DelegationCertificate::new(
                DelegationParent::Root(ROOT_AUTHORITY),
                owner().master_public_key().to_bytes(),
                subject,
                CapabilityScope {
                    domain: GRAPHSHELL_DOMAIN.into(),
                    resource: NETWORK.0.to_vec(),
                    path_prefix: PROJECTION_SERVICE.into(),
                    actions: [action.to_string()].into_iter().collect(),
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
        projection_policy(
            NETWORK,
            vec![TrustedRoot {
                authority: ROOT_AUTHORITY,
                issuer: owner().master_public_key().to_bytes(),
            }],
            vec![profile_ref()],
            None,
        )
    }

    /// Drive one session over the paired memory carrier and return what the
    /// responder decided.
    ///
    /// The client half holds its stream until the responder has decided, which
    /// is what a real client does and what the module docs require of a
    /// service: neither side may drop the connection out from under a reply
    /// that has been written.
    /// `Ok(action)` means the session was served under that action.
    async fn run(action: &str) -> Result<String, ProjectionRefusal> {
        let viewer = viewer();
        let subject = viewer.master_public_key().to_bytes();
        // The memory carrier authenticates its counterparty by construction,
        // so policy rule D6 requires the claimed subject to *be* the peer the
        // carrier proved. Giving the client node the viewer's own key is what
        // makes this an honest fixture rather than one that dodges the rule.
        let client_peer = PeerID::from_bytes(&subject).expect("client peer");
        let server_peer =
            PeerID::from_bytes(&owner().master_public_key().to_bytes()).expect("server peer");
        let (server, client) = MemoryTransport::pair(server_peer, client_peer);

        let mut requested = connect_action();
        requested.action = action.to_string();
        let delegations = vec![grant(subject, action)];
        let action_owned = action.to_string();

        let client_task = tokio::spawn(async move {
            let mut stream = client
                .connect(server_peer, projection_alpn())
                .await
                .expect("dial");
            let binding = initiator_binding(&projection_alpn(), client_peer);
            // The connect case goes through Graphshell's own helper, because
            // that is the path a real viewer takes. The other case cannot:
            // `open_session` fixes the action to `connect`, and editing a
            // signed hello afterwards would invalidate it. A peer asking for
            // something else builds its own hello, which is exactly what the
            // open action vocabulary allows and what this test needs to be
            // honest about.
            let hello = if action_owned == CONNECT_ACTION {
                open_session(
                    &viewer,
                    NETWORK,
                    profile_ref(),
                    TrafficClass::Interactive,
                    [5; 32],
                    &binding,
                    delegations,
                )
            } else {
                SessionHello::issue(
                    &viewer,
                    NETWORK,
                    profile_ref(),
                    RequestedAction {
                        domain: GRAPHSHELL_DOMAIN.into(),
                        path: PROJECTION_SERVICE.into(),
                        action: action_owned,
                    },
                    TrafficClass::Interactive,
                    [5; 32],
                    &binding,
                    delegations,
                )
            }
            .expect("issue hello");
            let _ = initiate_session(&mut stream, &hello, &policy().limits.clamped()).await;
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        });

        let outcome =
            accept_projection_session(&server, &policy(), &RevocationLedger::default(), NOW_MS, 0)
                .await
                .expect("accept path");
        client_task.abort();

        outcome.map(|session| session.principal.action.action.clone())
    }

    #[tokio::test]
    async fn a_connect_grant_is_admitted_and_served() {
        let served = run(CONNECT_ACTION).await.expect("must be served");
        assert_eq!(served, CONNECT_ACTION);
    }

    /// The hole the policy layer cannot close. `ServiceRule` has no action
    /// vocabulary, so a chain covering a *different* action at the projection
    /// path clears admission with that action intact. This service still
    /// refuses to serve it, and says so with a different word than a denial.
    #[tokio::test]
    async fn an_admitted_action_this_service_does_not_serve_is_refused() {
        let refusal = run("administer").await.expect_err("must be refused");
        assert_eq!(
            refusal,
            ProjectionRefusal::ActionNotServed("administer".to_string())
        );
    }
}
