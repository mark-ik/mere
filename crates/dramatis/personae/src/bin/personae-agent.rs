// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! personae-agent — SSH agent serving keys from the personae vault.
//!
//! Thin listener around [`personae::agent::VaultAgent`]. Stock `ssh` /
//! `ssh-add` talk to it over the standard agent endpoint:
//!
//! - Windows: the OpenSSH named pipe (default
//!   `\\.\pipe\openssh-ssh-agent`, where `ssh.exe` looks without any
//!   configuration; requires the stock ssh-agent service to be stopped).
//! - Unix: a socket file; export `SSH_AUTH_SOCK` pointing at it.
//!
//! ## Storage selection
//!
//! - Default: OS auto-unlock storage ([`personae::SealedProfileStorage`],
//!   DPAPI-wrapped root on Windows). On platforms without that ladder the
//!   agent refuses to start rather than storing plaintext.
//! - `PERSONAE_PASSPHRASE` set: the portable passphrase vault
//!   ([`personae::PassphraseEncryptedStorage`]) at `<dir>/vault.json`.
//!
//! ## Usage
//!
//! ```text
//! personae-agent [--dir <vault-dir>] [--profile <name>] [--socket <path>]
//! ```
//!
//! Import a key: `ssh-add <path-to-private-key>` (the key lands in the
//! vault, encrypted at rest). List: `ssh-add -l`. Remove: `ssh-add -d/-D`.

use std::path::PathBuf;

use personae::agent::VaultAgent;
use personae::bootstrap::{self, Unlock};
use personae::{IdentityVault, ProfileId};
#[cfg(windows)]
use ssh_agent_lib::agent::NamedPipeListener as Listener;
use ssh_agent_lib::agent::listen;
#[cfg(not(windows))]
use tokio::net::UnixListener as Listener;

#[cfg(windows)]
const DEFAULT_SOCKET: &str = r"\\.\pipe\openssh-ssh-agent";

struct Args {
    dir: PathBuf,
    profile: String,
    socket: String,
    /// Append logs to this file instead of stderr. The launch-at-login
    /// scheduled task runs the agent windowless, so stderr goes nowhere;
    /// this keeps the log inspectable.
    log_file: Option<PathBuf>,
}

fn default_socket(dir: &std::path::Path) -> String {
    #[cfg(windows)]
    {
        let _ = dir;
        DEFAULT_SOCKET.to_string()
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(|runtime| {
                PathBuf::from(runtime)
                    .join("personae-agent.sock")
                    .display()
                    .to_string()
            })
            .unwrap_or_else(|| dir.join("agent.sock").display().to_string())
    }
}

fn parse_args() -> Result<Args, String> {
    let mut dir = bootstrap::default_vault_dir();
    let mut profile = "default".to_string();
    let mut socket = None;
    let mut log_file = None;

    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--dir" => {
                dir = PathBuf::from(argv.next().ok_or("--dir needs a value")?);
            }
            "--profile" => {
                profile = argv.next().ok_or("--profile needs a value")?;
            }
            "--socket" => {
                socket = Some(argv.next().ok_or("--socket needs a value")?);
            }
            "--log-file" => {
                log_file = Some(PathBuf::from(
                    argv.next().ok_or("--log-file needs a value")?,
                ));
            }
            "--help" | "-h" => {
                return Err(
                    "usage: personae-agent [--dir <vault-dir>] [--profile <name>] \
                     [--socket <path>] [--log-file <path>]"
                        .to_string(),
                );
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let socket = socket.unwrap_or_else(|| default_socket(&dir));
    Ok(Args {
        dir,
        profile,
        socket,
        log_file,
    })
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

    if let Some(path) = &args.log_file {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(file) => {
                tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::INFO)
                    .with_ansi(false)
                    .with_writer(std::sync::Mutex::new(file))
                    .init();
            }
            Err(err) => {
                eprintln!("personae-agent: open log file {path:?}: {err}");
                std::process::exit(1);
            }
        }
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    if let Err(message) = run(args).await {
        eprintln!("personae-agent: {message}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), String> {
    let opened = bootstrap::open_storage(&args.dir, Unlock::from_env())
        .map_err(|err| format!("open vault: {err}"))?;
    tracing::info!("storage: {}", opened.description);

    let id = ProfileId(args.profile.clone());
    let (profile, created) = bootstrap::load_or_create_profile(&*opened.storage, &id)
        .map_err(|err| format!("open profile {:?}: {err}", args.profile))?;
    if created {
        tracing::info!(profile = %args.profile, "profile not found; created it");
    }
    let ssh_slots = profile
        .slots
        .keys()
        .filter(|key| key.mod_id == personae::agent::SSH_MOD_ID)
        .count();
    tracing::info!(
        profile = %args.profile,
        ssh_slots,
        "vault open; `ssh-add -l` lists keys, `ssh-add <file>` imports one"
    );

    let vault = IdentityVault::with_profile(opened.storage, profile);
    let agent = VaultAgent::new(vault);

    // A stale Unix socket file from a previous run blocks bind; remove it.
    // (Windows named pipes have no file to clean up.)
    #[cfg(not(windows))]
    let _ = std::fs::remove_file(&args.socket);

    tracing::info!(socket = %args.socket, "listening");
    let listener = Listener::bind(&args.socket).map_err(|err| {
        #[cfg(windows)]
        {
            format!(
                "bind {}: {err} (is the stock OpenSSH ssh-agent service, or another agent, \
                 already listening on this pipe?)",
                args.socket
            )
        }
        #[cfg(not(windows))]
        {
            format!("bind {}: {err}", args.socket)
        }
    })?;
    listen(listener, agent)
        .await
        .map_err(|err| format!("agent loop: {err}"))
}
