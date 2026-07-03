/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host comms subsystem (P6c): the live counterpart of the pane's placeholder
//! data. The tessera/sync sibling — same async-host seam ([`fetch`](crate::fetch)
//! / [`sync`](crate::sync)), a different payload.
//!
//! An [`armillary`] I/O actor owns a tokio runtime, stands up a [`comms::Comms`]
//! with the misfin and murm adapters over **real-but-local** stores, and serves
//! load / send commands, emitting results back over the wake. The host drains
//! them in `user_event` and folds them into the docked comms pane.
//!
//! Backends: the misfin mailbox is a real on-disk redb store (seeded with one demo
//! message) that the read adapter surfaces; **misfin sending is live** over
//! `errand::misfin_send` with a vault-derived client cert ([`build_misfin_sender`]),
//! its status surfaced under the compose box. The murm cabal is **networked**:
//! [`build_cabal`] binds a real `P2pandaTransport` and `subscribe_cabal`s the demo
//! cabal (gossip + LogSync), so authoring gossips to peers and a peer's posts land
//! over the wire. A background task drains the cabal's live `subscribe()` stream so
//! the pane updates the open thread + list the moment a post arrives. The transport
//! binds locally even offline (the cabal works locally and syncs once a peer
//! connects). Still to come: the connect-by-ticket step (the ticket is logged at
//! startup) and the misfin receive server.

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use armillary::{ActorHandle, Emitter, Wake, spawn};
use comms::misfin_adapter::MisfinAdapter;
use comms::murm_adapter::MurmAdapter;
use comms::{
    Comms, ConversationId, Draft, Identity, Inbox, Message, ProtocolAdapter, ProtocolKind,
};
use errand::ClientIdentity;
use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
use misfin::{
    MISFIN_PORT, MailboxStore, MisfinAddress, MisfinIdentitySpec, MisfinSender, MisfinServer,
    MisfinServerConfig, ServedMailbox, certificate_fingerprint, deterministic_identity,
    identity_salt,
};
use murm::{CabalKey, Murm, Post};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::sync::broadcast;
use transport::{P2pandaTransport, sync_overlay_topic};

/// The demo cabal's key — both installs share it, so two peers are on the same
/// cabal automatically (a real join path replaces the fixed key later). Its public
/// id is `BLAKE3(key)`.
const DEMO_CABAL_KEY: [u8; 32] = [0x5c; 32];

/// The result of standing up the comms backends: the actor's `CommsBackends`, the
/// live-cabal stream + conversation id (for the subscribe-drain), and this
/// install's connect info (misfin receive address + cabal join ticket).
struct CommsSetup {
    backends: CommsBackends,
    cabal_live: Option<(broadcast::Receiver<Post>, ConversationId)>,
    misfin_address: String,
    cabal_ticket: Option<String>,
}

/// This install's misfin receive address: `me@<lan-ip>`, **host-anchored** so two
/// installs on a LAN don't collide (the host is the namespace authority, like
/// email). For trusted LAN / direct addressing; reaching strangers privately is
/// the key-anchored-over-iroh path. Falls back to an unreachable `me@mere.local`
/// when no LAN IP is found.
fn local_misfin_address() -> String {
    match local_lan_ip() {
        Some(ip) => format!("me@{ip}"),
        None => {
            tracing::warn!("comms: no LAN IP found; misfin receive address is not reachable");
            "me@mere.local".to_string()
        }
    }
}

/// The LAN IP this machine uses for outbound traffic (what a peer on the same
/// network reaches it at). Reads a UDP socket's chosen local address; sends
/// nothing.
fn local_lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

/// A command the host routes to the comms actor.
pub enum CommsCommand {
    /// Reload the merged conversation list.
    Refresh,
    /// Load one conversation's messages.
    Open(ConversationId),
    /// Send a draft, then reload its thread + the list.
    Send(Draft),
    /// Connect the cabal to a peer from its join ticket (bootstrap the overlay so
    /// the two converge).
    ConnectCabal(String),
}

/// A result the comms actor delivers back to the UI loop.
#[derive(Clone)]
pub enum CommsUpdate {
    /// The merged conversation list (+ any backend failures).
    Inbox(Inbox),
    /// One conversation's messages.
    Thread(ConversationId, Vec<Message>),
    /// A draft was sent to this conversation (clear the compose buffer).
    Sent(ConversationId),
    /// A one-line outcome of the last send, shown under the compose box (a misfin
    /// status, or a failure reason).
    SendOutcome(String),
    /// This install's connect info: the misfin receive address, and the cabal join
    /// ticket (when the networked cabal is up) to share with a peer.
    Identity {
        misfin_address: String,
        cabal_ticket: Option<String>,
    },
}

/// The vault-derived client identity this install sends misfin mail with: the
/// certificate + private key (DER), minted once at startup from the comms persona.
struct MisfinSendIdentity {
    cert_der: Vec<u8>,
    key_pkcs8_der: Vec<u8>,
}

/// The comms actor's backends: the shared read/route model, the misfin send
/// identity (whose send rides errand at the host, not the read-only adapter), and
/// the murm `Murm` kept alive (it owns the transport + cabal engine).
struct CommsBackends {
    comms: Arc<Comms>,
    /// `None` when the send identity could not be derived (misfin send disabled).
    misfin_sender: Option<MisfinSendIdentity>,
    /// Keeps the murm transport + cabal engine alive, and drives `ConnectCabal`.
    /// `None` when the networked cabal couldn't be set up.
    murm: Option<Murm<P2pandaTransport>>,
    /// The cabal's public id, for tagging the overlay topics on connect.
    cabal_id: Option<[u8; 32]>,
}

/// Spawn the comms subsystem as an [`armillary`] I/O actor. The run closure builds
/// a tokio runtime on the actor thread, stands up [`Comms`] over local stores
/// (under `dir`), emits the initial inbox, then serves [`CommsCommand`]s. Setup
/// failure disables comms (no updates), never the shell. Returns the kernel's
/// command handle and the receiver it drains in `user_event`.
pub fn spawn_comms(wake: Wake, dir: PathBuf) -> (ActorHandle<CommsCommand>, Receiver<CommsUpdate>) {
    spawn(wake, move |commands, out: Emitter<CommsUpdate>| {
        // Multi-thread: the cabal's gossip + LogSync background tasks and the live
        // subscribe-drain run continuously on worker threads, not only during a
        // `block_on`.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build the comms runtime");

        let CommsSetup {
            backends,
            cabal_live,
            misfin_address,
            cabal_ticket,
        } = match runtime.block_on(build_comms(&dir)) {
            Ok(setup) => setup,
            Err(err) => {
                tracing::warn!(%err, "comms disabled: backend setup failed");
                return;
            }
        };

        // Surface this install's connect info: the misfin receive address + the
        // cabal join ticket to share with a peer.
        out.emit(CommsUpdate::Identity {
            misfin_address,
            cabal_ticket,
        });

        // The live cabal lane: a post landing (a peer's over gossip / LogSync, or
        // our own) refreshes the open thread + the list, so the pane updates without
        // a manual reload.
        if let Some((mut rx, conversation)) = cabal_live {
            let drain_comms = backends.comms.clone();
            let drain_out = out.clone();
            runtime.spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(_post) => {
                            if let Ok(messages) = drain_comms.messages(&conversation).await {
                                drain_out.emit(CommsUpdate::Thread(conversation.clone(), messages));
                            }
                            drain_out.emit(CommsUpdate::Inbox(drain_comms.inbox().await));
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        let comms = &backends.comms;
        // Emit the initial inbox so the pane has data the first time it opens.
        out.emit(CommsUpdate::Inbox(runtime.block_on(comms.inbox())));

        while let Ok(command) = commands.recv() {
            match command {
                CommsCommand::Refresh => {
                    out.emit(CommsUpdate::Inbox(runtime.block_on(comms.inbox())));
                }
                CommsCommand::Open(id) => match runtime.block_on(comms.messages(&id)) {
                    Ok(messages) => out.emit(CommsUpdate::Thread(id, messages)),
                    Err(err) => tracing::warn!(%err, "comms: loading a thread failed"),
                },
                CommsCommand::Send(draft) => {
                    let Some(target) = draft.conversation.clone() else {
                        continue;
                    };
                    if target.protocol == ProtocolKind::Misfin {
                        // misfin send rides errand at the host (the adapter is
                        // read-only). Sent mail is not kept locally, so there's no
                        // thread echo — the outcome line is the feedback.
                        match send_misfin(&runtime, backends.misfin_sender.as_ref(), &draft) {
                            Ok((delivered, line)) => {
                                out.emit(CommsUpdate::SendOutcome(line));
                                if delivered {
                                    out.emit(CommsUpdate::Sent(target));
                                }
                            }
                            Err(err) => {
                                out.emit(CommsUpdate::SendOutcome(format!("Send failed: {err}")));
                            }
                        }
                    } else {
                        // murm: route through the adapter, then reload the thread
                        // (the new post shows) + the list (recency changed).
                        match runtime.block_on(comms.send(&draft)) {
                            Ok(_) => {
                                if let Ok(messages) = runtime.block_on(comms.messages(&target)) {
                                    out.emit(CommsUpdate::Thread(target.clone(), messages));
                                }
                                out.emit(CommsUpdate::Sent(target));
                                out.emit(CommsUpdate::Inbox(runtime.block_on(comms.inbox())));
                            }
                            Err(err) => tracing::warn!(%err, "comms: send failed"),
                        }
                    }
                }
                CommsCommand::ConnectCabal(ticket) => {
                    let outcome = connect_cabal(&runtime, &backends, &ticket);
                    out.emit(CommsUpdate::SendOutcome(outcome));
                }
            }
        }
    })
}

/// Connect the cabal to a peer from its join `ticket`: register the peer's address
/// and tag the cabal's gossip + LogSync overlay topics, so the two converge.
/// Returns a one-line status for the pane.
fn connect_cabal(
    runtime: &tokio::runtime::Runtime,
    backends: &CommsBackends,
    ticket: &str,
) -> String {
    let (Some(murm), Some(cabal_id)) = (backends.murm.as_ref(), backends.cabal_id) else {
        return "Cabal is not networked".to_string();
    };
    let result = runtime.block_on(async {
        let peer = murm
            .transport()
            .add_peer_ticket(ticket)
            .await
            .map_err(|e| format!("add peer: {e}"))?;
        murm.transport()
            .set_topics(peer, &[cabal_id, sync_overlay_topic(cabal_id)])
            .await
            .map_err(|e| format!("set topics: {e}"))?;
        Ok::<(), String>(())
    });
    match result {
        Ok(()) => "Joining cabal — syncing with the peer…".to_string(),
        Err(err) => format!("Cabal connect failed: {err}"),
    }
}

/// Build the live [`Comms`] over backends under `dir`, the misfin send identity,
/// and (when the cabal comes up) the live post stream + cabal conversation id for
/// the subscribe-drain. The shared identity root drives the murm transport key and
/// the misfin cert derivation.
async fn build_comms(dir: &Path) -> Result<CommsSetup, String> {
    let comms_dir = dir.join("comms");
    std::fs::create_dir_all(&comms_dir).map_err(|error| error.to_string())?;

    let seed = load_wallet_seed(dir);
    let provider: Arc<dyn IdentityProvider> = Arc::new(InMemoryProvider::from_seed(seed));
    let misfin_address = local_misfin_address();

    // misfin: a real on-disk mailbox (the adapter reads it; the server writes into
    // it) + a vault-derived cert (the send identity and the server's TLS identity).
    let mailbox = MailboxStore::open(comms_dir.join("mailbox.redb")).map_err(|e| e.to_string())?;
    seed_mailbox_if_empty(&mailbox, &misfin_address);
    let misfin_sender = match build_misfin_sender(&provider, &misfin_address) {
        Ok(sender) => Some(sender),
        Err(err) => {
            tracing::warn!(%err, "misfin identity unavailable; misfin send/receive disabled");
            None
        }
    };

    // Stand up the in-app misfin receive server (host-anchored at `misfin_address`,
    // LAN / trusted-host use): it presents the vault cert and lands received mail in
    // the same mailbox the adapter reads. Bind failure disables receive, not comms.
    if let Some(sender) = &misfin_sender {
        start_misfin_server(sender, &misfin_address, mailbox.clone()).await;
    }

    let misfin = MisfinAdapter::new(
        Identity::new(ProtocolKind::Misfin, misfin_address.as_str()),
        misfin_address.as_str(),
        mailbox,
    );
    let mut comms = Comms::new().with_adapter(Box::new(misfin));

    // murm: a networked cabal over a real p2panda transport. Bind failure disables
    // the cabal (misfin still works); otherwise the cabal works locally and syncs
    // once a peer connects.
    let (murm, cabal_id, cabal_live, cabal_ticket) = match build_cabal(&provider, seed).await {
        Ok((murm, adapter, rx, conversation, ticket, cabal_id)) => {
            comms = comms.with_adapter(adapter);
            (Some(murm), Some(cabal_id), Some((rx, conversation)), ticket)
        }
        Err(err) => {
            tracing::warn!(%err, "comms: murm cabal unavailable; misfin only");
            (None, None, None, None)
        }
    };

    Ok(CommsSetup {
        backends: CommsBackends {
            comms: Arc::new(comms),
            misfin_sender,
            murm,
            cabal_id,
        },
        cabal_live,
        misfin_address,
        cabal_ticket,
    })
}

/// Bind + serve the in-app misfin receive server on `:1958`, serving `address`
/// with the vault cert, landing mail in `store`. Best-effort: any setup failure
/// just disables receive (logged).
async fn start_misfin_server(sender: &MisfinSendIdentity, address: &str, store: MailboxStore) {
    let Ok(served_address) = MisfinAddress::parse(address) else {
        return;
    };
    let config = MisfinServerConfig {
        tls_certificate_der: sender.cert_der.clone(),
        tls_private_key_pkcs8_der: sender.key_pkcs8_der.clone(),
        served: vec![ServedMailbox {
            address: served_address,
            fingerprint: certificate_fingerprint(&sender.cert_der),
        }],
    };
    let server = match MisfinServer::new(config, store) {
        Ok(server) => server,
        Err(err) => {
            tracing::warn!(%err, "comms: misfin server setup failed");
            return;
        }
    };
    match server
        .bind(SocketAddr::from(([0, 0, 0, 0], MISFIN_PORT)))
        .await
    {
        Ok(bound) => {
            tracing::info!(%address, "comms: misfin receive server up");
            tokio::spawn(async move {
                let _ = bound.serve(std::future::pending::<()>()).await;
            });
        }
        Err(err) => tracing::warn!(%err, "comms: misfin server bind failed; receive disabled"),
    }
}

/// Stand up the networked murm cabal: bind a p2panda transport (logging the join
/// ticket), `subscribe_cabal` (gossip + LogSync), seed a welcome post, and return
/// the keep-alive [`Murm`], the read/write adapter, the live post stream, and the
/// cabal conversation id.
#[allow(clippy::type_complexity)]
async fn build_cabal(
    provider: &Arc<dyn IdentityProvider>,
    seed: [u8; 32],
) -> Result<
    (
        Murm<P2pandaTransport>,
        Box<dyn ProtocolAdapter>,
        broadcast::Receiver<Post>,
        ConversationId,
        Option<String>,
        [u8; 32],
    ),
    String,
> {
    let keypair = Ed25519Keypair::from_seed(seed);
    let transport = P2pandaTransport::builder(&keypair)
        .gossip()
        .bind()
        .await
        .map_err(|e| format!("transport bind: {e}"))?;
    let ticket = transport.ticket().await.ok();
    if let Some(ticket) = &ticket {
        tracing::info!(%ticket, "comms: murm cabal up — share this ticket to join the cabal");
    }
    let murm = Murm::new(provider.clone(), transport);
    let synced = murm
        .subscribe_cabal(&CabalKey::new(DEMO_CABAL_KEY))
        .await
        .map_err(|e| format!("subscribe cabal: {e}"))?;
    if synced.history("session").is_empty() {
        let _ = synced
            .send_text("session", "Welcome to the Project cabal.")
            .await;
    }
    let rx = synced
        .subscribe()
        .map_err(|e| format!("cabal subscribe: {e}"))?;
    let cabal_id = *synced.handle().id().as_bytes();
    let conversation = ConversationId::new(ProtocolKind::Murm, hex(&cabal_id));
    let adapter: Box<dyn ProtocolAdapter> = Box::new(
        MurmAdapter::new(Identity::new(ProtocolKind::Murm, "me"))
            .with_cabal("Project cabal", Box::new(synced)),
    );
    Ok((murm, adapter, rx, conversation, ticket, cabal_id))
}

/// Load the wallet root's shared master seed, warning and falling back to an
/// ephemeral launch-local seed if bootstrap failed earlier.
fn load_wallet_seed(data_root: &Path) -> [u8; 32] {
    match session_runtime::load_identity_seed(data_root) {
        Ok(Some(seed)) => seed,
        Ok(None) => {
            tracing::warn!(
                ?data_root,
                "identity seed missing; comms identity is ephemeral this launch"
            );
            InMemoryProvider::random().master_keypair().to_seed()
        }
        Err(err) => {
            tracing::warn!(
                %err,
                ?data_root,
                "identity seed unavailable; comms identity is ephemeral this launch"
            );
            InMemoryProvider::random().master_keypair().to_seed()
        }
    }
}

/// Lowercase hex of bytes (the cabal id → conversation key).
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Derive this install's misfin send certificate from the comms persona: a
/// per-address salt → seed → deterministic Ed25519 self-signed cert (the P2
/// vault-derivation path, reproducible with no cert to store).
fn build_misfin_sender(
    provider: &Arc<dyn IdentityProvider>,
    address: &str,
) -> Result<MisfinSendIdentity, String> {
    let address = MisfinAddress::parse(address)?;
    let spec = MisfinIdentitySpec {
        address: address.clone(),
        blurb: Some("Mere".to_string()),
    };
    let salt = identity_salt(&address);
    let seed = provider
        .derive_keypair(&salt)
        .map_err(|e| e.to_string())?
        .to_seed();
    let material = deterministic_identity(&seed, &spec)?;
    Ok(MisfinSendIdentity {
        cert_der: material.certificate_der,
        key_pkcs8_der: material.private_key_pkcs8_der,
    })
}

/// Send a misfin `draft` over errand, presenting the vault-derived client cert.
/// Returns `(delivered, status_line)` for a completed round-trip (any misfin
/// status), or an error string for a connect / IO / timeout failure.
fn send_misfin(
    runtime: &tokio::runtime::Runtime,
    sender: Option<&MisfinSendIdentity>,
    draft: &Draft,
) -> Result<(bool, String), String> {
    let sender = sender.ok_or("misfin sending is not configured")?;
    let target = draft
        .conversation
        .as_ref()
        .ok_or("no target conversation")?;
    let url = errand::Url::parse(&format!("misfin://{}", target.key)).map_err(|e| e.to_string())?;
    let identity = ClientIdentity {
        cert_chain: vec![CertificateDer::from(sender.cert_der.clone())],
        private_key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(sender.key_pkcs8_der.clone())),
    };
    // Build the timeout *inside* the async block so its timer is created within the
    // runtime context (constructing it as a block_on argument panics: "no reactor").
    let response = runtime
        .block_on(async {
            tokio::time::timeout(
                Duration::from_secs(10),
                errand::misfin_send(&url, &draft.body, identity),
            )
            .await
        })
        .map_err(|_| "connect timed out".to_string())?
        .map_err(|e| e.to_string())?;
    let code = response.raw_status.unwrap_or(0);
    let line = match code {
        20 => "Delivered".to_string(),
        51 => "Mailbox doesn't exist".to_string(),
        53 => "Domain not serviced".to_string(),
        60 => "Certificate required".to_string(),
        61 => "Unauthorized sender".to_string(),
        62 => "Certificate invalid".to_string(),
        63 => "Fingerprint changed".to_string(),
        other => format!("misfin {other}: {}", response.meta),
    };
    Ok((code == 20, line))
}

/// Seed one demo gemmail into an empty mailbox, so the misfin conversation shows
/// something before a real server delivers mail.
fn seed_mailbox_if_empty(store: &MailboxStore, my_address: &str) {
    if store
        .list(my_address)
        .map(|m| !m.is_empty())
        .unwrap_or(true)
    {
        return;
    }
    let Ok(me) = MisfinAddress::parse(my_address) else {
        return;
    };
    let Ok(ana) = MisfinAddress::parse("ana@example.test") else {
        return;
    };
    let sender = MisfinSender {
        address: ana,
        blurb: Some("Ana".to_string()),
    };
    let body = "< ana@example.test Ana\n# Welcome\n\nA local demo message in your misfin mailbox.";
    let _ = store.store(&me, "fp-ana-demo", Some(&sender), body, 1_700_000_000);
}
