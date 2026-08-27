// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Physical H7 offline-edit and convergence receipt.
//!
//! ```text
//! h7_sync_peer serve --store <path>
//! h7_sync_peer connect --store <path> --peer <ticket>
//! h7_sync_peer serve|connect --store <path> --peer-file <path>
//!
//! H7_GRAPH  shared graph name
//! H7_SEED   this device name
//! H7_PEER   the other device name
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eidetic::PrivacyClass;
use graphshell::access::{AccessAction, AccessRecord, AccessTransition};
use graphshell::personal_sync::{
    BlobAvailabilityObservation, PersonalGraphEvent, PersonalGraphReplica, SyncProjection,
    SyncRoster, SyncSelection, accept_into,
};
use mere::kernel::graph::{EdgeAssertion, RelationKind, SemanticSubKind};
use muniment::RedbBackend;
use personae::{IdentityProvider, InMemoryProvider};
use serde::Serialize;
use stickleback::JoinedSpace;
use transport::{P2pandaTransport, PeerID, sync_overlay_topic};
use uuid::Uuid;

const GRAPH_DOMAIN: &[u8] = b"mere.graphshell/h7-physical-graph/v1";
const A: Uuid = Uuid::from_u128(0x7001);
const B: Uuid = Uuid::from_u128(0x7002);
const BLOB: [u8; 32] = [0x7b; 32];

#[derive(Clone, Copy)]
enum Role {
    Windows,
    Qpc,
}

#[derive(Serialize)]
struct Receipt {
    graph: String,
    nodes: usize,
    relations: usize,
    tags: Vec<String>,
    accesses: Vec<(String, u64)>,
    conflict_targets: Vec<String>,
    blob_devices: Vec<String>,
    writers: usize,
    pending: usize,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_default();
    let mut store = None;
    let mut peer_ticket = None;
    let mut peer_file = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--store" => store = args.next().map(PathBuf::from),
            "--peer" => peer_ticket = args.next(),
            "--peer-file" => peer_file = args.next().map(PathBuf::from),
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    let role = match mode.as_str() {
        "serve" => Role::Windows,
        "connect" => Role::Qpc,
        _ => {
            return Err("usage: h7_sync_peer serve|connect --store <path> \
                 [--peer <ticket> | --peer-file <path>]"
                .into());
        }
    };
    let store = store.ok_or("--store is required")?;
    let seed = env_hash("H7_SEED")?;
    let peer_seed = env_hash("H7_PEER")?;
    let graph = graph_id(&std::env::var("H7_GRAPH").map_err(|_| "H7_GRAPH is required")?);
    let identity = InMemoryProvider::from_seed(seed);
    let peer_identity = InMemoryProvider::from_seed(peer_seed);
    let roster = SyncRoster::new([
        identity.master_public_key().to_bytes(),
        peer_identity.master_public_key().to_bytes(),
    ]);
    let selection = selection();

    prepare_offline(&store, graph, seed, roster.clone(), selection.clone(), role).await?;
    let backend = RedbBackend::open(&store).map_err(|error| error.to_string())?;
    let replica = PersonalGraphReplica::new(backend, graph, seed, roster.clone(), selection);
    assert_offline(
        &replica
            .projection()
            .await
            .map_err(|error| error.to_string())?,
        role,
    )?;

    let transport = P2pandaTransport::builder_from_seed(seed)
        .gossip()
        .bind()
        .await
        .map_err(|error| format!("bind: {error}"))?;
    let peer = PeerID::from_public_key(peer_identity.master_public_key());
    println!(
        "H7_TICKET {}",
        transport.ticket().await.map_err(|e| e.to_string())?
    );
    std::io::stdout()
        .flush()
        .map_err(|error| error.to_string())?;
    let peer_ticket = match (peer_ticket, peer_file) {
        (Some(_), Some(_)) => return Err("use only one of --peer or --peer-file".into()),
        (Some(ticket), None) => Some(ticket),
        (None, Some(path)) => Some(wait_for_peer_ticket(&path).await?),
        (None, None) if matches!(role, Role::Qpc) => {
            return Err("connect requires --peer or --peer-file".into());
        }
        (None, None) => None,
    };
    if let Some(ticket) = peer_ticket {
        let ticket_peer = transport
            .add_peer_ticket(&ticket)
            .await
            .map_err(|error| format!("ticket: {error}"))?;
        if ticket_peer != peer {
            return Err("ticket names another H7 peer".into());
        }
    }
    transport
        .set_topics(peer, &[sync_overlay_topic(graph)])
        .await
        .map_err(|error| format!("set topics: {error}"))?;
    let store = replica.sync_store();
    let accepted = store.clone();
    let accepted_roster = roster.clone();
    let (endpoint, gossip) = transport
        .sync_parts()
        .ok_or("sync transport has no gossip")?;
    let joined = JoinedSpace::join::<_, u64, _, _>(
        // Must match the resident host's lane exactly, or this receipt peer
        // and a real device would talk past each other.
        stickleback::lane_id("graphshell/personal-graph/v1", graph),
        store,
        endpoint,
        gossip,
        graph,
        move |operation| {
            let store = accepted.clone();
            let roster = accepted_roster.clone();
            async move {
                accept_into(&store, graph, &roster, None, &operation)
                    .await
                    .unwrap_or(false)
            }
        },
    )
    .await
    .map_err(|error| error.to_string())?;

    let projection = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let projection = replica
                .projection()
                .await
                .map_err(|error| error.to_string())?;
            if converged(&projection) {
                return Ok::<_, String>(projection);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| {
        format!(
            "convergence timed out after {} sync rounds and {} accepted operations",
            joined.sync_status().sync_rounds,
            joined.ops_received()
        )
    })??;

    println!(
        "H7_RECEIPT {}",
        serde_json::to_string(&receipt(graph, &projection)).map_err(|error| error.to_string())?
    );
    Ok(())
}

async fn wait_for_peer_ticket(path: &Path) -> Result<String, String> {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            match std::fs::read_to_string(path) {
                Ok(ticket) if !ticket.trim().is_empty() => {
                    return Ok::<_, String>(ticket.trim().to_string());
                }
                Ok(_) | Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    })
    .await
    .map_err(|_| format!("timed out waiting for peer ticket at {}", path.display()))?
}

async fn prepare_offline(
    path: &Path,
    graph: [u8; 32],
    seed: [u8; 32],
    roster: SyncRoster,
    selection: SyncSelection,
    role: Role,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let backend = RedbBackend::open(path).map_err(|error| error.to_string())?;
    let mut replica = PersonalGraphReplica::new(backend, graph, seed, roster, selection);
    if !replica
        .projection()
        .await
        .map_err(|error| error.to_string())?
        .writers
        .is_empty()
    {
        return Ok(());
    }
    replica
        .author(vec![
            PersonalGraphEvent::AddNode {
                id: A,
                address: "https://h7.example/a".into(),
                title: "Shared A".into(),
            },
            PersonalGraphEvent::AddNode {
                id: B,
                address: "https://h7.example/b".into(),
                title: "Shared B".into(),
            },
        ])
        .await
        .map_err(|error| error.to_string())?;
    replica
        .author(local_events(role))
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn local_events(role: Role) -> Vec<PersonalGraphEvent> {
    let (tag, title, device, access_id, blob_id, at_ms) = match role {
        Role::Windows => (
            "windows",
            "Windows title",
            "windows-laptop",
            0x7101,
            0x7201,
            200,
        ),
        Role::Qpc => ("qpc", "Q-PC title", "qpc", 0x7102, 0x7202, 100),
    };
    let mut events = vec![
        PersonalGraphEvent::SetTitle {
            node: A,
            title: title.into(),
        },
        PersonalGraphEvent::AddTag {
            node: A,
            tag: tag.into(),
        },
        PersonalGraphEvent::AppendAccess {
            record: AccessRecord {
                record_id: Uuid::from_u128(access_id),
                container_id: A,
                address: "https://h7.example/a".into(),
                action: AccessAction::Examine,
                persona: "personae://h7-physical".into(),
                device: device.into(),
                application: "graphshell".into(),
                at_ms,
                handler: "graphshell.inspect".into(),
                dwell_ms: None,
                referring_container_id: None,
                referring_address: None,
                transition: AccessTransition::Unknown,
                capture_source: "graphshell.h7-physical".into(),
                source_event_id: None,
                privacy: PrivacyClass::TrustedPeersOnly,
            },
        },
        PersonalGraphEvent::ObserveBlobAvailability {
            observation: BlobAvailabilityObservation {
                record_id: Uuid::from_u128(blob_id),
                container_id: A,
                blob: BLOB,
                device: device.into(),
                available: true,
                at_ms,
            },
        },
    ];
    if matches!(role, Role::Windows) {
        events.push(PersonalGraphEvent::AssertRelation {
            from: A,
            to: B,
            assertion: EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
        });
    }
    events
}

fn selection() -> SyncSelection {
    SyncSelection::default()
        .with_access_records(true)
        .with_blob_availability(true)
}

fn assert_offline(projection: &SyncProjection, role: Role) -> Result<(), String> {
    let expected = match role {
        Role::Windows => "windows",
        Role::Qpc => "qpc",
    };
    let node = projection
        .graph
        .get_node_by_id(A)
        .ok_or("offline projection lost node A")?
        .1;
    if !node.tags.contains(expected) || projection.access_records.len() != 1 {
        return Err("offline edit did not survive durable reopen".into());
    }
    Ok(())
}

fn converged(projection: &SyncProjection) -> bool {
    let Some((_, node)) = projection.graph.get_node_by_id(A) else {
        return false;
    };
    node.tags.contains("windows")
        && node.tags.contains("qpc")
        && projection.graph.edge_count() == 1
        && projection.access_records.len() == 2
        && projection
            .conflicts
            .iter()
            .any(|conflict| conflict.target == format!("node/{A}/title"))
        && projection
            .available_blobs
            .get(&BLOB)
            .is_some_and(|devices| devices.contains("windows-laptop") && devices.contains("qpc"))
        && projection.pending.is_empty()
}

fn receipt(graph: [u8; 32], projection: &SyncProjection) -> Receipt {
    let node = projection.graph.get_node_by_id(A).unwrap().1;
    let mut tags = node.tags.iter().cloned().collect::<Vec<_>>();
    tags.sort();
    let accesses = projection
        .access_records
        .iter()
        .map(|record| (record.device.clone(), record.at_ms))
        .collect();
    let conflict_targets = projection
        .conflicts
        .iter()
        .map(|conflict| conflict.target.clone())
        .collect();
    let blob_devices = projection
        .available_blobs
        .get(&BLOB)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    Receipt {
        graph: hex(&graph),
        nodes: projection.graph.node_count(),
        relations: projection
            .graph
            .relations()
            .filter(|relation| matches!(relation.kind, RelationKind::Semantic(_)))
            .count(),
        tags,
        accesses,
        conflict_targets,
        blob_devices,
        writers: projection.writers.len(),
        pending: projection.pending.len(),
    }
}

fn env_hash(name: &str) -> Result<[u8; 32], String> {
    std::env::var(name)
        .map(|value| *blake3::hash(value.as_bytes()).as_bytes())
        .map_err(|_| format!("{name} is required"))
}

fn graph_id(name: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(GRAPH_DOMAIN);
    hasher.update(name.as_bytes());
    *hasher.finalize().as_bytes()
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
