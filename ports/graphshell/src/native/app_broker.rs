//! The first-party application door.
//!
//! Serves [`super::app_admission`]'s endpoint: an owner-only socket that a
//! first-party application — turnstone, today — opens to browse this device's
//! graph. Everything past the hello is the same machinery the browser door
//! uses, from `local_session`, so the two doors cannot come to two different
//! answers about what an admitted local client holds.
//!
//! **What is different here, and why.** Two things only:
//!
//! - *The wire.* An application speaks [`AppMessage`], not the browser's
//!   native-messaging frames. The carrier payload inside is identical; the
//!   envelope is separate so this door can version independently of a shipped
//!   browser extension.
//! - *No identity actions.* The browser door carries SSH import, because a
//!   browser has no other way to reach the resident identity UI. A first-party
//!   application runs on this device and can open that UI itself, so the door
//!   does not forward identity actions at all rather than forwarding them and
//!   relying on a session check to refuse.

use std::sync::Arc;
use std::sync::RwLock as StdRwLock;

use chirograph::{CarrierRequest, CarrierRequestBody, CarrierResponse, CarrierResponseBody};
use personae::IdentityStorage;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::browser_carrier::{
    BrowserChallenge, LocalLink, read_native_message_async, write_native_message_async,
};
use crate::identity_endpoint::IdentityEndpoint;
use crate::lifecycle::SessionAuthority;
use crate::native::app_admission::{AllowedApps, AppAdmissionError, AppHello, AppId};
use crate::native::browser_host::BrowserHostError;
use crate::native::device_broker::{DeviceSurface, DeviceSurfaceHandle};
use crate::native::local_endpoint::{LocalStream, connect_local, serve_local};
use crate::native::local_session::{LocalSession, admit_local_client, identity_endpoint_for};
use crate::native::personae_host::PersonaeHost;
use crate::session_loop::{SessionSummary, serve_admitted_session};
use chirograph::ResumeRequest;

/// Schema of the connect answer an application sends.
pub const APP_CONNECT_SCHEMA: &str = "mere.graphshell/app-connect/v1";

#[derive(Debug, thiserror::Error)]
pub enum AppBrokerError {
    #[error(transparent)]
    Admission(#[from] AppAdmissionError),
    #[error(transparent)]
    Host(#[from] BrowserHostError),
    #[error(transparent)]
    Carrier(#[from] crate::browser_carrier::BrowserCarrierError),
    #[error("the application stream ended before its hello")]
    MissingHello,
    #[error("the first application message must answer the host challenge")]
    ConnectRequired,
    #[error("application connect used another schema")]
    WrongSchema,
    #[error("application broker transport failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Application-to-host messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppMessage {
    /// Answer the host challenge and open the session.
    Connect {
        schema: String,
        host_nonce: String,
        client_nonce: String,
    },
    /// One carrier request against the admitted endpoint.
    Request { request: CarrierRequest },
}

/// Host-to-application messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppHostMessage {
    Challenge { challenge: BrowserChallenge },
    Connected { app: AppId, session: String },
    Response { response: CarrierResponse },
    Failure { message: String },
}

/// Open a connection to the resident host as `app`.
///
/// The application side of the door, for a first-party client that lives in
/// this workspace.
pub async fn connect_as_app(
    endpoint: &str,
    app: AppId,
) -> Result<Box<dyn LocalStream>, AppBrokerError> {
    let mut stream = connect_local(endpoint).await?;
    write_native_message_async(&mut stream, &AppHello::new(app)).await?;
    Ok(stream)
}

/// Serve first-party applications from the resident authority.
pub async fn serve_app_broker<S>(
    endpoint: &str,
    personae: Arc<PersonaeHost<S>>,
    allowed: AllowedApps,
    session_duration_ms: u64,
    surface: Option<DeviceSurfaceHandle>,
) -> Result<(), AppBrokerError>
where
    S: IdentityStorage + 'static,
{
    serve_local(
        endpoint,
        "first-party",
        move |stream: Box<dyn LocalStream>| {
            let personae = Arc::clone(&personae);
            let allowed = allowed.clone();
            let surface = surface.clone();
            async move {
                let (mut reader, mut writer) = tokio::io::split(stream);
                if let Err(error) = serve_app_connection(
                    &mut reader,
                    &mut writer,
                    personae,
                    &allowed,
                    session_duration_ms,
                    surface,
                )
                .await
                {
                    tracing::warn!(%error, "first-party session failed");
                }
            }
        },
    )
    .await?;
    Ok(())
}

#[cfg(test)]
pub(crate) use serve_app_connection as serve_app_connection_for_tests;

pub(crate) async fn serve_app_connection<S, R, W>(
    reader: &mut R,
    writer: &mut W,
    personae: Arc<PersonaeHost<S>>,
    allowed: &AllowedApps,
    session_duration_ms: u64,
    surface: Option<DeviceSurfaceHandle>,
) -> Result<Option<SessionSummary>, AppBrokerError>
where
    S: IdentityStorage + 'static,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let hello = read_native_message_async::<_, AppHello>(reader)
        .await?
        .ok_or(AppBrokerError::MissingHello)?;
    let app = hello.accept()?;
    allowed.admit(&app)?;
    // Read once, at session start, exactly as the browser door does: a
    // transfer accepted while an application is already connected becomes
    // visible on its next session rather than mid-stream.
    let surface = match surface {
        Some(surface) => surface.read().await.clone(),
        None => DeviceSurface::default(),
    };
    serve_admitted_app(reader, writer, personae, app, session_duration_ms, surface).await
}

async fn serve_admitted_app<S, R, W>(
    reader: &mut R,
    writer: &mut W,
    personae: Arc<PersonaeHost<S>>,
    app: AppId,
    session_duration_ms: u64,
    surface: DeviceSurface,
) -> Result<Option<SessionSummary>, AppBrokerError>
where
    S: IdentityStorage + 'static,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let challenge = BrowserChallenge::fresh();
    write_native_message_async(
        writer,
        &AppHostMessage::Challenge {
            challenge: challenge.clone(),
        },
    )
    .await?;

    let Some(message) = read_native_message_async::<_, AppMessage>(reader).await? else {
        return Ok(None);
    };
    let AppMessage::Connect {
        schema,
        host_nonce,
        client_nonce,
    } = message
    else {
        return Err(AppBrokerError::ConnectRequired);
    };
    if schema != APP_CONNECT_SCHEMA {
        return Err(AppBrokerError::WrongSchema);
    }
    // The application name is what gets bound into the transcript, so a link
    // minted for one application is not replayable as another.
    let link = LocalLink::bind(app.to_string(), &challenge, &host_nonce, &client_nonce)?;

    let identity = Arc::clone(&personae);
    let LocalSession {
        client: mut carrier,
        mut admitted,
        revocations,
    } = admit_local_client(identity.as_ref(), &link, session_duration_ms).await?;

    let authority = SessionAuthority::retain_admitted(&admitted);
    let session = authority.endpoint_context().session().0.clone();
    let mut endpoint = identity_endpoint_for(Arc::clone(&personae), &authority, surface);
    let server = tokio::spawn(async move {
        let revocations = StdRwLock::new(revocations);
        let mut resume = |_: &mut IdentityEndpoint<S>, _: ResumeRequest| {
            Err("identity resume is not implemented".to_string())
        };
        serve_admitted_session(
            &mut admitted,
            &authority,
            &revocations,
            &mut endpoint,
            &mut resume,
            crate::native::browser_host::now_ms,
        )
        .await
    });
    write_native_message_async(
        writer,
        &AppHostMessage::Connected {
            app: app.clone(),
            session,
        },
    )
    .await?;

    while let Some(message) = read_native_message_async::<_, AppMessage>(reader).await? {
        match message {
            AppMessage::Request { request } => {
                let terminal = matches!(
                    request.body,
                    CarrierRequestBody::Close | CarrierRequestBody::Suspend
                );
                let response = carrier.request(&request).await?;
                let ended = matches!(
                    response.body,
                    Ok(CarrierResponseBody::Closed | CarrierResponseBody::Suspended)
                );
                write_native_message_async(writer, &AppHostMessage::Response { response }).await?;
                if terminal || ended {
                    break;
                }
            }
            AppMessage::Connect { .. } => {
                write_native_message_async(
                    writer,
                    &AppHostMessage::Failure {
                        message: "this application session is already connected".to_string(),
                    },
                )
                .await?;
            }
        }
    }

    drop(carrier);
    let summary = server
        .await
        .map_err(BrowserHostError::from)?
        .map_err(BrowserHostError::from)?;
    Ok(Some(summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The connect schema is this door's, not the browser's. A client that
    /// reached the wrong endpoint is refused at the envelope rather than
    /// admitted into a session it cannot use.
    #[test]
    fn the_app_connect_schema_is_its_own() {
        assert_ne!(APP_CONNECT_SCHEMA, "mere.graphshell/browser-connect/v1");
    }

    /// The wire carries no identity action. This is a shape assertion, not a
    /// policy check: there is no variant to forward, so the door cannot grow
    /// one by accident.
    #[test]
    fn the_application_wire_cannot_carry_an_identity_action() {
        let json = serde_json::to_string(&AppMessage::Connect {
            schema: APP_CONNECT_SCHEMA.to_string(),
            host_nonce: "a".to_string(),
            client_nonce: "b".to_string(),
        })
        .unwrap();
        assert!(json.contains("connect"));
        let identity_action = serde_json::json!({
            "type": "native_identity",
            "request": {"id": 1, "session": "s", "action": {"type": "import_ssh_private"}},
        });
        assert!(
            serde_json::from_value::<AppMessage>(identity_action).is_err(),
            "the application wire must have no identity-action variant",
        );
    }

    /// A hello for an application this device does not admit never reaches
    /// the challenge.
    #[test]
    fn an_unadmitted_application_is_refused_at_the_hello() {
        let hello = AppHello::new(AppId::new("not-ours"));
        let app = hello.accept().unwrap();
        assert!(AllowedApps::default().admit(&app).is_err());
    }
}
