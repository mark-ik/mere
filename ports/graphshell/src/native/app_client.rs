//! Client side of the first-party door.
//!
//! What an application holds while talking to the resident host: the
//! handshake, a request counter, and typed calls for the three things a
//! reader of receipt cards does — open, snapshot, read a resource. The
//! server half is `app_broker`; the two share their wire types so they
//! cannot drift.
//!
//! The wire is strictly request/response, so the client is deliberately
//! sequential: one call at a time against `&mut self`, no demultiplexer. A
//! first-party app reading cards is human-paced; the day something needs
//! pipelining, that pressure should reshape the wire, not hide in a client.

use chirograph::{
    CapabilityProfile, CarrierRequest, CarrierRequestBody, CarrierResponseBody, ContentHash,
    PortableCardV1, PresentationCodec, ProjectionRequest, ProjectionSnapshot, ProtocolVersion,
    ResourceRequest, SessionOpen, SessionOpened,
};
use rand_core::{OsRng, RngCore};

use crate::browser_carrier::{read_native_message_async, write_native_message_async};
use crate::native::app_admission::{AppHello, AppId, AppRouteId, configured_app_endpoint};
use crate::native::app_broker::{APP_CONNECT_SCHEMA, AppBrokerError, AppHostMessage, AppMessage};
use crate::native::local_endpoint::{LocalStream, connect_local};

#[derive(Debug, thiserror::Error)]
pub enum AppClientError {
    #[error(transparent)]
    Broker(#[from] AppBrokerError),
    #[error(transparent)]
    Carrier(#[from] crate::browser_carrier::BrowserCarrierError),
    #[error("the resident host closed the stream mid-conversation")]
    Closed,
    #[error("the resident host refused: {0}")]
    Refused(String),
    #[error("expected {expected}, the host answered with {got}")]
    UnexpectedAnswer {
        expected: &'static str,
        got: &'static str,
    },
    #[error("a card resource was not valid PortableCardV1 JSON: {0}")]
    MalformedCard(#[from] serde_json::Error),
    #[error("transport failed: {0}")]
    Io(#[from] std::io::Error),
}

/// An open session against the resident host, as `app`.
pub struct AppBrokerClient {
    stream: Box<dyn LocalStream>,
    session: String,
    next_id: u64,
}

/// One card the resident host is showing, decoded and labeled.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceCard {
    pub label: String,
    pub card: PortableCardV1,
}

impl AppBrokerClient {
    /// Connect to the configured endpoint and complete the whole handshake:
    /// hello, challenge, connect, connected.
    pub async fn open(app: AppId) -> Result<Self, AppClientError> {
        Self::open_at(&configured_app_endpoint(), app).await
    }

    /// Connect to the configured endpoint and request one granted resident
    /// route through the version-two hello.
    pub async fn open_route(app: AppId, route: AppRouteId) -> Result<Self, AppClientError> {
        Self::open_route_at(&configured_app_endpoint(), app, route).await
    }

    /// The same, at an explicit endpoint. Tests and receipts use this.
    pub async fn open_at(endpoint: &str, app: AppId) -> Result<Self, AppClientError> {
        Self::open_with_hello(endpoint, AppHello::new(app.clone()), app).await
    }

    /// Route-aware connection at an explicit endpoint.
    pub async fn open_route_at(
        endpoint: &str,
        app: AppId,
        route: AppRouteId,
    ) -> Result<Self, AppClientError> {
        Self::open_with_hello(endpoint, AppHello::for_route(app.clone(), route), app).await
    }

    async fn open_with_hello(
        endpoint: &str,
        hello: AppHello,
        app: AppId,
    ) -> Result<Self, AppClientError> {
        let mut stream = connect_local(endpoint).await?;
        write_native_message_async(&mut stream, &hello).await?;
        let challenge = match read_host(&mut stream).await? {
            AppHostMessage::Challenge { challenge } => challenge,
            other => return Err(unexpected("a challenge", &other)),
        };
        let mut client_nonce = [0u8; 32];
        OsRng.fill_bytes(&mut client_nonce);
        write_native_message_async(
            &mut stream,
            &AppMessage::Connect {
                schema: APP_CONNECT_SCHEMA.to_string(),
                host_nonce: challenge.host_nonce,
                client_nonce: base64::Engine::encode(
                    &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                    client_nonce,
                ),
            },
        )
        .await?;
        let session = match read_host(&mut stream).await? {
            AppHostMessage::Connected {
                app: connected_app,
                session,
            } => {
                if connected_app != app {
                    return Err(AppClientError::Refused(format!(
                        "resident host connected {connected_app}, expected {app}"
                    )));
                }
                session
            }
            other => return Err(unexpected("connected", &other)),
        };
        Ok(Self {
            stream,
            session,
            next_id: 1,
        })
    }

    /// The session id the host admitted.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// Open the endpoint session and learn what it projects.
    pub async fn open_session(&mut self) -> Result<SessionOpened, AppClientError> {
        let body = self
            .request(CarrierRequestBody::Open(Box::new(SessionOpen {
                version: ProtocolVersion { major: 1, minor: 0 },
                capabilities: CapabilityProfile::default(),
            })))
            .await?;
        match body {
            CarrierResponseBody::Opened(opened) => Ok(*opened),
            other => Err(unexpected_body("opened", &other)),
        }
    }

    /// One projection snapshot.
    pub async fn snapshot(
        &mut self,
        request: ProjectionRequest,
    ) -> Result<ProjectionSnapshot, AppClientError> {
        let body = self.request(CarrierRequestBody::Snapshot(request)).await?;
        match body {
            CarrierResponseBody::Snapshot(snapshot) => Ok(*snapshot),
            other => Err(unexpected_body("a snapshot", &other)),
        }
    }

    /// The bytes behind one content hash — a card, or a capture it names.
    pub async fn resource(
        &mut self,
        session: chirograph::ProjectionSession,
        resource: ContentHash,
    ) -> Result<Vec<u8>, AppClientError> {
        let body = self
            .request(CarrierRequestBody::Resource(ResourceRequest {
                session,
                resource,
            }))
            .await?;
        match body {
            CarrierResponseBody::Resource(response) => Ok(response.bytes),
            other => Err(unexpected_body("the resource", &other)),
        }
    }

    /// Open, snapshot, and decode every portable card on offer.
    ///
    /// The one call a card reader needs. Captures are *not* fetched here —
    /// each card's `media` names them by hash, and the caller reads the ones
    /// it will actually show through [`Self::resource`], which is the whole
    /// point of the store read-through.
    pub async fn read_cards(&mut self) -> Result<Vec<DeviceCard>, AppClientError> {
        let opened = self.open_session().await?;
        let Some(projection) = opened.descriptor.projections.first() else {
            return Ok(Vec::new());
        };
        let snapshot = self.snapshot(projection.request.clone()).await?;
        let mut cards = Vec::new();
        for offer in snapshot.presentation.offers.values().flatten() {
            if offer.codec != PresentationCodec::PortableCardV1 {
                continue;
            }
            let bytes = self
                .resource(snapshot.session.clone(), offer.resource)
                .await?;
            cards.push(DeviceCard {
                label: offer.semantics.label.clone(),
                card: serde_json::from_slice(&bytes)?,
            });
        }
        Ok(cards)
    }

    /// Close the session. Consumes the client; the stream is done either way.
    pub async fn close(mut self) -> Result<(), AppClientError> {
        let _ = self.request(CarrierRequestBody::Close).await?;
        Ok(())
    }

    async fn request(
        &mut self,
        body: CarrierRequestBody,
    ) -> Result<CarrierResponseBody, AppClientError> {
        let id = self.next_id;
        self.next_id += 1;
        write_native_message_async(
            &mut self.stream,
            &AppMessage::Request {
                request: CarrierRequest { id, body },
            },
        )
        .await?;
        match read_host(&mut self.stream).await? {
            AppHostMessage::Response { response } => response
                .body
                .map_err(|error| AppClientError::Refused(error.message)),
            other => Err(unexpected("a carrier response", &other)),
        }
    }
}

async fn read_host(stream: &mut Box<dyn LocalStream>) -> Result<AppHostMessage, AppClientError> {
    read_native_message_async::<_, AppHostMessage>(stream)
        .await?
        .ok_or(AppClientError::Closed)
}

fn unexpected(expected: &'static str, got: &AppHostMessage) -> AppClientError {
    let got = match got {
        AppHostMessage::Challenge { .. } => "a challenge",
        AppHostMessage::Connected { .. } => "connected",
        AppHostMessage::Response { .. } => "a carrier response",
        AppHostMessage::Failure { .. } => "a failure",
    };
    AppClientError::UnexpectedAnswer { expected, got }
}

fn unexpected_body(expected: &'static str, got: &CarrierResponseBody) -> AppClientError {
    let got = match got {
        CarrierResponseBody::Descriptor(_) => "a descriptor",
        CarrierResponseBody::Snapshot(_) => "a snapshot",
        CarrierResponseBody::Resource(_) => "a resource",
        CarrierResponseBody::ResourceChunk(_) => "a resource chunk",
        CarrierResponseBody::Resume(_) => "a resume reply",
        CarrierResponseBody::Intent(_) => "an intent result",
        CarrierResponseBody::Opened(_) => "opened",
        _ => "another response",
    };
    AppClientError::UnexpectedAnswer { expected, got }
}
