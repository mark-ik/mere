//! Cabal sync over the gossip overlay + LogSync (P2pandaTransport-specific).
//!
//! [`Murm<T>`](crate::Murm) is generic over the transport, but gossip and
//! LogSync are [`P2pandaTransport`] capabilities. This module adds a concrete
//! `impl Murm<P2pandaTransport>` running both sync lanes of the
//! [sync-as-projection plan][plan] for a subscribed cabal:
//!
//! - **Live (gossip), Phase 3:** each authored post is broadcast to the cabal's
//!   gossip topic, and a background task ingests posts peers broadcast (verified
//!   + idempotent via [`CableEngine::ingest_post`]).
//! - **Offline catch-up (LogSync / RBSR), Phase 4:** a `LogSync` session over the
//!   cabal's redb store reconciles operations missed while a peer was away,
//!   draining each received operation into the engine via
//!   [`CableEngine::ingest_operation`]. LogSync joins gossip on the *derived*
//!   overlay topic ([`transport::sync_overlay_topic`]), so explicit bootstrap
//!   must tag peers with it.
//!
//! Together they replace the manual `push_cabal_to_peer` / `accept_*` snapshot
//! dance. Both ingest idempotently (content-addressed), so a post arriving on
//! both lanes lands once.
//!
//! [plan]: https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-06-02_logsync_sync_as_projection_plan.md

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use p2panda_core::Topic;
use p2panda_net::LogSync;
use p2panda_sync::protocols::TopicLogSyncEvent;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use transport::{GossipHandle, P2pandaTransport};

use crate::{decode_post, encode_post, CabalHandle, CabalKey, Murm, MurmError, Post, PostId};

/// Unix-epoch milliseconds, for stamping the last sync activity.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A snapshot of a cabal's sync activity, for a real (non-placebo) sync
/// indicator.
///
/// Counters accumulate over the cabal's subscription; read a snapshot with
/// [`SyncedCabal::sync_status`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncStatus {
    /// A LogSync reconciliation round is currently in progress.
    pub syncing: bool,
    /// LogSync rounds that have finished (a peer's catch-up completing).
    pub sync_rounds: u64,
    /// Operations caught up via LogSync (the offline-catch-up lane).
    pub ops_received: u64,
    /// Posts received live over the gossip overlay (may rarely double-count a
    /// re-broadcast).
    pub posts_received: u64,
    /// Unix-epoch milliseconds of the most recent sync or gossip activity, or
    /// `None` if nothing has arrived yet.
    pub last_activity_ms: Option<u64>,
}

/// The result of a manual [`SyncedCabal::resync`] checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncRound {
    /// New items (LogSync operations + gossip posts) that landed during the
    /// checkpoint window. `0` means already up to date.
    pub items_received: u64,
}

impl Murm<P2pandaTransport> {
    /// Open a cabal and join its gossip overlay for live sync.
    ///
    /// Returns a [`SyncedCabal`]: authoring through it broadcasts each post to
    /// peers in the cabal, and posts peers broadcast are ingested in the
    /// background. The cabal's gossip topic is its `cabal_id`.
    ///
    /// Requires the transport to have been built with
    /// [`.gossip()`](transport::P2pandaTransport). Until discovery is wired into
    /// this path, peers must be tagged with the cabal topic via the transport's
    /// explicit bootstrap (`add_peer` + `set_topics`).
    pub async fn subscribe_cabal(&self, cabal_key: &CabalKey) -> Result<SyncedCabal, MurmError> {
        let handle = self.open_cabal(cabal_key)?;
        let cabal_id = *handle.id().as_bytes();
        let status = Arc::new(Mutex::new(SyncStatus::default()));

        // ── Live lane: the cabal's gossip overlay (topic = cabal_id) ──
        //
        // Background task ingests posts peers broadcast. `ingest_post` verifies
        // the signature, the self-describing cabal id, and the per-author log
        // rule, and is idempotent — duplicate, foreign, or malformed posts are
        // rejected and dropped. Successful ingests bump the sync status.
        let gossip = self.transport().subscribe(cabal_id).await?;
        let engine = Arc::clone(self.cable());
        let gossip_status = Arc::clone(&status);
        let mut received = gossip.subscribe();
        let gossip_task = tokio::spawn(async move {
            while let Some(item) = received.next().await {
                let Ok(bytes) = item else { continue };
                let Ok(post) = decode_post(&bytes) else { continue };
                if engine.ingest_post(&cabal_id, post).is_ok() {
                    let mut s = gossip_status.lock().unwrap();
                    s.posts_received += 1;
                    s.last_activity_ms = Some(now_ms());
                }
            }
        });

        // ── Offline catch-up lane: a LogSync (RBSR) session over the cabal's
        // redb store ──
        //
        // Reconciles operations missed while a peer was away; each received
        // operation is ingested (idempotent with the gossip lane) and recorded in
        // the sync status. Only spawned when the transport has a gossip overlay,
        // which LogSync rides for peer-sampling. The same parts power `resync`.
        let logsync_task = match self.transport().sync_parts() {
            Some((endpoint, gossip_actor)) => {
                let store = self
                    .cable()
                    .cabal_store(&cabal_id)
                    .ok_or_else(|| MurmError::Backend("cabal store missing".to_string()))?;
                let log_sync = LogSync::builder(store, endpoint, gossip_actor)
                    .spawn()
                    .await
                    .map_err(|e| MurmError::Backend(format!("logsync spawn: {e}")))?;
                let sync_handle = log_sync
                    .stream(Topic::from(cabal_id), true)
                    .await
                    .map_err(|e| MurmError::Backend(format!("logsync stream: {e}")))?;
                let mut sub = sync_handle
                    .subscribe()
                    .await
                    .map_err(|e| MurmError::Backend(format!("logsync subscribe: {e}")))?;
                let engine = Arc::clone(self.cable());
                let logsync_status = Arc::clone(&status);
                Some(tokio::spawn(async move {
                    // Hold the session handles for the task's lifetime; dropping
                    // them ends the LogSync session.
                    let _log_sync = log_sync;
                    let _sync_handle = sync_handle;
                    while let Some(item) = sub.next().await {
                        let Ok(from_sync) = item else { continue };
                        match from_sync.event {
                            TopicLogSyncEvent::SyncStarted { .. } => {
                                let mut s = logsync_status.lock().unwrap();
                                s.syncing = true;
                                s.last_activity_ms = Some(now_ms());
                            }
                            TopicLogSyncEvent::OperationReceived { operation, .. } => {
                                if engine.ingest_operation(&cabal_id, &operation).is_ok() {
                                    let mut s = logsync_status.lock().unwrap();
                                    s.ops_received += 1;
                                    s.last_activity_ms = Some(now_ms());
                                }
                            }
                            TopicLogSyncEvent::SyncFinished { .. } => {
                                let mut s = logsync_status.lock().unwrap();
                                s.syncing = false;
                                s.sync_rounds += 1;
                                s.last_activity_ms = Some(now_ms());
                            }
                            _ => {}
                        }
                    }
                }))
            }
            None => None,
        };

        Ok(SyncedCabal {
            handle,
            gossip,
            gossip_task,
            logsync_task,
            status,
        })
    }
}

/// A cabal joined to its sync overlays (gossip + LogSync).
///
/// Authoring through it stores the post locally *and* broadcasts it to peers
/// (gossip); posts peers broadcast are ingested in the background, and a LogSync
/// session reconciles anything missed while a peer was away. So [`history`]
/// converges across online members *and* catches up peers that were offline.
/// Dropping it leaves the overlays and stops both background tasks.
///
/// [`history`]: SyncedCabal::history
pub struct SyncedCabal {
    handle: CabalHandle,
    gossip: GossipHandle,
    /// Live lane: ingests posts peers broadcast over the gossip overlay.
    gossip_task: JoinHandle<()>,
    /// Offline-catch-up lane: drains operations a LogSync session reconciles.
    /// `None` if the transport has no gossip overlay (LogSync needs it).
    logsync_task: Option<JoinHandle<()>>,
    /// Live sync activity, both lanes write here; read via [`sync_status`].
    ///
    /// [`sync_status`]: SyncedCabal::sync_status
    status: Arc<Mutex<SyncStatus>>,
}

impl SyncedCabal {
    /// Author a text post: store it locally and broadcast it to peers in the
    /// cabal. Returns the post id.
    pub async fn send_text(&self, channel: &str, text: &str) -> Result<PostId, MurmError> {
        let id = self.handle.send_text(channel, text)?;
        if let Some(post) = self.handle.get_post(&id) {
            self.gossip
                .publish(encode_post(&post))
                .await
                .map_err(|e| MurmError::Backend(format!("gossip publish: {e}")))?;
        }
        Ok(id)
    }

    /// All posts in a channel (local view; converges as peers' posts arrive).
    pub fn history(&self, channel: &str) -> Vec<Post> {
        self.handle.history(channel)
    }

    /// The underlying cabal handle, for operations not exposed here.
    pub fn handle(&self) -> &CabalHandle {
        &self.handle
    }

    /// A snapshot of this cabal's sync activity, for a real sync indicator:
    /// whether a round is in progress, rounds finished, operations caught up via
    /// LogSync, posts received live over gossip, and when activity last happened.
    pub fn sync_status(&self) -> SyncStatus {
        self.status.lock().unwrap().clone()
    }

    /// Run a manual "sync now" checkpoint and report what arrived.
    ///
    /// The gossip + LogSync lanes already sync **continuously**, so this is not a
    /// fake spinner and not a forced full re-fetch (p2panda 0.6.1 exposes no
    /// public "re-initiate" hook): it watches the live lanes until they go quiet
    /// and returns the real count of items that landed during the window. It
    /// returns as soon as activity settles (typically well under a second), so a
    /// quiet, already-synced cabal reports `0` quickly rather than spinning.
    pub async fn resync(&self) -> Result<SyncRound, MurmError> {
        let received = || {
            let s = self.status.lock().unwrap();
            s.ops_received + s.posts_received
        };
        let start = received();
        let mut last = start;
        let mut quiet = 0u8;
        // Poll up to ~3s; stop after ~300ms with nothing new (settled).
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let now = received();
            if now == last {
                quiet += 1;
                if quiet >= 3 {
                    break;
                }
            } else {
                last = now;
                quiet = 0;
            }
        }
        Ok(SyncRound {
            items_received: last.saturating_sub(start),
        })
    }
}

impl Drop for SyncedCabal {
    fn drop(&mut self) {
        // Stop both sync lanes when the synced cabal goes away.
        self.gossip_task.abort();
        if let Some(task) = &self.logsync_task {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::{IdentityProvider, InMemoryProvider};
    use std::time::Duration;
    use transport::{PeerID, P2pandaTransport};

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_murm_peers_converge_over_gossip() {
        let alice_provider = Arc::new(InMemoryProvider::from_seed([30; 32]));
        let bob_provider = Arc::new(InMemoryProvider::from_seed([31; 32]));
        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());
        let alice_kp = alice_provider.master_keypair().clone();
        let bob_kp = bob_provider.master_keypair().clone();

        let alice_t = P2pandaTransport::builder(&alice_kp)
            .gossip()
            .bind()
            .await
            .expect("bind alice");
        let bob_t = P2pandaTransport::builder(&bob_kp)
            .gossip()
            .bind()
            .await
            .expect("bind bob");

        let cabal_key = CabalKey::new([0x77; 32]);
        let cabal_id = murmuring::cable::hash_cabal_id(cabal_key.as_bytes());

        // Explicit bootstrap: cross-register transport addresses + tag the cabal
        // topic so gossip can form the overlay (discovery does this in prod).
        alice_t
            .add_peer(bob_t.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        alice_t.set_topics(bob_id, &[cabal_id]).await.unwrap();
        bob_t.add_peer(alice_t.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob_t.set_topics(alice_id, &[cabal_id]).await.unwrap();

        let alice = Murm::new(alice_provider, alice_t);
        let bob = Murm::new(bob_provider, bob_t);

        let alice_cabal = alice.subscribe_cabal(&cabal_key).await.expect("alice cabal");
        let bob_cabal = bob.subscribe_cabal(&cabal_key).await.expect("bob cabal");

        // Alice authors; bob's background task should ingest it over gossip.
        // Re-publish until convergence (the overlay needs a moment to form),
        // bounded by a timeout.
        let convergence = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                alice_cabal
                    .send_text("session", "hello over gossip")
                    .await
                    .expect("alice send");
                let bob_has = bob_cabal.history("session").iter().any(|p| {
                    matches!(&p.kind, crate::PostKind::Text { text, .. } if text == "hello over gossip")
                });
                if bob_has {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await;

        assert!(
            convergence.is_ok(),
            "bob converged on alice's post over gossip within the timeout"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_murm_peers_catch_up_over_logsync() {
        // Offline catch-up: alice authors three posts *before* bob is connected,
        // so the gossip overlay never delivers them to bob. Bob then comes online
        // and catches up via LogSync (RBSR) over the redb store.
        let alice_provider = Arc::new(InMemoryProvider::from_seed([40; 32]));
        let bob_provider = Arc::new(InMemoryProvider::from_seed([41; 32]));
        let alice_id = PeerID::from_public_key(alice_provider.master_public_key());
        let bob_id = PeerID::from_public_key(bob_provider.master_public_key());
        let alice_kp = alice_provider.master_keypair().clone();
        let bob_kp = bob_provider.master_keypair().clone();

        let alice_t = P2pandaTransport::builder(&alice_kp)
            .gossip()
            .bind()
            .await
            .expect("bind alice");
        let bob_t = P2pandaTransport::builder(&bob_kp)
            .gossip()
            .bind()
            .await
            .expect("bind bob");

        let cabal_key = CabalKey::new([0x88; 32]);
        let cabal_id = murmuring::cable::hash_cabal_id(cabal_key.as_bytes());
        // LogSync joins gossip on the *derived* overlay topic — tag peers with it
        // (not the raw cabal id), or the catch-up overlay never forms.
        let overlay = transport::sync_overlay_topic(cabal_id);

        // Bootstrap the transports before constructing Murm. The overlay only
        // forms once both peers subscribe, so tagging now does not leak alice's
        // pre-authored posts over gossip.
        alice_t
            .add_peer(bob_t.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        alice_t.set_topics(bob_id, &[overlay]).await.unwrap();
        bob_t
            .add_peer(alice_t.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob_t.set_topics(alice_id, &[overlay]).await.unwrap();

        let alice = Murm::new(alice_provider, alice_t);
        let bob = Murm::new(bob_provider, bob_t);

        // Alice subscribes (her LogSync starts serving) and authors three posts
        // while bob is absent — these never reach bob over gossip.
        let alice_cabal = alice
            .subscribe_cabal(&cabal_key)
            .await
            .expect("alice cabal");
        for i in 0..3 {
            alice_cabal
                .send_text("session", &format!("msg-{i}"))
                .await
                .expect("alice send");
        }

        // Bob comes online; LogSync catches him up on alice's three posts.
        let bob_cabal = bob.subscribe_cabal(&cabal_key).await.expect("bob cabal");

        let caught_up = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if bob_cabal.history("session").len() >= 3 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await;

        assert!(
            caught_up.is_ok(),
            "bob caught up on alice's 3 posts over LogSync within the timeout"
        );
        assert_eq!(
            bob_cabal.history("session").len(),
            3,
            "bob holds exactly alice's three posts"
        );

        // Real (non-placebo) sync feedback: bob's status reflects the catch-up.
        let status = bob_cabal.sync_status();
        assert!(
            status.ops_received >= 3,
            "sync status recorded the caught-up operations (got {})",
            status.ops_received
        );
        assert!(
            status.last_activity_ms.is_some(),
            "sync status carries a last-activity timestamp"
        );

        // The manual "sync now" checkpoint returns quickly (already synced, so
        // nothing new lands) and leaves history intact.
        let round = bob_cabal.resync().await.expect("resync runs");
        println!("resync checkpoint: items_received={}", round.items_received);
        assert_eq!(
            bob_cabal.history("session").len(),
            3,
            "resync leaves history intact"
        );
    }
}
