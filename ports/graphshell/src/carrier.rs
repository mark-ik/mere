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
