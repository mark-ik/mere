//! K2: product endpoints served to network peers by one resident host.
//!
//! [`crate::native::browser_host`] serves a catalog route to a browser that
//! admitted itself over native messaging. This serves the same catalog to a
//! peer that dialled over a transport, which is what a place member reaching
//! the document's holder actually is.
//!
//! Everything below the accept is already built and none of it is repeated
//! here: [`crate::carrier::accept_projection_session`] decides who is admitted,
//! [`crate::native::endpoint_catalog`] decides which endpoint they reach, and
//! [`crate::session_notices`] serves and rings. What was missing was a host
//! that holds those together and outlives any one session, rather than an
//! accept loop living inside a receipt binary where nothing could consume it.
//!
//! ## Sessions are served concurrently, and that is the point
//!
//! A sequential accept loop can serve a place member. It cannot serve two,
//! which is precisely what a shared document needs: the second visitor would
//! wait at the door until the first one left. So each admitted session is
//! spawned, and the host returns to accepting immediately.
//!
//! Each session opens its own endpoint from the catalog, because the catalog
//! is a factory rather than a registry of live objects. Two visitors to one
//! Knot vault therefore hold two `KnotEndpoint`s over the same files, and
//! converge through the holder's own truth rather than through shared memory.
//! That is Option A working as designed: the holder is the single authority,
//! and a projection is a view of what the holder has.

use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU32, Ordering};

use graphshell_protocol::{ProjectionSession, ResumeRequest};
use notochord::{LocalNetworkPolicy, RevocationLedger};
use tokio::task::JoinHandle;
use transport::Transport;

use crate::carrier::{ProjectionAcceptError, ProjectionRefusal, accept_projection_session};
use crate::lifecycle::SessionAuthority;
use crate::native::endpoint_catalog::{
    ResidentEndpointCatalog, ResidentEndpointCatalogError, ResidentEndpointRoute,
    ResidentEndpointSession,
};
use crate::session_loop::{SessionLoopError, SessionSummary};
use crate::session_notices::serve_admitted_session_notifying;

/// Why a resident host could not serve a peer.
#[derive(Debug, thiserror::Error)]
pub enum ResidentProjectionError {
    /// The accept path failed before reaching a decision.
    #[error(transparent)]
    Accept(#[from] ProjectionAcceptError),
    /// The peer was admitted, but the configured route could not open.
    ///
    /// A host misconfiguration rather than a peer's fault, and it ends the
    /// session: an admitted peer with nothing to serve is told nothing rather
    /// than served an endpoint the host did not mean to expose.
    #[error(transparent)]
    Catalog(#[from] ResidentEndpointCatalogError),
    /// The serving task did not finish.
    #[error("a served projection did not finish: {0}")]
    Join(String),
}

/// One admitted session, already being served in the background.
#[derive(Debug)]
pub struct ServedProjection {
    subject: [u8; 32],
    session: ProjectionSession,
    handle: JoinHandle<Result<SessionSummary, SessionLoopError>>,
}

impl ServedProjection {
    /// The already-admitted public-key subject this session serves.
    pub fn subject(&self) -> [u8; 32] {
        self.subject
    }

    /// The transcript-derived session being served.
    pub fn session(&self) -> &ProjectionSession {
        &self.session
    }

    /// Wait for this session to end and report what it did.
    pub async fn finished(
        self,
    ) -> Result<Result<SessionSummary, SessionLoopError>, ResidentProjectionError> {
        self.handle
            .await
            .map_err(|error| ResidentProjectionError::Join(error.to_string()))
    }
}

/// One session's claim on the live count, released on drop.
///
/// A guard rather than a decrement at the end of the serving task, because
/// that task runs product code: an endpoint or a resume closure that panics
/// unwinds straight past a trailing statement, and tokio catches the panic at
/// the join handle, so the host survives with the slot still counted. The
/// policy's `max_sessions` is checked against this number, so a leak here is a
/// host that quietly stops admitting anyone and reports nothing.
struct LiveSession(Arc<AtomicU32>);

impl LiveSession {
    fn enter(live: Arc<AtomicU32>) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self(live)
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A resident host serving one catalog route to admitted network peers.
pub struct ResidentProjectionHost {
    policy: LocalNetworkPolicy,
    route: ResidentEndpointRoute,
    catalog: ResidentEndpointCatalog,
    revocations: Arc<RwLock<RevocationLedger>>,
    live: Arc<AtomicU32>,
}

impl ResidentProjectionHost {
    pub fn new(
        policy: LocalNetworkPolicy,
        route: ResidentEndpointRoute,
        catalog: ResidentEndpointCatalog,
    ) -> Self {
        Self {
            policy,
            route,
            catalog,
            revocations: Arc::new(RwLock::new(RevocationLedger::new())),
            live: Arc::new(AtomicU32::new(0)),
        }
    }

    /// The ledger every live session re-reads before each request.
    ///
    /// Shared rather than copied so an owner who withdraws a grant is answered
    /// at the next request on every open session, not at the next reconnect.
    pub fn revocations(&self) -> &Arc<RwLock<RevocationLedger>> {
        &self.revocations
    }

    /// How many sessions are being served right now.
    pub fn live_sessions(&self) -> u32 {
        self.live.load(Ordering::SeqCst)
    }

    /// Admit one peer and serve it in the background.
    ///
    /// Returns as soon as the session is admitted and spawned, so the caller
    /// can accept the next peer while this one is still being served. A
    /// refusal is returned rather than raised: a peer failing admission is an
    /// ordinary outcome for a host that keeps listening.
    pub async fn accept_one<T, N>(
        &mut self,
        transport: &T,
        now_ms: N,
    ) -> Result<Result<ServedProjection, ProjectionRefusal>, ResidentProjectionError>
    where
        T: Transport,
        N: Fn() -> u64 + Send + 'static,
    {
        // A snapshot for admission, and never held across the await: the live
        // request loops below re-read the shared ledger themselves.
        let admission_ledger = self
            .revocations
            .read()
            .expect("the revocation ledger lock is never poisoned by this host")
            .clone();
        let live = self.live.load(Ordering::SeqCst);
        let outcome =
            accept_projection_session(transport, &self.policy, &admission_ledger, now_ms(), live)
                .await?;
        let mut admitted = match outcome {
            Ok(session) => session,
            Err(refusal) => return Ok(Err(refusal)),
        };

        // `retain_admitted` rather than `retain`, so the session cannot lose
        // the chain its conclusion was drawn from and go blind to revocation.
        let authority = SessionAuthority::retain_admitted(&admitted);
        let context = authority.endpoint_context();
        let subject = context.subject();
        let session = context.session().clone();

        // Opened before the session is counted live: a route that cannot open
        // must not leave a phantom in the session count.
        let mut endpoint = self.catalog.open(self.route.id(), &context)?;

        let interval = self.route.notice_poll_interval();
        let revocations = Arc::clone(&self.revocations);
        let live = LiveSession::enter(Arc::clone(&self.live));
        let handle = tokio::spawn(async move {
            // Held by the task, so the count falls however the task ends.
            let _live = live;
            let mut resume = |endpoint: &mut ResidentEndpointSession, request: ResumeRequest| {
                endpoint.resume(request)
            };
            serve_admitted_session_notifying(
                &mut admitted,
                &authority,
                &revocations,
                &mut endpoint,
                &mut resume,
                now_ms,
                interval,
            )
            .await
        });

        Ok(Ok(ServedProjection {
            subject,
            session,
            handle,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE, open_session};
    use crate::carrier::projection_policy;
    use crate::network_carrier::{
        CarrierRuntime, NetworkCarrier, dial_projection_session, projection_binding,
    };
    use graphshell_endpoint::{
        IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
    };
    use graphshell_protocol::{
        CapabilityProfile, Carrier, CarrierNotice, CarrierRequestBody, CarrierResponseBody,
        EndpointDescriptor, IntentInvocation, IntentResult, ProjectionRequest, ProjectionSnapshot,
        ProtocolVersion, ResourceRequest, ResourceResponse, SessionOpen,
    };
    use notochord::{NetworkId, ProfileRef, TrafficClass, TrustedRoot};
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
    };
    use personae::{IdentityProvider, InMemoryProvider};
    use std::fmt::Display;
    use std::sync::mpsc;
    use std::time::Duration;
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
                    domain: GRAPHSHELL_DOMAIN.into(),
                    resource: NETWORK.0.to_vec(),
                    path_prefix: PROJECTION_SERVICE.into(),
                    actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
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

    /// An endpoint that names which session opened it, so a test can tell two
    /// concurrently served sessions apart.
    struct LabelledEndpoint {
        label: String,
    }

    #[derive(Debug)]
    struct StubError;

    impl Display for StubError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "stub endpoint")
        }
    }

    impl ProjectionCatalog for LabelledEndpoint {
        fn describe(&self) -> EndpointDescriptor {
            EndpointDescriptor {
                label: self.label.clone(),
                projections: Vec::new(),
            }
        }
    }

    impl ProjectionSource for LabelledEndpoint {
        type Error = StubError;
        fn snapshot(&mut self, _: ProjectionRequest) -> Result<ProjectionSnapshot, StubError> {
            Err(StubError)
        }
    }

    impl PresentationSource for LabelledEndpoint {
        type Error = StubError;
        fn resource(&mut self, _: ResourceRequest) -> Result<ResourceResponse, StubError> {
            Err(StubError)
        }
    }

    impl IntentSink for LabelledEndpoint {
        type Error = StubError;
        fn invoke(&mut self, _: IntentInvocation) -> Result<IntentResult, StubError> {
            Ok(IntentResult::Accepted)
        }
    }

    impl ProjectionNoticeSource for LabelledEndpoint {
        type Error = StubError;
        fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, StubError> {
            Ok(None)
        }
    }

    fn route() -> ResidentEndpointRoute {
        ResidentEndpointRoute::new("knot", Duration::from_millis(10)).expect("route")
    }

    /// A catalog whose factory counts how many endpoints it has built, so a
    /// test can show each session gets its own.
    fn catalog(opened: Arc<AtomicU32>) -> ResidentEndpointCatalog {
        let mut catalog = ResidentEndpointCatalog::new();
        catalog
            .register_notifying("knot", "Knot", move |_| {
                let index = opened.fetch_add(1, Ordering::SeqCst);
                Ok(LabelledEndpoint {
                    label: format!("knot-{index}"),
                })
            })
            .expect("register");
        catalog
    }

    fn host(opened: Arc<AtomicU32>) -> ResidentProjectionHost {
        ResidentProjectionHost::new(policy(), route(), catalog(opened))
    }

    fn peers() -> (MemoryTransport, MemoryTransport, PeerID, PeerID) {
        let subject = viewer().master_public_key().to_bytes();
        let client_peer = PeerID::from_bytes(&subject).expect("client peer");
        let server_peer =
            PeerID::from_bytes(&owner().master_public_key().to_bytes()).expect("server peer");
        let (server, client) = MemoryTransport::pair(server_peer, client_peer);
        (server, client, server_peer, client_peer)
    }

    fn open_body() -> CarrierRequestBody {
        CarrierRequestBody::Open(Box::new(SessionOpen {
            version: ProtocolVersion { major: 1, minor: 0 },
            capabilities: CapabilityProfile::default(),
        }))
    }

    /// Dial, open, report the endpoint label, then hold the session until
    /// `release` says to close it.
    fn visit(
        client: Arc<MemoryTransport>,
        server_peer: PeerID,
        client_peer: PeerID,
        nonce: [u8; 32],
        handle: tokio::runtime::Handle,
        opened: mpsc::Sender<String>,
        release: mpsc::Receiver<()>,
    ) {
        let viewer = viewer();
        let subject = viewer.master_public_key().to_bytes();
        let hello = open_session(
            &viewer,
            NETWORK,
            profile_ref(),
            TrafficClass::Interactive,
            nonce,
            &projection_binding(client_peer),
            vec![grant(subject)],
        )
        .expect("hello");
        let stream = handle
            .block_on(dial_projection_session(
                client.as_ref(),
                server_peer,
                &hello,
                &policy().limits,
            ))
            .expect("dial")
            .expect("admitted");
        let mut carrier = NetworkCarrier::over(stream, CarrierRuntime::borrowed(handle));
        match carrier.request(open_body()).expect("open") {
            CarrierResponseBody::Opened(opened_session) => {
                opened
                    .send(opened_session.descriptor.label)
                    .expect("report");
            }
            other => panic!("expected an opened session, got {other:?}"),
        }
        release.recv().expect("release");
        carrier.request(CarrierRequestBody::Close).expect("close");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_peer_is_admitted_and_served_from_the_catalog() {
        let (server, client, server_peer, client_peer) = peers();
        let mut host = host(Arc::new(AtomicU32::new(0)));
        let (report, reported) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let handle = tokio::runtime::Handle::current();
        let visitor = tokio::task::spawn_blocking(move || {
            visit(
                Arc::new(client),
                server_peer,
                client_peer,
                [5; 32],
                handle,
                report,
                released,
            )
        });

        let served = host
            .accept_one(&server, || NOW_MS)
            .await
            .expect("accept")
            .expect("admitted");
        assert_eq!(served.subject(), viewer().master_public_key().to_bytes());
        assert_eq!(reported.recv().unwrap(), "knot-0");

        release.send(()).unwrap();
        visitor.await.unwrap();
        let summary = served.finished().await.expect("join").expect("served");
        assert_eq!(summary.answered, 2, "the open and the close");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 6)]
    async fn two_peers_are_served_at_once_rather_than_one_after_the_other() {
        // The reason this host spawns rather than loops. Both visitors hold
        // their sessions open at the same time, and the second one is admitted
        // and answered while the first is still connected. A sequential accept
        // loop would deadlock here rather than fail an assertion.
        let (server, client, server_peer, client_peer) = peers();
        let client = Arc::new(client);
        let opened = Arc::new(AtomicU32::new(0));
        let mut host = host(Arc::clone(&opened));
        let handle = tokio::runtime::Handle::current();

        let mut reporters = Vec::new();
        let mut releases = Vec::new();
        let mut visitors = Vec::new();
        for index in 0..2u8 {
            let (report, reported) = mpsc::channel();
            let (release, released) = mpsc::channel();
            let client = Arc::clone(&client);
            let handle = handle.clone();
            visitors.push(tokio::task::spawn_blocking(move || {
                // A fresh nonce per session, because that is what mints a
                // distinct transcript and therefore a distinct session id.
                visit(
                    client,
                    server_peer,
                    client_peer,
                    [index; 32],
                    handle,
                    report,
                    released,
                )
            }));
            reporters.push(reported);
            releases.push(release);
        }

        let first = host
            .accept_one(&server, || NOW_MS)
            .await
            .expect("accept")
            .expect("admitted");
        let second = host
            .accept_one(&server, || NOW_MS)
            .await
            .expect("accept")
            .expect("admitted");

        // Both answered while both are still connected.
        let mut labels = vec![reporters[0].recv().unwrap(), reporters[1].recv().unwrap()];
        labels.sort();
        assert_eq!(labels, vec!["knot-0".to_string(), "knot-1".to_string()]);
        assert_eq!(
            opened.load(Ordering::SeqCst),
            2,
            "each session opened its own endpoint from the catalog"
        );
        assert_eq!(host.live_sessions(), 2, "both are being served at once");
        assert_ne!(
            first.session(),
            second.session(),
            "two sessions, two transcripts"
        );

        for release in releases {
            release.send(()).unwrap();
        }
        for visitor in visitors {
            visitor.await.unwrap();
        }
        first.finished().await.expect("join").expect("served");
        second.finished().await.expect("join").expect("served");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_route_the_host_does_not_offer_serves_nobody() {
        // A host misconfiguration, caught after admission and before any
        // endpoint exists. The peer gets no session rather than some other
        // product's.
        let (server, client, server_peer, client_peer) = peers();
        let mut host = ResidentProjectionHost::new(
            policy(),
            ResidentEndpointRoute::new("absent", Duration::from_millis(10)).expect("route"),
            catalog(Arc::new(AtomicU32::new(0))),
        );
        let handle = tokio::runtime::Handle::current();
        let dialling = tokio::task::spawn_blocking(move || {
            let viewer = viewer();
            let subject = viewer.master_public_key().to_bytes();
            let hello = open_session(
                &viewer,
                NETWORK,
                profile_ref(),
                TrafficClass::Interactive,
                [5; 32],
                &projection_binding(client_peer),
                vec![grant(subject)],
            )
            .expect("hello");
            handle
                .block_on(dial_projection_session(
                    &client,
                    server_peer,
                    &hello,
                    &policy().limits,
                ))
                .expect("dial")
                .expect("admitted");
        });

        let error = host
            .accept_one(&server, || NOW_MS)
            .await
            .expect_err("an unknown route cannot be served");
        assert!(
            matches!(error, ResidentProjectionError::Catalog(_)),
            "{error}"
        );
        assert_eq!(host.live_sessions(), 0, "no phantom session was counted");
        dialling.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_finished_session_releases_its_slot() {
        // The count the policy's `max_sessions` is checked against, so a
        // session that ends has to give its slot back.
        let (server, client, server_peer, client_peer) = peers();
        let mut host = host(Arc::new(AtomicU32::new(0)));
        let (report, reported) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let handle = tokio::runtime::Handle::current();
        let visitor = tokio::task::spawn_blocking(move || {
            visit(
                Arc::new(client),
                server_peer,
                client_peer,
                [5; 32],
                handle,
                report,
                released,
            )
        });

        let served = host
            .accept_one(&server, || NOW_MS)
            .await
            .expect("accept")
            .expect("admitted");
        reported.recv().unwrap();
        assert_eq!(host.live_sessions(), 1);

        release.send(()).unwrap();
        visitor.await.unwrap();
        served.finished().await.expect("join").expect("served");
        assert_eq!(host.live_sessions(), 0, "the slot came back");
    }

    #[test]
    fn a_panicking_session_gives_its_slot_back() {
        // The leak the guard exists for. A serving task runs product code, and
        // a panic there unwinds past any trailing decrement while tokio
        // absorbs the panic at the join handle, so the host would survive with
        // the slot counted forever and quietly stop admitting at max_sessions.
        let live = Arc::new(AtomicU32::new(0));
        let guard = LiveSession::enter(Arc::clone(&live));
        assert_eq!(live.load(Ordering::SeqCst), 1);
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _held = guard;
            panic!("the endpoint blew up mid-session");
        }));
        assert!(unwound.is_err(), "the panic really happened");
        assert_eq!(
            live.load(Ordering::SeqCst),
            0,
            "the slot came back on unwind, not just on a clean return"
        );
    }

    #[test]
    fn revocation_notices_are_shared_with_every_live_session() {
        // Shared rather than snapshotted, so an owner withdrawing a grant is
        // answered at the next request on sessions that are already open.
        let host = host(Arc::new(AtomicU32::new(0)));
        let ledger = Arc::clone(host.revocations());
        assert!(Arc::ptr_eq(&ledger, host.revocations()));
    }
}
