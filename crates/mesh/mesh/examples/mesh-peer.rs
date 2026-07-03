/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! mesh-peer — the milestone-1 two-machine rehearsal bin.
//!
//! One device posts a job into the personal space, another claims it and
//! returns the result over LogSync. Run `work` on the workstation, copy its
//! ticket, then `post` on the laptop with `--peer <ticket>`; paste the
//! laptop's ticket into the workstation's stdin if the one-way bootstrap
//! doesn't connect (both directions tagged is the proven test shape).
//!
//! ```text
//! mesh-peer work [--peer <ticket>]
//! mesh-peer post <text> [--peer <ticket>]
//!
//! env:
//!   MESH_SPACE  shared passphrase naming the mesh (hashed to the mesh id);
//!               every participating device sets the same value
//!   MESH_SEED   this device's identity seed string; distinct per device
//!   MESH_DB     optional sqlite URL (e.g. sqlite://C:/mesh/mesh.db) for a
//!               durable store; defaults to in-memory
//! ```
//!
//! Both modes print the live board and the real sync status (rounds, ops
//! received, last activity) — the no-placebo rule.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
use mesh::{JobState, MeshEvent, MeshStore, SyncedMesh, WorkerAction, execute, next_action};
use p2panda_core::Hash;
use tokio::io::{AsyncBufReadExt, BufReader};
use transport::P2pandaTransport;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn hex8(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn env_hash(var: &str) -> Result<[u8; 32], String> {
    let value = std::env::var(var).map_err(|_| format!("set {var} (any string)"))?;
    Ok(*Hash::digest(value.as_bytes()).as_bytes())
}

enum Mode {
    Work,
    Post(String),
}

struct Args {
    mode: Mode,
    peer_tickets: Vec<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mode = match args.next().as_deref() {
        Some("work") => Mode::Work,
        Some("post") => {
            let text = args
                .next()
                .ok_or_else(|| "post needs the text to echo: mesh-peer post <text>".to_string())?;
            Mode::Post(text)
        }
        other => {
            return Err(format!(
                "usage: mesh-peer work|post <text> [--peer <ticket>] (got {other:?})"
            ));
        }
    };
    let mut peer_tickets = Vec::new();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--peer" => peer_tickets.push(
                args.next()
                    .ok_or_else(|| "--peer needs a ticket".to_string())?,
            ),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args { mode, peer_tickets })
}

async fn connect_ticket(
    transport: &P2pandaTransport,
    mesh_id: [u8; 32],
    ticket: &str,
) -> Result<(), String> {
    let peer = transport
        .add_peer_ticket(ticket.trim())
        .await
        .map_err(|e| format!("bad ticket: {e}"))?;
    transport
        .set_topics(peer, &[transport::sync_overlay_topic(mesh_id)])
        .await
        .map_err(|e| format!("set topics: {e}"))?;
    println!("connected peer {}", hex8(&peer.to_bytes()));
    Ok(())
}

fn print_board(board: &mesh::JobBoard) {
    for job in board.jobs() {
        let state = match &job.state {
            JobState::Posted => "posted".to_string(),
            JobState::Claimed { winner } => format!("claimed by {}", hex8(winner)),
            JobState::Done { winner, result } => format!(
                "done by {} → {:?}",
                hex8(winner),
                String::from_utf8_lossy(result)
            ),
        };
        println!("  job {} [{:?}] {}", hex8(&job.id.0), job.kind, state);
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = parse_args()?;
    let mesh_id = env_hash("MESH_SPACE")?;
    let seed = env_hash("MESH_SEED")?;

    let provider = InMemoryProvider::from_seed(seed);
    let author: Ed25519Keypair = provider
        .derive_keypair(b"mesh-author")
        .map_err(|e| format!("derive author key: {e}"))?;
    let me = author.public_key().to_bytes();

    let transport = Arc::new(
        P2pandaTransport::builder(provider.master_keypair())
            .gossip()
            .bind()
            .await
            .map_err(|e| format!("bind transport: {e}"))?,
    );
    let ticket = transport
        .ticket()
        .await
        .map_err(|e| format!("ticket: {e}"))?;
    let dialable = transport
        .endpoint_addr()
        .await
        .map_err(|e| format!("endpoint addr: {e}"))?
        .ip_addrs()
        .map(|a| a.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    println!("mesh-peer — resource-coordination M1 rehearsal");
    println!("space  : {}", hex8(&mesh_id));
    println!("me     : {}", hex8(&me));
    println!("addrs  : {dialable}");
    println!("ticket : {ticket}");
    println!("paste a peer's ticket on stdin (or pass --peer) to connect.\n");

    for t in &args.peer_tickets {
        connect_ticket(&transport, mesh_id, t).await?;
    }

    // Tickets pasted while running connect too, so the operator can wire the
    // two machines in either start order.
    let stdin_transport = Arc::clone(&transport);
    tokio::spawn(async move {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            if let Err(e) = connect_ticket(&stdin_transport, mesh_id, &line).await {
                eprintln!("{e}");
            }
        }
    });

    let store = match std::env::var("MESH_DB") {
        Ok(url) => MeshStore::at_url(&url).await,
        Err(_) => MeshStore::in_memory().await,
    }
    .map_err(|e| format!("store: {e}"))?;

    let (endpoint, gossip) = transport
        .sync_parts()
        .ok_or_else(|| "transport has no sync parts (gossip not enabled?)".to_string())?;
    let synced = SyncedMesh::join(endpoint, gossip, store, mesh_id)
        .await
        .map_err(|e| format!("join mesh: {e}"))?;

    match args.mode {
        Mode::Post(text) => {
            let posted = synced
                .author(
                    &author,
                    &MeshEvent::JobPosted {
                        kind: mesh::JobKind::Echo,
                        payload: text.into_bytes(),
                        nonce: now_ms(),
                        at_ms: now_ms(),
                    },
                )
                .await
                .map_err(|e| format!("post: {e}"))?;
            let id = mesh::JobId(*posted.hash.as_bytes());
            println!("posted job {} — waiting for a worker…", hex8(&id.0));

            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let board = synced.board().await.map_err(|e| format!("board: {e}"))?;
                let status = synced.sync_status();
                println!(
                    "board ({} jobs; sync: {} rounds, {} ops received):",
                    board.len(),
                    status.sync_rounds,
                    status.ops_received
                );
                print_board(&board);
                if let Some(JobState::Done { winner, result }) =
                    board.job(id).map(|j| j.state.clone())
                {
                    println!(
                        "\nresult landed: {:?} (worked by {})",
                        String::from_utf8_lossy(&result),
                        hex8(&winner)
                    );
                    // The stdin ticket-reader is a blocking task; returning
                    // from main would wait on it forever, so a CLI exits
                    // explicitly (the tokio-documented pattern for
                    // interactive bins).
                    std::process::exit(0);
                }
            }
        }
        Mode::Work => {
            println!("working — watching the board for jobs…");
            let mut last_status = String::new();
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let board = synced.board().await.map_err(|e| format!("board: {e}"))?;
                match next_action(&board, &me) {
                    WorkerAction::Claim(id) => {
                        println!("claiming job {}", hex8(&id.0));
                        synced
                            .author(
                                &author,
                                &MeshEvent::JobClaimed {
                                    job: id.0,
                                    at_ms: now_ms(),
                                },
                            )
                            .await
                            .map_err(|e| format!("claim: {e}"))?;
                    }
                    WorkerAction::Execute(id) => {
                        let job = board.job(id).expect("execute targets a known job");
                        let result = execute(job.kind, &job.payload);
                        println!(
                            "executing job {} [{:?}] → {:?}",
                            hex8(&id.0),
                            job.kind,
                            String::from_utf8_lossy(&result)
                        );
                        synced
                            .author(
                                &author,
                                &MeshEvent::JobDone {
                                    job: id.0,
                                    result,
                                    at_ms: now_ms(),
                                },
                            )
                            .await
                            .map_err(|e| format!("return result: {e}"))?;
                    }
                    WorkerAction::Idle => {
                        let status = synced.sync_status();
                        let line = format!(
                            "idle — board {} jobs; sync {} rounds, {} ops received",
                            board.len(),
                            status.sync_rounds,
                            status.ops_received
                        );
                        // Only print when something changed; an idle worker
                        // shouldn't scroll the terminal.
                        if line != last_status {
                            println!("{line}");
                            print_board(&board);
                            last_status = line;
                        }
                    }
                }
            }
        }
    }
}
