/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host p2p sync subsystem (S5.0 foundation + S5.1 connect).
//!
//! The tessera counterpart of [`fetch`](crate::fetch): the same async-host seam,
//! a different payload. A [`SyncHost`] owns a tokio runtime, stands up one
//! `P2pandaTransport` (a random per-launch identity) and joins a tessera moot's
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
//! Still S5.x-shaped: identity is ephemeral (S5.2 swaps in the vault), the store
//! is in-memory (S5.2 moves it under the session dir), the moot is a fixed demo
//! id (real moot selection is S5.3).

use std::sync::mpsc::Receiver;
use std::time::Duration;

use armillary::{spawn, ActorHandle, Emitter, Wake};
use identity::{IdentityProvider, InMemoryProvider};
use moothold::tessera::{
    to_operation, ChainRoot, CommitmentId, Scope, SyncStatus, SyncedMoot, TesseraEvent,
    TesseraStore,
};
use transport::{sync_overlay_topic, P2pandaTransport};

use meerkat::SyncIndicator;

/// The demo moot the foundation joins (real moot selection is S5.3).
pub const DEMO_MOOT: [u8; 32] = [0x7e; 32];
/// The chip label for the tessera lane.
pub const LANE_LABEL: &str = "tessera";

/// A change in a synced lane's status, delivered to the UI loop.
pub struct SyncUpdate {
    pub status: SyncStatus,
}

/// Project the real `SyncStatus` into the chrome's [`SyncIndicator`] view-model.
/// `active` is always `true` here: an update only exists once a lane is joined.
pub fn to_indicator(status: &SyncStatus, label: &str) -> SyncIndicator {
    SyncIndicator {
        label: label.to_string(),
        active: true,
        syncing: status.syncing,
        ops: status.ops_received,
        last_activity_ms: status.last_activity_ms,
    }
}

/// The sync actor's outbound commands. Today just "connect to a peer by ticket"
/// (the S5.1 verb); more sync verbs join here as their wiring lands.
pub enum SyncCommand {
    /// Dial a peer from its ticket string so the LogSync overlay forms and the two
    /// converge.
    Connect(String),
}

/// Spawn the sync subsystem as an [`armillary`] I/O actor. Its `run` closure builds
/// the tokio runtime *on the actor thread* (so armillary stays runtime-free), binds
/// one `P2pandaTransport` (random per-launch identity), joins the tessera moot's
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
    moot_id: [u8; 32],
) -> (ActorHandle<SyncCommand>, Receiver<SyncUpdate>) {
    spawn(wake, move |commands, out: Emitter<SyncUpdate>| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build the sync runtime");

        // Bind the transport + join the moot once at startup (both async). A random
        // identity makes each launched instance a distinct peer.
        let setup: Result<(P2pandaTransport, SyncedMoot), String> = runtime.block_on(async {
            let provider = InMemoryProvider::random();
            let keypair = provider.master_keypair().clone();
            let transport = P2pandaTransport::builder(&keypair)
                .gossip()
                .bind()
                .await
                .map_err(|e| format!("transport bind: {e}"))?;
            let (endpoint, gossip) = transport
                .sync_parts()
                .ok_or_else(|| "transport has no gossip overlay".to_string())?;
            let store = TesseraStore::in_memory().map_err(|e| format!("tessera store: {e}"))?;
            // Seed a small tessera log (commit -> fulfil -> govern) so a peer that
            // connects has something to catch up — otherwise both stores are empty
            // and convergence is invisible.
            author_starter_log(&provider, moot_id, &store);
            let moot = SyncedMoot::join(endpoint, gossip, store, moot_id)
                .await
                .map_err(|e| format!("join moot: {e}"))?;
            Ok((transport, moot))
        });

        let transport = match setup {
            Ok((transport, moot)) => {
                // Log this node's dialable ticket so it can be shared with a peer.
                if let Ok(ticket) = runtime.block_on(transport.ticket()) {
                    tracing::info!(%ticket, "p2p sync up: joined tessera demo moot — share this ticket with a peer");
                }
                // The status poll task emits each change through the actor's Emitter
                // (which wakes the kernel), running on the runtime's workers in the
                // background while the command loop below blocks on `recv`.
                // The task owns an Arc clone, so the moot (and its own LogSync drain
                // task) stays alive as long as this poll loop runs.
                let poll_out = out.clone();
                runtime.spawn(async move {
                    let mut last: Option<SyncStatus> = None;
                    loop {
                        let status = moot.sync_status();
                        if last.as_ref() != Some(&status) {
                            poll_out.emit(SyncUpdate { status: status.clone() });
                            last = Some(status);
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                });
                Some(transport)
            }
            Err(err) => {
                tracing::warn!(%err, "p2p sync disabled: transport / moot setup failed");
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
                },
            }
        }
    })
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
    let e1 = TesseraEvent::CommitmentFulfilled { by, commitment: cid, at_ms: 1_050 };
    let e2 = TesseraEvent::GovernanceParticipation { by, at_ms: 1_100 };
    let op0 = to_operation(&kp, moot_id, &e0, 0, None);
    let op1 = to_operation(&kp, moot_id, &e1, 1, Some(*op0.hash.as_bytes()));
    let op2 = to_operation(&kp, moot_id, &e2, 2, Some(*op1.hash.as_bytes()));
    for op in [op0, op1, op2] {
        let _ = store.insert(&op);
    }
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
        let indicator = to_indicator(&status, "tessera");
        assert_eq!(indicator.label, "tessera");
        assert!(indicator.active, "an update means the lane is joined");
        assert!(indicator.syncing);
        assert_eq!(indicator.ops, 3);
        assert_eq!(indicator.last_activity_ms, Some(1_000));
        assert_eq!(indicator.summary(), "tessera: syncing");
    }

    #[test]
    fn a_freshly_joined_lane_reads_idle() {
        assert_eq!(to_indicator(&SyncStatus::default(), "tessera").summary(), "tessera: idle");
    }
}
