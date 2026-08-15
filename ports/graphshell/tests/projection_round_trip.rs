//! C3's receipt: the Graphshell protocol over a real transport connection.
//!
//! The module tests prove each half against a duplex. This proves the whole
//! path against the thing it actually runs on: a viewer dials, the owner's
//! policy admits it, the served loop answers and rings, and a blocking
//! `NetworkCarrier` drives it, with no subprocess and no stdio anywhere.
//!
//! The client runs on a blocking thread because that is the deployment shape
//! the carrier was designed for and the reason its surface blocks: `run_hub`
//! is already a dedicated thread doing a blocking receive.

use std::fmt::Display;
use std::sync::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use graphshell::admission::open_session;
use graphshell::carrier::{accept_projection_session, projection_policy};
use graphshell::lifecycle::SessionAuthority;
use graphshell::network_carrier::{
    CarrierRuntime, NetworkCarrier, dial_projection_session, projection_binding,
};
use graphshell::session_notices::serve_admitted_session_notifying;
use graphshell_endpoint::ProjectionNoticeSource;
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource};
use chirograph::{
    CapabilityProfile, Carrier, CarrierNotice, CarrierRequestBody, CarrierResponseBody,
    EndpointDescriptor, IntentInvocation, IntentResult, ProjectionRequest, ProjectionSession,
    ProjectionSnapshot, ProtocolVersion, ResourceRequest, ResourceResponse, ResumeRequest,
    Revision, SceneEpoch, SessionOpen,
};
use notochord::{
    LocalNetworkPolicy, NetworkId, ProfileRef, RevocationLedger, TrafficClass, TrustedRoot,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use personae::{IdentityProvider, InMemoryProvider};
use transport::PeerID;
use transport::memory::MemoryTransport;

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

fn grant(subject: [u8; 32]) -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        &owner(),
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            owner().master_public_key().to_bytes(),
            subject,
            CapabilityScope {
                domain: graphshell::admission::GRAPHSHELL_DOMAIN.into(),
                resource: NETWORK.0.to_vec(),
                path_prefix: graphshell::admission::PROJECTION_SERVICE.into(),
                actions: [graphshell::admission::CONNECT_ACTION.to_string()]
                    .into_iter()
                    .collect(),
            },
            5,
            10,
            Some(NOW_MS + 3_600_000),
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

/// An endpoint with one revision to announce, so the round trip covers the
/// bell as well as the request/response half.
struct RingingEndpoint {
    pending: Vec<CarrierNotice>,
    polls: std::sync::Arc<AtomicUsize>,
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
            label: "round-trip".into(),
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

fn bell() -> CarrierNotice {
    CarrierNotice {
        session: ProjectionSession("round-trip:scene".into()),
        epoch: SceneEpoch(3),
        revision: Revision(2),
    }
}

fn open_body() -> CarrierRequestBody {
    CarrierRequestBody::Open(Box::new(SessionOpen {
        version: ProtocolVersion { major: 1, minor: 0 },
        capabilities: CapabilityProfile::default(),
    }))
}

/// A dial, an admission, a served session, and a bell, over one transport.
///
/// `MemoryTransport` authenticates its counterparty by construction, which is
/// why the client node is given the viewer's own key: the responder checks the
/// claimed subject against the peer the carrier proved, so an honest fixture
/// has to make those the same key rather than dodge the rule.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_viewer_dials_is_admitted_and_is_served_over_one_transport() {
    let viewer = viewer();
    let subject = viewer.master_public_key().to_bytes();
    let client_peer = PeerID::from_bytes(&subject).expect("client peer");
    let server_peer =
        PeerID::from_bytes(&owner().master_public_key().to_bytes()).expect("server peer");
    let (server, client) = MemoryTransport::pair(server_peer, client_peer);

    let polls = std::sync::Arc::new(AtomicUsize::new(0));
    let counted = std::sync::Arc::clone(&polls);
    let serving = tokio::spawn(async move {
        let mut session =
            accept_projection_session(&server, &policy(), &RevocationLedger::default(), NOW_MS, 0)
                .await
                .expect("accept path")
                .expect("the viewer is admitted");

        // The real path: the authority the loop rechecks is the one the
        // handshake concluded, not one the test asserted.
        let authority = SessionAuthority::retain_admitted(&session);
        let mut endpoint = RingingEndpoint {
            pending: vec![bell()],
            polls: counted,
        };
        let mut resume = |_: &mut RingingEndpoint, _: ResumeRequest| Err("no resume".to_string());
        serve_admitted_session_notifying(
            &mut session,
            &authority,
            &RwLock::new(RevocationLedger::default()),
            &mut endpoint,
            &mut resume,
            || NOW_MS,
            Duration::from_millis(10),
        )
        .await
        .expect("served session")
    });

    let handle = tokio::runtime::Handle::current();
    let driven = tokio::task::spawn_blocking(move || {
        let binding = projection_binding(client_peer);
        let hello = open_session(
            &viewer,
            NETWORK,
            profile_ref(),
            TrafficClass::Interactive,
            [5; 32],
            &binding,
            vec![grant(subject)],
        )
        .expect("issue hello");

        let stream = handle
            .block_on(dial_projection_session(
                &client,
                server_peer,
                &hello,
                &policy().limits,
            ))
            .expect("dial")
            .expect("the owner admits this viewer");

        let mut carrier = NetworkCarrier::over(stream, CarrierRuntime::borrowed(handle.clone()));
        // The verb stdio must refuse: this carrier proved its peer.
        let opened = carrier.request(open_body()).expect("open");
        let heard = carrier.wait_for_notice().expect("the endpoint rings");
        carrier.request(CarrierRequestBody::Close).expect("close");
        carrier.shutdown().expect("shutdown");
        (opened, heard)
    })
    .await
    .expect("client thread");

    let (opened, heard) = driven;
    match opened {
        CarrierResponseBody::Opened(opened) => {
            assert_eq!(opened.descriptor.label, "round-trip");
        }
        other => panic!("expected an opened session, got {other:?}"),
    }
    assert_eq!(heard, bell(), "the bell crossed the transport");

    let summary = serving.await.expect("serving task");
    assert_eq!(summary.answered, 2, "open and close were both answered");
    assert!(
        polls.load(Ordering::SeqCst) > 0,
        "the endpoint was asked for notices"
    );
}
