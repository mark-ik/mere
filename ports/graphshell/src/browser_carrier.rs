// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Browser-extension carrier over WebExtensions native messaging.
//!
//! The browser never receives a Personae private key or signing handle. The
//! installed native host brokers a short-lived, transcript-bound
//! [`notochord::SessionHello`] after the browser has launched it through an
//! explicitly allow-listed extension id.
//!
//! Native messaging is a JSON message transport, not a byte stream. This
//! module therefore keeps its framing at the edge and uses a private duplex
//! stream for the existing Notochord handshake and Graphshell session loop.
//! Application requests cannot enter that stream until the handshake accepts.

use std::collections::{BTreeSet, VecDeque};
use std::io::{Read, Write};

use base64::Engine;
use chirograph::{CarrierNotice, CarrierOutput, CarrierRequest, CarrierResponse};
use notochord::{
    AdmittedSession, CarrierKind, DenyReason, IoHandshakeError, LocalNetworkPolicy, NetworkId,
    ProfileRef, ProofBinding, RevocationLedger, SessionFacts, SessionReply, TrafficClass,
    initiate_session,
};
use personae::IdentityProvider;
use personae::delegation::SignedDelegationCertificate;
use rand_core::{OsRng, RngCore};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, DuplexStream,
};

use crate::admission::{PROJECTION_PROTOCOL, open_session};
use crate::identity_projection::SshUnlockPolicyIntentV1;

/// Native messaging host name shared by the extension and installer.
pub const NATIVE_HOST_NAME: &str = "org.mere.graphshell";
/// Stable development id produced by the public `key` in the Chromium
/// extension manifest.
pub const CHROMIUM_EXTENSION_ID: &str = "oajkkocppbpbmfblepgbiidagliniofd";
/// Stable Firefox add-on id used by the extension and native host manifest.
pub const FIREFOX_EXTENSION_ID: &str = "graphshell@mere.systems";
/// A tighter application bound than Chrome's 64 MiB extension-to-host limit.
pub const MAX_NATIVE_MESSAGE_BYTES: u32 = 1024 * 1024;

const CHALLENGE_SCHEMA: &str = "mere.graphshell/browser-challenge/v1";
const CONNECT_SCHEMA: &str = "mere.graphshell/browser-connect/v1";
const LINK_DOMAIN: &[u8] = b"mere.graphshell/native-messaging-link/v1";

/// Browser family inferred from the arguments supplied by the browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserFamily {
    Chromium,
    Firefox,
}

/// Launcher identity observed at process start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserLauncher {
    pub family: BrowserFamily,
    pub extension_id: String,
}

impl BrowserLauncher {
    /// Parse the platform arguments documented by Chromium and Firefox.
    ///
    /// `args` excludes the executable name. Chromium supplies its extension
    /// origin first. Firefox supplies the native manifest path followed by the
    /// add-on id.
    pub fn parse(args: &[String]) -> Result<Self, BrowserCarrierError> {
        if let Some(origin) = args.first()
            && let Some(extension_id) = origin
                .strip_prefix("chrome-extension://")
                .and_then(|value| value.strip_suffix('/'))
        {
            return validate_extension_id(BrowserFamily::Chromium, extension_id);
        }

        if let Some(extension_id) = args.get(1)
            && !extension_id.trim().is_empty()
        {
            return validate_extension_id(BrowserFamily::Firefox, extension_id);
        }

        Err(BrowserCarrierError::UnknownLauncher)
    }

    pub fn label(&self) -> String {
        match self.family {
            BrowserFamily::Chromium => format!("chrome-extension://{}/", self.extension_id),
            BrowserFamily::Firefox => self.extension_id.clone(),
        }
    }
}

fn validate_extension_id(
    family: BrowserFamily,
    extension_id: &str,
) -> Result<BrowserLauncher, BrowserCarrierError> {
    if extension_id.is_empty()
        || extension_id.len() > 128
        || !extension_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'.' | b'_' | b'-'))
    {
        return Err(BrowserCarrierError::InvalidExtensionId);
    }
    Ok(BrowserLauncher {
        family,
        extension_id: extension_id.to_string(),
    })
}

/// Exact extension allow-list. Wildcards are deliberately not representable.
#[derive(Clone, Debug)]
pub struct AllowedExtensions {
    chromium: BTreeSet<String>,
    firefox: BTreeSet<String>,
}

impl Default for AllowedExtensions {
    fn default() -> Self {
        Self {
            chromium: [CHROMIUM_EXTENSION_ID.to_string()].into_iter().collect(),
            firefox: [FIREFOX_EXTENSION_ID.to_string()].into_iter().collect(),
        }
    }
}

impl AllowedExtensions {
    /// Add comma-separated extension ids to the built-in package ids.
    pub fn with_additional(mut self, csv: &str) -> Self {
        for value in csv
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if value.contains('@') {
                self.firefox.insert(value.to_string());
            } else {
                self.chromium.insert(value.to_string());
            }
        }
        self
    }

    pub fn admit(&self, launcher: &BrowserLauncher) -> Result<(), BrowserCarrierError> {
        let allowed = match launcher.family {
            BrowserFamily::Chromium => self.chromium.contains(&launcher.extension_id),
            BrowserFamily::Firefox => self.firefox.contains(&launcher.extension_id),
        };
        allowed
            .then_some(())
            .ok_or_else(|| BrowserCarrierError::LauncherNotAllowed(launcher.label()))
    }
}

/// First host-to-browser message on every native-messaging process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserChallenge {
    pub schema: String,
    pub host_nonce: String,
}

impl BrowserChallenge {
    pub fn fresh() -> Self {
        let mut nonce = [0u8; 32];
        OsRng.fill_bytes(&mut nonce);
        Self {
            schema: CHALLENGE_SCHEMA.to_string(),
            host_nonce: encode_nonce(nonce),
        }
    }

    fn nonce(&self) -> Result<[u8; 32], BrowserCarrierError> {
        if self.schema != CHALLENGE_SCHEMA {
            return Err(BrowserCarrierError::WrongSchema);
        }
        decode_nonce(&self.host_nonce)
    }
}

/// Browser-to-host messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserMessage {
    Connect {
        schema: String,
        host_nonce: String,
        client_nonce: String,
    },
    Request {
        request: CarrierRequest,
    },
    NativeIdentity {
        request: NativeIdentityRequest,
    },
}

/// One native-only identity interaction. The request can select public
/// options, but it has no field capable of carrying a path, key, or
/// passphrase.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeIdentityRequest {
    pub id: u64,
    pub session: String,
    pub action: NativeIdentityAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NativeIdentityAction {
    ImportSshPrivate {
        unlock_policy: SshUnlockPolicyIntentV1,
    },
}

/// Public result of a native-only identity interaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NativeIdentityResult {
    ImportedSshPrivate {
        fingerprint: String,
        comment: String,
        unlock_policy: String,
        replaced_existing: bool,
    },
    Cancelled,
    Rejected {
        reason: NativeIdentityFailure,
    },
}

/// Bounded public failure vocabulary. Local paths, key bytes, passphrases,
/// and parser diagnostics never become native-messaging responses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeIdentityFailure {
    WrongSession,
    UiUnavailable,
    SelectedFileUnreadable,
    SelectedFileTooLarge,
    InvalidPrivateKey,
    IncorrectPassphrase,
    ImportRejected,
}

/// Host-to-browser messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserHostMessage {
    Challenge {
        challenge: BrowserChallenge,
    },
    Connected {
        launcher: BrowserLauncher,
        session: String,
        subject: String,
    },
    Response {
        response: CarrierResponse,
    },
    NativeIdentityResult {
        id: u64,
        result: NativeIdentityResult,
    },
    Failure {
        message: String,
    },
}

/// A checked connect answer and its transcript link, for any local client.
///
/// The label is bound into the transcript, so a link minted for one client
/// cannot be replayed as another even on the same challenge. What the label
/// says is the caller's business: an extension origin for a browser, an
/// application name for a first-party app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalLink {
    pub label: String,
    pub host_nonce: [u8; 32],
    pub client_nonce: [u8; 32],
    pub shared_link: [u8; 16],
}

impl LocalLink {
    /// Check a connect answer against this host process and bind `label`.
    pub fn accept(
        label: String,
        challenge: &BrowserChallenge,
        message: BrowserMessage,
    ) -> Result<Self, BrowserCarrierError> {
        let BrowserMessage::Connect {
            schema,
            host_nonce,
            client_nonce,
        } = message
        else {
            return Err(BrowserCarrierError::ConnectRequired);
        };
        if schema != CONNECT_SCHEMA {
            return Err(BrowserCarrierError::WrongSchema);
        }
        Self::bind(label, challenge, &host_nonce, &client_nonce)
    }

    /// Bind a client label and its answered nonces into a transcript link.
    ///
    /// Separate from [`LocalLink::accept`] so a door with its own message
    /// vocabulary can bind the same way without having to speak the browser
    /// connect frame.
    pub fn bind(
        label: String,
        challenge: &BrowserChallenge,
        host_nonce: &str,
        client_nonce: &str,
    ) -> Result<Self, BrowserCarrierError> {
        let expected = challenge.nonce()?;
        let echoed = decode_nonce(host_nonce)?;
        if echoed != expected {
            return Err(BrowserCarrierError::ChallengeMismatch);
        }
        let client_nonce = decode_nonce(client_nonce)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(LINK_DOMAIN);
        hasher.update(&(label.len() as u64).to_le_bytes());
        hasher.update(label.as_bytes());
        hasher.update(&expected);
        hasher.update(&client_nonce);
        let mut shared_link = [0u8; 16];
        shared_link.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
        Ok(Self {
            label,
            host_nonce: expected,
            client_nonce,
            shared_link,
        })
    }
}

/// A checked connection request and its transcript link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserLink {
    pub launcher: BrowserLauncher,
    pub host_nonce: [u8; 32],
    pub client_nonce: [u8; 32],
    pub shared_link: [u8; 16],
}

impl BrowserLink {
    pub fn accept(
        launcher: BrowserLauncher,
        challenge: &BrowserChallenge,
        message: BrowserMessage,
    ) -> Result<Self, BrowserCarrierError> {
        let link = LocalLink::accept(launcher.label(), challenge, message)?;
        Ok(Self {
            launcher,
            host_nonce: link.host_nonce,
            client_nonce: link.client_nonce,
            shared_link: link.shared_link,
        })
    }

    /// This link as the client-agnostic form admission takes.
    pub fn as_local(&self) -> LocalLink {
        LocalLink {
            label: self.launcher.label(),
            host_nonce: self.host_nonce,
            client_nonce: self.client_nonce,
            shared_link: self.shared_link,
        }
    }
}

/// Errors before or during a browser carrier session.
#[derive(Debug, thiserror::Error)]
pub enum BrowserCarrierError {
    #[error("native messaging launcher arguments do not identify a supported browser")]
    UnknownLauncher,
    #[error("native messaging launcher supplied an invalid extension id")]
    InvalidExtensionId,
    #[error("extension is not in Graphshell's exact allow-list: {0}")]
    LauncherNotAllowed(String),
    #[error("the first browser message must answer the host challenge")]
    ConnectRequired,
    #[error("browser carrier message uses another schema")]
    WrongSchema,
    #[error("browser carrier challenge does not match this host process")]
    ChallengeMismatch,
    #[error("browser carrier nonce is not a 32-byte base64url value")]
    InvalidNonce,
    #[error("native messaging frame of {len} bytes exceeds the {max} byte bound")]
    FrameTooLarge { len: u64, max: u32 },
    #[error("native messaging stream failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("native messaging JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("browser session handshake failed: {0}")]
    Handshake(#[from] IoHandshakeError),
    #[error("browser session was refused: {0}")]
    Refused(DenyReason),
    #[error("the two halves of browser session admission disagreed")]
    AdmissionMismatch,
    #[error("browser application stream ended before its response")]
    ApplicationEnded,
    #[error("browser application stream sent an unexpected carrier output: {0}")]
    UnexpectedApplicationOutput(String),
}

/// Read one browser native-messaging JSON frame.
///
/// Native messaging uses a native-endian 32-bit byte length. End-of-stream
/// between messages is normal process completion; a partial prefix or body is
/// an error.
pub fn read_native_message<R, T>(reader: &mut R) -> Result<Option<T>, BrowserCarrierError>
where
    R: Read,
    T: DeserializeOwned,
{
    let mut length = [0u8; 4];
    let mut read = 0usize;
    while read < length.len() {
        match reader.read(&mut length[read..])? {
            0 if read == 0 => return Ok(None),
            0 => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "native messaging length prefix was truncated",
                )
                .into());
            }
            count => read += count,
        }
    }
    let length = u32::from_ne_bytes(length);
    if length > MAX_NATIVE_MESSAGE_BYTES {
        return Err(BrowserCarrierError::FrameTooLarge {
            len: u64::from(length),
            max: MAX_NATIVE_MESSAGE_BYTES,
        });
    }
    let mut body = vec![0u8; length as usize];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

/// Write one browser native-messaging JSON frame.
pub fn write_native_message<W, T>(writer: &mut W, message: &T) -> Result<(), BrowserCarrierError>
where
    W: Write,
    T: Serialize,
{
    let body = serde_json::to_vec(message)?;
    let length = u32::try_from(body.len()).map_err(|_| BrowserCarrierError::FrameTooLarge {
        len: body.len() as u64,
        max: MAX_NATIVE_MESSAGE_BYTES,
    })?;
    if length > MAX_NATIVE_MESSAGE_BYTES {
        return Err(BrowserCarrierError::FrameTooLarge {
            len: u64::from(length),
            max: MAX_NATIVE_MESSAGE_BYTES,
        });
    }
    writer.write_all(&length.to_ne_bytes())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

/// Read one browser native-messaging JSON frame from an asynchronous stream.
///
/// The resident device host uses this variant for its local browser broker.
/// It has the same size and truncation behavior as [`read_native_message`].
pub async fn read_native_message_async<R, T>(
    reader: &mut R,
) -> Result<Option<T>, BrowserCarrierError>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut length = [0u8; 4];
    let mut read = 0usize;
    while read < length.len() {
        match AsyncReadExt::read(reader, &mut length[read..]).await? {
            0 if read == 0 => return Ok(None),
            0 => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "native messaging length prefix was truncated",
                )
                .into());
            }
            count => read += count,
        }
    }
    let length = u32::from_ne_bytes(length);
    if length > MAX_NATIVE_MESSAGE_BYTES {
        return Err(BrowserCarrierError::FrameTooLarge {
            len: u64::from(length),
            max: MAX_NATIVE_MESSAGE_BYTES,
        });
    }
    let mut body = vec![0u8; length as usize];
    AsyncReadExt::read_exact(reader, &mut body).await?;
    Ok(Some(serde_json::from_slice(&body)?))
}

/// Write one browser native-messaging JSON frame to an asynchronous stream.
pub async fn write_native_message_async<W, T>(
    writer: &mut W,
    message: &T,
) -> Result<(), BrowserCarrierError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let body = serde_json::to_vec(message)?;
    let length = u32::try_from(body.len()).map_err(|_| BrowserCarrierError::FrameTooLarge {
        len: body.len() as u64,
        max: MAX_NATIVE_MESSAGE_BYTES,
    })?;
    if length > MAX_NATIVE_MESSAGE_BYTES {
        return Err(BrowserCarrierError::FrameTooLarge {
            len: u64::from(length),
            max: MAX_NATIVE_MESSAGE_BYTES,
        });
    }
    AsyncWriteExt::write_all(writer, &length.to_ne_bytes()).await?;
    AsyncWriteExt::write_all(writer, &body).await?;
    AsyncWriteExt::flush(writer).await?;
    Ok(())
}

/// Run the existing SessionHello admission over a private stream whose link
/// id was derived from this browser launch.
///
/// The native host acts as a narrowly-scoped identity broker for the
/// extension. This is not transport authentication: `SessionFacts` therefore
/// reports `CarrierKind::Other` with no authenticated initiator. The fresh
/// shared link still pins the signed hello to this process and challenge.
pub async fn admit_browser_session<P: IdentityProvider>(
    identity: &P,
    network: NetworkId,
    profile: ProfileRef,
    delegations: Vec<SignedDelegationCertificate>,
    link: &BrowserLink,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    now_ms: u64,
) -> Result<(BrowserSessionClient, AdmittedSession<DuplexStream>), BrowserCarrierError> {
    admit_local_session(
        identity,
        network,
        profile,
        delegations,
        &link.as_local(),
        policy,
        ledger,
        now_ms,
    )
    .await
}

/// Admit one local client over an in-process duplex.
///
/// The browser path and the first-party path differ in who is on the far end
/// and what the transcript binds, not in how a session is admitted, so both
/// arrive here.
#[allow(clippy::too_many_arguments)]
pub async fn admit_local_session<P: IdentityProvider>(
    identity: &P,
    network: NetworkId,
    profile: ProfileRef,
    delegations: Vec<SignedDelegationCertificate>,
    link: &LocalLink,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    now_ms: u64,
) -> Result<(BrowserSessionClient, AdmittedSession<DuplexStream>), BrowserCarrierError> {
    let (mut client, server) = tokio::io::duplex(256 * 1024);
    let binding = ProofBinding::initiator(PROJECTION_PROTOCOL, None, Some(link.shared_link));
    let hello = open_session(
        identity,
        network,
        profile,
        TrafficClass::Interactive,
        link.client_nonce,
        &binding,
        delegations,
    )
    .map_err(|error| BrowserCarrierError::Handshake(IoHandshakeError::Frame(error)))?;
    let facts = SessionFacts::new(PROJECTION_PROTOCOL, CarrierKind::Other)
        .with_ingress(None, Some(link.shared_link));
    let limits = policy.limits.clamped();

    let initiator = async { initiate_session(&mut client, &hello, &limits).await };
    let responder =
        async { notochord::admit_session(server, policy, ledger, &facts, now_ms, 0).await };
    let (reply, admitted) = tokio::join!(initiator, responder);
    let reply = reply?;
    let admitted = match admitted? {
        Ok(admitted) => admitted,
        Err(reason) => return Err(BrowserCarrierError::Refused(reason)),
    };
    let SessionReply::Accept { session_id, .. } = reply else {
        let SessionReply::Reject { reason } = reply else {
            unreachable!()
        };
        return Err(BrowserCarrierError::Refused(reason));
    };
    if session_id != admitted.principal.session_id {
        return Err(BrowserCarrierError::AdmissionMismatch);
    }
    Ok((BrowserSessionClient::new(client), admitted))
}

/// Browser-side end of an admitted private application stream.
pub struct BrowserSessionClient {
    reader: tokio::io::Lines<BufReader<tokio::io::ReadHalf<DuplexStream>>>,
    writer: tokio::io::WriteHalf<DuplexStream>,
    notices: VecDeque<CarrierNotice>,
}

impl BrowserSessionClient {
    fn new(stream: DuplexStream) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        Self {
            reader: BufReader::new(reader).lines(),
            writer,
            notices: VecDeque::new(),
        }
    }

    pub async fn request(
        &mut self,
        request: &CarrierRequest,
    ) -> Result<CarrierResponse, BrowserCarrierError> {
        let mut line = serde_json::to_vec(request)?;
        line.push(b'\n');
        self.writer.write_all(&line).await?;
        self.writer.flush().await?;
        loop {
            match self.read_output().await? {
                CarrierOutput::Notice(notice) => self.notices.push_back(notice),
                CarrierOutput::Response(response) if response.id == request.id => {
                    return Ok(response);
                }
                CarrierOutput::Response(response) => {
                    return Err(BrowserCarrierError::UnexpectedApplicationOutput(format!(
                        "response {} arrived while waiting for {}",
                        response.id, request.id
                    )));
                }
            }
        }
    }

    pub fn take_notice(&mut self) -> Option<CarrierNotice> {
        self.notices.pop_front()
    }

    pub async fn wait_for_notice(&mut self) -> Result<CarrierNotice, BrowserCarrierError> {
        if let Some(notice) = self.take_notice() {
            return Ok(notice);
        }
        match self.read_output().await? {
            CarrierOutput::Notice(notice) => Ok(notice),
            CarrierOutput::Response(response) => {
                Err(BrowserCarrierError::UnexpectedApplicationOutput(format!(
                    "response {} arrived while waiting for a notice",
                    response.id
                )))
            }
        }
    }

    async fn read_output(&mut self) -> Result<CarrierOutput, BrowserCarrierError> {
        let line = self
            .reader
            .next_line()
            .await?
            .ok_or(BrowserCarrierError::ApplicationEnded)?;
        Ok(serde_json::from_str(&line)?)
    }
}

fn encode_nonce(nonce: [u8; 32]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce)
}

fn decode_nonce(value: &str) -> Result<[u8; 32], BrowserCarrierError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| BrowserCarrierError::InvalidNonce)?;
    bytes
        .try_into()
        .map_err(|_| BrowserCarrierError::InvalidNonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
    use notochord::TrustedRoot;
    use notochord::{HandshakeLimits, ServiceAccess, ServiceRule};
    use personae::InMemoryProvider;
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
    };
    use std::io::Cursor;

    #[tokio::test]
    async fn admitted_client_queues_a_notice_that_precedes_its_response() {
        let (client_stream, server_stream) = tokio::io::duplex(4096);
        let mut client = BrowserSessionClient::new(client_stream);
        let notice = chirograph::CarrierNotice {
            session: chirograph::ProjectionSession("resident:knot".into()),
            epoch: chirograph::SceneEpoch(3),
            revision: chirograph::Revision(7),
        };
        let expected = notice.clone();
        let server = tokio::spawn(async move {
            let (reader, mut writer) = tokio::io::split(server_stream);
            let mut lines = BufReader::new(reader).lines();
            let request: CarrierRequest =
                serde_json::from_str(&lines.next_line().await.unwrap().expect("one request"))
                    .unwrap();
            for output in [
                CarrierOutput::Notice(notice),
                CarrierOutput::Response(CarrierResponse {
                    id: request.id,
                    body: Ok(chirograph::CarrierResponseBody::Closed),
                }),
            ] {
                let mut line = serde_json::to_vec(&output).unwrap();
                line.push(b'\n');
                writer.write_all(&line).await.unwrap();
            }
            writer.flush().await.unwrap();
        });

        let response = client
            .request(&CarrierRequest {
                id: 41,
                body: chirograph::CarrierRequestBody::Close,
            })
            .await
            .unwrap();
        assert!(matches!(
            response.body,
            Ok(chirograph::CarrierResponseBody::Closed)
        ));
        assert_eq!(client.take_notice(), Some(expected));
        server.await.unwrap();
    }

    const NETWORK: NetworkId = NetworkId([0x41; 32]);
    const ROOT: [u8; 32] = [0x42; 32];

    fn profile() -> ProfileRef {
        ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        }
    }

    fn link(client: [u8; 32]) -> BrowserLink {
        let launcher =
            BrowserLauncher::parse(&[format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")])
                .unwrap();
        let challenge = BrowserChallenge {
            schema: CHALLENGE_SCHEMA.into(),
            host_nonce: encode_nonce([3; 32]),
        };
        BrowserLink::accept(
            launcher,
            &challenge,
            BrowserMessage::Connect {
                schema: CONNECT_SCHEMA.into(),
                host_nonce: challenge.host_nonce.clone(),
                client_nonce: encode_nonce(client),
            },
        )
        .unwrap()
    }

    fn grant(owner: &InMemoryProvider, subject: [u8; 32]) -> SignedDelegationCertificate {
        SignedDelegationCertificate::issue(
            owner,
            DelegationCertificate::new(
                DelegationParent::Root(ROOT),
                owner.master_public_key().to_bytes(),
                subject,
                CapabilityScope {
                    domain: GRAPHSHELL_DOMAIN.into(),
                    resource: NETWORK.0.to_vec(),
                    path_prefix: PROJECTION_SERVICE.into(),
                    actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
                },
                1,
                1,
                Some(100),
                1,
                [4; 32],
            ),
        )
        .unwrap()
    }

    fn policy(owner: &InMemoryProvider) -> LocalNetworkPolicy {
        let mut policy = LocalNetworkPolicy::closed(NETWORK);
        policy.trusted_roots = vec![TrustedRoot {
            authority: ROOT,
            issuer: owner.master_public_key().to_bytes(),
        }];
        policy.accepted_profiles = vec![profile()];
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
        policy
    }

    #[test]
    fn native_messaging_frames_json_with_native_endian_length() {
        let message = BrowserHostMessage::Failure {
            message: "bounded".into(),
        };
        let mut bytes = Vec::new();
        write_native_message(&mut bytes, &message).unwrap();
        let declared = u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert_eq!(declared, bytes.len() - 4);
        let decoded: BrowserHostMessage = read_native_message(&mut Cursor::new(bytes))
            .unwrap()
            .unwrap();
        assert_eq!(decoded, message);
    }

    #[tokio::test]
    async fn asynchronous_native_framing_matches_the_browser_wire() {
        let expected = BrowserHostMessage::Failure {
            message: "resident".into(),
        };
        let (mut writer, mut reader) = tokio::io::duplex(1024);
        let message = expected.clone();
        let sending = tokio::spawn(async move {
            write_native_message_async(&mut writer, &message)
                .await
                .unwrap();
        });
        let decoded: BrowserHostMessage = read_native_message_async(&mut reader)
            .await
            .unwrap()
            .unwrap();
        sending.await.unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn native_ssh_import_message_has_no_secret_bearing_fields() {
        let message = BrowserMessage::NativeIdentity {
            request: NativeIdentityRequest {
                id: 17,
                session: "receipt-session".into(),
                action: NativeIdentityAction::ImportSshPrivate {
                    unlock_policy: SshUnlockPolicyIntentV1::ShortTtl { idle_seconds: 90 },
                },
            },
        };
        let encoded = serde_json::to_string(&message).unwrap();

        assert!(encoded.contains("\"type\":\"native_identity\""));
        assert!(encoded.contains("\"idle_seconds\":90"));
        for forbidden in ["\"path\"", "\"passphrase\"", "\"key_bytes\"", "\"payload\""] {
            assert!(
                !encoded.contains(forbidden),
                "{forbidden} leaked into {encoded}"
            );
        }

        let decoded: BrowserMessage = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn launcher_allowlist_is_exact() {
        let allowed =
            BrowserLauncher::parse(&[format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")])
                .unwrap();
        AllowedExtensions::default().admit(&allowed).unwrap();

        let other =
            BrowserLauncher::parse(
                &["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/".into()],
            )
            .unwrap();
        assert!(AllowedExtensions::default().admit(&other).is_err());
    }

    #[test]
    fn challenge_replay_changes_the_transcript_link() {
        assert_ne!(link([7; 32]).shared_link, link([8; 32]).shared_link);

        let challenge = BrowserChallenge {
            schema: CHALLENGE_SCHEMA.into(),
            host_nonce: encode_nonce([3; 32]),
        };
        let wrong = BrowserLink::accept(
            BrowserLauncher::parse(&[format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")])
                .unwrap(),
            &challenge,
            BrowserMessage::Connect {
                schema: CONNECT_SCHEMA.into(),
                host_nonce: encode_nonce([9; 32]),
                client_nonce: encode_nonce([7; 32]),
            },
        );
        assert!(matches!(wrong, Err(BrowserCarrierError::ChallengeMismatch)));
    }

    #[tokio::test]
    async fn browser_application_stream_exists_only_after_sessionhello_accepts() {
        let owner = InMemoryProvider::from_seed([1; 32]);
        let viewer = InMemoryProvider::from_seed([2; 32]);
        let (_, admitted) = admit_browser_session(
            &viewer,
            NETWORK,
            profile(),
            vec![grant(&owner, viewer.master_public_key().to_bytes())],
            &link([7; 32]),
            &policy(&owner),
            &RevocationLedger::new(),
            10,
        )
        .await
        .unwrap();
        assert_eq!(
            admitted.principal.subject,
            viewer.master_public_key().to_bytes()
        );
        assert_eq!(admitted.facts.transport, CarrierKind::Other);
        assert_eq!(
            admitted.facts.ingress.shared_link,
            Some(link([7; 32]).shared_link)
        );
    }

    #[tokio::test]
    async fn a_captured_brokered_hello_cannot_cross_browser_challenges() {
        let viewer = InMemoryProvider::from_seed([2; 32]);
        let first = link([7; 32]);
        let second = link([8; 32]);
        let hello = open_session(
            &viewer,
            NETWORK,
            profile(),
            TrafficClass::Interactive,
            first.client_nonce,
            &ProofBinding::initiator(PROJECTION_PROTOCOL, None, Some(first.shared_link)),
            Vec::new(),
        )
        .unwrap()
        .encode(&HandshakeLimits::default())
        .unwrap();
        let facts = SessionFacts::new(PROJECTION_PROTOCOL, CarrierKind::Other)
            .with_ingress(None, Some(second.shared_link));
        let (_, outcome) = crate::admission::admit_session(
            &LocalNetworkPolicy::closed(NETWORK),
            &RevocationLedger::new(),
            &hello,
            &facts,
            10,
            0,
        );
        assert!(outcome.is_err());
    }
}
