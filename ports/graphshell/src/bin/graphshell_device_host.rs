//! Resident Graphshell authority.
//!
//! One process owns the selected Personae profile, OpenSSH agent endpoint, and
//! private browser-session broker. The browser-launched executable is only a
//! relay into this process.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use graphshell::browser_carrier::AllowedExtensions;
use graphshell::identity::VaultProtectionView;
#[cfg(feature = "personal-sync")]
use graphshell::native::device_broker::{DeviceSupplementalCards, serve_browser_broker_with_cards};
use graphshell::native::device_broker::{configured_device_endpoint, serve_browser_broker};
use graphshell::native::identity_ui::SystemNativeIdentityUi;
#[cfg(feature = "personal-sync")]
use graphshell::native::owner_settings::{self, DataRootMigration, OwnerSettings, SyncOverrides};
use graphshell::native::personae_host::PersonaeHost;
#[cfg(windows)]
use graphshell::native::personae_host::STANDARD_WINDOWS_AGENT_ENDPOINT;
#[cfg(feature = "personal-sync")]
use graphshell::native::personal_sync_host::{PersonalSyncHost, PersonalSyncHostConfig};
#[cfg(feature = "personal-sync")]
use graphshell::personal_sync::{SyncRoster, SyncSelection};
use graphshell::profile::{default_vault_dir, selected_profile};
#[cfg(feature = "personal-sync")]
use personae::IdentityProvider;
use personae::bootstrap::{self, PASSPHRASE_ENV, Unlock};
use personae::{IdentityVault, ProfileId};
use ssh_agent_lib::agent::listen;

const EXTRA_EXTENSIONS_ENV: &str = "GRAPHSHELL_EXTENSION_IDS";
const DATA_ROOT_ENV: &str = "GRAPHSHELL_DATA_ROOT";
const SESSION_SECONDS_ENV: &str = "GRAPHSHELL_BROWSER_SESSION_SECONDS";
#[cfg(feature = "personal-sync")]
const PERSONAL_GRAPH_DOMAIN: &[u8] = b"mere.graphshell/personal-graph/v1";

enum AgentEndpoint {
    Standard(String),
    Receipt(String),
}

struct Args {
    vault_dir: PathBuf,
    profile: ProfileId,
    agent: AgentEndpoint,
    browser_endpoint: String,
    data_root: Option<PathBuf>,
    log_file: Option<PathBuf>,
    /// Command-line overrides folded over the profile's stored settings.
    #[cfg(feature = "personal-sync")]
    sync_overrides: SyncOverrides,
    /// Peer tickets stay arguments rather than settings: a ticket is only
    /// valid until that peer rebinds, so storing one would go stale.
    #[cfg(feature = "personal-sync")]
    sync_peer_tickets: Vec<String>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    if let Err(error) = init_logging(args.log_file.as_deref()) {
        eprintln!("graphshell device host: initialize logging: {error}");
        std::process::exit(1);
    }
    if let Err(error) = run(args).await {
        tracing::error!(%error, "device host stopped");
        std::process::exit(1);
    }
}

fn parse_args() -> Result<Args, String> {
    let mut vault_dir = default_vault_dir();
    let mut profile = selected_profile();
    let mut agent_endpoint = None;
    let mut receipt_agent_endpoint = None;
    let mut browser_endpoint = configured_device_endpoint();
    let mut data_root = std::env::var_os(DATA_ROOT_ENV).map(PathBuf::from);
    let mut log_file = None;
    #[cfg(feature = "personal-sync")]
    let mut sync_graph = None;
    #[cfg(feature = "personal-sync")]
    let mut sync_store = None;
    #[cfg(feature = "personal-sync")]
    let mut sync_roots = Vec::new();
    #[cfg(feature = "personal-sync")]
    let mut sync_peers = Vec::new();
    #[cfg(feature = "personal-sync")]
    let mut sync_peer_nodes = Vec::new();
    #[cfg(feature = "personal-sync")]
    let mut sync_facets = Vec::new();
    #[cfg(feature = "personal-sync")]
    let mut sync_access = false;
    #[cfg(feature = "personal-sync")]
    let mut sync_scenes = false;
    #[cfg(feature = "personal-sync")]
    let mut sync_handlers = false;
    #[cfg(feature = "personal-sync")]
    let mut sync_blobs = false;
    let mut argv = std::env::args().skip(1);

    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--dir" => {
                vault_dir = PathBuf::from(argv.next().ok_or("--dir needs a value")?);
            }
            "--profile" => {
                profile = ProfileId(argv.next().ok_or("--profile needs a value")?);
            }
            "--agent-endpoint" => {
                agent_endpoint = Some(argv.next().ok_or("--agent-endpoint needs a value")?);
            }
            "--receipt-agent-endpoint" => {
                receipt_agent_endpoint = Some(
                    argv.next()
                        .ok_or("--receipt-agent-endpoint needs a value")?,
                );
            }
            "--browser-endpoint" => {
                browser_endpoint = argv.next().ok_or("--browser-endpoint needs a value")?;
            }
            "--data-root" => {
                data_root = Some(PathBuf::from(
                    argv.next().ok_or("--data-root needs a value")?,
                ));
            }
            "--log-file" => {
                log_file = Some(PathBuf::from(
                    argv.next().ok_or("--log-file needs a value")?,
                ));
            }
            #[cfg(feature = "personal-sync")]
            "--sync-graph" => {
                sync_graph = Some(argv.next().ok_or("--sync-graph needs a value")?);
            }
            #[cfg(feature = "personal-sync")]
            "--sync-store" => {
                sync_store = Some(PathBuf::from(
                    argv.next().ok_or("--sync-store needs a value")?,
                ));
            }
            #[cfg(feature = "personal-sync")]
            "--sync-root" => {
                let value = argv.next().ok_or("--sync-root needs a value")?;
                owner_settings::parse_hex32(&value).map_err(|error| error.to_string())?;
                sync_roots.push(value);
            }
            #[cfg(feature = "personal-sync")]
            "--sync-peer" => {
                sync_peers.push(argv.next().ok_or("--sync-peer needs a value")?);
            }
            // Prefer this over --sync-peer: a ticket embeds the peer's current
            // address and is rebuilt on every bind, so a stored one is stale
            // after that device restarts. A node id is stable.
            #[cfg(feature = "personal-sync")]
            "--sync-peer-node" => {
                let value = argv.next().ok_or("--sync-peer-node needs a value")?;
                owner_settings::parse_hex32(&value).map_err(|error| error.to_string())?;
                sync_peer_nodes.push(value);
            }
            #[cfg(feature = "personal-sync")]
            "--sync-facet" => {
                sync_facets.push(argv.next().ok_or("--sync-facet needs a value")?);
            }
            #[cfg(feature = "personal-sync")]
            "--sync-access" => sync_access = true,
            #[cfg(feature = "personal-sync")]
            "--sync-scenes" => sync_scenes = true,
            #[cfg(feature = "personal-sync")]
            "--sync-handlers" => sync_handlers = true,
            #[cfg(feature = "personal-sync")]
            "--sync-blobs" => sync_blobs = true,
            "--help" | "-h" => {
                return Err(usage().to_string());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if agent_endpoint.is_some() && receipt_agent_endpoint.is_some() {
        return Err(
            "--agent-endpoint and --receipt-agent-endpoint are mutually exclusive".to_string(),
        );
    }
    let agent = match receipt_agent_endpoint {
        Some(endpoint) => AgentEndpoint::Receipt(endpoint),
        None => AgentEndpoint::Standard(
            agent_endpoint.unwrap_or_else(|| default_agent_endpoint(&vault_dir)),
        ),
    };
    Ok(Args {
        vault_dir,
        profile,
        agent,
        browser_endpoint,
        data_root,
        log_file,
        #[cfg(feature = "personal-sync")]
        sync_overrides: SyncOverrides {
            graph: sync_graph,
            store_path: sync_store,
            roster_roots: sync_roots,
            paired_nodes: sync_peer_nodes,
            facets: sync_facets,
            access_records: sync_access,
            saved_scenes: sync_scenes,
            handler_preferences: sync_handlers,
            blob_availability: sync_blobs,
        },
        #[cfg(feature = "personal-sync")]
        sync_peer_tickets: sync_peers,
    })
}

#[cfg(feature = "personal-sync")]
fn usage() -> &'static str {
    "usage: graphshell_device_host [--dir <vault-dir>] [--profile <name>] \
     [--agent-endpoint <standard-endpoint>] \
     [--browser-endpoint <private-endpoint>] [--data-root <dir>] \
     [--log-file <path>]\n\
     personal sync: [--sync-graph <name>] [--sync-store <path>] \
     [--sync-root <64-hex-public-root>] [--sync-peer-node <64-hex-node-id>] \
     [--sync-peer <ticket>] \
     [--sync-facet <id>] [--sync-access] [--sync-scenes] \
     [--sync-handlers] [--sync-blobs]\n\
     receipt only: --receipt-agent-endpoint <isolated-endpoint>"
}

#[cfg(not(feature = "personal-sync"))]
fn usage() -> &'static str {
    "usage: graphshell_device_host [--dir <vault-dir>] [--profile <name>] \
     [--agent-endpoint <standard-endpoint>] \
     [--browser-endpoint <private-endpoint>] [--data-root <dir>] \
     [--log-file <path>]\n\
     receipt only: --receipt-agent-endpoint <isolated-endpoint>"
}

fn default_agent_endpoint(vault_dir: &Path) -> String {
    #[cfg(windows)]
    {
        let _ = vault_dir;
        STANDARD_WINDOWS_AGENT_ENDPOINT.to_string()
    }
    #[cfg(not(windows))]
    {
        std::env::var("SSH_AUTH_SOCK")
            .ok()
            .filter(|endpoint| !endpoint.trim().is_empty())
            .unwrap_or_else(|| {
                vault_dir
                    .join("graphshell-agent.sock")
                    .display()
                    .to_string()
            })
    }
}

fn init_logging(path: Option<&Path>) -> Result<(), std::io::Error> {
    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(file))
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }
    Ok(())
}

async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let opened = bootstrap::open_storage(&args.vault_dir, Unlock::from_env())?;
    tracing::info!(storage = %opened.description, "Personae storage open");
    let (profile, created) = bootstrap::load_or_create_profile(&*opened.storage, &args.profile)?;
    if created {
        tracing::warn!(profile = %args.profile.0, "selected profile was created");
    }
    let protection = if std::env::var_os(PASSPHRASE_ENV).is_some() {
        VaultProtectionView::Passphrase
    } else {
        VaultProtectionView::OsProtected
    };
    let personae = Arc::new(PersonaeHost::new(
        IdentityVault::with_profile(opened.storage, profile),
        args.data_root.clone(),
        protection,
    ));

    #[cfg(not(windows))]
    prepare_unix_agent_endpoint(match &args.agent {
        AgentEndpoint::Standard(endpoint) | AgentEndpoint::Receipt(endpoint) => endpoint,
    })
    .await?;

    let agent_listener = match &args.agent {
        AgentEndpoint::Standard(endpoint) => personae.bind_standard_listener(endpoint)?,
        AgentEndpoint::Receipt(endpoint) => personae.bind_receipt_listener(endpoint)?,
    };
    let agent_endpoint = match &args.agent {
        AgentEndpoint::Standard(endpoint) | AgentEndpoint::Receipt(endpoint) => endpoint,
    };
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(agent_endpoint, std::fs::Permissions::from_mode(0o600))?;
    }
    tracing::info!(endpoint = agent_endpoint, "SSH agent listening");

    let allowlist = AllowedExtensions::default()
        .with_additional(&std::env::var(EXTRA_EXTENSIONS_ENV).unwrap_or_default());
    let agent = listen(agent_listener, personae.agent_session());
    #[cfg(feature = "personal-sync")]
    let supplemental_cards = {
        let app_dir = owner_settings::default_app_dir();
        let settings_file = owner_settings::settings_path(&app_dir, &args.profile);
        let stored = OwnerSettings::load(&settings_file)?;
        tracing::info!(
            path = %settings_file.display(),
            configured = stored.sync.is_some(),
            "owner settings"
        );
        let resolved = owner_settings::resolve_sync(stored.sync, args.sync_overrides);
        if let Some(sync) = resolved {
            let graph = personal_graph_id(&sync.graph);
            // An explicit --data-root or GRAPHSHELL_DATA_ROOT is the owner
            // naming a location, so leave it alone. Otherwise take the default
            // and bring any store still sitting inside the Personae vault with
            // it, once.
            let data_root = match args.data_root.clone() {
                Some(explicit) => explicit,
                None => {
                    let current = owner_settings::default_data_root(&app_dir);
                    let legacy = owner_settings::legacy_data_root(&args.vault_dir);
                    if let DataRootMigration::Moved { from, to } =
                        owner_settings::migrate_data_root(&legacy, &current)?
                    {
                        tracing::info!(
                            from = %from.display(),
                            to = %to.display(),
                            "moved the Graphshell data root out of the Personae vault"
                        );
                    }
                    current
                }
            };
            let store_path = sync.store_path.clone().unwrap_or_else(|| {
                data_root
                    .join("personal-sync")
                    .join(format!("{}.redb", hex(&graph)))
            });
            let mut roots = sync.roster_root_keys()?;
            roots.push(personae.master_public_key().to_bytes());
            roots.sort_unstable();
            roots.dedup();
            let paired_nodes = sync.paired_node_keys()?;
            let selection = SyncSelection::default()
                .with_facets(sync.lanes.facets.clone())
                .with_access_records(sync.lanes.access_records)
                .with_saved_scenes(sync.lanes.saved_scenes)
                .with_handler_preferences(sync.lanes.handler_preferences)
                .with_blob_availability(sync.lanes.blob_availability);
            let personal_sync = Arc::new(
                PersonalSyncHost::open(
                    personae.as_ref(),
                    PersonalSyncHostConfig {
                        graph,
                        store_path,
                        roster: SyncRoster::new(roots),
                        selection,
                        peer_tickets: args.sync_peer_tickets,
                        paired_nodes,
                    },
                )
                .await?,
            );
            // node_id is the durable half of this line: a peer pairs with it
            // once and it survives restarts. The ticket is logged too because
            // it still bootstraps across networks, where mDNS cannot reach.
            tracing::info!(
                graph = %hex(&graph),
                node_id = %hex(&personal_sync.node_id()),
                paired = sync.paired_devices.len(),
                ticket = %personal_sync.ticket().await?,
                "personal graph sync listening"
            );
            let cards: DeviceSupplementalCards = Arc::new(tokio::sync::RwLock::new(
                personal_sync.supplemental_cards().await?,
            ));
            let refresh_cards = Arc::clone(&cards);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    match personal_sync.supplemental_cards().await {
                        Ok(snapshot) => *refresh_cards.write().await = snapshot,
                        Err(error) => {
                            tracing::warn!(%error, "personal sync projection refresh failed")
                        }
                    }
                }
            });
            Some(cards)
        } else {
            None
        }
    };
    #[cfg(feature = "personal-sync")]
    let browser = async {
        match supplemental_cards {
            Some(cards) => {
                serve_browser_broker_with_cards(
                    &args.browser_endpoint,
                    Arc::clone(&personae),
                    Arc::new(SystemNativeIdentityUi::default()),
                    allowlist,
                    session_duration_ms(),
                    cards,
                )
                .await
            }
            None => {
                serve_browser_broker(
                    &args.browser_endpoint,
                    Arc::clone(&personae),
                    Arc::new(SystemNativeIdentityUi::default()),
                    allowlist,
                    session_duration_ms(),
                )
                .await
            }
        }
    };
    #[cfg(not(feature = "personal-sync"))]
    let browser = serve_browser_broker(
        &args.browser_endpoint,
        Arc::clone(&personae),
        Arc::new(SystemNativeIdentityUi::default()),
        allowlist,
        session_duration_ms(),
    );
    tokio::pin!(agent);
    tokio::pin!(browser);
    tokio::select! {
        result = &mut agent => {
            result?;
            Err("SSH agent listener ended unexpectedly".into())
        }
        result = &mut browser => {
            result?;
            Err("browser device broker ended unexpectedly".into())
        }
    }
}

#[cfg(feature = "personal-sync")]
fn personal_graph_id(name: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(PERSONAL_GRAPH_DOMAIN);
    hasher.update(name.as_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(feature = "personal-sync")]
fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn session_duration_ms() -> u64 {
    let seconds = std::env::var(SESSION_SECONDS_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60 * 60)
        .clamp(60, 24 * 60 * 60);
    seconds * 1_000
}

#[cfg(not(windows))]
async fn prepare_unix_agent_endpoint(endpoint: &str) -> Result<(), std::io::Error> {
    match tokio::net::UnixStream::connect(endpoint).await {
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "another SSH agent owns the configured socket",
        )),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            match std::fs::remove_file(endpoint) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}
