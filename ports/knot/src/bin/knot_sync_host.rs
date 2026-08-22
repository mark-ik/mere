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
//! knot_sync_host [<data-root>] [<persona-uuid>] [--label <name>] [--log-file <path>]
//! pair and exit:   --pair-writer <64-hex>
//! unpair and exit: --unpair-writer <64-hex>
//! what the others need: --pairing-facts
//! ```
//!
//! Both positionals are optional. Omitted, the family answers: the shared
//! root ([`pandect::shared_root`]), and the sole persona wallet under
//! it — the ordinary machine runs `knot_sync_host` bare. Zero or several
//! personas are told plainly rather than guessed among, and a scratch root or
//! explicit persona still overrides, which is what the tests and receipts use.

use std::path::{Path, PathBuf};

use knot::{
    KnotSettings, KnotSyncHost, KnotSyncHostConfig, KnotSyncSettings, StartupUnlockedPersonalVault,
    knot_settings_path, local_device_root, personal_vault_writer,
};

const PAIRING_POLL: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg_attr(test, derive(Debug))]
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
    // Management verbs edit settings or derive public pairing facts. They do
    // not open Knot's vault or operation store, so they run beside a resident.
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
/// The writer is epoch-derived, but derivation needs Personae startup unlock,
/// not a second Knot store owner. This remains usable while the resident runs.
fn pairing_facts(args: &Args) -> Result<String, String> {
    let device_root = local_device_root(&args.data_root, &args.label)?;
    let writer = personal_vault_writer(&args.data_root, args.persona, device_root)?;
    Ok(format!(
        "writer {}\n\nOn each other device, run:\n  knot_sync_host <data-root> {} --pair-writer {}",
        knot::hex32(&writer),
        args.persona.as_uuid(),
        knot::hex32(&writer),
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
            peer_hints: sync.dial_hints(),
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

    // Reconcile pairing live. Writer admission and evidence access are shared
    // mutable Personae materializations, while the address-book topic is only
    // the route used to reach that admitted identity.
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
            Ok(keys) => keys.into_iter().collect::<std::collections::HashSet<_>>(),
            Err(error) => {
                tracing::warn!(%error, "knot sync settings hold an unusable writer key");
                continue;
            }
        };
        for writer in desired.iter().copied() {
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
        for writer in applied.difference(&desired).copied().collect::<Vec<_>>() {
            match host.unpair_writer(writer).await {
                Ok(()) => {
                    applied.remove(&writer);
                    tracing::info!(
                        writer = %knot::hex32(&writer),
                        "revoked an unpaired device without a restart"
                    );
                }
                Err(error) => tracing::warn!(
                    %error,
                    writer = %knot::hex32(&writer),
                    "revoked an unpaired device but could not remove its route"
                ),
            }
        }
        host.refresh_dial_hints(&sync, &settings_file).await;
    }
}

fn parse_args() -> Result<Args, String> {
    parse_from(std::env::args().skip(1).collect())
}

fn parse_from(args: Vec<String>) -> Result<Args, String> {
    // Up to two leading positionals: a data root, a persona UUID, or both in
    // that order. A UUID cannot be mistaken for a path in practice, so one
    // positional is read as whichever it parses as. Omitted, the family
    // answers: the shared root, and the sole persona wallet under it.
    let positional: Vec<&String> = args
        .iter()
        .take_while(|arg| !arg.starts_with("--"))
        .collect();
    let parse_persona = |value: &str| {
        value
            .parse::<uuid::Uuid>()
            .map(personae::PersonaId::from_uuid)
            .map_err(|error| format!("persona must be a UUID: {error}"))
    };
    let (data_root, persona) = match positional.as_slice() {
        [] => (None, None),
        [one] => match one.parse::<uuid::Uuid>() {
            Ok(uuid) => (None, Some(personae::PersonaId::from_uuid(uuid))),
            Err(_) => (Some(PathBuf::from(one.as_str())), None),
        },
        [root, persona] => (
            Some(PathBuf::from(root.as_str())),
            Some(parse_persona(persona)?),
        ),
        _ => return Err(usage()),
    };
    let data_root = data_root.unwrap_or_else(pandect::shared_root::shared_root);
    let persona = match persona {
        Some(persona) => persona,
        // Resolving rather than guessing: personas are real cryptographic
        // identities, and syncing the wrong one is not a recoverable oops.
        None => {
            let personas = pandect::wallet_store::list_personas(&data_root).map_err(|error| {
                format!(
                    "could not list personas under {}: {error}",
                    data_root.display()
                )
            })?;
            match personas.as_slice() {
                [only] => *only,
                [] => {
                    return Err(format!(
                        "no persona wallet exists under {} yet; pair this device first, or \
                         name a persona explicitly\n{}",
                        data_root.display(),
                        usage()
                    ));
                }
                several => {
                    return Err(format!(
                        "several personas live under {}; name one of: {}\n{}",
                        data_root.display(),
                        several
                            .iter()
                            .map(|persona| persona.as_uuid().to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                        usage()
                    ));
                }
            }
        }
    };
    let mut argv = args.iter().skip(positional.len()).cloned();
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
    "usage: knot_sync_host [<data-root>] [<persona-uuid>] [--label <name>] \
     [--log-file <path>]\n\
     omitted, the family answers: the shared root (MERE_ROOT or the platform \
     data dir), and the sole persona wallet under it\n\
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

#[cfg(test)]
mod tests {
    use super::*;
    use pandect::wallet_store::{
        KeyEpochId, PersonaChainRoot, PersonaWalletManifest, save_persona_wallet,
    };

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("knot-sync-host-args-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn seed_wallet(root: &std::path::Path, uuid: u128) -> personae::PersonaId {
        let persona = personae::PersonaId::from_uuid(uuid::Uuid::from_u128(uuid));
        save_persona_wallet(
            root,
            &PersonaWalletManifest::new(
                persona,
                PersonaChainRoot([7u8; 32]),
                KeyEpochId(uuid::Uuid::from_u128(0x9999)),
            ),
        )
        .unwrap();
        persona
    }

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    #[test]
    fn a_root_alone_resolves_its_sole_persona() {
        // The ordinary machine: nobody should have to hand-copy a UUID to
        // sync the only persona they have.
        let root = scratch("sole");
        let persona = seed_wallet(&root, 0x42);
        let parsed = parse_from(args(&[root.to_str().unwrap(), "--label", "study"])).unwrap();
        assert_eq!(parsed.persona, persona);
        assert_eq!(parsed.data_root, root);
        assert_eq!(parsed.label, "study", "flags still parse after resolution");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_lone_uuid_reads_as_a_persona_not_a_path() {
        let parsed = parse_from(args(&["00000000-0000-0000-0000-000000000042"])).unwrap();
        assert_eq!(
            parsed.persona,
            personae::PersonaId::from_uuid(uuid::Uuid::from_u128(0x42))
        );
    }

    #[test]
    fn both_positionals_still_work_exactly_as_before() {
        let root = scratch("explicit");
        let parsed = parse_from(args(&[
            root.to_str().unwrap(),
            "00000000-0000-0000-0000-000000000011",
        ]))
        .unwrap();
        assert_eq!(parsed.data_root, root);
        assert_eq!(
            parsed.persona,
            personae::PersonaId::from_uuid(uuid::Uuid::from_u128(0x11))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn zero_and_several_personas_are_told_not_guessed() {
        let empty = scratch("zero");
        let error = parse_from(args(&[empty.to_str().unwrap()])).unwrap_err();
        assert!(error.contains("no persona wallet exists"), "{error}");

        let crowded = scratch("several");
        let a = seed_wallet(&crowded, 0x21);
        let b = seed_wallet(&crowded, 0x22);
        let error = parse_from(args(&[crowded.to_str().unwrap()])).unwrap_err();
        assert!(error.contains(&a.as_uuid().to_string()), "{error}");
        assert!(
            error.contains(&b.as_uuid().to_string()),
            "names what exists so the fix is a copy, not a hunt: {error}"
        );
        let _ = std::fs::remove_dir_all(&empty);
        let _ = std::fs::remove_dir_all(&crowded);
    }
}
