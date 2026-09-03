// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! C4a's host fixture: a real Graphshell projection, served to a browser over
//! WebRTC.
//!
//! Everything below the signaling is the shipping path and none of it is
//! reimplemented here. [`serve_webrtc_join`] admits the peer over the door's
//! link challenge and redemption proof; [`ResidentProjectionHost`] serves it
//! through the same catalog route a peer dialling a transport would reach;
//! [`LiveEndpoint`] is the projection whose revision moves only for the
//! admitted intent. What this binary adds is the one thing a browser cannot
//! do without: somewhere to post an offer.
//!
//! ## The signaling is deliberately the dumbest thing that works
//!
//! One POST carries the offer, one response carries the answer, and there is
//! no trickle channel — the same shape `crates/probes/webrtc-ping` established
//! at C1, for the same reason. C5 replaces it with `mer3ly.net`; until then a
//! relay nobody authenticated is exactly what the door is designed to be
//! carried over, so a fixture using a hand-rolled loopback HTTP server proves
//! more about the door than a polished one would.
//!
//! ## Run it
//!
//! ```text
//! cargo run -p graphshell --features webrtc-session --bin c4_webrtc_host -- \
//!     --bind 192.168.4.36 --advertise 192.168.4.36
//! ```
//!
//! It prints the invite fragment. Paste that into the browser page, which
//! posts its offer to `/offer` and joins.
//!
//! **`--advertise` is not optional in practice.** The C1 headed receipt spent
//! a run on this: the carrier labels every inbound datagram with the *first*
//! advertised address, so a host advertising both loopback and its LAN address
//! attributes a browser's packets to `127.0.0.1` and the DTLS handshake times
//! out with no useful error. Declare exactly the address the browser can
//! reach.
//!
//! ## What the keys here are, and are not
//!
//! The host identity is generated fresh on every run from [`OsRng`]. That is
//! not laziness about persistence: a fixture with a hard-coded seed is a
//! private key in a public repository, and the first person to reuse this
//! shape in something real inherits it. A restarted fixture is a different
//! host, its old invitations no longer verify, and that is correct.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use graphshell::carrier::projection_policy;
use graphshell::live_endpoint::{ADMITTED_INTENT, LiveEndpoint, REFUSED_INTENT, SharedLiveEndpoint};
use graphshell::native::endpoint_catalog::{ResidentEndpointCatalog, ResidentEndpointRoute};
use graphshell::native::projection_host::ResidentProjectionHost;
use graphshell::webrtc_door::{InviteTerms, issue_invite};
use graphshell::webrtc_session::{HostedInvite, serve_webrtc_join};
use notochord::{NetworkId, ProfileRef, RevocationLedger, TrustedRoot};
use personae::{IdentityProvider, InMemoryProvider};
use rand_core::{OsRng, RngCore};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use webrtc_carrier::ReleaseRefV1;
use webrtc_carrier::native::{Answerer, AnswererConfig, CarrierConfig};

/// The fixture's own network and trust anchor. Local to this process: a
/// receipt host is its own root, which is what makes the run reproducible
/// without a deployment behind it.
const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT_AUTHORITY: [u8; 32] = [7; 32];

/// How long a session's delegation lives. Long enough for a headed run,
/// short enough that a forgotten fixture stops admitting.
const DELEGATION_TTL_MS: u64 = 60 * 60 * 1000;

struct Args {
    signal_port: u16,
    bind: IpAddr,
    advertise: Vec<IpAddr>,
    udp_port: u16,
    uses: u32,
    invite_ttl_ms: u64,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = Args {
            signal_port: 8788,
            bind: IpAddr::from([0, 0, 0, 0]),
            advertise: Vec::new(),
            udp_port: 0,
            uses: 8,
            invite_ttl_ms: 60 * 60 * 1000,
        };
        let mut argv = std::env::args().skip(1);
        while let Some(flag) = argv.next() {
            let mut value = || {
                argv.next()
                    .ok_or_else(|| format!("{flag} needs a value"))
            };
            match flag.as_str() {
                "--signal-port" => {
                    args.signal_port = value()?.parse().map_err(|_| "bad --signal-port")?
                }
                "--bind" => args.bind = value()?.parse().map_err(|_| "bad --bind address")?,
                "--advertise" => args
                    .advertise
                    .push(value()?.parse().map_err(|_| "bad --advertise address")?),
                "--udp-port" => args.udp_port = value()?.parse().map_err(|_| "bad --udp-port")?,
                "--uses" => args.uses = value()?.parse().map_err(|_| "bad --uses")?,
                "--invite-ttl-ms" => {
                    args.invite_ttl_ms = value()?.parse().map_err(|_| "bad --invite-ttl-ms")?
                }
                "--help" | "-h" => return Err(USAGE.to_string()),
                other => return Err(format!("unknown flag {other}\n\n{USAGE}")),
            }
        }
        Ok(args)
    }
}

const USAGE: &str = "c4_webrtc_host \
[--signal-port 8788] [--bind 0.0.0.0] [--advertise IP]... [--udp-port 0] \
[--uses 8] [--invite-ttl-ms 3600000]";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_millis() as u64
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse().map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;

    // Fresh every run. See the module doc.
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    // Not `Clone`, and rightly so — it holds the master private key. One
    // provider, shared by reference.
    let provider = Arc::new(InMemoryProvider::from_seed(seed));
    let host_key = provider.master_public_key().to_bytes();

    let profile = ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    };
    let policy = projection_policy(
        NETWORK,
        vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: host_key,
        }],
        vec![profile.clone()],
        None,
    );

    // One invitation for the whole run, with a use ceiling so a page reload
    // is an ordinary thing rather than a reason to restart the fixture.
    let issue = issue_invite(
        provider.as_ref(),
        &InviteTerms::projection(
            NETWORK,
            profile,
            now_ms() + args.invite_ttl_ms,
            args.uses,
            // The release this fixture claims to be. A claim bound by the
            // host signature, never a grant of executable trust.
            ReleaseRefV1 {
                manifest_blake3: [0x5a; 32],
                publisher_key_id: [0x6b; 32],
            },
        ),
    )?;
    let fragment = issue.descriptor.invite.to_fragment();
    let hosted = Arc::new(Mutex::new(HostedInvite {
        invite: issue.descriptor.invite,
        redemption: issue.redemption,
    }));

    // The product host, with the live endpoint on one catalog route.
    let mut catalog = ResidentEndpointCatalog::new();
    // `register_resumable_notifying`, not `register_notifying`: the plainer
    // registration erases resume behind a default that refuses it, and the
    // first headed run found exactly that — the bell rang, the browser asked
    // to resume, and the host said the endpoint could not. An endpoint with a
    // diff history has to be registered as one.
    // One board for every session, not one per session: a peer that drops
    // and rejoins must find the board where the host left it, and `POST
    // /nudge` moves it while a peer is away (the resume-on-reconnect row).
    let board = SharedLiveEndpoint::new(LiveEndpoint::new());
    let route_board = board.clone();
    catalog.register_resumable_notifying("live", "C4 live board", move |_context| {
        Ok(route_board.clone())
    })?;
    let route = ResidentEndpointRoute::new("live", Duration::from_millis(250))?;
    let host = Arc::new(Mutex::new(ResidentProjectionHost::new(
        policy.clone(), route, catalog,
    )));

    let signal_addr = SocketAddr::new(IpAddr::from([127, 0, 0, 1]), args.signal_port);
    let listener = TcpListener::bind(signal_addr).await?;

    println!("graphshell C4a WebRTC host");
    println!("  host key      {}", hex(&host_key));
    println!("  offer         POST http://{signal_addr}/offer   (text/plain: offer SDP in, answer SDP out)");
    println!("  invite        GET  http://{signal_addr}/invite  (the fragment below, as text)");
    println!("  health        GET  http://{signal_addr}/health");
    println!("  nudge         POST http://{signal_addr}/nudge   (append a card natively; answers the new revision)");
    println!("  carrier bind  {}:{}", args.bind, args.udp_port);
    if args.advertise.is_empty() {
        println!(
            "  advertising   (discovered) — pass --advertise with the address the browser \
             reaches, or DTLS will time out; see the module doc"
        );
    } else {
        println!(
            "  advertising   {}",
            args.advertise
                .iter()
                .map(IpAddr::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!("  intents       admitted={ADMITTED_INTENT}");
    println!("                refused ={REFUSED_INTENT}");
    println!();
    println!("INVITE FRAGMENT (paste into the browser page):");
    println!("{fragment}");
    println!();
    println!("READY — waiting for a browser offer. Ctrl-C to stop.");

    let ledger = Arc::new(RevocationLedger::new());
    let mut session = 0u64;

    loop {
        let (stream, _peer) = listener.accept().await?;
        let hosted = Arc::clone(&hosted);
        let host = Arc::clone(&host);
        let ledger = Arc::clone(&ledger);
        let provider = Arc::clone(&provider);
        let policy = policy.clone();
        let fragment = fragment.clone();
        let bind = SocketAddr::new(args.bind, args.udp_port);
        let advertise = args.advertise.clone();
        let board = board.clone();
        session += 1;
        let id = session;
        tokio::spawn(async move {
            if let Err(error) = serve_request(
                stream, id, hosted, host, ledger, provider, policy, fragment, bind, advertise, board,
            )
            .await
            {
                eprintln!("[signal {id}] {error}");
            }
        });
    }
}

/// One HTTP request. Deliberately minimal: this is a fixture's signaling, not
/// a server.
#[allow(clippy::too_many_arguments)]
async fn serve_request(
    mut stream: TcpStream,
    id: u64,
    hosted: Arc<Mutex<HostedInvite>>,
    host: Arc<Mutex<ResidentProjectionHost>>,
    ledger: Arc<RevocationLedger>,
    provider: Arc<InMemoryProvider>,
    policy: notochord::LocalNetworkPolicy,
    fragment: String,
    bind: SocketAddr,
    advertise: Vec<IpAddr>,
    board: SharedLiveEndpoint,
) -> Result<(), String> {
    let (read, mut write) = stream.split();
    let mut reader = BufReader::new(read);

    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .await
        .map_err(|error| format!("request line: {error}"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .await
            .map_err(|error| format!("header: {error}"))?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .map(str::trim)
            .and_then(|value| value.parse::<usize>().ok())
        {
            content_length = value;
        }
    }

    match (method.as_str(), path.as_str()) {
        ("OPTIONS", _) => respond(&mut write, "204 No Content", "").await,
        ("GET", "/health") => respond(&mut write, "200 OK", "ok").await,
        ("GET", "/invite") => respond(&mut write, "200 OK", &fragment).await,
        ("POST", "/nudge") => {
            let revision = board.with(|endpoint| endpoint.append());
            println!("[nudge] the host appended a card natively; board at revision {}", revision.0);
            respond(&mut write, "200 OK", &revision.0.to_string()).await
        }
        ("POST", "/offer") => {
            let mut body = vec![0u8; content_length];
            if content_length > 0 {
                reader
                    .read_exact(&mut body)
                    .await
                    .map_err(|error| format!("body: {error}"))?;
            }
            let offer = String::from_utf8_lossy(&body).into_owned();
            if offer.trim().is_empty() {
                return respond(&mut write, "400 Bad Request", "empty offer").await;
            }
            println!("[session {id}] offer received, {} bytes", offer.len());

            // Bind a fresh answerer per offer: `accept` consumes it, and each
            // session gets its own socket and certificate.
            let mut answerer = Answerer::bind(AnswererConfig {
                bind,
                advertise: advertise.clone(),
                carrier: CarrierConfig::default(),
            })
            .await
            .map_err(|error| format!("bind: {error}"))?;

            let answer = match answerer.answer(&offer) {
                Ok(answer) => answer,
                Err(error) => {
                    println!("[session {id}] refused the offer: {error}");
                    return respond(&mut write, "400 Bad Request", &error.to_string()).await;
                }
            };
            respond(&mut write, "200 OK", &answer).await?;

            // The answer is on its way; everything after it is the shipping
            // path. Spawned so the signaling socket is free immediately.
            tokio::spawn(async move {
                if let Err(error) =
                    admit_and_serve(id, answerer, hosted, host, ledger, provider, policy).await
                {
                    println!("[session {id}] {error}");
                }
            });
            Ok(())
        }
        _ => respond(&mut write, "404 Not Found", "try POST /offer, GET /invite, GET /health").await,
    }
}

/// Accept the channel, run the door, and hand the admitted session to the
/// resident host.
async fn admit_and_serve(
    id: u64,
    answerer: Answerer,
    hosted: Arc<Mutex<HostedInvite>>,
    host: Arc<Mutex<ResidentProjectionHost>>,
    ledger: Arc<RevocationLedger>,
    provider: Arc<InMemoryProvider>,
    policy: notochord::LocalNetworkPolicy,
) -> Result<(), String> {
    let carrier = answerer
        .accept()
        .await
        .map_err(|error| format!("the data channel never opened: {error}"))?;
    println!(
        "[session {id}] OPEN  label {:?}  udp {}",
        carrier.channel_label(),
        carrier.local_addr()
    );
    if let Some(prints) = carrier.fingerprints() {
        println!("[session {id}] dtls client {}", prints.client().to_sdp_hex());
        println!("[session {id}] dtls server {}", prints.server().to_sdp_hex());
    }

    let live = host.lock().await.live_sessions();
    let served = {
        // The invitation's use count is the one piece of shared state the
        // join mutates, so it is held only across the join itself.
        let mut hosted = hosted.lock().await;
        serve_webrtc_join(
            carrier,
            provider.as_ref(),
            &mut hosted,
            &policy,
            &ledger,
            ROOT_AUTHORITY,
            DELEGATION_TTL_MS,
            now_ms(),
            live,
        )
        .await
        .map_err(|error| format!("join refused: {error}"))?
    };
    println!("[session {id}] admitted");

    let projection = host
        .lock()
        .await
        .serve_admitted(served.session, now_ms)
        .map_err(|error| format!("the resident host could not serve: {error}"))?;
    println!(
        "[session {id}] serving subject {} on {}",
        hex8(&projection.subject()),
        projection.session().0
    );

    let summary = projection
        .finished()
        .await
        .map_err(|error| format!("the serving task failed: {error}"))?
        .map_err(|error| format!("session loop: {error}"))?;
    println!(
        "[session {id}] served {} request(s); ended {:?}",
        summary.answered, summary.end
    );

    // The pump and the carrier outlive the session loop on purpose; ending
    // them politely is what flushes the loop's final answer. See
    // `ServedJoin::finish`.
    match served.pump.await {
        Ok(end) => println!("[session {id}] pump ended: {end}"),
        Err(error) => println!("[session {id}] pump panicked: {error}"),
    }
    if let Err(error) = served.control.close().await {
        println!("[session {id}] carrier did not close cleanly: {error}");
    }
    Ok(())
}

async fn respond(
    write: &mut (impl AsyncWriteExt + Unpin),
    status: &str,
    body: &str,
) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Headers: content-type\r\n\
         Connection: close\r\n\
         \r\n{body}",
        body.len()
    );
    write
        .write_all(response.as_bytes())
        .await
        .map_err(|error| format!("response: {error}"))?;
    write
        .flush()
        .await
        .map_err(|error| format!("flush: {error}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex8(bytes: &[u8]) -> String {
    hex(&bytes[..4.min(bytes.len())])
}
