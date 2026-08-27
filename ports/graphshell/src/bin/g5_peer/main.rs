// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! g5_peer — the two-machine Graphshell projection rehearsal.
//!
//! G5's done-when asks for two processes on different devices to exchange
//! tickets, open a granted projection, and reject a revoked intent. Every
//! piece under that is unit-proven and none of it had crossed a real link:
//! the carrier receipts run over a `tokio` duplex or an in-process p2panda
//! pair. This is the bin that puts them on two machines.
//!
//! Modelled on `mesh-peer`, the milestone-1 two-machine rehearsal, down to the
//! ticket exchange and the environment-named identity, because that shape is
//! already proven on this LAN.
//!
//! ```text
//! g5_peer serve   [--revoked]
//! g5_peer connect --peer <ticket>
//! g5_peer connect --discover
//!
//! env:
//!   G5_OWNER    shared secret naming the owner that grants projections;
//!               both devices set the same value
//!   G5_SEED     this device's identity seed; distinct per device
//!   G5_NETWORK  shared name for the network the policy governs
//!   G5_PEER     --discover only: the *other* device's G5_SEED, from which its
//!               peer id is derived
//! ```
//!
//! [`serve`] documents the revocation arrangement, [`identity`] the shortcut
//! this rehearsal takes with the owner key, and [`connect`] what mDNS
//! discovery does and does not buy.

mod connect;
mod identity;
mod script;
mod serve;

use std::time::Duration;

use notochord::NetworkId;
use personae::{IdentityProvider, InMemoryProvider};

use crate::connect::{PeerSource, connect};
use crate::identity::env_hash;
use crate::serve::serve;

pub(crate) const ROOT_AUTHORITY: [u8; 32] = [7; 32];

/// How long a dial waits for mDNS to name an address before giving up.
pub(crate) const DIAL_DEADLINE: Duration = Duration::from_secs(20);

/// Route library warnings to stderr when `RUST_LOG` asks for them.
///
/// Off by default so ordinary runs read exactly as before. It earns its place
/// because the discovery stack below us reports fatal conditions *only* through
/// `tracing`: `swarm-discovery`'s supervisor tears its whole service down when
/// any of its actors stops and says so at `warn`, with no error returned and no
/// change to this process's exit status. Without a subscriber that is invisible,
/// which is exactly how a peer can sit there announcing nothing while still
/// printing `waiting for a peer...`. `RUST_LOG=warn` makes it audible.
fn init_tracing() {
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .init();
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), String> {
    init_tracing();
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_default();
    let mut peer_ticket = None;
    let mut revoked = false;
    let mut discover = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--peer" => peer_ticket = args.next(),
            // No ticket: derive the peer id from a shared name and let mDNS
            // resolve its address. See `PeerSource` for what that does and
            // does not buy.
            "--discover" => discover = true,
            "--revoked" => revoked = true,
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }

    let owner = InMemoryProvider::from_seed(env_hash("G5_OWNER")?);
    // The *seed*, carried separately, because the carrier and the identity
    // must be the same key. Seeding the transport with the derived public key
    // instead produces a peer id the responder authenticates but the initiator
    // never claims, and the proof fails with SessionProofInvalid.
    let seed = env_hash("G5_SEED")?;
    let me = InMemoryProvider::from_seed(seed);
    let network = NetworkId(env_hash("G5_NETWORK")?);

    match mode.as_str() {
        "serve" => serve(owner, me, seed, network, revoked).await,
        "connect" if discover => {
            let peer_key = InMemoryProvider::from_seed(env_hash("G5_PEER")?).master_public_key();
            let peer = transport::PeerID::from_bytes(&peer_key.to_bytes())
                .map_err(|e| format!("peer id: {e}"))?;
            connect(owner, me, seed, network, PeerSource::Discovered(peer)).await
        }
        "connect" => {
            let ticket = peer_ticket.ok_or("connect needs --peer <ticket> or --discover")?;
            connect(owner, me, seed, network, PeerSource::Ticket(ticket)).await
        }
        other => Err(format!(
            "usage: g5_peer serve [--revoked] | g5_peer connect --peer <ticket>\n\
             (got {other:?})"
        )),
    }
}
