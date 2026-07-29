//! Physical H6 transfer rehearsal over Graphshell's admitted carrier.
//!
//! The source serves a real URL plus a real file as one typed transfer. The
//! destination fetches the manifest, suspends the connection, resumes on a
//! fresh admission, fetches the addressed blob, and applies it to an
//! independent Mere and Muniment store.
//!
//! ```text
//! h6_transfer_peer serve --file <path> [--revoked]
//! h6_transfer_peer connect --peer <ticket> [--receipt <path>] [--expect-revoked]
//!
//! env:
//!   H6_OWNER    shared owner seed
//!   H6_SEED     this device's identity seed
//!   H6_PEER     the other device's identity seed
//!   H6_NETWORK  shared admitted-network name
//! ```

mod destination;
mod identity;
mod source;

use std::path::PathBuf;
use std::time::Duration;

use notochord::NetworkId;
use personae::{IdentityProvider, InMemoryProvider};

use crate::destination::connect;
use crate::identity::env_hash;
use crate::source::serve;

pub(crate) const ROOT_AUTHORITY: [u8; 32] = [7; 32];
pub(crate) const DIAL_DEADLINE: Duration = Duration::from_secs(20);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_default();
    let mut file = None;
    let mut peer_ticket = None;
    let mut receipt = None;
    let mut revoked = false;
    let mut expect_revoked = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--file" => file = args.next().map(PathBuf::from),
            "--peer" => peer_ticket = args.next(),
            "--receipt" => receipt = args.next().map(PathBuf::from),
            "--revoked" => revoked = true,
            "--expect-revoked" => expect_revoked = true,
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }

    let owner = InMemoryProvider::from_seed(env_hash("H6_OWNER")?);
    let seed = env_hash("H6_SEED")?;
    let me = InMemoryProvider::from_seed(seed);
    let peer_key = InMemoryProvider::from_seed(env_hash("H6_PEER")?)
        .master_public_key()
        .to_bytes();
    let network = NetworkId(env_hash("H6_NETWORK")?);

    match mode.as_str() {
        "serve" => {
            let file = file.ok_or("serve needs --file <path>")?;
            serve(owner, me, seed, peer_key, network, &file, revoked).await
        }
        "connect" => {
            let ticket = peer_ticket.ok_or("connect needs --peer <ticket>")?;
            connect(
                owner,
                me,
                seed,
                peer_key,
                network,
                &ticket,
                receipt.as_deref(),
                expect_revoked,
            )
            .await
        }
        other => Err(format!(
            "usage: h6_transfer_peer serve --file <path> [--revoked] | \\\n+             h6_transfer_peer connect --peer <ticket> [--receipt <path>] [--expect-revoked]\n\
             (got {other:?})"
        )),
    }
}
