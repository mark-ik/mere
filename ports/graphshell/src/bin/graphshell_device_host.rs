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
use graphshell::native::device_broker::serve_browser_broker_with_cards;
use graphshell::native::device_broker::{configured_device_endpoint, serve_browser_broker};
#[cfg(feature = "personal-sync")]
use graphshell::native::device_sync;
use graphshell::native::identity_ui::SystemNativeIdentityUi;
#[cfg(feature = "personal-sync")]
use graphshell::native::owner_settings::{self, SyncOverrides};
#[cfg(feature = "personal-sync")]
use graphshell::native::pairing;
use graphshell::native::personae_host::PersonaeHost;
#[cfg(windows)]
use graphshell::native::personae_host::STANDARD_WINDOWS_AGENT_ENDPOINT;
use graphshell::profile::{default_vault_dir, selected_profile};
use personae::bootstrap::{self, PASSPHRASE_ENV, Unlock};
use personae::{IdentityVault, ProfileId};
use ssh_agent_lib::agent::listen;

const EXTRA_EXTENSIONS_ENV: &str = "GRAPHSHELL_EXTENSION_IDS";
const DATA_ROOT_ENV: &str = "GRAPHSHELL_DATA_ROOT";
const SESSION_SECONDS_ENV: &str = "GRAPHSHELL_BROWSER_SESSION_SECONDS";

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
    /// Pair a device into this profile's settings and exit, rather than
    /// starting the host.
    #[cfg(feature = "personal-sync")]
    pair: Option<PairRequest>,
    /// Forget a device and exit.
    #[cfg(feature = "personal-sync")]
    unpair: Option<String>,
    /// Print what the other device needs in order to pair, and exit.
    #[cfg(feature = "personal-sync")]
    pairing_facts: bool,
    /// Nodes to author as the host starts. See `device_sync::SeedNote`: this
    /// is a stopgap until typed intents over the admitted session exist.
    #[cfg(feature = "personal-sync")]
    seed_notes: Vec<device_sync::SeedNote>,
    /// Blob operations to run once as the host starts. Same reason as
    /// `seed_notes`: this process owns the store and must keep running to
    /// serve what it stages.
    #[cfg(feature = "personal-sync")]
    blob_actions: Vec<device_sync::BlobAction>,
}

#[cfg(feature = "personal-sync")]
struct PairRequest {
    node_id: String,
    root: Option<String>,
    label: String,
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
    // Pairing is a management operation, not a run of the host: it edits the
    // settings and reports, so it writes to the console rather than the log.
    #[cfg(feature = "personal-sync")]
    {
        let management = match (&args.pair, &args.unpair, args.pairing_facts) {
            (Some(_), Some(_), _) => {
                eprintln!(
                    "graphshell device host: --pair-node and --unpair-node are mutually exclusive"
                );
                std::process::exit(2);
            }
            (Some(request), None, _) => Some(pair_device(&args, request)),
            (None, Some(node_id), _) => Some(unpair_device(&args, node_id)),
            (None, None, true) => Some(report_pairing_facts(&args)),
            (None, None, false) => None,
        };
        if let Some(result) = management {
            match result {
                Ok(message) => {
                    println!("{message}");
                    return;
                }
                Err(error) => {
                    eprintln!("graphshell device host: {error}");
                    std::process::exit(1);
                }
            }
        }
    }
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
    let mut sync_relays = Vec::new();
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
    #[cfg(feature = "personal-sync")]
    let mut pair_node = None;
    #[cfg(feature = "personal-sync")]
    let mut pair_root = None;
    #[cfg(feature = "personal-sync")]
    let mut pair_label = String::new();
    #[cfg(feature = "personal-sync")]
    let mut unpair_node = None;
    #[cfg(feature = "personal-sync")]
    let mut pairing_facts = false;
    #[cfg(feature = "personal-sync")]
    let mut seed_notes = Vec::new();
    #[cfg(feature = "personal-sync")]
    let mut blob_actions = Vec::new();
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
            "--sync-relay" => {
                sync_relays.push(argv.next().ok_or("--sync-relay needs a url")?);
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
            "--pair-node" => {
                let value = argv.next().ok_or("--pair-node needs a value")?;
                owner_settings::parse_hex32(&value).map_err(|error| error.to_string())?;
                pair_node = Some(value);
            }
            #[cfg(feature = "personal-sync")]
            "--pair-root" => {
                let value = argv.next().ok_or("--pair-root needs a value")?;
                owner_settings::parse_hex32(&value).map_err(|error| error.to_string())?;
                pair_root = Some(value);
            }
            #[cfg(feature = "personal-sync")]
            "--pair-label" => {
                pair_label = argv.next().ok_or("--pair-label needs a value")?;
            }
            #[cfg(feature = "personal-sync")]
            "--pairing-facts" => pairing_facts = true,
            #[cfg(feature = "personal-sync")]
            "--seed-node" => {
                let address = argv.next().ok_or("--seed-node needs an address")?;
                let title = argv
                    .next()
                    .ok_or("--seed-node needs a title after the address")?;
                seed_notes.push(device_sync::SeedNote { address, title });
            }
            #[cfg(feature = "personal-sync")]
            "--stage-blob" => {
                let path = argv.next().ok_or("--stage-blob needs a file path")?;
                blob_actions.push(device_sync::BlobAction::Stage {
                    path: PathBuf::from(path),
                });
            }
            #[cfg(feature = "personal-sync")]
            "--fetch-blob" => {
                let value = argv.next().ok_or("--fetch-blob needs a 64-hex hash")?;
                let blob =
                    owner_settings::parse_hex32(&value).map_err(|error| error.to_string())?;
                blob_actions.push(device_sync::BlobAction::Fetch { blob });
            }
            #[cfg(feature = "personal-sync")]
            "--unpair-node" => {
                let value = argv.next().ok_or("--unpair-node needs a value")?;
                owner_settings::parse_hex32(&value).map_err(|error| error.to_string())?;
                unpair_node = Some(value);
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
            relay_urls: sync_relays,
            facets: sync_facets,
            access_records: sync_access,
            saved_scenes: sync_scenes,
            handler_preferences: sync_handlers,
            blob_availability: sync_blobs,
        },
        #[cfg(feature = "personal-sync")]
        sync_peer_tickets: sync_peers,
        #[cfg(feature = "personal-sync")]
        pair: pair_node.map(|node_id| PairRequest {
            node_id,
            root: pair_root,
            label: pair_label,
        }),
        #[cfg(feature = "personal-sync")]
        unpair: unpair_node,
        #[cfg(feature = "personal-sync")]
        pairing_facts,
        #[cfg(feature = "personal-sync")]
        seed_notes,
        #[cfg(feature = "personal-sync")]
        blob_actions,
    })
}

/// Print what the other device needs in order to pair with this one.
///
/// Opens the vault but not the store, so it works while the resident host is
/// running and holding the store's lock.
#[cfg(feature = "personal-sync")]
fn report_pairing_facts(args: &Args) -> Result<String, String> {
    let opened = bootstrap::open_storage(&args.vault_dir, Unlock::from_env())
        .map_err(|error| error.to_string())?;
    let (profile, _created) = bootstrap::load_or_create_profile(&*opened.storage, &args.profile)
        .map_err(|error| error.to_string())?;
    let vault = IdentityVault::with_profile(opened.storage, profile);
    let facts = pairing::pairing_facts(&vault, &owner_settings::default_app_dir(), &args.profile)
        .map_err(|error| error.to_string())?;
    let Some(facts) = facts else {
        return Err(format!(
            "personal sync is not configured for profile {:?}",
            args.profile.0
        ));
    };
    Ok(format!(
        "graph   {}\nnode_id {}\nroot    {}\n\nOn the other device, run:\n  \
         graphshell_device_host --pair-node {} --pair-root {} --pair-label <name>",
        owner_settings::hex32(&facts.graph),
        owner_settings::hex32(&facts.node_id),
        owner_settings::hex32(&facts.root),
        owner_settings::hex32(&facts.node_id),
        owner_settings::hex32(&facts.root),
    ))
}

#[cfg(feature = "personal-sync")]
fn unpair_device(args: &Args, node_id: &str) -> Result<String, String> {
    let node = owner_settings::parse_hex32(node_id).map_err(|error| error.to_string())?;
    let outcome = pairing::unpair_device(&owner_settings::default_app_dir(), &args.profile, node)
        .map_err(|error| error.to_string())?;
    Ok(match outcome {
        pairing::UnpairOutcome::Removed { path } => {
            format!("unpaired {} in {}", node_id, path.display())
        }
        pairing::UnpairOutcome::NotPaired => {
            format!("{node_id} was not paired; settings unchanged")
        }
    })
}

#[cfg(feature = "personal-sync")]
fn pair_device(args: &Args, request: &PairRequest) -> Result<String, String> {
    let node = owner_settings::parse_hex32(&request.node_id).map_err(|error| error.to_string())?;
    let root = match &request.root {
        Some(value) => Some(owner_settings::parse_hex32(value).map_err(|error| error.to_string())?),
        None => None,
    };
    let outcome = pairing::pair_device(
        &owner_settings::default_app_dir(),
        &args.profile,
        node,
        root,
        &request.label,
        now_ms(),
    )
    .map_err(|error| error.to_string())?;
    Ok(match outcome {
        pairing::PairOutcome::Added { path, receive_only } => format!(
            "paired {} as {:?} in {}{}",
            request.node_id,
            request.label,
            path.display(),
            if receive_only {
                "\nno --pair-root given: this device will receive the graph, \
                 and its own writes will be refused"
            } else {
                ""
            }
        ),
        pairing::PairOutcome::AlreadyPaired => {
            format!("{} was already paired; settings unchanged", request.node_id)
        }
    })
}

#[cfg(feature = "personal-sync")]
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(feature = "personal-sync")]
fn usage() -> &'static str {
    "usage: graphshell_device_host [--dir <vault-dir>] [--profile <name>] \
     [--agent-endpoint <standard-endpoint>] \
     [--browser-endpoint <private-endpoint>] [--data-root <dir>] \
     [--log-file <path>]\n\
     personal sync: [--sync-graph <name>] [--sync-store <path>] \
     [--sync-root <64-hex-public-root>] [--sync-peer-node <64-hex-node-id>] \
     [--sync-peer <ticket>] [--sync-relay <url>] \
     [--sync-facet <id>] [--sync-access] [--sync-scenes] \
     [--sync-handlers] [--sync-blobs]\n\
     pair and exit: --pair-node <64-hex-node-id> \
     [--pair-root <64-hex-master-root>] [--pair-label <name>]\n\
     unpair and exit: --unpair-node <64-hex-node-id>\n\
     what the other device needs: --pairing-facts\n\
     seed a node at start: [--seed-node <address> <title>]\n\
     blobs at start: [--stage-blob <file>] [--fetch-blob <64-hex-hash>]\n\
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
    let supplemental_cards = device_sync::start(
        personae.as_ref(),
        &owner_settings::default_app_dir(),
        &args.vault_dir,
        &args.profile,
        args.data_root.clone(),
        args.sync_overrides,
        args.sync_peer_tickets,
        args.seed_notes,
        args.blob_actions,
    )
    .await?;
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
