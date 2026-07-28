//! Native-messaging host for the Graphshell browser-extension profile.
//!
//! stdout is protocol-only. Diagnostics go to stderr because a stray byte on
//! stdout corrupts the browser's native-messaging framing.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use graphshell::browser_carrier::{
    AllowedExtensions, BrowserChallenge, BrowserHostMessage, BrowserLink, BrowserMessage,
    admit_browser_session, read_native_message, write_native_message,
};
use graphshell::carrier::projection_policy;
use graphshell::identity::VaultProtectionView;
use graphshell::identity_endpoint::IdentityEndpoint;
use graphshell::lifecycle::SessionAuthority;
use graphshell::native::personae_host::PersonaeHost;
use graphshell::profile::{GraphshellIdentity, default_vault_dir, selected_profile};
use graphshell::session_loop::serve_admitted_session;
use graphshell_protocol::{CarrierRequestBody, CarrierResponseBody, ResumeRequest};
use notochord::{NetworkId, ProfileRef, RevocationLedger, TrustedRoot};
use personae::bootstrap::{self, PASSPHRASE_ENV, Unlock};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use personae::{IdentityProvider, IdentityVault};

use graphshell::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};

const NETWORK_DOMAIN: &[u8] = b"mere.graphshell/local-browser-network/v1";
const ROOT_DOMAIN: &[u8] = b"mere.graphshell/local-browser-root/v1";
const PROFILE_ID: &str = "mere.base";
const SESSION_SECONDS_ENV: &str = "GRAPHSHELL_BROWSER_SESSION_SECONDS";
const EXTRA_EXTENSIONS_ENV: &str = "GRAPHSHELL_EXTENSION_IDS";
const DATA_ROOT_ENV: &str = "GRAPHSHELL_DATA_ROOT";

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("graphshell native host: {error}");
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let launcher = graphshell::browser_carrier::BrowserLauncher::parse(&args)?;
    let allowlist = AllowedExtensions::default()
        .with_additional(&std::env::var(EXTRA_EXTENSIONS_ENV).unwrap_or_default());
    allowlist.admit(&launcher)?;

    let challenge = BrowserChallenge::fresh();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    write_native_message(
        &mut writer,
        &BrowserHostMessage::Challenge {
            challenge: challenge.clone(),
        },
    )?;

    let Some(connect) = read_native_message::<_, BrowserMessage>(&mut reader)? else {
        return Ok(());
    };
    let link = BrowserLink::accept(launcher.clone(), &challenge, connect)?;

    let vault_dir = default_vault_dir();
    let profile_id = selected_profile();
    let identity = GraphshellIdentity::load(&vault_dir, &profile_id)?;
    let opened = bootstrap::open_storage(&vault_dir, Unlock::from_env())?;
    let (profile, _) = bootstrap::load_or_create_profile(&*opened.storage, &profile_id)?;
    let protection = if std::env::var_os(PASSPHRASE_ENV).is_some() {
        VaultProtectionView::Passphrase
    } else {
        VaultProtectionView::OsProtected
    };
    let data_root = std::env::var_os(DATA_ROOT_ENV).map(PathBuf::from);
    let personae = Arc::new(PersonaeHost::new(
        IdentityVault::with_profile(opened.storage, profile),
        data_root,
        protection,
    ));

    let network = local_network(identity.master_public_key().to_bytes());
    let root = local_root(identity.master_public_key().to_bytes());
    let expires_at_ms = now_ms().saturating_add(session_duration_ms());
    let grant = local_browser_grant(&identity, network, root, link.host_nonce, expires_at_ms)?;
    let profile = ProfileRef {
        id: PROFILE_ID.to_string(),
        revision: 1,
    };
    let policy = projection_policy(
        network,
        vec![TrustedRoot {
            authority: root,
            issuer: identity.master_public_key().to_bytes(),
        }],
        vec![profile.clone()],
        Some(1),
    );
    let revocations = RevocationLedger::new();
    let (mut browser, mut admitted) = admit_browser_session(
        &identity,
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
    let connected = BrowserHostMessage::Connected {
        launcher,
        session: authority.session().0.clone(),
        subject: base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(authority.principal().subject),
    };
    let mut endpoint = IdentityEndpoint::for_admitted(Arc::clone(&personae), &authority);
    let server = tokio::spawn(async move {
        let revocations = RwLock::new(revocations);
        let mut resume = |_: &mut IdentityEndpoint<Box<dyn personae::IdentityStorage>>,
                          _: ResumeRequest| {
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
    write_native_message(&mut writer, &connected)?;

    while let Some(message) = read_native_message::<_, BrowserMessage>(&mut reader)? {
        let BrowserMessage::Request { request } = message else {
            write_native_message(
                &mut writer,
                &BrowserHostMessage::Failure {
                    message: "the browser session is already connected".to_string(),
                },
            )?;
            continue;
        };
        let terminal = matches!(
            request.body,
            CarrierRequestBody::Close | CarrierRequestBody::Suspend
        );
        let response = browser.request(&request).await?;
        let ended = matches!(
            response.body,
            Ok(CarrierResponseBody::Closed | CarrierResponseBody::Suspended)
        );
        write_native_message(&mut writer, &BrowserHostMessage::Response { response })?;
        if terminal || ended {
            break;
        }
    }

    drop(browser);
    match server.await {
        Ok(Ok(summary)) => {
            eprintln!(
                "graphshell native host: served {} request(s); ended {:?}",
                summary.answered, summary.end
            );
        }
        Ok(Err(error)) => return Err(error.into()),
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_millis() as u64
}

fn session_duration_ms() -> u64 {
    let seconds = std::env::var(SESSION_SECONDS_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60 * 60)
        .clamp(60, 24 * 60 * 60);
    seconds * 1_000
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

fn local_browser_grant(
    identity: &GraphshellIdentity,
    network: NetworkId,
    root: [u8; 32],
    nonce: [u8; 32],
    expires_at_ms: u64,
) -> Result<SignedDelegationCertificate, personae::IdentityError> {
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
