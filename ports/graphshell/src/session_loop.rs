//! The Graphshell request/response loop over an admitted stream.
//!
//! [`crate::carrier`] gets a peer admitted; this is what happens next. The
//! wire stays NDJSON, the same lines `graphshell-stdio` speaks, because the
//! carriers differ in who they can prove and how long they last, not in what
//! a `Snapshot` request means. Converting Graphshell to length-prefixed frames
//! merely to match Murm and Notochord would be alignment for its own sake.
//!
//! ## Where the two carriers actually differ
//!
//! The verbs that mean the same thing everywhere come from
//! [`graphshell_endpoint::dispatch_common`]. Only the session plane is this
//! module's own, and its answers are the exact inverse of stdio's:
//!
//! | verb | stdio | admitted carrier |
//! |---|---|---|
//! | `Open` | refused: inherited pipes prove no peer | answered: the handshake proved one |
//! | `Suspend` | refused: no session outlives the process | answered: this one can reconnect |
//! | `Close` | terminal | terminal |
//!
//! ## Authority is rechecked, not assumed
//!
//! Admission happened once. A grant can expire or be revoked while a session
//! is open, so every request is checked against the retained authority before
//! it reaches the endpoint. This is where [`crate::lifecycle`] stops being a
//! set of unit-tested rules and starts governing live traffic: a lapsed
//! session is refused and the loop ends, rather than serving scenes on
//! authority that stopped holding.

use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource, SessionPlaneVerb,
    dispatch_common,
};
use graphshell_protocol::{
    CarrierFailure, CarrierRequest, CarrierResponse, CarrierResponseBody, ResumeReply,
    ResumeRequest, SessionOpened, SessionStatus,
};
use notochord::{AdmittedSession, RevocationLedger};
use std::fmt::Display;
use std::sync::RwLock;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::lifecycle::{Lapse, SessionAuthority};

/// Why a session loop stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionEnd {
    /// The peer sent `Close`.
    Closed,
    /// The peer sent `Suspend`; the session may resume later.
    Suspended,
    /// The peer stopped sending without saying goodbye.
    Disconnected,
    /// The session's authority stopped holding, and it was told so.
    Lapsed(Lapse),
}

impl SessionEnd {
    /// The status a client may render for this ending.
    pub fn status(self) -> SessionStatus {
        match self {
            SessionEnd::Closed | SessionEnd::Suspended => SessionStatus::Stale,
            SessionEnd::Disconnected => SessionStatus::Disconnected,
            SessionEnd::Lapsed(lapse) => lapse.status(),
        }
    }
}

/// What one served session did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionSummary {
    /// Requests answered, including refusals.
    pub answered: u64,
    /// Why the loop stopped.
    pub end: SessionEnd,
}

/// Failure that ends a session loop without an answer.
#[derive(Debug, thiserror::Error)]
pub enum SessionLoopError {
    /// The socket failed.
    #[error("session transport failed: {0}")]
    Transport(#[from] std::io::Error),
}

/// Serve one admitted session until it closes, suspends, or lapses.
///
/// `now_ms` is called per request rather than once, so a session that outlives
/// its grant notices at the next request instead of at the next reconnect.
/// `revocations` is shared for the same reason in the other direction: an
/// owner who withdraws a grant mid-session is answered at the very next
/// request, not at the next reconnect.
pub async fn serve_admitted_session<E, S, F, N>(
    session: &mut AdmittedSession<S>,
    authority: &SessionAuthority,
    revocations: &RwLock<RevocationLedger>,
    endpoint: &mut E,
    resume: &mut F,
    now_ms: N,
) -> Result<SessionSummary, SessionLoopError>
where
    E: ProjectionCatalog + ProjectionSource + PresentationSource + IntentSink,
    <E as ProjectionSource>::Error: Display,
    <E as PresentationSource>::Error: Display,
    <E as IntentSink>::Error: Display,
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnMut(&mut E, ResumeRequest) -> Result<ResumeReply, String>,
    N: Fn() -> u64,
{
    let (reader, mut writer) = tokio::io::split(&mut session.stream);
    let mut lines = BufReader::new(reader).lines();
    let mut answered = 0u64;

    let end = loop {
        let Some(line) = lines.next_line().await? else {
            // The peer stopped without a verb. Not an error: a dropped link
            // and a rude client look identical from here, and neither is this
            // module's to judge.
            break SessionEnd::Disconnected;
        };
        if line.trim().is_empty() {
            continue;
        }

        let request: CarrierRequest = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                write_line(
                    &mut writer,
                    &failure(0, format!("invalid carrier request: {error}")),
                )
                .await?;
                answered += 1;
                continue;
            }
        };
        let id = request.id;

        // Before the endpoint sees it. A revoked grant must not buy one more
        // snapshot on its way out.
        //
        // Read fresh each request rather than closed over once: the owner may
        // revoke *during* a session, and a loop holding one snapshot of the
        // ledger would serve the rest of that session on authority that had
        // already been withdrawn. The lock is taken and dropped without an
        // await in between.
        let lapse = {
            let ledger = revocations
                .read()
                .expect("the revocation ledger lock is never poisoned by this loop");
            authority.lapse(&ledger, now_ms())
        };
        if let Some(lapse) = lapse {
            write_line(&mut writer, &failure(id, lapse_message(lapse))).await?;
            answered += 1;
            break SessionEnd::Lapsed(lapse);
        }

        let session_plane = match dispatch_common(endpoint, request, resume) {
            Ok(response) => {
                write_line(&mut writer, &response).await?;
                answered += 1;
                continue;
            }
            Err(session_plane) => session_plane,
        };

        answered += 1;
        match session_plane.verb {
            // Answerable here precisely because stdio cannot: the carrier
            // authenticated this peer before a byte of this reached us.
            SessionPlaneVerb::Open(open) => {
                let opened = SessionOpened {
                    version: open.version,
                    descriptor: endpoint.describe(),
                    status: SessionStatus::Live,
                    // A hint for pre-emptive renewal. The endpoint still
                    // rechecks every request, because a grant can be revoked
                    // long before it expires.
                    expires_at_ms: authority.deadline_ms(),
                };
                write_line(
                    &mut writer,
                    &ok(id, CarrierResponseBody::Opened(Box::new(opened))),
                )
                .await?;
            }
            SessionPlaneVerb::Close => {
                write_line(&mut writer, &ok(id, CarrierResponseBody::Closed)).await?;
                break SessionEnd::Closed;
            }
            SessionPlaneVerb::Suspend => {
                write_line(&mut writer, &ok(id, CarrierResponseBody::Suspended)).await?;
                break SessionEnd::Suspended;
            }
        }
    };

    // Finish rather than drop, for the reason recorded in `crate::carrier`:
    // both arms happen to drain today through Drop implementations nobody
    // chose, and an ending that says so explicitly is a guarantee instead.
    let _ = writer.shutdown().await;
    Ok(SessionSummary { answered, end })
}

fn ok(id: u64, body: CarrierResponseBody) -> CarrierResponse {
    CarrierResponse { id, body: Ok(body) }
}

fn failure(id: u64, message: String) -> CarrierResponse {
    CarrierResponse {
        id,
        body: Err(CarrierFailure { message }),
    }
}

fn lapse_message(lapse: Lapse) -> String {
    match lapse {
        Lapse::Expired { at_ms } => format!("session authority expired at {at_ms}"),
        Lapse::Revoked => "session authority was revoked".to_string(),
    }
}

async fn write_line<W>(writer: &mut W, response: &CarrierResponse) -> Result<(), SessionLoopError>
where
    W: AsyncWrite + Unpin,
{
    let mut line = serde_json::to_vec(response).map_err(std::io::Error::other)?;
    line.push(b'\n');
    writer.write_all(&line).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
    use crate::browser_carrier::{
        BrowserChallenge, BrowserLauncher, BrowserLink, BrowserMessage, CHROMIUM_EXTENSION_ID,
        admit_browser_session,
    };
    use crate::identity::VaultProtectionView;
    use crate::identity_endpoint::IdentityEndpoint;
    use crate::identity_projection::{SIGNING_APPROVE_ONCE_INTENT, SigningDecisionIntentV1};
    use graphshell_client::{ClientState, PresentationResolution, ResolvedContent};
    use graphshell_protocol::{
        CapabilityProfile, CarrierRequestBody, EndpointDescriptor, IntentInvocation, IntentResult,
        PresentationCapability, ProjectionRequest, ProjectionSnapshot, ProtocolVersion,
        ResourceRequest, ResourceResponse, Revision, SceneEpoch, SessionOpen,
    };
    use notochord::{
        AdmittedPrincipal, CarrierKind, LocalNetworkPolicy, NetworkId, ProfileRef, RequestedAction,
        ServiceAccess, ServiceRule, SessionClaims, SessionFacts, TrafficClass, TrustedRoot,
    };
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
        SignedDelegationCertificate, SignedDelegationRevocation,
    };
    use personae::{
        Ed25519Keypair, IdentityProvider, IdentityVault, InMemoryProvider, InMemoryStorage,
        Profile, ProfileId,
    };
    use sceno::InstanceId;
    use signature::Verifier;
    use ssh_agent_lib::agent::Session;
    use ssh_agent_lib::proto::SignRequest;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::io::BufReader as TokioBufReader;

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

    /// The smallest endpoint satisfying the dispatcher's bounds. It never has
    /// to answer: every test here is about the session plane, which is the
    /// half `dispatch_common` declines.
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
                label: "test".into(),
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

    fn authority() -> SessionAuthority {
        SessionAuthority::retain(principal(), vec![grant()])
    }

    fn admitted<S>(stream: S) -> AdmittedSession<S> {
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

    fn revoked() -> RevocationLedger {
        let mut ledger = RevocationLedger::new();
        let certificate = grant();
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
        assert!(ledger.fold(&statement));
        ledger
    }

    fn request(id: u64, body: CarrierRequestBody) -> String {
        let mut line = serde_json::to_string(&carrier_request(id, body)).unwrap();
        line.push('\n');
        line
    }

    fn carrier_request(id: u64, body: CarrierRequestBody) -> CarrierRequest {
        CarrierRequest { id, body }
    }

    fn open_request(id: u64) -> String {
        request(
            id,
            CarrierRequestBody::Open(Box::new(SessionOpen {
                version: ProtocolVersion { major: 1, minor: 0 },
                capabilities: CapabilityProfile::default(),
            })),
        )
    }

    fn intent_request(id: u64) -> String {
        let authority = authority();
        request(
            id,
            CarrierRequestBody::Intent(IntentInvocation {
                session: authority.session().clone(),
                target: InstanceId(1),
                observed_epoch: SceneEpoch(1),
                observed_revision: Revision(1),
                intent: "fixture.inspect".to_string(),
                payload: Vec::new(),
            }),
        )
    }

    /// Drive the loop against `script` under `clock`, returning what the peer
    /// read back and how the loop ended.
    async fn run_with(
        script: &str,
        ledger: RevocationLedger,
        clock: impl Fn() -> u64,
    ) -> (Vec<CarrierResponse>, SessionSummary) {
        let (mut peer, server) = tokio::io::duplex(64 * 1024);
        let script = script.to_string();
        let peer_task = tokio::spawn(async move {
            peer.write_all(script.as_bytes()).await.unwrap();
            let mut lines = TokioBufReader::new(peer).lines();
            let mut out = Vec::new();
            while let Some(line) = lines.next_line().await.unwrap() {
                if !line.trim().is_empty() {
                    out.push(serde_json::from_str::<CarrierResponse>(&line).unwrap());
                }
            }
            out
        });

        let mut session = admitted(server);
        let mut endpoint = StubEndpoint;
        let mut resume = |_: &mut StubEndpoint, _: ResumeRequest| Err("no resume".to_string());
        let summary = serve_admitted_session(
            &mut session,
            &authority(),
            &RwLock::new(ledger),
            &mut endpoint,
            &mut resume,
            clock,
        )
        .await
        .expect("loop");
        (peer_task.await.unwrap(), summary)
    }

    async fn run(script: &str, ledger: RevocationLedger) -> (Vec<CarrierResponse>, SessionSummary) {
        run_with(script, ledger, || NOW_MS).await
    }

    #[tokio::test]
    async fn browser_carrier_delivers_public_identity_cards_and_an_approval() {
        use base64::Engine;
        use ssh_key::{Algorithm, LineEnding};

        let mut private =
            ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        private.set_comment("admitted-identity");
        let public = ssh_key::PublicKey::from(&private);
        let private_openssh = private.to_openssh(LineEnding::LF).unwrap().to_string();
        let mut profile = Profile::new(
            ProfileId("research".to_string()),
            "Research",
            Ed25519Keypair::from_seed([0x7c; 32]),
        );
        profile.slots.insert(
            personae::ssh_slot::protocol_key_for(&private),
            personae::ssh_slot::slot_for(&private, personae::UnlockTier::PerUse).unwrap(),
        );
        let host = Arc::new(crate::native::personae_host::PersonaeHost::new(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            None,
            VaultProtectionView::Ephemeral,
        ));
        let verify_data = b"approved by admitted browser client".to_vec();
        let sign_data = verify_data.clone();
        let sign_credential = public.key_data().clone().into();
        let mut agent = host.agent_session();
        let signing = tokio::spawn(async move {
            agent
                .sign(SignRequest {
                    credential: sign_credential,
                    data: sign_data,
                    flags: 0,
                })
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if !host.snapshot().unwrap().pending_signing.is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        let launcher =
            BrowserLauncher::parse(&[format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")])
                .unwrap();
        let challenge = BrowserChallenge::fresh();
        let link = BrowserLink::accept(
            launcher,
            &challenge,
            BrowserMessage::Connect {
                schema: "mere.graphshell/browser-connect/v1".to_string(),
                host_nonce: challenge.host_nonce.clone(),
                client_nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0x31; 32]),
            },
        )
        .unwrap();
        let mut policy = LocalNetworkPolicy::closed(NetworkId(NETWORK));
        policy.accepted_profiles = vec![ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        }];
        policy.trusted_roots = vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: owner().master_public_key().to_bytes(),
        }];
        policy.services.insert(
            PROJECTION_SERVICE.to_string(),
            ServiceRule::new(
                ServiceAccess::MemberOnly,
                GRAPHSHELL_DOMAIN,
                [CONNECT_ACTION],
                false,
                None,
            ),
        );
        let (mut browser, mut admitted) = admit_browser_session(
            &viewer(),
            NetworkId(NETWORK),
            ProfileRef {
                id: "mere.base".into(),
                revision: 1,
            },
            vec![grant()],
            &link,
            &policy,
            &RevocationLedger::default(),
            NOW_MS,
        )
        .await
        .unwrap();
        let authority = SessionAuthority::retain_admitted(&admitted);
        let mut endpoint = IdentityEndpoint::for_admitted(Arc::clone(&host), &authority);
        let projection = endpoint.request();
        let expected_session = projection.session.clone();
        assert_eq!(
            projection.session,
            authority.session().clone(),
            "the browser cannot choose the projection session id"
        );

        let server_task = tokio::spawn(async move {
            let revocations = RwLock::new(RevocationLedger::default());
            let mut resume = |_: &mut IdentityEndpoint<InMemoryStorage>, _: ResumeRequest| {
                Err("identity resume is not implemented".to_string())
            };
            serve_admitted_session(
                &mut admitted,
                &authority,
                &revocations,
                &mut endpoint,
                &mut resume,
                || NOW_MS,
            )
            .await
            .unwrap()
        });

        let opened = browser
            .request(&carrier_request(
                1,
                CarrierRequestBody::Open(Box::new(SessionOpen {
                    version: ProtocolVersion { major: 1, minor: 0 },
                    capabilities: CapabilityProfile::default(),
                })),
            ))
            .await
            .unwrap();
        assert!(matches!(opened.body, Ok(CarrierResponseBody::Opened(_))));

        let snapshot_response = browser
            .request(&carrier_request(
                2,
                CarrierRequestBody::Snapshot(projection),
            ))
            .await
            .unwrap();
        let snapshot = match snapshot_response.body {
            Ok(CarrierResponseBody::Snapshot(snapshot)) => *snapshot,
            other => panic!("expected identity snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.session, expected_session);
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains(&private_openssh)
        );

        let resources = snapshot
            .presentation
            .offers
            .values()
            .flatten()
            .map(|offer| offer.resource)
            .collect::<Vec<_>>();
        let session = snapshot.session.clone();
        let mut client = ClientState::default();
        client.apply_snapshot(snapshot).unwrap();
        for (offset, resource) in resources.into_iter().enumerate() {
            let response = browser
                .request(&carrier_request(
                    3 + offset as u64,
                    CarrierRequestBody::Resource(ResourceRequest {
                        session: session.clone(),
                        resource,
                    }),
                ))
                .await
                .unwrap();
            let resource = match response.body {
                Ok(CarrierResponseBody::Resource(resource)) => resource,
                other => panic!("expected identity resource, got {other:?}"),
            };
            let text = String::from_utf8(resource.bytes.clone()).unwrap();
            assert!(!text.contains(&private_openssh));
            assert!(!text.contains("BEGIN OPENSSH PRIVATE KEY"));
            client.apply_resource(resource).unwrap();
        }

        let first = client
            .mounted(&session)
            .unwrap()
            .scene
            .active_items_in_order()[0]
            .0;
        let resolved = client
            .resolve(
                &session,
                first,
                &CapabilityProfile::new([PresentationCapability::PortableCard]),
            )
            .unwrap();
        assert!(matches!(
            resolved,
            PresentationResolution::Ready(resolved)
                if matches!(resolved.content, ResolvedContent::PortableCard(_))
        ));

        let mounted = client.mounted(&session).unwrap();
        let target = mounted
            .presentation
            .bindings
            .iter()
            .find(|binding| {
                mounted.presentation.offers.get(&binding.key).unwrap()[0]
                    .semantics
                    .actions
                    .iter()
                    .any(|action| action.intent.0 == SIGNING_APPROVE_ONCE_INTENT)
            })
            .unwrap()
            .instance;
        let request_id = match client
            .resolve(
                &session,
                target,
                &CapabilityProfile::new([PresentationCapability::PortableCard]),
            )
            .unwrap()
        {
            PresentationResolution::Ready(resolved) => match resolved.content {
                ResolvedContent::PortableCard(card) => card
                    .values
                    .iter()
                    .find(|value| value.label == "Request")
                    .and_then(|value| uuid::Uuid::parse_str(&value.value).ok())
                    .expect("pending card discloses its public request id"),
                other => panic!("expected pending portable card, got {other:?}"),
            },
            other => panic!("expected ready pending card, got {other:?}"),
        };
        let acknowledgement = client.acknowledgement(&session).unwrap();
        let intent = browser
            .request(&carrier_request(
                90,
                CarrierRequestBody::Intent(IntentInvocation {
                    session: session.clone(),
                    target,
                    observed_epoch: acknowledgement.epoch,
                    observed_revision: acknowledgement.revision,
                    intent: SIGNING_APPROVE_ONCE_INTENT.to_string(),
                    payload: serde_json::to_vec(&SigningDecisionIntentV1 { request_id }).unwrap(),
                }),
            ))
            .await
            .unwrap();
        assert!(matches!(
            intent.body,
            Ok(CarrierResponseBody::Intent(IntentResult::Accepted))
        ));
        let signature = signing.await.unwrap().unwrap();
        public.key_data().verify(&verify_data, &signature).unwrap();
        assert_eq!(host.snapshot().unwrap().signing_history.len(), 1);

        let closed = browser
            .request(&carrier_request(100, CarrierRequestBody::Close))
            .await
            .unwrap();
        assert!(matches!(closed.body, Ok(CarrierResponseBody::Closed)));
        assert_eq!(server_task.await.unwrap().end, SessionEnd::Closed);
    }

    /// The inverse of stdio's refusal, and the reason this carrier exists: the
    /// handshake proved the peer before a byte of this reached us.
    #[tokio::test]
    async fn open_is_answered_because_this_carrier_authenticated_its_peer() {
        let script = format!(
            "{}{}",
            open_request(1),
            request(2, CarrierRequestBody::Close)
        );
        let (responses, summary) = run(&script, RevocationLedger::default()).await;
        match &responses[0].body {
            Ok(CarrierResponseBody::Opened(opened)) => {
                assert_eq!(opened.status, SessionStatus::Live);
                assert_eq!(
                    opened.expires_at_ms,
                    Some(EXPIRY_MS),
                    "the grant's deadline is offered as a renewal hint"
                );
            }
            other => panic!("expected Opened, got {other:?}"),
        }
        assert_eq!(summary.end, SessionEnd::Closed);
    }

    #[tokio::test]
    async fn suspend_is_answered_because_this_session_can_reconnect() {
        let (responses, summary) = run(
            &request(3, CarrierRequestBody::Suspend),
            RevocationLedger::default(),
        )
        .await;
        assert_eq!(responses[0].body, Ok(CarrierResponseBody::Suspended));
        assert_eq!(summary.end, SessionEnd::Suspended);
    }

    #[tokio::test]
    async fn close_is_terminal_and_nothing_after_it_is_served() {
        let script = format!(
            "{}{}",
            request(4, CarrierRequestBody::Close),
            open_request(5)
        );
        let (responses, summary) = run(&script, RevocationLedger::default()).await;
        assert_eq!(responses.len(), 1, "the request after Close is not served");
        assert_eq!(responses[0].body, Ok(CarrierResponseBody::Closed));
        assert_eq!(summary.end, SessionEnd::Closed);
    }

    /// Where G5e's rules stop being unit tests and start governing traffic.
    #[tokio::test]
    async fn a_revoked_grant_buys_no_further_requests() {
        let (responses, summary) = run(&open_request(6), revoked()).await;
        assert_eq!(responses.len(), 1);
        match &responses[0].body {
            Err(failure) => assert!(
                failure.message.contains("revoked"),
                "the peer is told why: {}",
                failure.message
            ),
            other => panic!("a revoked session must not be opened: {other:?}"),
        }
        assert_eq!(summary.end, SessionEnd::Lapsed(Lapse::Revoked));
    }

    /// G5 names the application verb explicitly. Keep a literal invocation at
    /// the gate so a future request-loop rewrite cannot prove only `Open`.
    #[tokio::test]
    async fn a_revoked_grant_refuses_a_literal_intent_before_dispatch() {
        let (responses, summary) = run(&intent_request(7), revoked()).await;
        assert_eq!(responses.len(), 1);
        match &responses[0].body {
            Err(failure) => assert!(
                failure.message.contains("revoked"),
                "the intent refusal names the lapsed authority: {}",
                failure.message
            ),
            other => panic!("a revoked intent must not reach the endpoint: {other:?}"),
        }
        assert_eq!(summary.end, SessionEnd::Lapsed(Lapse::Revoked));
    }

    /// The check is per request, not per connection: a session that outlives
    /// its grant stops at the next thing it asks for.
    #[tokio::test]
    async fn expiry_mid_session_stops_the_next_request() {
        let script = format!("{}{}", open_request(7), open_request(8));
        let calls = AtomicU64::new(0);
        let (responses, summary) = run_with(&script, RevocationLedger::default(), || {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                NOW_MS
            } else {
                EXPIRY_MS + 1
            }
        })
        .await;

        assert!(
            matches!(responses[0].body, Ok(CarrierResponseBody::Opened(_))),
            "the first request is inside the grant"
        );
        assert!(
            responses[1].body.is_err(),
            "the second is not, and is refused"
        );
        assert_eq!(
            summary.end,
            SessionEnd::Lapsed(Lapse::Expired { at_ms: EXPIRY_MS })
        );
    }
}
