/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host p2p sync subsystem (S5.0 foundation + S5.1 connect).
//!
//! The tessera counterpart of [`fetch`](crate::fetch): the same async-host seam,
//! a different payload. A [`SyncHost`] owns a tokio runtime, stands up one
//! `P2pandaTransport` (the shared identity-root seed) and joins a tessera moot's
//! `SyncedMoot` LogSync session on it, then polls the lane's real `SyncStatus`
//! and pushes each change over an `mpsc` channel + an `EventLoopProxy<()>` wake
//! (delivery model 2, shared with the fetcher). The host drains it in
//! `user_event` and folds it into the chrome sync chip.
//!
//! In the actor-constellation framing it is an **I/O actor**: a "connect to peer"
//! verb is an outbound command ([`connect`](SyncHost::connect)) the host routes
//! to it; status flows back as the inbound message.
//!
//! S5.1: the lane bootstraps a peer from a **ticket** string (this node's is
//! logged at startup to hand out; [`connect`](SyncHost::connect) dials another's),
//! and the host authors a small starter tessera log on launch so a connecting
//! peer has something to sync.
//! S5.2: identity is the wallet root's persistent master seed (a stable node id
//! across restarts; the passphrase-encrypted persona vault is still the follow-on), and
//! the tessera store is on disk (`<data>/mere/moots/<moot>.redb`), so a peer's log
//! survives restart. Still S5.x-shaped: the moot is a fixed demo id (real moot
//! selection is S5.3).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use armillary::{ActorHandle, Emitter, Wake, spawn};
use identity::{IdentityProvider, InMemoryProvider};
use moothold::tessera::{
    ChainRoot, CommitmentId, Ledger, Scope, TesseraConfig, TesseraEvent, TesseraStore,
    TesseraStoreError, to_operation, verify,
};
use p2panda_core::Topic;
use p2panda_net::LogSync;
use transport::{P2pandaTransport, SyncStatus, SyncedSpace, sync_overlay_topic};

use meerkat::SyncIndicator;

/// The demo moot the foundation joins (real moot selection is S5.3).
pub const DEMO_MOOT: [u8; 32] = [0x7e; 32];
/// The chip label for the tessera lane.
pub const LANE_LABEL: &str = "tessera";

/// The host-composed tessera sync session: the shared [`SyncedSpace`] drain (for
/// status), the [`TesseraStore`] to fold, the moot it syncs, and the `LogSync`
/// session + handle held for liveness. moothold owns no p2panda-net after the
/// sibling-posture purity split, so the host builds the pump; the tessera lane
/// is receive-only, so nothing publishes on the handle.
struct TesseraSync {
    space: SyncedSpace,
    store: TesseraStore,
    moot_id: [u8; 32],
    _keepalive: Box<dyn std::any::Any + Send>,
}

impl TesseraSync {
    fn sync_status(&self) -> SyncStatus {
        self.space.sync_status()
    }

    fn ledger(&self, config: TesseraConfig) -> Result<Ledger, TesseraStoreError> {
        self.store.fold_moot(self.moot_id, config)
    }
}

/// A change in a synced lane's status, delivered to the UI loop. Carries the raw
/// reconciliation `status` plus the two things the chip actually wants to show: this
/// persona's folded ledger `standing` (the point of tessera) and the dialable `ticket`
/// to share with a peer. (Tessera ledger fold + ticket surface.)
pub struct SyncUpdate {
    pub active: bool,
    pub status: SyncStatus,
    /// This persona's earned tessera standing (the folded ledger score), or `None`
    /// before a ledger has folded.
    pub standing: Option<i64>,
    /// This install's dialable ticket, or `None` if the transport produced none.
    pub ticket: Option<String>,
}

/// Project a [`SyncUpdate`] into the chrome's [`SyncIndicator`] view-model. `active`
/// is always `true` here: an update only exists once a lane is joined.
pub fn to_indicator(update: &SyncUpdate, label: &str) -> SyncIndicator {
    SyncIndicator {
        label: label.to_string(),
        active: update.active,
        syncing: update.status.syncing,
        ops: update.status.ops_received,
        last_activity_ms: update.status.last_activity_ms,
        standing: update.standing,
        ticket: update.ticket.clone(),
    }
}

/// Unix-epoch milliseconds, for the ledger fold's decay clock. (Tessera ledger fold.)
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The sync actor's outbound commands. Today just "connect to a peer by ticket"
/// (the S5.1 verb); more sync verbs join here as their wiring lands.
pub enum SyncCommand {
    /// Dial a peer from its ticket string so the LogSync overlay forms and the two
    /// converge.
    Connect(String),
    /// Retry startup after an explicit wallet unlock.
    UnlockNow,
    /// Return the lane to an offline state after a manual relock.
    LockNow,
}

/// Spawn the sync subsystem as an [`armillary`] I/O actor. Its `run` closure builds
/// the tokio runtime *on the actor thread* (so armillary stays runtime-free), binds
/// one `P2pandaTransport` (persistent seed-file identity), joins the tessera moot's
/// `SyncedMoot` (seeded with a starter log), and:
///
/// - a background poll task emits a [`SyncUpdate`] (via the actor's `Emitter`, which
///   wakes the kernel) whenever the lane's `SyncStatus` changes;
/// - the command loop runs each [`SyncCommand`] to completion on the runtime.
///
/// If setup fails, p2p is simply off (no updates, the chip stays `p2p off`, a
/// `Connect` just logs the failure) — networking is never fatal to the shell.
/// Returns the kernel's command handle plus the receiver it drains in `user_event`.
pub fn spawn_sync(
    wake: Wake,
    data_root: PathBuf,
    moot_id: [u8; 32],
) -> (ActorHandle<SyncCommand>, Receiver<SyncUpdate>) {
    spawn(wake, move |commands, out: Emitter<SyncUpdate>| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build the sync runtime");
        let dir = data_root;
        let lane_generation = Arc::new(AtomicU64::new(0));
        let mut transport =
            match activate_sync_lane(&runtime, &out, &dir, moot_id, &lane_generation) {
                Ok(transport) => Some(transport),
                Err(err) => {
                    tracing::warn!(%err, "p2p sync disabled: transport / moot setup failed");
                    out.emit(sync_offline_update());
                    None
                }
            };

        // The command loop: outbound verbs the kernel routes here, run to completion
        // on the runtime. Ends when the handle drops (the channel closes), which also
        // drops the runtime and stops the poll task.
        let overlay = sync_overlay_topic(moot_id);
        while let Ok(command) = commands.recv() {
            match command {
                SyncCommand::Connect(ticket) => {
                    let Some(transport) = transport.as_ref() else {
                        tracing::warn!("connect to peer: p2p is off");
                        continue;
                    };
                    let result = runtime.block_on(async {
                        let peer = transport
                            .add_peer_ticket(&ticket)
                            .await
                            .map_err(|e| format!("add peer: {e}"))?;
                        transport
                            .set_topics(peer, &[overlay])
                            .await
                            .map_err(|e| format!("set topics: {e}"))?;
                        Ok::<(), String>(())
                    });
                    match result {
                        Ok(()) => tracing::info!("p2p: connecting to peer (ticket bootstrap)"),
                        Err(err) => tracing::warn!(%err, "connect to peer failed"),
                    }
                }
                SyncCommand::UnlockNow => {
                    if transport.is_some() {
                        continue;
                    }
                    match activate_sync_lane(&runtime, &out, &dir, moot_id, &lane_generation) {
                        Ok(new_transport) => {
                            tracing::info!("p2p sync resumed after wallet unlock");
                            transport = Some(new_transport);
                        }
                        Err(err) => {
                            tracing::warn!(%err, "p2p sync stayed offline after wallet unlock")
                        }
                    }
                }
                SyncCommand::LockNow => {
                    transport = None;
                    lane_generation.fetch_add(1, Ordering::Relaxed);
                    out.emit(sync_offline_update());
                }
            }
        }
    })
}

fn sync_offline_update() -> SyncUpdate {
    SyncUpdate {
        active: false,
        status: SyncStatus::default(),
        standing: None,
        ticket: None,
    }
}

async fn build_sync_lane(
    dir: &Path,
    moot_id: [u8; 32],
) -> Result<(P2pandaTransport, TesseraSync, Option<ChainRoot>), String> {
    let Some(seed) = load_wallet_seed(dir) else {
        return Err(
            "wallet startup is locked; sync stays offline until unlock support lands".to_string(),
        );
    };
    let provider = InMemoryProvider::from_seed(seed);
    let keypair = provider.master_keypair().clone();
    let transport = P2pandaTransport::builder(&keypair)
        .gossip()
        .bind()
        .await
        .map_err(|e| format!("transport bind: {e}"))?;
    let (endpoint, gossip) = transport
        .sync_parts()
        .ok_or_else(|| "transport has no gossip overlay".to_string())?;
    let moots = dir.join("moots");
    let _ = std::fs::create_dir_all(&moots);
    let store = TesseraStore::open(moots.join(format!("{}.redb", hex32(&moot_id))))
        .map_err(|e| format!("tessera store: {e}"))?;
    if store.is_empty().unwrap_or(true) {
        author_starter_log(&provider, moot_id, &store);
    }
    let host_root = provider
        .derive_keypair(b"tessera-host-author")
        .ok()
        .map(|kp| ChainRoot(kp.public_key().to_bytes()));
    // Host-composed pump: build the tessera LogSync session over the store,
    // drain it via the shared `SyncedSpace`, and keep the session + handle alive.
    // The tessera lane is receive-only (authoring is a direct store.insert), so
    // nothing publishes on the handle.
    let log_sync = LogSync::builder(store.clone(), endpoint, gossip)
        .spawn()
        .await
        .map_err(|e| format!("logsync spawn: {e}"))?;
    let handle = log_sync
        .stream(Topic::from(moot_id), true)
        .await
        .map_err(|e| format!("logsync stream: {e}"))?;
    let sub = handle
        .subscribe()
        .await
        .map_err(|e| format!("logsync subscribe: {e}"))?;
    let accept_store = store.clone();
    let space = SyncedSpace::drive(sub, move |op| {
        std::future::ready(verify(&op) && matches!(accept_store.insert(&op), Ok(true)))
    });
    let moot = TesseraSync {
        space,
        store,
        moot_id,
        _keepalive: Box::new((log_sync, handle)),
    };
    Ok((transport, moot, host_root))
}

fn activate_sync_lane(
    runtime: &tokio::runtime::Runtime,
    out: &Emitter<SyncUpdate>,
    dir: &Path,
    moot_id: [u8; 32],
    lane_generation: &Arc<AtomicU64>,
) -> Result<P2pandaTransport, String> {
    let (transport, moot, host_root) = runtime.block_on(build_sync_lane(dir, moot_id))?;
    let generation = lane_generation.fetch_add(1, Ordering::Relaxed) + 1;
    let ticket = runtime
        .block_on(transport.ticket())
        .ok()
        .map(|ticket| ticket.to_string());
    if let Some(ticket) = &ticket {
        tracing::info!(%ticket, "p2p sync up: joined tessera demo moot — share this ticket with a peer");
    }
    let poll_out = out.clone();
    let poll_generation = lane_generation.clone();
    runtime.spawn(async move {
        let mut last: Option<SyncStatus> = None;
        loop {
            if poll_generation.load(Ordering::Relaxed) != generation {
                break;
            }
            let status = moot.sync_status();
            if last.as_ref() != Some(&status) {
                let standing = host_root.as_ref().and_then(|root| {
                    moot.ledger(TesseraConfig::default())
                        .ok()
                        .map(|ledger| ledger.score(root, now_ms()))
                });
                poll_out.emit(SyncUpdate {
                    active: true,
                    status: status.clone(),
                    standing,
                    ticket: ticket.clone(),
                });
                last = Some(status);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    Ok(transport)
}

/// Author a small `commit -> fulfil -> govern` tessera log for this host's own
/// persona (derived from its identity) into `store`, so a peer that connects has
/// a real log to catch up. Best-effort: a derivation / insert failure is skipped.
fn author_starter_log(provider: &InMemoryProvider, moot_id: [u8; 32], store: &TesseraStore) {
    let Ok(kp) = provider.derive_keypair(b"tessera-host-author") else {
        return;
    };
    let by = ChainRoot(kp.public_key().to_bytes());
    let cid = CommitmentId([0xc1; 32]);
    let e0 = TesseraEvent::CommitmentMade {
        by,
        commitment: cid,
        scope: Scope("host/session".into()),
        cadence_ms: 1_000,
        duration_ms: None,
        at_ms: 1_000,
    };
    let e1 = TesseraEvent::CommitmentFulfilled {
        by,
        commitment: cid,
        at_ms: 1_050,
    };
    let e2 = TesseraEvent::GovernanceParticipation { by, at_ms: 1_100 };
    let op0 = to_operation(&kp, moot_id, &e0, 0, None);
    let op1 = to_operation(&kp, moot_id, &e1, 1, Some(*op0.hash.as_bytes()));
    let op2 = to_operation(&kp, moot_id, &e2, 2, Some(*op1.hash.as_bytes()));
    for op in [op0, op1, op2] {
        let _ = store.insert(&op);
    }
}

/// Load the wallet root's shared master seed, warning and falling back to an
/// ephemeral launch-local seed if bootstrap failed earlier.
fn load_wallet_seed(data_root: &std::path::Path) -> Option<[u8; 32]> {
    match session_runtime::load_identity_seed(data_root) {
        Ok(Some(seed)) => Some(seed),
        Ok(None) => {
            if session_runtime::identity_seed_locked_at_startup(data_root) {
                tracing::warn!(
                    ?data_root,
                    "wallet startup is locked; sync identity stays offline this launch"
                );
                None
            } else {
                tracing::warn!(
                    ?data_root,
                    "identity seed missing; sync identity is ephemeral this launch"
                );
                Some(InMemoryProvider::random().master_keypair().to_seed())
            }
        }
        Err(err) => {
            if session_runtime::identity_seed_locked_at_startup(data_root) {
                tracing::warn!(
                    %err,
                    ?data_root,
                    "wallet startup is locked; sync identity stays offline this launch"
                );
                None
            } else {
                tracing::warn!(
                    %err,
                    ?data_root,
                    "identity seed unavailable; sync identity is ephemeral this launch"
                );
                Some(InMemoryProvider::random().master_keypair().to_seed())
            }
        }
    }
}

/// Lowercase hex of a 32-byte id, for a per-moot store filename.
fn hex32(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(64);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_indicator_projects_the_status() {
        let status = SyncStatus {
            syncing: true,
            sync_rounds: 1,
            ops_received: 3,
            last_activity_ms: Some(1_000),
        };
        let update = SyncUpdate {
            active: true,
            status,
            standing: Some(4),
            ticket: Some("dial:xyz".into()),
        };
        let indicator = to_indicator(&update, "tessera");
        assert_eq!(indicator.label, "tessera");
        assert!(indicator.active, "an update means the lane is joined");
        assert!(indicator.syncing);
        assert_eq!(indicator.ops, 3);
        assert_eq!(indicator.last_activity_ms, Some(1_000));
        assert_eq!(
            indicator.standing,
            Some(4),
            "the folded standing projects through"
        );
        assert_eq!(
            indicator.ticket(),
            Some("dial:xyz"),
            "the ticket projects through"
        );
        // A round in progress still wins the headline over standing.
        assert_eq!(indicator.summary(), "tessera: syncing");
    }

    #[test]
    fn a_freshly_joined_lane_reads_idle() {
        assert_eq!(
            to_indicator(
                &SyncUpdate {
                    active: true,
                    status: SyncStatus::default(),
                    standing: None,
                    ticket: None,
                },
                "tessera"
            )
            .summary(),
            "tessera: idle"
        );
    }

    #[test]
    fn hex32_is_64_lowercase_hex() {
        // DEMO_MOOT is [0x7e; 32], so its hex is "7e" repeated 32 times.
        assert_eq!(hex32(&DEMO_MOOT), "7e".repeat(32));
    }

    #[test]
    fn the_wallet_seed_is_stable_across_calls() {
        let dir = std::env::temp_dir().join(format!("meerkat-seed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        session_runtime::save_identity_seed(&dir, [0x77; 32]).unwrap();
        let first = load_wallet_seed(&dir).expect("seed should load");
        let second = load_wallet_seed(&dir).expect("seed should load");
        assert_eq!(
            first, second,
            "the persisted seed is reused on the next call"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
