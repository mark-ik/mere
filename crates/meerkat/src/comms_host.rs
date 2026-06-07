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
//! First cut: no networking. The misfin mailbox is a real on-disk redb store
//! (seeded with one demo message); the murm cabal is a real local `CableEngine`
//! cabal authored into over a stub transport (seeded with one welcome post). The
//! live misfin server (a listening socket) and networked murm cabals (joining a
//! real overlay) are the next steps; the adapter seam stays identical.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use armillary::{spawn, ActorHandle, Emitter, Wake};
use comms::misfin_adapter::MisfinAdapter;
use comms::murm_adapter::MurmAdapter;
use comms::{Comms, ConversationId, Draft, Identity, Inbox, Message, ProtocolKind};
use identity::{IdentityProvider, InMemoryProvider};
use misfin::{MailboxStore, MisfinAddress, MisfinSender};
use murm::{Alpn, CabalKey, Murm, PeerID, Transport};
use tokio::io::DuplexStream;
use transport::TransportError;

/// This install's (demo) misfin mailbox address — the inbox the pane reads.
const MY_MISFIN_ADDR: &str = "me@mere.local";
/// The demo cabal's key (its public id is derived from this; a real join path
/// replaces the fixed key later).
const DEMO_CABAL_KEY: [u8; 32] = [0x5c; 32];
/// The murm identity seed for the local demo cabal (stable per install would be a
/// vault concern; fixed here for the local demo).
const MURM_SEED: [u8; 32] = [0x5c; 32];

/// A command the host routes to the comms actor.
pub enum CommsCommand {
    /// Reload the merged conversation list.
    Refresh,
    /// Load one conversation's messages.
    Open(ConversationId),
    /// Send a draft, then reload its thread + the list.
    Send(Draft),
}

/// A result the comms actor delivers back to the UI loop.
pub enum CommsUpdate {
    /// The merged conversation list (+ any backend failures).
    Inbox(Inbox),
    /// One conversation's messages.
    Thread(ConversationId, Vec<Message>),
    /// A draft was sent to this conversation (clear the compose buffer).
    Sent(ConversationId),
}

/// A transport that never connects — murm's `open_cabal` never touches it, so a
/// local cabal authored into needs no networking. (Networked cabals come later.)
struct StubTransport {
    peer: PeerID,
}

impl Transport for StubTransport {
    type Stream = DuplexStream;

    fn local_peer_id(&self) -> PeerID {
        self.peer
    }

    async fn connect(&self, _peer: PeerID, _alpn: Alpn) -> Result<Self::Stream, TransportError> {
        Err(TransportError::ConnectionRefused)
    }

    async fn accept(&self, _alpn: Alpn) -> Result<Self::Stream, TransportError> {
        Err(TransportError::ConnectionRefused)
    }
}

/// Spawn the comms subsystem as an [`armillary`] I/O actor. The run closure builds
/// a tokio runtime on the actor thread, stands up [`Comms`] over local stores
/// (under `dir`), emits the initial inbox, then serves [`CommsCommand`]s. Setup
/// failure disables comms (no updates), never the shell. Returns the kernel's
/// command handle and the receiver it drains in `user_event`.
pub fn spawn_comms(wake: Wake, dir: PathBuf) -> (ActorHandle<CommsCommand>, Receiver<CommsUpdate>) {
    spawn(wake, move |commands, out: Emitter<CommsUpdate>| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build the comms runtime");

        let comms = match build_comms(&dir) {
            Ok(comms) => comms,
            Err(err) => {
                tracing::warn!(%err, "comms disabled: backend setup failed");
                return;
            },
        };

        // Emit the initial inbox so the pane has data the first time it opens.
        out.emit(CommsUpdate::Inbox(runtime.block_on(comms.inbox())));

        while let Ok(command) = commands.recv() {
            match command {
                CommsCommand::Refresh => {
                    out.emit(CommsUpdate::Inbox(runtime.block_on(comms.inbox())));
                },
                CommsCommand::Open(id) => match runtime.block_on(comms.messages(&id)) {
                    Ok(messages) => out.emit(CommsUpdate::Thread(id, messages)),
                    Err(err) => tracing::warn!(%err, "comms: loading a thread failed"),
                },
                CommsCommand::Send(draft) => match runtime.block_on(comms.send(&draft)) {
                    Ok(_) => {
                        if let Some(id) = draft.conversation.clone() {
                            if let Ok(messages) = runtime.block_on(comms.messages(&id)) {
                                out.emit(CommsUpdate::Thread(id.clone(), messages));
                            }
                            out.emit(CommsUpdate::Sent(id));
                        }
                        // A send changes recency; refresh the list too.
                        out.emit(CommsUpdate::Inbox(runtime.block_on(comms.inbox())));
                    },
                    Err(err) => tracing::warn!(%err, "comms: send failed"),
                },
            }
        }
    })
}

/// Build the live [`Comms`] over local backends under `dir`.
fn build_comms(dir: &PathBuf) -> Result<Comms, String> {
    let comms_dir = dir.join("comms");
    std::fs::create_dir_all(&comms_dir).map_err(|error| error.to_string())?;

    // misfin: a real on-disk mailbox; seed one demo message on first launch.
    let mailbox = MailboxStore::open(comms_dir.join("mailbox.redb")).map_err(|e| e.to_string())?;
    seed_mailbox_if_empty(&mailbox);
    let misfin = MisfinAdapter::new(
        Identity::new(ProtocolKind::Misfin, MY_MISFIN_ADDR),
        MY_MISFIN_ADDR,
        mailbox,
    );

    // murm: a local cabal authored into over a stub transport; seed one post.
    let provider: Arc<dyn IdentityProvider> = Arc::new(InMemoryProvider::from_seed(MURM_SEED));
    let peer = PeerID::from_public_key(provider.master_public_key());
    let murm = Murm::new(provider, StubTransport { peer });
    let cabal = murm
        .open_cabal(&CabalKey::new(DEMO_CABAL_KEY))
        .map_err(|e| e.to_string())?;
    if cabal.history("session").is_empty() {
        let _ = cabal.send_text("session", "Welcome to the Project cabal (local demo).");
    }
    let murm_adapter =
        MurmAdapter::new(Identity::new(ProtocolKind::Murm, "me")).with_cabal("Project cabal", Box::new(cabal));

    Ok(Comms::new()
        .with_adapter(Box::new(misfin))
        .with_adapter(Box::new(murm_adapter)))
}

/// Seed one demo gemmail into an empty mailbox, so the misfin conversation shows
/// something before a real server delivers mail.
fn seed_mailbox_if_empty(store: &MailboxStore) {
    if store.list(MY_MISFIN_ADDR).map(|m| !m.is_empty()).unwrap_or(true) {
        return;
    }
    let Ok(me) = MisfinAddress::parse(MY_MISFIN_ADDR) else {
        return;
    };
    let Ok(ana) = MisfinAddress::parse("ana@example.test") else {
        return;
    };
    let sender = MisfinSender { address: ana, blurb: Some("Ana".to_string()) };
    let body = "< ana@example.test Ana\n# Welcome\n\nA local demo message in your misfin mailbox.";
    let _ = store.store(&me, "fp-ana-demo", Some(&sender), body, 1_700_000_000);
}
