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

use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

use identity::{IdentityProvider, InMemoryProvider};
use moothold::tessera::{
    to_operation, ChainRoot, CommitmentId, Scope, SyncStatus, SyncedMoot, TesseraEvent,
    TesseraStore,
};
use tokio::runtime::Runtime;
use transport::{sync_overlay_topic, P2pandaTransport};
use winit::event_loop::EventLoopProxy;

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

/// Owns the sync runtime + the joined lane. Construction binds the transport and
/// joins the moot (once, at startup); a background poller then pushes status
/// changes to the UI. If setup fails, p2p is simply off (no updates, the chip
/// stays `p2p off`, [`connect`](Self::connect) errors) — networking is never
/// fatal to the shell.
pub struct SyncHost {
    /// The sync runtime: holds the lane + poller tasks, and runs the brief
    /// transport calls of [`connect`](Self::connect) to completion (called from
    /// the UI thread, which is never a runtime worker).
    runtime: Runtime,
    /// The moot this lane syncs (the LogSync topic; its overlay is the
    /// [`connect`](Self::connect) target).
    moot_id: [u8; 32],
    /// The bound transport, kept for `connect` / `my_ticket`. `None` if setup
    /// failed (p2p off).
    transport: Option<P2pandaTransport>,
    /// The joined lane, held so its drain task lives. `None` if setup failed.
    _moot: Option<Arc<SyncedMoot>>,
}

impl SyncHost {
    /// Build the sync subsystem: a runtime, the transport (random identity), and
    /// the tessera lane (seeded with a starter log), plus a status poller that
    /// wakes the loop on change. Returns the host and the receiver the UI drains
    /// in `user_event`.
    pub fn new(proxy: EventLoopProxy<()>, moot_id: [u8; 32]) -> (Self, Receiver<SyncUpdate>) {
        let (tx, rx) = channel();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build the sync runtime");

        // Bind the transport + join the moot once at startup (both async — iroh
        // endpoint + LogSync session). A random identity makes each launched
        // instance a distinct peer. The lane's own drain task runs on this runtime.
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

        let host = match setup {
            Ok((transport, moot)) => {
                let moot = Arc::new(moot);
                // Log this node's dialable ticket so it can be shared with a peer
                // (the S5.1 "connect to peer" exchange).
                if let Ok(ticket) = runtime.block_on(transport.ticket()) {
                    tracing::info!(%ticket, "p2p sync up: joined tessera demo moot — share this ticket with a peer");
                }
                let poll_moot = Arc::clone(&moot);
                let poll_tx = tx.clone();
                let poll_proxy = proxy.clone();
                runtime.spawn(async move {
                    let mut last: Option<SyncStatus> = None;
                    loop {
                        let status = poll_moot.sync_status();
                        if last.as_ref() != Some(&status) {
                            if poll_tx.send(SyncUpdate { status: status.clone() }).is_err() {
                                break; // the UI receiver is gone (shutting down)
                            }
                            let _ = poll_proxy.send_event(());
                            last = Some(status);
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                });
                SyncHost {
                    runtime,
                    moot_id,
                    transport: Some(transport),
                    _moot: Some(moot),
                }
            }
            Err(err) => {
                tracing::warn!(%err, "p2p sync disabled: transport / moot setup failed");
                SyncHost {
                    runtime,
                    moot_id,
                    transport: None,
                    _moot: None,
                }
            }
        };
        (host, rx)
    }

    /// Connect the lane to a peer from its `ticket` string: register it and tag it
    /// on this moot's overlay topic so the LogSync overlay forms and the two
    /// converge. The outbound command of the sync I/O actor (the "connect to
    /// peer" verb). Brief (it does not wait on convergence); errors if p2p is off
    /// or the ticket is malformed.
    pub fn connect(&self, ticket: &str) -> Result<(), String> {
        let transport = self.transport.as_ref().ok_or("p2p is off")?;
        let overlay = sync_overlay_topic(self.moot_id);
        self.runtime.block_on(async {
            let peer = transport
                .add_peer_ticket(ticket)
                .await
                .map_err(|e| format!("add peer: {e}"))?;
            transport
                .set_topics(peer, &[overlay])
                .await
                .map_err(|e| format!("set topics: {e}"))?;
            Ok::<(), String>(())
        })?;
        tracing::info!("p2p: connecting to peer (ticket bootstrap)");
        Ok(())
    }
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
