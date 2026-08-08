//! Carrier admission for the private Knot publishing service.
//!
//! This is intentionally parallel to Graphshell's projection carrier rather
//! than calling it. The ALPN, service path, action vocabulary, and eventual
//! application bytes belong to Knot; carrier facts, Noise, and Notochord do
//! not.

use notochord::{
    AdmittedSession, DenyReason, IoHandshakeError, LocalNetworkPolicy, NetworkId, ProfileRef,
    RevocationLedger, ServiceAccess, ServiceRule, TrustedRoot, admit_session,
};
use personae::Ed25519Keypair;
use personae::delegation::path_covers;
use tokio::io::AsyncWriteExt;
use transport::noise::{NoiseStream, secure_responder};
use transport::{Alpn, Transport, TransportError};

use crate::{
    KNOT_PUBLISH_ALPN, KNOT_PUBLISH_DOMAIN, KNOT_PUBLISH_READ_ACTION, KNOT_PUBLISH_SERVICE,
};

/// ALPN accepted for one Phase A Knot publishing session.
pub fn publish_alpn() -> Alpn {
    Alpn::from_bytes(KNOT_PUBLISH_ALPN)
}

/// Owner policy for the read-only Knot publishing service.
///
/// Publication-specific actions are structural children of the service path,
/// so a ticket may carry a leaf grant for exactly one publication while the
/// owner keeps one base service rule.
pub fn publish_policy(
    network: NetworkId,
    trusted_roots: Vec<TrustedRoot>,
    accepted_profiles: Vec<ProfileRef>,
    max_sessions: Option<u32>,
) -> LocalNetworkPolicy {
    let mut policy = LocalNetworkPolicy::closed(network);
    policy.trusted_roots = trusted_roots;
    policy.accepted_profiles = accepted_profiles;
    policy.services.insert(
        KNOT_PUBLISH_SERVICE.into(),
        ServiceRule::new(
            ServiceAccess::MemberOnly,
            KNOT_PUBLISH_DOMAIN,
            [KNOT_PUBLISH_READ_ACTION],
            true,
            max_sessions,
        ),
    );
    policy
}

/// Why a candidate publishing stream never reached its source adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishRefusal {
    /// Notochord refused the signed session hello.
    NotAdmitted(DenyReason),
    /// The inner encrypted ALPN or identity disagreed with the outer carrier.
    CarrierNoiseMismatch,
    /// A policy may admit a broader action vocabulary than this host serves.
    ActionNotServed(String),
    /// The carrier admitted a handshake concurrently with another session;
    /// the host's atomic serving budget refused it before application bytes.
    CapacityExhausted,
}

/// Failure before the carrier could decide whether to serve a publishing
/// session.
#[derive(Debug, thiserror::Error)]
pub enum PublishCarrierError {
    #[error("Knot publishing carrier accept failed: {0}")]
    Carrier(TransportError),
    #[error("Knot publishing Noise handshake failed: {0}")]
    Noise(TransportError),
    #[error(transparent)]
    Handshake(#[from] IoHandshakeError),
}

/// Accept, Noise-secure, and Notochord-admit one publishing stream.
///
/// On success, no application byte has been read. The caller owns the returned
/// `NoiseStream` and may hand it to the private candidate codec exactly once.
pub async fn accept_publish_session<T: Transport>(
    transport: &T,
    identity: &Ed25519Keypair,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    now_ms: u64,
    active_sessions: u32,
) -> Result<Result<AdmittedSession<NoiseStream<T::Stream>>, PublishRefusal>, PublishCarrierError> {
    let accepted = transport
        .accept(publish_alpn())
        .await
        .map_err(PublishCarrierError::Carrier)?;
    let (stream, facts) = accepted.into_session();

    let (mut stream, noise_peer, encrypted_alpn) = secure_responder(identity, stream)
        .await
        .map_err(PublishCarrierError::Noise)?;
    if encrypted_alpn != publish_alpn()
        || facts.authenticated_initiator != Some(noise_peer.to_bytes())
    {
        let _ = stream.shutdown().await;
        return Ok(Err(PublishRefusal::CarrierNoiseMismatch));
    }

    let admitted = admit_session(stream, policy, ledger, &facts, now_ms, active_sessions).await?;
    let mut session = match admitted {
        Ok(session) => session,
        Err(reason) => return Ok(Err(PublishRefusal::NotAdmitted(reason))),
    };
    if !serves_publish_action(&session.principal) {
        let action = session.principal.action.action.clone();
        let _ = session.stream.shutdown().await;
        return Ok(Err(PublishRefusal::ActionNotServed(action)));
    }
    Ok(Ok(session))
}

fn serves_publish_action(principal: &notochord::AdmittedPrincipal) -> bool {
    principal.action.domain == KNOT_PUBLISH_DOMAIN
        && principal.action.action == KNOT_PUBLISH_READ_ACTION
        && path_covers(KNOT_PUBLISH_SERVICE, &principal.action.path)
}
