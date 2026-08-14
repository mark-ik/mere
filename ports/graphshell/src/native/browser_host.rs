//! Resident identity authority served over the browser native-messaging edge.

use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use graphshell_protocol::{CarrierRequestBody, CarrierResponseBody, ResumeRequest};
use notochord::{
    LocalNetworkPolicy, NetworkId, ProfileRef, RevocationLedger, ServiceAccess, ServiceRule,
    TrustedRoot,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationError, DelegationParent,
    SignedDelegationCertificate,
};
use personae::{IdentityProvider, IdentityStorage};

use crate::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
use crate::browser_carrier::{
    BrowserCarrierError, BrowserChallenge, BrowserHostMessage, BrowserLauncher, BrowserLink,
    BrowserMessage, NativeIdentityFailure, NativeIdentityResult, admit_browser_session,
    read_native_message_async, write_native_message_async,
};
use crate::identity_endpoint::IdentityEndpoint;
use crate::lifecycle::SessionAuthority;
use crate::native::device_broker::DeviceSurface;
use crate::native::endpoint_catalog::{
    ResidentEndpointCatalog, ResidentEndpointCatalogError, ResidentEndpointRoute,
    ResidentEndpointSession,
};
use crate::native::identity_ui::{NativeIdentityUi, apply_native_identity_action};
use crate::native::personae_host::PersonaeHost;
use crate::session_loop::{SessionLoopError, SessionSummary, serve_admitted_session};
use crate::session_notices::serve_admitted_session_notifying;

const NETWORK_DOMAIN: &[u8] = b"mere.graphshell/local-browser-network/v1";
const ROOT_DOMAIN: &[u8] = b"mere.graphshell/local-browser-root/v1";
const PROFILE_ID: &str = "mere.base";

#[derive(Debug, thiserror::Error)]
pub enum BrowserHostError {
    #[error(transparent)]
    Carrier(#[from] BrowserCarrierError),
    #[error("local browser grant failed: {0}")]
    Delegation(#[from] DelegationError),
    #[error(transparent)]
    Session(#[from] SessionLoopError),
    #[error(transparent)]
    Catalog(#[from] ResidentEndpointCatalogError),
    #[error("browser session task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

/// Serve one browser-launched native-messaging process.
///
/// This is shared by the installed host and the headed approval receipt host,
/// so the receipt exercises the product carrier rather than a second wire
/// implementation.
pub async fn serve_identity_native_messages<P, S, U, R, W>(
    identity: &P,
    personae: Arc<PersonaeHost<S>>,
    native_ui: &U,
    launcher: BrowserLauncher,
    reader: &mut R,
    writer: &mut W,
    session_duration_ms: u64,
) -> Result<Option<SessionSummary>, BrowserHostError>
where
    P: IdentityProvider,
    S: IdentityStorage + 'static,
    U: NativeIdentityUi,
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    serve_identity_native_messages_with_cards(
        identity,
        personae,
        native_ui,
        launcher,
        reader,
        writer,
        session_duration_ms,
        DeviceSurface::default(),
    )
    .await
}

/// Serve the admitted resident-device surface with additional public cards.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn serve_identity_native_messages_with_cards<P, S, U, R, W>(
    identity: &P,
    personae: Arc<PersonaeHost<S>>,
    native_ui: &U,
    launcher: BrowserLauncher,
    reader: &mut R,
    writer: &mut W,
    session_duration_ms: u64,
    surface: DeviceSurface,
) -> Result<Option<SessionSummary>, BrowserHostError>
where
    P: IdentityProvider,
    S: IdentityStorage + 'static,
    U: NativeIdentityUi,
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    serve_native_messages(
        identity,
        personae,
        native_ui,
        launcher,
        reader,
        writer,
        session_duration_ms,
        BrowserSessionEndpoint::Identity { surface },
    )
    .await
}

/// Serve one browser-native session through a host-configured catalog route.
///
/// The browser first completes ordinary native-message admission. Only then
/// does the host open `route` from `catalog`, using the resulting admitted
/// context. The route is supplied by the host process, never by the browser.
#[allow(clippy::too_many_arguments)]
pub async fn serve_catalog_native_messages<P, S, U, R, W>(
    identity: &P,
    personae: Arc<PersonaeHost<S>>,
    native_ui: &U,
    launcher: BrowserLauncher,
    reader: &mut R,
    writer: &mut W,
    session_duration_ms: u64,
    catalog: ResidentEndpointCatalog,
    route: ResidentEndpointRoute,
) -> Result<Option<SessionSummary>, BrowserHostError>
where
    P: IdentityProvider,
    S: IdentityStorage + 'static,
    U: NativeIdentityUi,
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    serve_native_messages(
        identity,
        personae,
        native_ui,
        launcher,
        reader,
        writer,
        session_duration_ms,
        BrowserSessionEndpoint::Catalog { catalog, route },
    )
    .await
}

enum BrowserSessionEndpoint {
    Identity {
        surface: DeviceSurface,
    },
    Catalog {
        catalog: ResidentEndpointCatalog,
        route: ResidentEndpointRoute,
    },
}

#[allow(clippy::too_many_arguments)]
async fn serve_native_messages<P, S, U, R, W>(
    identity: &P,
    personae: Arc<PersonaeHost<S>>,
    native_ui: &U,
    launcher: BrowserLauncher,
    reader: &mut R,
    writer: &mut W,
    session_duration_ms: u64,
    selected_endpoint: BrowserSessionEndpoint,
) -> Result<Option<SessionSummary>, BrowserHostError>
where
    P: IdentityProvider,
    S: IdentityStorage + 'static,
    U: NativeIdentityUi,
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let challenge = BrowserChallenge::fresh();
    write_native_message_async(
        writer,
        &BrowserHostMessage::Challenge {
            challenge: challenge.clone(),
        },
    )
    .await?;

    let Some(connect) = read_native_message_async::<_, BrowserMessage>(reader).await? else {
        return Ok(None);
    };
    let link = BrowserLink::accept(launcher.clone(), &challenge, connect)?;
    let subject = identity.master_public_key().to_bytes();
    let network = local_network(subject);
    let root = local_root(subject);
    let grant = local_browser_grant(
        identity,
        network,
        root,
        link.host_nonce,
        now_ms().saturating_add(session_duration_ms),
    )?;
    let profile = ProfileRef {
        id: PROFILE_ID.to_string(),
        revision: 1,
    };
    let mut policy = LocalNetworkPolicy::closed(network);
    policy.trusted_roots = vec![TrustedRoot {
        authority: root,
        issuer: subject,
    }];
    policy.accepted_profiles = vec![profile.clone()];
    policy.services.insert(
        PROJECTION_SERVICE.to_string(),
        ServiceRule::new(
            ServiceAccess::MemberOnly,
            GRAPHSHELL_DOMAIN,
            [CONNECT_ACTION],
            false,
            Some(1),
        ),
    );
    let revocations = RevocationLedger::new();
    let (mut browser, mut admitted) = admit_browser_session(
        identity,
        network,
        profile,
        vec![grant],
        &link,
        &policy,
        &revocations,
        now_ms(),
    )
    .await?;

    let authority = SessionAuthority::retain_admitted(&admitted);
    let endpoint_context = authority.endpoint_context();
    let session = endpoint_context.session().0.clone();
    let connected = BrowserHostMessage::Connected {
        launcher,
        session: session.clone(),
        subject: base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(endpoint_context.subject()),
    };
    let server = match selected_endpoint {
        BrowserSessionEndpoint::Identity { surface } => {
            let mut endpoint = IdentityEndpoint::for_admitted_with_cards(
                Arc::clone(&personae),
                &authority,
                surface.cards,
            );
            endpoint.with_decisions(surface.decisions);
            // Content a card names but no transfer staged — a receipt's
            // captures — is read from the store on demand rather than held
            // resident, so browsing a graph of receipts costs one blob at a
            // time instead of all of them.
            if let Some(reader) = surface.blob_reader.clone() {
                endpoint.with_reader(Box::new(move |resource| reader(resource)));
            }
            // A transfer whose bytes are too large to hold resident is refused
            // here rather than part-served: the session continues without it,
            // and the reason is logged where the operator can see it. Serving
            // half a transfer would look to the browser like a transfer that
            // worked.
            if !surface.released_blobs.is_empty() {
                let count = surface.released_blobs.len();
                match endpoint.release_transfer(surface.released_blobs) {
                    Ok(()) => {
                        tracing::info!(blobs = count, "released transfer blobs to this browser")
                    }
                    Err(error) => tracing::warn!(%error, "transfer blobs were not released"),
                }
            }
            tokio::spawn(async move {
                let revocations = RwLock::new(revocations);
                let mut resume = |_: &mut IdentityEndpoint<S>, _: ResumeRequest| {
                    Err("identity resume is not implemented".to_string())
                };
                serve_admitted_session(
                    &mut admitted,
                    &authority,
                    &revocations,
                    &mut endpoint,
                    &mut resume,
                    now_ms,
                )
                .await
            })
        }
        BrowserSessionEndpoint::Catalog { mut catalog, route } => {
            // Route selection occurs after admission and before `Connected`.
            // A missing route therefore never yields a live browser session,
            // and the endpoint only sees the narrow admitted context.
            let mut endpoint = catalog.open(route.id(), &endpoint_context)?;
            tokio::spawn(async move {
                let revocations = RwLock::new(revocations);
                let mut resume = |endpoint: &mut ResidentEndpointSession,
                                  request: ResumeRequest| {
                    endpoint.resume(request)
                };
                serve_admitted_session_notifying(
                    &mut admitted,
                    &authority,
                    &revocations,
                    &mut endpoint,
                    &mut resume,
                    now_ms,
                    route.notice_poll_interval(),
                )
                .await
            })
        }
    };
    write_native_message_async(writer, &connected).await?;

    while let Some(message) = read_native_message_async::<_, BrowserMessage>(reader).await? {
        match message {
            BrowserMessage::Request { request } => {
                let terminal = matches!(
                    request.body,
                    CarrierRequestBody::Close | CarrierRequestBody::Suspend
                );
                let response = browser.request(&request).await?;
                let ended = matches!(
                    response.body,
                    Ok(CarrierResponseBody::Closed | CarrierResponseBody::Suspended)
                );
                write_native_message_async(writer, &BrowserHostMessage::Response { response })
                    .await?;
                if terminal || ended {
                    break;
                }
            }
            BrowserMessage::NativeIdentity { request } => {
                let result = if request.session == session {
                    apply_native_identity_action(&personae, native_ui, request.action)
                } else {
                    NativeIdentityResult::Rejected {
                        reason: NativeIdentityFailure::WrongSession,
                    }
                };
                write_native_message_async(
                    writer,
                    &BrowserHostMessage::NativeIdentityResult {
                        id: request.id,
                        result,
                    },
                )
                .await?;
            }
            BrowserMessage::Connect { .. } => {
                write_native_message_async(
                    writer,
                    &BrowserHostMessage::Failure {
                        message: "the browser session is already connected".to_string(),
                    },
                )
                .await?;
            }
        }
    }

    drop(browser);
    Ok(Some(server.await??))
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_millis() as u64
}

fn local_network(subject: [u8; 32]) -> NetworkId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(NETWORK_DOMAIN);
    hasher.update(&subject);
    NetworkId(*hasher.finalize().as_bytes())
}

fn local_root(subject: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(ROOT_DOMAIN);
    hasher.update(&subject);
    *hasher.finalize().as_bytes()
}

fn local_browser_grant<P: IdentityProvider>(
    identity: &P,
    network: NetworkId,
    root: [u8; 32],
    nonce: [u8; 32],
    expires_at_ms: u64,
) -> Result<SignedDelegationCertificate, DelegationError> {
    let issued_at_ms = now_ms().saturating_sub(5_000);
    SignedDelegationCertificate::issue(
        identity,
        DelegationCertificate::new(
            DelegationParent::Root(root),
            identity.master_public_key().to_bytes(),
            identity.master_public_key().to_bytes(),
            CapabilityScope {
                domain: GRAPHSHELL_DOMAIN.to_string(),
                resource: network.0.to_vec(),
                path_prefix: PROJECTION_SERVICE.to_string(),
                actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
            },
            issued_at_ms,
            issued_at_ms,
            Some(expires_at_ms),
            1,
            nonce,
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use graphshell_protocol::{
        CapabilityProfile, CarrierRequest, CarrierRequestBody, CarrierResponseBody,
        EndpointDescriptor, IntentInvocation, IntentResult, PortableCardV1, ProjectionRequest,
        ProjectionSnapshot, ProtocolVersion, ResourceRequest, ResourceResponse, SessionOpen,
    };
    use personae::{
        Ed25519Keypair, IdentityVault, InMemoryProvider, InMemoryStorage, Profile, ProfileId,
    };

    use super::*;
    use crate::browser_carrier::{BrowserHostMessage, CHROMIUM_EXTENSION_ID};
    use crate::identity::VaultProtectionView;
    use crate::lifecycle::AdmittedEndpointContext;
    use crate::native::endpoint_catalog::ResidentEndpoint;
    use crate::native::identity_ui::UnavailableNativeIdentityUi;

    struct CatalogFixtureEndpoint;

    impl ResidentEndpoint for CatalogFixtureEndpoint {
        fn describe(&self) -> EndpointDescriptor {
            EndpointDescriptor {
                label: "Catalog fixture".to_string(),
                projections: Vec::new(),
            }
        }

        fn snapshot(&mut self, _: ProjectionRequest) -> Result<ProjectionSnapshot, String> {
            Err("fixture has no projection".to_string())
        }

        fn resource(&mut self, _: ResourceRequest) -> Result<ResourceResponse, String> {
            Err("fixture has no resources".to_string())
        }

        fn invoke(&mut self, _: IntentInvocation) -> Result<IntentResult, String> {
            Err("fixture has no intents".to_string())
        }
    }

    #[tokio::test]
    async fn admitted_browser_receives_resident_personal_sync_cards() {
        let identity = InMemoryProvider::from_seed([0x81; 32]);
        let profile = Profile::new(
            ProfileId("default".into()),
            "Default",
            Ed25519Keypair::from_seed([0x82; 32]),
        );
        let personae = Arc::new(PersonaeHost::new(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            None,
            VaultProtectionView::Ephemeral,
        ));
        let launcher =
            BrowserLauncher::parse(&[format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")])
                .unwrap();
        let supplemental = crate::identity_endpoint::SupplementalCard {
            adapter: "graphshell.personal-sync".into(),
            source_id: "receipt-graph".into(),
            card: PortableCardV1 {
                title: "Personal graph sync".into(),
                values: Vec::new(),
                badges: vec!["Durable".into()],
                media: Vec::new(),
            },
            actions: Vec::new(),
        };
        let (host_stream, browser_stream) = tokio::io::duplex(64 * 1024);
        let (mut host_reader, mut host_writer) = tokio::io::split(host_stream);
        let (mut browser_reader, mut browser_writer) = tokio::io::split(browser_stream);
        let ui = UnavailableNativeIdentityUi;

        let host = serve_identity_native_messages_with_cards(
            &identity,
            personae,
            &ui,
            launcher,
            &mut host_reader,
            &mut host_writer,
            60_000,
            DeviceSurface {
                cards: vec![supplemental],
                released_blobs: Vec::new(),
                decisions: Default::default(),
                // This test serves only what it stages; read-through is the
                // resident host's composition, not the broker's.
                blob_reader: None,
            },
        );
        let browser = async {
            let challenge =
                match read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                    .await
                    .unwrap()
                    .unwrap()
                {
                    BrowserHostMessage::Challenge { challenge } => challenge,
                    other => panic!("expected browser challenge, got {other:?}"),
                };
            write_native_message_async(
                &mut browser_writer,
                &BrowserMessage::Connect {
                    schema: "mere.graphshell/browser-connect/v1".into(),
                    host_nonce: challenge.host_nonce,
                    client_nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode([0x83; 32]),
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                    .await
                    .unwrap()
                    .unwrap(),
                BrowserHostMessage::Connected { .. }
            ));

            write_native_message_async(
                &mut browser_writer,
                &BrowserMessage::Request {
                    request: CarrierRequest {
                        id: 1,
                        body: CarrierRequestBody::Open(Box::new(SessionOpen {
                            version: ProtocolVersion { major: 1, minor: 0 },
                            capabilities: CapabilityProfile::default(),
                        })),
                    },
                },
            )
            .await
            .unwrap();
            let projection =
                match read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                    .await
                    .unwrap()
                    .unwrap()
                {
                    BrowserHostMessage::Response { response } => match response.body.unwrap() {
                        CarrierResponseBody::Opened(opened) => {
                            opened.descriptor.projections[0].request.clone()
                        }
                        other => panic!("expected opened response, got {other:?}"),
                    },
                    other => panic!("expected carrier response, got {other:?}"),
                };

            write_native_message_async(
                &mut browser_writer,
                &BrowserMessage::Request {
                    request: CarrierRequest {
                        id: 2,
                        body: CarrierRequestBody::Snapshot(projection),
                    },
                },
            )
            .await
            .unwrap();
            let snapshot =
                match read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                    .await
                    .unwrap()
                    .unwrap()
                {
                    BrowserHostMessage::Response { response } => match response.body.unwrap() {
                        CarrierResponseBody::Snapshot(snapshot) => snapshot,
                        other => panic!("expected snapshot response, got {other:?}"),
                    },
                    other => panic!("expected carrier response, got {other:?}"),
                };
            let resource = snapshot
                .presentation
                .offers
                .values()
                .flatten()
                .find(|offer| offer.semantics.label == "Personal graph sync")
                .expect("admitted device scene includes personal sync")
                .resource;

            write_native_message_async(
                &mut browser_writer,
                &BrowserMessage::Request {
                    request: CarrierRequest {
                        id: 3,
                        body: CarrierRequestBody::Resource(ResourceRequest {
                            session: snapshot.session.clone(),
                            resource,
                        }),
                    },
                },
            )
            .await
            .unwrap();
            let card = match read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                .await
                .unwrap()
                .unwrap()
            {
                BrowserHostMessage::Response { response } => match response.body.unwrap() {
                    CarrierResponseBody::Resource(resource) => {
                        serde_json::from_slice::<PortableCardV1>(&resource.bytes).unwrap()
                    }
                    other => panic!("expected resource response, got {other:?}"),
                },
                other => panic!("expected carrier response, got {other:?}"),
            };

            write_native_message_async(
                &mut browser_writer,
                &BrowserMessage::Request {
                    request: CarrierRequest {
                        id: 4,
                        body: CarrierRequestBody::Close,
                    },
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                    .await
                    .unwrap()
                    .unwrap(),
                BrowserHostMessage::Response {
                    response: graphshell_protocol::CarrierResponse {
                        body: Ok(CarrierResponseBody::Closed),
                        ..
                    }
                }
            ));
            card
        };

        let (summary, card) = tokio::join!(host, browser);
        assert_eq!(card.title, "Personal graph sync");
        assert!(summary.unwrap().is_some());
    }

    #[tokio::test]
    async fn browser_route_opens_a_catalog_endpoint_only_after_admission() {
        let identity = InMemoryProvider::from_seed([0x91; 32]);
        let profile = Profile::new(
            ProfileId("default".into()),
            "Default",
            Ed25519Keypair::from_seed([0x92; 32]),
        );
        let personae = Arc::new(PersonaeHost::new(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            None,
            VaultProtectionView::Ephemeral,
        ));
        let launcher =
            BrowserLauncher::parse(&[format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")])
                .unwrap();
        let seen = Arc::new(Mutex::new(None::<AdmittedEndpointContext>));
        let factory_seen = Arc::clone(&seen);
        let mut catalog = ResidentEndpointCatalog::new();
        catalog
            .register_erased("fixture", "Catalog fixture", move |context| {
                *factory_seen.lock().unwrap() = Some(context.clone());
                Ok(Box::new(CatalogFixtureEndpoint))
            })
            .unwrap();
        let route = ResidentEndpointRoute::new("fixture", Duration::from_millis(10)).unwrap();
        let (host_stream, browser_stream) = tokio::io::duplex(64 * 1024);
        let (mut host_reader, mut host_writer) = tokio::io::split(host_stream);
        let (mut browser_reader, mut browser_writer) = tokio::io::split(browser_stream);
        let ui = UnavailableNativeIdentityUi;

        let host = serve_catalog_native_messages(
            &identity,
            personae,
            &ui,
            launcher,
            &mut host_reader,
            &mut host_writer,
            60_000,
            catalog,
            route,
        );
        let browser = async {
            let challenge =
                match read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                    .await
                    .unwrap()
                    .unwrap()
                {
                    BrowserHostMessage::Challenge { challenge } => challenge,
                    other => panic!("expected browser challenge, got {other:?}"),
                };
            write_native_message_async(
                &mut browser_writer,
                &BrowserMessage::Connect {
                    schema: "mere.graphshell/browser-connect/v1".into(),
                    host_nonce: challenge.host_nonce,
                    client_nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode([0x93; 32]),
                },
            )
            .await
            .unwrap();
            let (session, subject) =
                match read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                    .await
                    .unwrap()
                    .unwrap()
                {
                    BrowserHostMessage::Connected {
                        session, subject, ..
                    } => (session, subject),
                    other => panic!("expected connected browser session, got {other:?}"),
                };
            let context = seen
                .lock()
                .unwrap()
                .clone()
                .expect("the host opened the catalog only after admission");
            assert_eq!(context.session().0, session);
            let subject: [u8; 32] = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(subject)
                .unwrap()
                .try_into()
                .unwrap();
            assert_eq!(context.subject(), subject);

            write_native_message_async(
                &mut browser_writer,
                &BrowserMessage::Request {
                    request: CarrierRequest {
                        id: 1,
                        body: CarrierRequestBody::Open(Box::new(SessionOpen {
                            version: ProtocolVersion { major: 1, minor: 0 },
                            capabilities: CapabilityProfile::default(),
                        })),
                    },
                },
            )
            .await
            .unwrap();
            match read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                .await
                .unwrap()
                .unwrap()
            {
                BrowserHostMessage::Response { response } => match response.body.unwrap() {
                    CarrierResponseBody::Opened(opened) => {
                        assert_eq!(opened.descriptor.label, "Catalog fixture")
                    }
                    other => panic!("expected catalog endpoint descriptor, got {other:?}"),
                },
                other => panic!("expected carrier response, got {other:?}"),
            }

            write_native_message_async(
                &mut browser_writer,
                &BrowserMessage::Request {
                    request: CarrierRequest {
                        id: 2,
                        body: CarrierRequestBody::Close,
                    },
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                read_native_message_async::<_, BrowserHostMessage>(&mut browser_reader)
                    .await
                    .unwrap()
                    .unwrap(),
                BrowserHostMessage::Response {
                    response: graphshell_protocol::CarrierResponse {
                        body: Ok(CarrierResponseBody::Closed),
                        ..
                    }
                }
            ));
        };

        let (summary, ()) = tokio::join!(host, browser);
        assert!(summary.unwrap().is_some());
    }
}
