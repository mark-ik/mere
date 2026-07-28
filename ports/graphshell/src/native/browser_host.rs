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
use crate::native::identity_ui::{NativeIdentityUi, apply_native_identity_action};
use crate::native::personae_host::PersonaeHost;
use crate::session_loop::{SessionLoopError, SessionSummary, serve_admitted_session};

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
    let session = authority.session().0.clone();
    let connected = BrowserHostMessage::Connected {
        launcher,
        session: session.clone(),
        subject: base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(authority.principal().subject),
    };
    let mut endpoint = IdentityEndpoint::for_admitted(Arc::clone(&personae), &authority);
    let server = tokio::spawn(async move {
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
    });
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
