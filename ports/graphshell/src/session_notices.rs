//! C3: the revision bell on an admitted session.
//!
//! [`crate::session_loop`] answers what a peer asks. This adds the half a
//! remote projection needs and a request/response loop cannot express: telling
//! a client the scene moved when nothing was asked.
//!
//! Stdio has had this since `serve_resumable_notifying`, and the admitted loop
//! did not, which meant a remote client's `wait_for_notice` waited on a frame
//! that was never going to come. Two peers editing one document is exactly the
//! case that needs it: without a bell, the second peer sees the first peer's
//! edit only when it next happens to ask.
//!
//! ## Why this is not a second loop
//!
//! The loop is [`crate::session_loop`]'s, with a poller passed in. Authority
//! rechecks, lapse handling, the session plane, and the framing stay in one
//! place, because a notice lane is a reason to poll between reads rather than
//! a reason to own a second copy of the rules.

use std::fmt::Display;
use std::sync::RwLock;
use std::time::Duration;

use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
};
use chirograph::{ResumeReply, ResumeRequest};
use notochord::{AdmittedSession, RevocationLedger};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::lifecycle::SessionAuthority;
use crate::session_loop::{SessionLoopError, SessionSummary, serve_admitted_session_with};

/// Serve one admitted session, ringing when the endpoint has news.
///
/// `poll_interval` bounds how long a bell waits behind a quiet peer. It is not
/// a polling rate for the endpoint's own watcher, which keeps its own schedule;
/// it is how often this loop stops waiting for a request to ask whether one
/// fired.
pub async fn serve_admitted_session_notifying<E, S, F, N>(
    session: &mut AdmittedSession<S>,
    authority: &SessionAuthority,
    revocations: &RwLock<RevocationLedger>,
    endpoint: &mut E,
    resume: &mut F,
    now_ms: N,
    poll_interval: Duration,
) -> Result<SessionSummary, SessionLoopError>
where
    E: ProjectionCatalog
        + ProjectionSource
        + PresentationSource
        + IntentSink
        + ProjectionNoticeSource,
    <E as ProjectionSource>::Error: Display,
    <E as PresentationSource>::Error: Display,
    <E as IntentSink>::Error: Display,
    <E as ProjectionNoticeSource>::Error: Display,
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnMut(&mut E, ResumeRequest) -> Result<ResumeReply, String>,
    N: Fn() -> u64,
{
    serve_admitted_session_with(
        session,
        authority,
        revocations,
        endpoint,
        resume,
        now_ms,
        Some((poll_interval, |endpoint: &mut E| {
            endpoint.poll_notice().map_err(|error| error.to_string())
        })),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
    use crate::network_carrier::{CarrierRuntime, NetworkCarrier};
    use crate::session_loop::SessionEnd;
    use chirograph::{
        CapabilityProfile, Carrier, CarrierNotice, CarrierRequestBody, CarrierResponseBody,
        EndpointDescriptor, IntentInvocation, IntentResult, ProjectionRequest, ProjectionSession,
        ProjectionSnapshot, ProtocolVersion, ResourceRequest, ResourceResponse, Revision,
        SceneEpoch, SessionOpen,
    };
    use notochord::{
        AdmittedPrincipal, CarrierKind, NetworkId, ProfileRef, RequestedAction, SessionClaims,
        SessionFacts, TrafficClass,
    };
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
    };
    use personae::{IdentityProvider, InMemoryProvider};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
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

    fn notice(revision: u64) -> CarrierNotice {
        CarrierNotice {
            session: ProjectionSession("fixture:scene".into()),
            epoch: SceneEpoch(3),
            revision: Revision(revision),
        }
    }

    /// An endpoint that rings a fixed number of times and counts how often it
    /// was asked, so a test can tell "no notice" from "never polled".
    struct RingingEndpoint {
        pending: Vec<CarrierNotice>,
        polls: Arc<AtomicUsize>,
    }

    #[derive(Debug)]
    struct StubError;

    impl Display for StubError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "stub endpoint")
        }
    }

    impl ProjectionCatalog for RingingEndpoint {
        fn describe(&self) -> EndpointDescriptor {
            EndpointDescriptor {
                label: "ringing".into(),
                projections: Vec::new(),
            }
        }
    }

    impl ProjectionSource for RingingEndpoint {
        type Error = StubError;
        fn snapshot(&mut self, _: ProjectionRequest) -> Result<ProjectionSnapshot, StubError> {
            Err(StubError)
        }
    }

    impl PresentationSource for RingingEndpoint {
        type Error = StubError;
        fn resource(&mut self, _: ResourceRequest) -> Result<ResourceResponse, StubError> {
            Err(StubError)
        }
    }

    impl IntentSink for RingingEndpoint {
        type Error = StubError;
        fn invoke(&mut self, _: IntentInvocation) -> Result<IntentResult, StubError> {
            Ok(IntentResult::Accepted)
        }
    }

    impl ProjectionNoticeSource for RingingEndpoint {
        type Error = StubError;
        fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, StubError> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Ok(if self.pending.is_empty() {
                None
            } else {
                Some(self.pending.remove(0))
            })
        }
    }

    fn open_body() -> CarrierRequestBody {
        CarrierRequestBody::Open(Box::new(SessionOpen {
            version: ProtocolVersion { major: 1, minor: 0 },
            capabilities: CapabilityProfile::default(),
        }))
    }

    /// Serve `endpoint` with a notice lane, driving it from a real
    /// `NetworkCarrier` on a blocking thread: the two halves of C3 against
    /// each other, in the shape production uses.
    fn with_served<T>(
        pending: Vec<CarrierNotice>,
        client: impl FnOnce(&mut NetworkCarrier<DuplexStream>) -> T + Send + 'static,
    ) -> (T, SessionSummary, usize)
    where
        T: Send + 'static,
    {
        with_served_every(Duration::from_millis(10), pending, client)
    }

    fn with_served_every<T>(
        poll_interval: Duration,
        pending: Vec<CarrierNotice>,
        client: impl FnOnce(&mut NetworkCarrier<DuplexStream>) -> T + Send + 'static,
    ) -> (T, SessionSummary, usize)
    where
        T: Send + 'static,
    {
        let runtime = Runtime::new().unwrap();
        let (near, far) = tokio::io::duplex(64 * 1024);
        let polls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&polls);
        let served = runtime.spawn(async move {
            let mut session = admitted(far);
            let mut endpoint = RingingEndpoint {
                pending,
                polls: counted,
            };
            let mut resume =
                |_: &mut RingingEndpoint, _: ResumeRequest| Err("no resume".to_string());
            serve_admitted_session_notifying(
                &mut session,
                &SessionAuthority::retain(principal(), vec![grant()]),
                &RwLock::new(RevocationLedger::new()),
                &mut endpoint,
                &mut resume,
                || NOW_MS,
                poll_interval,
            )
            .await
            .unwrap()
        });

        let mut carrier =
            NetworkCarrier::over(near, CarrierRuntime::borrowed(runtime.handle().clone()));
        let outcome = client(&mut carrier);
        let summary = runtime.block_on(served).unwrap();
        (outcome, summary, polls.load(Ordering::SeqCst))
    }

    #[test]
    fn a_quiet_peer_still_hears_the_bell() {
        // The gap this module closes. The client asks nothing and waits; the
        // endpoint's revision has to reach it anyway.
        let (heard, summary, _) = with_served(vec![notice(2)], |carrier| {
            let heard = carrier.wait_for_notice().unwrap();
            carrier.request(CarrierRequestBody::Close).unwrap();
            heard
        });
        assert_eq!(heard, notice(2));
        assert_eq!(summary.end, SessionEnd::Closed);
    }

    #[test]
    fn a_bell_rung_between_requests_arrives_without_waiting_for_the_interval() {
        // The busy-client case: a peer sending faster than the poll interval
        // would never let the timeout arm fire, so the loop polls before each
        // read as well. The notice rides ahead of the next response.
        let (notices, summary, _) = with_served(vec![notice(4)], |carrier| {
            carrier.request(open_body()).unwrap();
            carrier.request(CarrierRequestBody::Close).unwrap();
            let mut seen = Vec::new();
            while let Some(notice) = carrier.take_notice() {
                seen.push(notice);
            }
            seen
        });
        assert_eq!(notices, vec![notice(4)]);
        assert_eq!(summary.end, SessionEnd::Closed);
    }

    #[test]
    fn the_endpoint_is_asked_even_while_no_request_is_in_flight() {
        // "No notice" and "never polled" look the same on the wire, so the
        // count is what distinguishes a working lane from a silent one.
        let (_, summary, polls) = with_served(Vec::new(), |carrier| {
            std::thread::sleep(Duration::from_millis(60));
            carrier.request(CarrierRequestBody::Close).unwrap()
        });
        assert!(
            polls > 1,
            "the loop polls between reads, not only per request: {polls}"
        );
        assert_eq!(summary.end, SessionEnd::Closed);
    }

    #[test]
    fn every_pending_notice_is_flushed_in_one_wake_rather_than_one_per_interval() {
        // An endpoint that moved several revisions while the peer was quiet
        // has all of them waiting. Draining one per wake would let a busy
        // source outrun its own bell.
        //
        // The interval is long precisely so the two behaviours cannot be
        // confused: a loop that emitted one notice per wake would deliver the
        // third a full two intervals late, while a loop that drains delivers
        // all three before it ever waits. `take_notice` is not used here, since it
        // reports what the carrier has already read, never what is still on
        // the wire, so only repeated waits can show what the server sent.
        let started = std::time::Instant::now();
        let (heard, _, _) = with_served_every(
            Duration::from_secs(5),
            vec![notice(2), notice(3), notice(4)],
            |carrier| {
                let mut seen = Vec::new();
                for _ in 0..3 {
                    seen.push(carrier.wait_for_notice().unwrap());
                }
                carrier.request(CarrierRequestBody::Close).unwrap();
                seen
            },
        );
        assert_eq!(heard, vec![notice(2), notice(3), notice(4)]);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "all three arrived on one wake, well inside a single interval: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_response_still_reaches_the_client_that_asked_for_it() {
        // The lane must not cost the loop its day job.
        let (opened, summary, _) = with_served(vec![notice(2)], |carrier| {
            let opened = carrier.request(open_body()).unwrap();
            carrier.request(CarrierRequestBody::Close).unwrap();
            opened
        });
        match opened {
            CarrierResponseBody::Opened(opened) => assert_eq!(opened.descriptor.label, "ringing"),
            other => panic!("expected an opened session, got {other:?}"),
        }
        assert_eq!(summary.answered, 2);
    }
}
