// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! mesh-peer — the two-machine rehearsal bin.
//!
//! One device posts a job into the personal space, another claims it and
//! returns the result over LogSync. Run `work` on the workstation, copy its
//! ticket, then `post` on the laptop with `--peer <ticket>`; paste the
//! laptop's ticket into the workstation's stdin if the one-way bootstrap
//! doesn't connect (both directions tagged is the proven test shape).
//!
//! **Scope: transport rehearsal, not a host.** This bin runs one job at a time,
//! inline on its own decision loop. It cannot heartbeat while working and
//! cannot stop a run on demand, so it declares itself
//! [`DevicePolicy::unsupervised`] and the worker will not hand it leased work.
//! Supervising in-flight jobs — the non-blocking run map, live progress,
//! cancellation on owner reclaim, and leased completion — is gate H0 of the
//! mesh host lanes plan, and belongs in a reusable host service above
//! `mere-mesh`, not in an example.
//!
//! `post` writes an M1 inline-payload job, because operations replicate here
//! and blobs do not: a V2 spec names bytes the other device would have to
//! already hold. The worker loop runs both unleased generations through the M2
//! resource registry.
//!
//! ```text
//! mesh-peer work [--peer <ticket>]
//! mesh-peer post <text> [--peer <ticket>]
//!
//! env:
//!   MESH_SPACE  shared passphrase naming the mesh (hashed to the mesh id);
//!               every participating device sets the same value
//!   MESH_SEED   this device's identity seed string; distinct per device
//!   MESH_DB     optional redb path (e.g. C:/mesh/mesh.redb) for a durable
//!               store; defaults to in-memory
//! ```
//!
//! Both modes print the live board and the real sync status (rounds, ops
//! received, last activity) — the no-placebo rule.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
use mesh::{
    DevicePolicy, HostFacts, HostOffer, JobControl, JobState, MemoryBlobSpace, MeshEvent,
    MeshStore, ResourceRegistry, SyncedMesh, WorkerAction, next_action, registry::run_job,
    registry::run_legacy,
};
use muniment::Backend;
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
            JobState::Committed { winner, output } => format!(
                "committed by {} → {} ({} bytes)",
                hex8(winner),
                hex8(&output.blob.digest.bytes),
                output.blob.byte_len
            ),
        };
        let asked = match (&job.spec, job.kind) {
            (Some(spec), _) => spec.resource.to_string(),
            (None, Some(kind)) => format!("{kind:?}"),
            (None, None) => "?".to_string(),
        };
        println!("  job {} [{asked}] {state}", hex8(&job.id.0));
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

    // The backend is a runtime choice (a redb file when MESH_DB is set, else
    // in-memory), so the two arms produce different `MeshStore<B>` types; `run`
    // is generic over the backend and each arm monomorphizes it.
    return match std::env::var("MESH_DB") {
        Ok(path) => {
            let store = MeshStore::at_path(&path).map_err(|e| format!("store: {e}"))?;
            run(store, transport, author, me, mesh_id, args.mode).await
        }
        Err(_) => {
            run(
                MeshStore::in_memory(),
                transport,
                author,
                me,
                mesh_id,
                args.mode,
            )
            .await
        }
    };
}

/// Join the mesh over `store` and run the requested mode. Generic over the
/// backend so the same loop drives an in-memory or a redb-backed store.
async fn run<B: Backend + Clone + Send + Sync + 'static>(
    store: MeshStore<B>,
    transport: Arc<P2pandaTransport>,
    author: Ed25519Keypair,
    me: [u8; 32],
    mesh_id: [u8; 32],
    mode: Mode,
) -> Result<(), String> {
    let (endpoint, gossip) = transport
        .sync_parts()
        .ok_or_else(|| "transport has no sync parts (gossip not enabled?)".to_string())?;
    let synced = SyncedMesh::join(endpoint, gossip, store, mesh_id)
        .await
        .map_err(|e| format!("join mesh: {e}"))?;

    match mode {
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
            // What this device advertises. The blob space is device-local: a V2
            // job whose input this machine does not hold simply fails to run,
            // because a signed spec names bytes, it does not deliver them.
            let registry = ResourceRegistry::builtin();
            // This bin runs one job at a time, inline, on its decision loop. It
            // therefore declares itself unsupervised, and `next_action` will not
            // hand it leased work: taking a lease means promising to heartbeat
            // and to stop on demand, and a loop that blocks on execution can do
            // neither. Leased jobs are the host supervisor's business (mesh host
            // lanes plan, gate H0); this bin stays the transport rehearsal.
            let policy = DevicePolicy::unsupervised();
            let blobs = MemoryBlobSpace::in_memory();
            println!(
                "working — watching the board for unleased jobs (offering {})…",
                registry
                    .resources()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let mut last_status = String::new();
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let board = synced.board().await.map_err(|e| format!("board: {e}"))?;
                let offer = HostOffer::new(&registry, HostFacts::cpu(1024), &policy).at(now_ms());
                match next_action(&board, &me, &offer) {
                    // Unreachable while `supervises_leases` is false, and left
                    // as a hard failure rather than a silent skip so that
                    // flipping the flag without building a supervisor is loud.
                    action @ (WorkerAction::Grant { .. }
                    | WorkerAction::Heartbeat { .. }
                    | WorkerAction::Reclaim { .. }) => {
                        return Err(format!(
                            "mesh-peer does not supervise leases; {action:?} needs a host \
                             supervisor (mesh host lanes plan, gate H0)"
                        ));
                    }
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
                        // Blocking, single-job, uncancellable — which is exactly
                        // why this bin refuses leases. The control handle is
                        // constructed so the seam is the real one, and dropped
                        // unused because nothing here can act on it.
                        let (_cancel, control) = JobControl::new();
                        let event = match &job.spec {
                            // Unleased by construction (see the policy above), so
                            // the plain V2 completion is the correct one. A leased
                            // job would need `JobCompletedUnderLease` naming its
                            // lease, which is the supervisor's job to author.
                            Some(spec) => {
                                let output = run_job(&registry, spec, &blobs, &blobs, &control)
                                    .await
                                    .map_err(|e| format!("run {}: {e}", spec.resource))?;
                                println!(
                                    "executed job {} [{}] → {} ({} bytes)",
                                    hex8(&id.0),
                                    spec.resource,
                                    hex8(&output.blob.digest.bytes),
                                    output.blob.byte_len
                                );
                                MeshEvent::JobDoneV2 {
                                    job: id.0,
                                    output: Box::new(output),
                                    at_ms: now_ms(),
                                }
                            }
                            None => {
                                // `payload` is `None` only after an accepted checkpoint erased a
                                // TERMINAL job's input, and `next_action` already guards on it
                                // before handing back `Execute`. A missing payload here is a
                                // broken invariant, not a case to skip quietly.
                                let payload = job
                                    .payload
                                    .as_deref()
                                    .expect("execute targets a job whose input is retained");
                                let kind = job.kind.expect("an M1 job carries its kind");
                                let result = run_legacy(&registry, kind, payload, &control)
                                    .await
                                    .map_err(|e| format!("run {kind:?}: {e}"))?;
                                println!(
                                    "executed job {} [{kind:?}] → {:?}",
                                    hex8(&id.0),
                                    String::from_utf8_lossy(&result)
                                );
                                MeshEvent::JobDone {
                                    job: id.0,
                                    result,
                                    at_ms: now_ms(),
                                }
                            }
                        };
                        synced
                            .author(&author, &event)
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
