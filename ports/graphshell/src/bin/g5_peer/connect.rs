//! The dialling half: three sessions, an interruption, and a resume.

use std::time::{Duration, Instant};

use graphshell::admission::open_session;
use graphshell::carrier::projection_alpn;
use graphshell_protocol::{
    CarrierRequest, CarrierRequestBody, CarrierResponse, ProjectionSession, ResumeRequest,
    Revision, SceneEpoch,
};
use notochord::{NetworkId, SessionReply, TrafficClass, initiate_session};
use personae::{IdentityProvider, InMemoryProvider};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::sleep;
use transport::p2panda_transport::{MdnsDiscoveryMode, P2pandaTransport};
use transport::{Transport, initiator_binding};

use crate::DIAL_DEADLINE;
use crate::identity::{assert_same_key, grant, hex8, now_ms, profile};
use crate::script::{RESUME_SESSION, intent_body, open_body, projection_request, summarize};

/// How this run learned which peer to dial.
///
/// The distinction matters and is easy to overstate. p2panda's mDNS populates
/// the address book so a `connect` to a **known** peer id succeeds without an
/// explicit `add_peer`. It does **not** answer "who is on this LAN":
/// `mere-transport` exposes no way to enumerate what discovery found, so
/// `Discovered` still derives the peer id from a shared name. mDNS removes the
/// need to exchange an *address*, not the need to know *who*.
pub(crate) enum PeerSource {
    /// A ticket carried by hand: id and address together, and the only form
    /// that works off this LAN.
    Ticket(String),
    /// A peer id known in advance, with mDNS expected to resolve its address.
    Discovered(transport::PeerID),
}

pub(crate) async fn connect(
    owner: InMemoryProvider,
    me: InMemoryProvider,
    seed: [u8; 32],
    network: NetworkId,
    source: PeerSource,
) -> Result<(), String> {
    let carrier = P2pandaTransport::builder_from_seed(seed)
        .alpns(vec![projection_alpn()])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .map_err(|e| format!("bind: {e}"))?;
    assert_same_key(&carrier, &me)?;

    println!("g5_peer connect");
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
            // Nothing is added to the address book. If the dial succeeds, mDNS
            // resolved the address by itself.
            //
            // The ticket is discarded; asking for it forces the endpoint, and
            // with it the mDNS actor, to start now rather than lazily on the
            // first dial. `serve` gets this for free by printing its ticket.
            carrier.ticket().await.map_err(|e| format!("ticket: {e}"))?;
            println!(
                "  no ticket, no add_peer; waiting for mDNS to resolve {}",
                hex8(&peer.to_bytes())
            );
            peer
        }
    };

    // Phase one: open, take a snapshot, and suspend. Suspend rather than
    // close, because the point is a session the peer intends to come back to.
    println!("  --- session 1 ---");
    let first = run_session(
        &carrier,
        peer,
        &me,
        &owner,
        network,
        vec![
            (1, open_body()),
            (2, CarrierRequestBody::Snapshot(projection_request())),
            (3, CarrierRequestBody::Suspend),
        ],
    )
    .await?;
    if first.is_empty() {
        return Ok(());
    }

    // The interruption. Phase one's connection is gone; this is a new dial,
    // a new handshake, and a new admission. Nothing but the endpoint's own
    // history connects the two.
    println!("  --- interruption: reconnecting ---");
    println!("  --- session 2 ---");
    run_session(
        &carrier,
        peer,
        &me,
        &owner,
        network,
        vec![
            (4, open_body()),
            // Resuming from revision 1, which is where a client that
            // acknowledged the initial snapshot and then dropped would be.
            // The endpoint has since moved to revision 3, so a correct resume
            // replays the two contiguous diffs rather than resending a scene.
            (
                5,
                CarrierRequestBody::Resume(ResumeRequest {
                    session: ProjectionSession(RESUME_SESSION.into()),
                    epoch: SceneEpoch(3),
                    revision: Revision(1),
                }),
            ),
            // A real IntentInvocation, so the revoked run refuses the verb the
            // done-when actually names rather than a stand-in.
            (6, intent_body()),
            (7, CarrierRequestBody::Close),
        ],
    )
    .await?;

    // A third session whose *first* request is an intent. In the granted run
    // it is accepted; in the revoked run it is the verb the refusal lands on,
    // which is what G5's done-when names. Earlier arrangements always refused
    // an `Open`, because the gate fires on whatever arrives first.
    println!("  --- session 3: intent first ---");
    run_session(
        &carrier,
        peer,
        &me,
        &owner,
        network,
        vec![(8, intent_body()), (9, CarrierRequestBody::Close)],
    )
    .await?;
    Ok(())
}

/// Dial, prove the subject, run `script`, and report each answer.
async fn run_session(
    carrier: &P2pandaTransport,
    peer: transport::PeerID,
    me: &InMemoryProvider,
    owner: &InMemoryProvider,
    network: NetworkId,
    script: Vec<(u64, CarrierRequestBody)>,
) -> Result<Vec<CarrierResponse>, String> {
    // mDNS fills the address book asynchronously while p2panda reads it
    // synchronously, so a ticketless dial races discovery: it wins on loopback
    // and loses on a real link. Retry rather than call a race an absent peer.
    let started = Instant::now();
    let mut stream = loop {
        match carrier.connect(peer, projection_alpn()).await {
            Ok(stream) => break stream,
            Err(error) => {
                if started.elapsed() >= DIAL_DEADLINE {
                    return Err(format!("connect: {error}"));
                }
                sleep(Duration::from_millis(250)).await;
            }
        }
    };

    let subject = me.master_public_key().to_bytes();
    let local = transport::PeerID::from_bytes(&subject).map_err(|e| format!("peer id: {e}"))?;
    let binding = initiator_binding(&projection_alpn(), local);
    let hello = open_session(
        me,
        network,
        profile(),
        TrafficClass::Interactive,
        [5; 32],
        &binding,
        vec![grant(owner, subject, network, now_ms() + 3_600_000)],
    )
    .map_err(|e| format!("hello: {e}"))?;

    let limits = Default::default();
    match initiate_session(&mut stream, &hello, &limits)
        .await
        .map_err(|e| format!("handshake: {e}"))?
    {
        SessionReply::Reject { reason } => {
            println!("  refused at admission: {reason:?}");
            return Ok(Vec::new());
        }
        SessionReply::Accept { .. } => println!("  admitted"),
    }

    // The session plane an authenticated carrier can actually answer.
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();

    let mut answers = Vec::new();
    for (id, body) in script {
        let mut line =
            serde_json::to_vec(&CarrierRequest { id, body }).map_err(|e| format!("encode: {e}"))?;
        line.push(b'\n');
        writer
            .write_all(&line)
            .await
            .map_err(|e| format!("write: {e}"))?;
        writer.flush().await.map_err(|e| format!("flush: {e}"))?;

        match lines.next_line().await.map_err(|e| format!("read: {e}"))? {
            Some(response) => {
                let decoded: CarrierResponse =
                    serde_json::from_str(&response).map_err(|e| format!("decode: {e}"))?;
                match &decoded.body {
                    Ok(body) => println!("  #{id} -> {}", summarize(body)),
                    Err(failure) => println!("  #{id} -> refused: {}", failure.message),
                }
                answers.push(decoded);
            }
            None => {
                println!("  #{id} -> the endpoint closed without answering");
                break;
            }
        }
    }
    Ok(answers)
}
