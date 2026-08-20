//! C3: the dialling half of the projection carrier.
//!
//! [`crate::carrier`] accepts a projection session; this dials one. What
//! happens after either is [`graphshell_network::NetworkCarrier`], which
//! carries the protocol over any stream and is deliberately ignorant of how
//! the stream was obtained.
//!
//! What lives here is what the carrier must not know: the projection ALPN, the
//! initiator binding, and the dial that produces an admitted stream. Those are
//! this service's vocabulary, and they belong beside
//! [`crate::carrier::accept_projection_session`] rather than in a crate that
//! serves every service equally. The dialling half and the accepting half of
//! one service stay adjacent.
//!
//! Nothing here re-derives connectivity. Tickets, mDNS, relays, and hole
//! punching belong to the transport and are proven across two real machines.
//!
//! The carrier types are re-exported because this is where a caller comes to
//! get a projection carrier, and making them import the crate separately would
//! be ceremony rather than a boundary.

use notochord::{
    DenyReason, HandshakeLimits, IoHandshakeError, ProofBinding, SessionHello, SessionReply,
    initiate_session,
};
use transport::{PeerID, Transport, TransportError, initiator_binding};

use crate::carrier::projection_alpn;

pub use graphshell_network::{CarrierRuntime, NetworkCarrier};

/// Errors that stop a dial before it reaches a decision.
#[derive(Debug, thiserror::Error)]
pub enum DialError {
    /// The transport could not reach the peer.
    #[error("projection carrier could not reach the peer: {0}")]
    Connect(#[from] TransportError),
    /// The handshake could not complete on the connected stream.
    #[error(transparent)]
    Handshake(#[from] IoHandshakeError),
}

/// The binding an initiator signs for a projection session.
///
/// It must carry the dialling transport's *own* peer id. The responder checks
/// the claimed subject against the peer its carrier proved, so a binding
/// minted for any other id is refused rather than merely unverified.
pub fn projection_binding(local: PeerID) -> ProofBinding {
    initiator_binding(&projection_alpn(), local)
}

/// Dial one projection session and prove the subject.
///
/// The mirror of [`crate::carrier::accept_projection_session`]: on
/// `Ok(Ok(stream))` the peer has admitted this session and the stream is ready
/// for `SessionOpen`, which is where [`NetworkCarrier::over`] takes over.
///
/// Discovery races are the caller's to absorb. mDNS fills the address book
/// asynchronously while a dial reads it synchronously, so a ticketless dial on
/// a real link can fail once and succeed a moment later; retrying belongs to a
/// caller that knows its own deadline, not to a carrier.
pub async fn dial_projection_session<T: Transport>(
    transport: &T,
    peer: PeerID,
    hello: &SessionHello,
    limits: &HandshakeLimits,
) -> Result<Result<T::Stream, DenyReason>, DialError> {
    let mut stream = transport.connect(peer, projection_alpn()).await?;
    match initiate_session(&mut stream, hello, limits).await? {
        SessionReply::Reject { reason } => Ok(Err(reason)),
        SessionReply::Accept { .. } => Ok(Ok(stream)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
    use crate::lifecycle::SessionAuthority;
    use crate::session_loop::{SessionEnd, serve_admitted_session};
    use chirograph::{
        CapabilityProfile, Carrier, CarrierRequestBody, CarrierResponseBody, EndpointDescriptor,
        IntentInvocation, IntentResult, ProjectionRequest, ProjectionSnapshot, ProtocolVersion,
        ResourceRequest, ResourceResponse, ResumeRequest, SessionOpen,
    };
    use graphshell_endpoint::{
        IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource,
    };
    use notochord::{
        AdmittedPrincipal, AdmittedSession, CarrierKind, NetworkId, ProfileRef, RequestedAction,
        RevocationLedger, SessionClaims, SessionFacts, TrafficClass,
    };
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
    };
    use personae::{IdentityProvider, InMemoryProvider};
    use std::fmt::Display;
    use std::sync::RwLock;
    use tokio::io::DuplexStream;
    use tokio::runtime::Runtime;

    const NOW_MS: u64 = 50;
    const EXPIRY_MS: u64 = 100;
    const ROOT_AUTHORITY: [u8; 32] = [7; 32];
    const NETWORK: [u8; 32] = [3; 32];

    fn owner() -> InMemoryProvider {
        InMemoryProvider::from_seed([1; 32])
    }

    fn viewer() -> InMemoryProvider {
        InMemoryProvider::from_seed([4; 32])
    }

    fn grant() -> SignedDelegationCertificate {
        SignedDelegationCertificate::issue(
            &owner(),
            DelegationCertificate::new(
                DelegationParent::Root(ROOT_AUTHORITY),
                owner().master_public_key().to_bytes(),
                viewer().master_public_key().to_bytes(),
                CapabilityScope {
                    domain: GRAPHSHELL_DOMAIN.into(),
                    resource: NETWORK.to_vec(),
                    path_prefix: PROJECTION_SERVICE.into(),
                    actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
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
                domain: GRAPHSHELL_DOMAIN.to_string(),
                path: PROJECTION_SERVICE.to_string(),
                action: CONNECT_ACTION.to_string(),
            },
        }
    }

    fn admitted(stream: DuplexStream) -> AdmittedSession<DuplexStream> {
        AdmittedSession {
            stream,
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
                delegations: vec![grant()],
            },
            facts: SessionFacts::new(b"mere/graphshell/v1", CarrierKind::P2panda),
            limits: Default::default(),
        }
    }

    /// An endpoint that answers discovery and nothing else, so this test is
    /// about the carrier rather than any product's adapter.
    struct StubEndpoint;

    #[derive(Debug)]
    struct StubError;

    impl Display for StubError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "stub endpoint")
        }
    }

    impl ProjectionCatalog for StubEndpoint {
        fn describe(&self) -> EndpointDescriptor {
            EndpointDescriptor {
                label: "stub".into(),
                projections: Vec::new(),
            }
        }
    }

    impl ProjectionSource for StubEndpoint {
        type Error = StubError;
        fn snapshot(&mut self, _: ProjectionRequest) -> Result<ProjectionSnapshot, StubError> {
            Err(StubError)
        }
    }

    impl PresentationSource for StubEndpoint {
        type Error = StubError;
        fn resource(&mut self, _: ResourceRequest) -> Result<ResourceResponse, StubError> {
            Err(StubError)
        }
    }

    impl IntentSink for StubEndpoint {
        type Error = StubError;
        fn invoke(&mut self, _: IntentInvocation) -> Result<IntentResult, StubError> {
            Err(StubError)
        }
    }

    fn open_body() -> CarrierRequestBody {
        CarrierRequestBody::Open(Box::new(SessionOpen {
            version: ProtocolVersion { major: 1, minor: 0 },
            capabilities: CapabilityProfile::default(),
        }))
    }

    #[test]
    fn the_carrier_speaks_to_the_served_loop_it_will_meet_in_production() {
        // The re-exported carrier against this port's own served loop. If the
        // two halves ever disagree about framing, this is where it surfaces,
        // and it stays here rather than in the carrier crate because the
        // served loop is the port's.
        let runtime = Runtime::new().unwrap();
        let (client, server) = tokio::io::duplex(64 * 1024);
        let served = runtime.spawn(async move {
            let mut session = admitted(server);
            let mut endpoint = StubEndpoint;
            let mut resume = |_: &mut StubEndpoint, _: ResumeRequest| Err("no resume".to_string());
            serve_admitted_session(
                &mut session,
                &SessionAuthority::retain(principal(), vec![grant()]),
                &RwLock::new(RevocationLedger::new()),
                &mut endpoint,
                &mut resume,
                || NOW_MS,
            )
            .await
            .unwrap()
        });

        let mut carrier =
            NetworkCarrier::over(client, CarrierRuntime::borrowed(runtime.handle().clone()));

        // The verb stdio must refuse and an admitted carrier can answer: the
        // handshake proved a peer before a byte of this arrived.
        match carrier.request(open_body()).unwrap() {
            CarrierResponseBody::Opened(opened) => assert_eq!(opened.descriptor.label, "stub"),
            other => panic!("expected an opened session, got {other:?}"),
        }
        assert!(matches!(
            carrier.request(CarrierRequestBody::Close).unwrap(),
            CarrierResponseBody::Closed
        ));

        let summary = runtime.block_on(served).unwrap();
        assert_eq!(summary.end, SessionEnd::Closed);
        assert_eq!(summary.answered, 2);
    }
}
