// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! k2_peer — the two-machine rehearsal for a place-held Knot document.
//!
//! K2 is proven against a paired in-process fixture, which exercises every
//! layer except the physical one. This is the bin that puts it on two devices:
//! a real `KnotEndpoint` over a real directory on the holder, reached by a
//! visitor that never holds a replica and edits by sending intents.
//!
//! Modelled on `graphshell`'s `g5_peer`, down to the ticket exchange and the
//! environment-named identity, because that shape is already proven on this
//! LAN. What is new here is the product half: the holder registers Knot in a
//! resident catalog, and the visitor saves and then watches the holder's own
//! file become the truth both of them read.
//!
//! An example rather than a `[[bin]]` on purpose. Knot depends on graphshell
//! only as a dev-dependency, and examples get dev-dependencies where bins do
//! not, so this rehearsal costs the crate graph nothing.
//!
//! ```text
//! cargo run -p knot-editor --example k2_peer -- hold  --root <vault-dir>
//! cargo run -p knot-editor --example k2_peer -- visit --peer <ticket>
//! cargo run -p knot-editor --example k2_peer -- visit --discover
//!
//! env:
//!   K2_OWNER    shared secret naming the owner that grants projections;
//!               both devices set the same value
//!   K2_SEED     this device's identity seed; distinct per device
//!   K2_NETWORK  shared name for the network the policy governs
//!   K2_PEER     --discover only: the *other* device's K2_SEED, from which
//!               its peer id is derived
//! ```
//!
//! ## The rehearsal shortcut, stated plainly
//!
//! Both sides derive the owner keypair from `K2_OWNER`, so the visiting side
//! mints its own grant. A real deployment issues that certificate out of band
//! and the visitor never holds the owner key. What this proves is the
//! transport, admission, projection, save, and revision-bell path across two
//! machines; it proves nothing about how a grant is distributed.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chirograph::{
    CapabilityProfile, IntentResult, PresentationCapability, ProjectionSession, SaveTextV1,
    SessionStatus,
};
use graphshell::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE, open_session};
use graphshell::carrier::projection_policy;
use graphshell::client::{ResolvedContent, RetainedEndpointSession};
use graphshell::native::endpoint_catalog::{ResidentEndpointCatalog, ResidentEndpointRoute};
use graphshell::native::projection_host::ResidentProjectionHost;
use graphshell::network_carrier::{
    CarrierRuntime, NetworkCarrier, dial_projection_session, projection_binding,
};
use notochord::{NetworkId, ProfileRef, TrustedRoot};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use personae::{IdentityProvider, InMemoryProvider};
use transport::p2panda_transport::{MdnsDiscoveryMode, P2pandaTransport};
use transport::{PeerID, Transport};

const ROOT_AUTHORITY: [u8; 32] = [7; 32];
/// How long a dial waits for mDNS to name an address before giving up.
const DIAL_DEADLINE: Duration = Duration::from_secs(20);
/// The route the holder offers its vault on.
const ROUTE: &str = "knot";

fn env_hash(var: &str) -> Result<[u8; 32], String> {
    let value = std::env::var(var).map_err(|_| format!("set {var} (any string)"))?;
    Ok(*blake3::hash(value.as_bytes()).as_bytes())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_millis() as u64
}

fn profile_ref() -> ProfileRef {
    ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }
}

fn hex8(bytes: &[u8; 32]) -> String {
    bytes.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

/// The grant the owner issues for projections on `network`.
///
/// Back-dated a minute, with `not_before` matching `issued_at` because the rule
/// is `issued_at <= not_before`. The minute absorbs clock skew: these are two
/// machines and the responder judges validity by *its* clock, so a certificate
/// stamped "valid from now" by a visitor running slightly ahead is not yet
/// valid to the holder.
fn grant(
    owner: &InMemoryProvider,
    subject: [u8; 32],
    network: NetworkId,
    expires_at_ms: u64,
) -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        owner,
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            owner.master_public_key().to_bytes(),
            subject,
            CapabilityScope {
                domain: GRAPHSHELL_DOMAIN.into(),
                resource: network.0.to_vec(),
                path_prefix: PROJECTION_SERVICE.into(),
                actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
            },
            now_ms().saturating_sub(60_000),
            now_ms().saturating_sub(60_000),
            Some(expires_at_ms),
            1,
            [11; 32],
        ),
    )
    .expect("issue certificate")
}

/// The carrier and the Personae identity must be the same key.
///
/// D6 requires the claimed subject to *be* the peer the carrier authenticated.
/// If these diverge the failure surfaces far away as `SessionProofInvalid`
/// during admission, so it is asserted where the cause is visible.
fn assert_same_key(carrier: &P2pandaTransport, me: &InMemoryProvider) -> Result<(), String> {
    let carried = carrier.local_peer_id().to_bytes();
    let claimed = me.master_public_key().to_bytes();
    if carried != claimed {
        return Err(format!(
            "carrier identity {} is not the Personae identity {}; seed both from the same bytes",
            hex8(&carried),
            hex8(&claimed)
        ));
    }
    Ok(())
}

fn viewing_profile() -> CapabilityProfile {
    CapabilityProfile::new([
        PresentationCapability::EditableText,
        PresentationCapability::PortableCard,
    ])
}

/// Hold the vault: serve Knot projections to admitted visitors until stopped.
async fn hold(
    owner: InMemoryProvider,
    me: InMemoryProvider,
    seed: [u8; 32],
    network: NetworkId,
    root: PathBuf,
) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    let carrier = P2pandaTransport::builder_from_seed(seed)
        .alpns(vec![graphshell::carrier::projection_alpn()])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .map_err(|e| format!("bind: {e}"))?;
    assert_same_key(&carrier, &me)?;

    let ticket = carrier.ticket().await.map_err(|e| format!("ticket: {e}"))?;
    println!("k2_peer hold");
    println!("  vault:  {}", root.display());
    println!("  ticket: {ticket}");
    println!("  run on the other device:");
    println!("    cargo run -p knot-editor --example k2_peer -- visit --peer {ticket}");

    let policy = projection_policy(
        network,
        vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: owner.master_public_key().to_bytes(),
        }],
        vec![profile_ref()],
        None,
    );

    // One route over one vault. Each admitted session opens its own endpoint,
    // so two visitors hold live views of the same files and converge through
    // the holder's own truth rather than through shared memory.
    let mut catalog = ResidentEndpointCatalog::new();
    let vault = root.clone();
    catalog
        .register_resumable_notifying(ROUTE, "Knot", move |_| {
            // Resumable *and* notifying: a visitor recovering after a bell asks
            // for a resume, and the typed silent registration would answer that
            // refusal-shaped, which is what K2 found the hard way.
            knot_editor::KnotEndpoint::open_writable(
                &vault,
                knot_editor::KnotWriteGrant::new(64 * 1024),
            )
            .map_err(|error| error.to_string())
        })
        .map_err(|e| format!("register: {e}"))?;
    let mut host = ResidentProjectionHost::new(
        policy,
        ResidentEndpointRoute::new(ROUTE, Duration::from_millis(250))
            .map_err(|e| format!("route: {e}"))?,
        catalog,
    );

    for visit in 1.. {
        println!("  waiting for visitor {visit}...");
        match host
            .accept_one(&carrier, now_ms)
            .await
            .map_err(|e| format!("accept: {e}"))?
        {
            Ok(served) => println!(
                "  visitor {visit}: admitted {} ({} live)",
                hex8(&served.subject()),
                host.live_sessions()
            ),
            Err(refusal) => println!("  visitor {visit}: refused: {refusal:?}"),
        }
        // The served session runs in the background; the host goes straight
        // back to accepting, which is what lets a second visitor in while the
        // first still holds the document open.
    }
    Ok(())
}

/// How this run learned which peer to dial.
enum PeerSource {
    /// A ticket carried by hand: id and address together, and the only form
    /// that works off this LAN.
    Ticket(String),
    /// A peer id known in advance, with mDNS expected to resolve its address.
    Discovered(PeerID),
}

/// Visit the holder: mount its vault, read, save, and wait for the bell.
async fn visit(
    owner: InMemoryProvider,
    me: InMemoryProvider,
    seed: [u8; 32],
    network: NetworkId,
    source: PeerSource,
) -> Result<(), String> {
    let carrier = P2pandaTransport::builder_from_seed(seed)
        .alpns(vec![graphshell::carrier::projection_alpn()])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .map_err(|e| format!("bind: {e}"))?;
    assert_same_key(&carrier, &me)?;

    println!("k2_peer visit");
    let peer = match source {
        PeerSource::Ticket(ticket) => {
            let peer = carrier
                .add_peer_ticket(&ticket)
                .await
                .map_err(|e| format!("ticket: {e}"))?;
            println!("  peer from ticket: {}", hex8(&peer.to_bytes()));
            peer
        }
        PeerSource::Discovered(peer) => {
            // Nothing is added to the address book. Asking for our own ticket
            // forces the endpoint, and with it the mDNS actor, to start now
            // rather than lazily on the first dial.
            carrier.ticket().await.map_err(|e| format!("ticket: {e}"))?;
            println!(
                "  no ticket, no add_peer; waiting for mDNS to resolve {}",
                hex8(&peer.to_bytes())
            );
            peer
        }
    };

    let subject = me.master_public_key().to_bytes();
    let hello = open_session(
        &me,
        network,
        profile_ref(),
        notochord::TrafficClass::Interactive,
        session_nonce(seed),
        &projection_binding(carrier.local_peer_id()),
        vec![grant(&owner, subject, network, now_ms() + 3_600_000)],
    )
    .map_err(|e| format!("hello: {e}"))?;

    // mDNS fills the address book asynchronously while p2panda reads it
    // synchronously, so a ticketless dial races discovery. Retry rather than
    // call a race an absent peer.
    let started = Instant::now();
    let limits = Default::default();
    let stream = loop {
        match dial_projection_session(&carrier, peer, &hello, &limits).await {
            Ok(Ok(stream)) => break stream,
            Ok(Err(reason)) => return Err(format!("the holder refused this visitor: {reason:?}")),
            Err(error) => {
                if started.elapsed() >= DIAL_DEADLINE {
                    return Err(format!("dial: {error}"));
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    };
    println!("  admitted");

    // The carrier blocks, so the whole visit runs on a thread that is not a
    // runtime worker. The session never leaves it, which is what `Box<dyn
    // Carrier>` not being `Send` asks of every caller.
    let handle = tokio::runtime::Handle::current();
    let visit_result = tokio::task::spawn_blocking(move || drive_visit(stream, handle))
        .await
        .map_err(|e| format!("visit thread: {e}"))?;
    let close_result = carrier.close().await;
    match (visit_result, close_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(format!("transport close: {error}")),
        (Ok(()), Ok(())) => Ok(()),
    }
}

/// A fresh nonce per session.
///
/// The projection session id is a digest of the transcript and the transcript
/// carries this, so two sessions from one subject reusing a nonce land on the
/// same session id. Derived from the clock rather than an RNG because the
/// requirement is distinctness across this peer's own sessions, not secrecy,
/// and blake3 is already in this crate's graph.
fn session_nonce(seed: [u8; 32]) -> [u8; 32] {
    let mut material = Vec::with_capacity(48);
    material.extend_from_slice(&seed);
    material.extend_from_slice(&now_ms().to_le_bytes());
    material.extend_from_slice(&std::process::id().to_le_bytes());
    *blake3::hash(&material).as_bytes()
}

/// Mount, read, save, and wait for the holder to ring.
fn drive_visit<S>(stream: S, handle: tokio::runtime::Handle) -> Result<(), String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + 'static,
{
    let carrier = NetworkCarrier::over(stream, CarrierRuntime::borrowed(handle));
    let mut retained = RetainedEndpointSession::over(Box::new(carrier), viewing_profile())
        .map_err(|e| format!("discover: {e}"))?;
    println!("  endpoint: {}", retained.descriptor().label);
    let session = retained.mount(0).map_err(|e| format!("mount: {e}"))?;

    let (target, before, token, action) = read_document(&mut retained, &session)?;
    println!("  opened {} bytes of source", before.len());

    let stamp = now_ms();
    let edited = format!("{before}\nVisited at {stamp}.\n");
    let result = retained
        .invoke(
            &session,
            target,
            &action,
            &SaveTextV1 {
                base_token: token,
                source: edited.clone(),
            },
        )
        .map_err(|e| format!("save: {e}"))?;
    match result {
        IntentResult::Accepted => println!("  save accepted by the holder"),
        other => return Err(format!("the holder refused the save: {other:?}")),
    }

    // The holder wrote its own file; the bell says so and the ordinary resume
    // path brings it back. This is the half a request/response loop cannot do.
    println!("  waiting for the holder's revision bell...");
    match retained.wait_for_change() {
        Ok(true) => println!("  bell heard, and it carried a revision we had not seen"),
        Ok(false) => println!("  bell heard, already current"),
        Err(error) => return Err(format!("bell: {error}")),
    }

    let (_, after, _, _) = read_document(&mut retained, &session)?;
    let status = retained
        .client()
        .mounted(&session)
        .map(|scene| scene.status)
        .unwrap_or(SessionStatus::Disconnected);
    println!("  session status: {status:?}");
    if after == edited {
        println!("  the holder's copy is what we wrote");
    } else {
        println!("  NOTE: the holder's copy differs from what we sent");
        println!(
            "    sent {} bytes, read back {} bytes",
            edited.len(),
            after.len()
        );
    }

    retained.close().map_err(|e| format!("close: {e}"))?;
    println!("  closed");
    Ok(())
}

type Document = (
    sceno::InstanceId,
    String,
    Vec<u8>,
    chirograph::AdvertisedAction,
);

/// The first editable document the holder discloses, with the token that makes
/// a save revision-checked.
fn read_document(
    retained: &mut RetainedEndpointSession,
    session: &ProjectionSession,
) -> Result<Document, String> {
    retained
        .resolve_all(session)
        .map_err(|e| format!("resolve: {e}"))?
        .into_iter()
        .find_map(|(target, presentation)| match presentation.content {
            ResolvedContent::EditableText(editable) => presentation
                .semantics
                .actions
                .first()
                .cloned()
                .map(|action| (target, editable.source, editable.base_token, action)),
            _ => None,
        })
        .ok_or_else(|| {
            "the holder disclosed no editable document; put a .knot file in its vault".to_string()
        })
}

fn usage() -> String {
    "usage:\n  k2_peer hold  --root <vault-dir>\n  k2_peer visit --peer <ticket>\n  k2_peer visit --discover"
        .to_string()
}

#[tokio::main]
async fn main() -> Result<(), String> {
    if std::env::var_os("RUST_LOG").is_some() {
        // The discovery stack reports fatal conditions only through `tracing`:
        // swarm-discovery tears its service down at `warn` with no error
        // returned, which is how a peer sits there announcing nothing while
        // still printing "waiting". `RUST_LOG=warn` makes that audible.
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();
    }

    let owner = InMemoryProvider::from_seed(env_hash("K2_OWNER")?);
    let seed = env_hash("K2_SEED")?;
    let me = InMemoryProvider::from_seed(seed);
    let network = NetworkId(env_hash("K2_NETWORK")?);

    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    match args.first().map(String::as_str) {
        Some("hold") => {
            let root = flag("--root").ok_or_else(|| format!("hold needs --root\n{}", usage()))?;
            hold(owner, me, seed, network, PathBuf::from(root)).await
        }
        Some("visit") => {
            let source = if let Some(ticket) = flag("--peer") {
                PeerSource::Ticket(ticket)
            } else if args.iter().any(|a| a == "--discover") {
                let peer_seed = env_hash("K2_PEER")?;
                let peer_key = InMemoryProvider::from_seed(peer_seed)
                    .master_public_key()
                    .to_bytes();
                PeerSource::Discovered(
                    PeerID::from_bytes(&peer_key).map_err(|e| format!("peer id: {e}"))?,
                )
            } else {
                return Err(format!("visit needs --peer or --discover\n{}", usage()));
            };
            visit(owner, me, seed, network, source).await
        }
        _ => Err(usage()),
    }
}
