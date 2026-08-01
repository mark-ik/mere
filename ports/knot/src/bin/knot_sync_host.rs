//! Resident Knot sync for one persona.
//!
//! `knot_endpoint` serves a projection to whoever launches it and exits with
//! them. This process instead stays up and keeps the persona's vault space
//! joined, which is what makes an edit on one device reach the others without
//! either being opened at the time.
//!
//! It deliberately does not serve a projection. Knot's endpoint is mounted per
//! session by a host; the sync lane wants a different lifetime, and running
//! both from one process would tie the lane's uptime to a viewer's.
//!
//! ```text
//! knot_sync_host <data-root> <persona-uuid> [--label <name>] [--log-file <path>]
//! pair and exit:   --pair-writer <64-hex>
//! unpair and exit: --unpair-writer <64-hex>
//! what the others need: --pairing-facts
//! ```

use std::path::{Path, PathBuf};

use knot::{
    KnotSettings, KnotSyncHost, KnotSyncHostConfig, KnotSyncSettings, StartupUnlockedPersonalVault,
    knot_settings_path, local_device_root,
};

const PAIRING_POLL: std::time::Duration = std::time::Duration::from_secs(5);

struct Args {
    data_root: PathBuf,
    persona: personae::PersonaId,
    label: String,
    log_file: Option<PathBuf>,
    pair: Option<String>,
    unpair: Option<String>,
    pairing_facts: bool,
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
    // Management verbs edit settings and report; they never open the vault, so
    // they run while the resident host holds it.
    if let Some(result) = management(&args) {
        match result {
            Ok(message) => {
                println!("{message}");
                return;
            }
            Err(error) => {
                eprintln!("knot sync host: {error}");
                std::process::exit(1);
            }
        }
    }
    if let Err(error) = init_logging(args.log_file.as_deref()) {
        eprintln!("knot sync host: initialize logging: {error}");
        std::process::exit(1);
    }
    if let Err(error) = run(args).await {
        tracing::error!(%error, "knot sync host stopped");
        std::process::exit(1);
    }
}

fn management(args: &Args) -> Option<Result<String, String>> {
    match (&args.pair, &args.unpair, args.pairing_facts) {
        (Some(_), Some(_), _) => Some(Err(
            "--pair-writer and --unpair-writer are mutually exclusive".into(),
        )),
        (Some(writer), None, _) => Some(pair(args, writer, true)),
        (None, Some(writer), _) => Some(pair(args, writer, false)),
        (None, None, true) => Some(pairing_facts(args)),
        (None, None, false) => None,
    }
}

fn pair(args: &Args, writer: &str, add: bool) -> Result<String, String> {
    let key = knot::parse_hex32(writer).map_err(|error| error.to_string())?;
    let path = knot_settings_path(&args.data_root, args.persona);
    let mut settings = KnotSettings::load(&path).map_err(|error| error.to_string())?;
    let sync = settings.sync.get_or_insert_with(KnotSyncSettings::default);
    let changed = if add {
        sync.pair(key)
    } else {
        sync.unpair(key)
    };
    if !changed {
        return Ok(format!(
            "{writer} was already {}; settings unchanged",
            if add { "paired" } else { "not paired" }
        ));
    }
    settings.save(&path).map_err(|error| error.to_string())?;
    Ok(format!(
        "{} {writer} in {}",
        if add { "paired" } else { "unpaired" },
        path.display()
    ))
}

/// What the persona's other devices need in order to admit and reach this one.
///
/// Opens the vault, because the writer key is epoch-derived. Unlike
/// Graphshell's equivalent this cannot avoid that, so it will not run while
/// the resident host holds the store.
fn pairing_facts(args: &Args) -> Result<String, String> {
    let device_root = local_device_root(&args.data_root, &args.label)?;
    let authority =
        StartupUnlockedPersonalVault::open(&args.data_root, args.persona, device_root, [])?;
    Ok(format!(
        "writer {}\n\nOn each other device, run:\n  knot_sync_host <data-root> {} --pair-writer {}",
        knot::hex32(&authority.writer()),
        args.persona.as_uuid(),
        knot::hex32(&authority.writer()),
    ))
}

async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let settings_file = knot_settings_path(&args.data_root, args.persona);
    let stored = KnotSettings::load(&settings_file)?;
    tracing::info!(
        path = %settings_file.display(),
        configured = stored.sync.is_some(),
        "knot sync settings"
    );
    let Some(sync) = stored.sync else {
        return Err(format!(
            "knot sync is not configured for this persona; add a sync section to {}",
            settings_file.display()
        )
        .into());
    };

    let device_root = local_device_root(&args.data_root, &args.label)?;
    let admitted = sync.paired_writer_keys()?;
    let authority = StartupUnlockedPersonalVault::open(
        &args.data_root,
        args.persona,
        device_root,
        admitted.clone(),
    )?;

    let relays = sync
        .relay_urls
        .iter()
        .map(|url| {
            url.parse::<transport::p2panda_transport::RelayUrl>()
                .map_err(|error| format!("relay url {url:?}: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let host = KnotSyncHost::open(
        authority.store(),
        authority.signing_seed(),
        KnotSyncHostConfig {
            paired_writers: admitted,
            relay_urls: relays,
        },
    )
    .await?;

    // The writer key is what the other devices admit AND dial, so this one
    // line is the whole of what a peer needs.
    tracing::info!(
        persona = %args.persona.as_uuid(),
        writer = %knot::hex32(&host.node_id()),
        paired = sync.paired_writers.len(),
        relays = sync.relay_urls.len(),
        "knot vault sync listening"
    );
    if sync.paired_writers.is_empty() {
        tracing::warn!(
            "no paired writers: this device will hold its own vault and \
             converge with nothing"
        );
    }

    // Reconcile additions live, so pairing does not wait for a restart.
    // Additive only for now: dropping a writer changes admission, which the
    // store fixed at open, so unpairing still takes effect on the next start.
    let mut applied: std::collections::HashSet<[u8; 32]> =
        sync.paired_writer_keys()?.into_iter().collect();
    loop {
        tokio::time::sleep(PAIRING_POLL).await;
        let reloaded = match KnotSettings::load(&settings_file) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "could not reload knot sync settings");
                continue;
            }
        };
        let Some(sync) = reloaded.sync else { continue };
        let desired = match sync.paired_writer_keys() {
            Ok(keys) => keys,
            Err(error) => {
                tracing::warn!(%error, "knot sync settings hold an unusable writer key");
                continue;
            }
        };
        for writer in desired {
            if !applied.insert(writer) {
                continue;
            }
            match host.pair_writer(writer).await {
                Ok(()) => tracing::info!(
                    writer = %knot::hex32(&writer),
                    "reaching a newly paired device without a restart"
                ),
                Err(error) => {
                    applied.remove(&writer);
                    tracing::warn!(%error, writer = %knot::hex32(&writer), "could not reach a paired device");
                }
            }
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut argv = std::env::args().skip(1);
    let data_root = PathBuf::from(argv.next().ok_or_else(usage)?);
    let persona = argv
        .next()
        .ok_or_else(usage)?
        .parse::<uuid::Uuid>()
        .map(personae::PersonaId::from_uuid)
        .map_err(|error| format!("persona must be a UUID: {error}"))?;
    let mut label = "knot".to_string();
    let mut log_file = None;
    let mut pair = None;
    let mut unpair = None;
    let mut pairing_facts = false;
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--label" => label = argv.next().ok_or("--label needs a value")?,
            "--log-file" => {
                log_file = Some(PathBuf::from(
                    argv.next().ok_or("--log-file needs a value")?,
                ))
            }
            "--pair-writer" => pair = Some(argv.next().ok_or("--pair-writer needs a value")?),
            "--unpair-writer" => unpair = Some(argv.next().ok_or("--unpair-writer needs a value")?),
            "--pairing-facts" => pairing_facts = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        data_root,
        persona,
        label,
        log_file,
        pair,
        unpair,
        pairing_facts,
    })
}

fn usage() -> String {
    "usage: knot_sync_host <data-root> <persona-uuid> [--label <name>] \
     [--log-file <path>]\n\
     pair and exit: --pair-writer <64-hex> | --unpair-writer <64-hex>\n\
     what the others need: --pairing-facts"
        .to_string()
}

fn init_logging(path: Option<&Path>) -> Result<(), std::io::Error> {
    match path {
        Some(path) => {
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
        }
        None => tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init(),
    }
    Ok(())
}
