/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host p2p sync subsystem (S5.0 foundation).
//!
//! The tessera counterpart of [`fetch`](crate::fetch): the same async-host seam,
//! a different payload. A [`SyncHost`] owns a tokio runtime, stands up one
//! `P2pandaTransport` and joins a tessera moot's `SyncedMoot` LogSync session on
//! it, then polls the lane's real `SyncStatus` and pushes each change over an
//! `mpsc` channel + an `EventLoopProxy<()>` wake (delivery model 2, shared with
//! the fetcher). The host drains it in `user_event` and folds it into the chrome
//! sync chip. See the host p2p wiring plan.
//!
//! Scope of this slice: it stands the subsystem up and surfaces honest status.
//! It does **not** bootstrap a peer (S5.1), so a lone instance shows `idle` with
//! nothing to sync — the true state. Identity is an ephemeral seed (S5.2 swaps in
//! the vault); the store is in-memory (S5.2 moves it under the session dir); the
//! moot is a fixed demo id (real moot selection is S5.3).

use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

use identity::InMemoryProvider;
use moothold::tessera::{SyncStatus, SyncedMoot, TesseraStore};
use tokio::runtime::Runtime;
use transport::P2pandaTransport;
use winit::event_loop::EventLoopProxy;

use meerkat::SyncIndicator;

/// The demo moot the foundation joins (real moot selection is S5.3).
pub const DEMO_MOOT: [u8; 32] = [0x7e; 32];
/// The ephemeral node seed (S5.0). S5.1 needs a *distinct per-instance* seed (a
/// launch flag) so two instances are different peers; S5.2 swaps in the vault.
pub const DEMO_SEED: [u8; 32] = [0x5e; 32];
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
/// stays `p2p off`) — networking is never fatal to the shell.
pub struct SyncHost {
    // All three are held for the host's lifetime; dropping the runtime first
    // (declaration order) aborts the poller + LogSync tasks before the lane and
    // transport they reference are dropped.
    _runtime: Runtime,
    _moot: Option<Arc<SyncedMoot>>,
    _transport: Option<P2pandaTransport>,
}

impl SyncHost {
    /// Build the sync subsystem: a runtime, the transport, and the tessera lane,
    /// plus a status poller that wakes the loop on change. Returns the host and
    /// the receiver the UI drains in `user_event`.
    pub fn new(
        proxy: EventLoopProxy<()>,
        moot_id: [u8; 32],
        seed: [u8; 32],
    ) -> (Self, Receiver<SyncUpdate>) {
        let (tx, rx) = channel();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build the sync runtime");

        // Bind the transport + join the moot once at startup (both async — iroh
        // endpoint + LogSync session). Block on setup; the lane's own drain task
        // then runs on this runtime.
        let setup: Result<(P2pandaTransport, SyncedMoot), String> = runtime.block_on(async {
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
            let store = TesseraStore::in_memory().map_err(|e| format!("tessera store: {e}"))?;
            let moot = SyncedMoot::join(endpoint, gossip, store, moot_id)
                .await
                .map_err(|e| format!("join moot: {e}"))?;
            Ok((transport, moot))
        });

        let host = match setup {
            Ok((transport, moot)) => {
                let moot = Arc::new(moot);
                // Poller: push an update whenever the lane's status changes,
                // waking the UI. Sends the initial (joined, idle) state at once.
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
                tracing::info!("p2p sync up: joined tessera demo moot");
                SyncHost {
                    _runtime: runtime,
                    _moot: Some(moot),
                    _transport: Some(transport),
                }
            }
            Err(err) => {
                tracing::warn!(%err, "p2p sync disabled: transport / moot setup failed");
                SyncHost {
                    _runtime: runtime,
                    _moot: None,
                    _transport: None,
                }
            }
        };
        (host, rx)
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
        // The honest chip text for a mid-round lane.
        assert_eq!(indicator.summary(), "tessera: syncing");
    }

    #[test]
    fn a_freshly_joined_lane_reads_idle() {
        // Default status (joined, nothing caught up) is the lone-peer truth.
        assert_eq!(to_indicator(&SyncStatus::default(), "tessera").summary(), "tessera: idle");
    }
}
