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
//!
//! env:
//!   G5_OWNER    shared secret naming the owner that grants projections;
//!               both devices set the same value
//!   G5_SEED     this device's identity seed; distinct per device
//!   G5_NETWORK  shared name for the network the policy governs
//! ```
//!
//! `--revoked` folds a revocation of the peer's own grant before the loop
//! starts, so the next request it makes is refused with the reason and the
//! session ends. Mid-session revocation is not simulated here: the loop takes
//! the ledger by reference, so a revocation arriving *during* a session needs
//! a shared ledger the loop can re-read. The refusal path itself is identical
//! either way, because the check is per request rather than per connection.
//!
//! ## The rehearsal shortcut, stated plainly
//!
//! Both sides derive the owner keypair from `G5_OWNER`, so the connecting side
//! mints its own grant. A real deployment issues that certificate out of band
//! and the viewer never holds the owner key. What this proves is the transport,
//! admission, session-loop, and revocation path across two machines; it does
//! not prove anything about how a grant is distributed. `--revoked` stays
//! honest either way: revocation is by certificate id, so the server's refusal
//! is the same one a real owner would cause.

use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use graphshell::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE, open_session};
use graphshell::carrier::{accept_projection_session, projection_alpn, projection_policy};
use graphshell::lifecycle::SessionAuthority;
use graphshell::resume::ResumeFixtureEndpoint;
use graphshell::session_loop::serve_admitted_session;
use graphshell_endpoint::ResumableProjectionSource;
use graphshell_protocol::{
    CapabilityProfile, CarrierRequest, CarrierRequestBody, CarrierResponse, IntentInvocation,
    ProjectionSession, ProtocolVersion, ResumeRequest, Revision, SceneEpoch, SessionOpen,
};
use notochord::{
    NetworkId, ProfileRef, RevocationLedger, SessionReply, TrafficClass, TrustedRoot,
    initiate_session,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
    SignedDelegationCertificate, SignedDelegationRevocation,
};
use personae::{IdentityProvider, InMemoryProvider};
use sceno::{Arrangement, InstanceId, Score};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use transport::p2panda_transport::{MdnsDiscoveryMode, P2pandaTransport};
use transport::{Transport, initiator_binding};

const ROOT_AUTHORITY: [u8; 32] = [7; 32];

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

fn profile() -> ProfileRef {
    ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }
}

/// The grant the owner issues for projections on `network`.
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
            // Back-dated a minute, and `not_before` matches `issued_at`
            // because the rule is `issued_at <= not_before`. The minute is
            // for clock skew: these two processes are on different machines
            // and the responder judges validity by *its* clock, so a
            // certificate stamped "valid from now" by a client whose clock
            // runs slightly ahead is not yet valid to the server.
            now_ms().saturating_sub(60_000),
            now_ms().saturating_sub(60_000),
            Some(expires_at_ms),
            1,
            [11; 32],
        ),
    )
    .expect("issue certificate")
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), String> {
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

async fn serve(
    owner: InMemoryProvider,
    me: InMemoryProvider,
    seed: [u8; 32],
    network: NetworkId,
    revoked: bool,
) -> Result<(), String> {
    // mDNS on: peers on this LAN populate each other's address books without a
    // pasted ticket. The ticket is still printed, because discovery is a
    // convenience and a hand-carried ticket is the fallback that always works
    // (and the only one that works across networks).
    let carrier = P2pandaTransport::builder_from_seed(seed)
        .alpns(vec![projection_alpn()])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .map_err(|e| format!("bind: {e}"))?;
    assert_same_key(&carrier, &me)?;

    let ticket = carrier.ticket().await.map_err(|e| format!("ticket: {e}"))?;
    println!("g5_peer serve");
    println!("  ticket: {ticket}");
    println!("  run on the other device:");
    println!("    g5_peer connect --peer {ticket}");
    if revoked {
        println!("  the grant will be revoked after session 2");
    }
    println!("  waiting for a peer...");

    let policy = projection_policy(
        network,
        vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: owner.master_public_key().to_bytes(),
        }],
        vec![profile()],
        None,
    );
    // Shared, because the session loop re-reads it per request: an owner who
    // revokes mid-session is answered at the next request rather than at the
    // next reconnect.
    let revocations = RwLock::new(RevocationLedger::new());

    // One endpoint across every session, because that is what makes a resume
    // a resume: its diff history and current revision have to outlive the
    // connection that dropped. A fresh endpoint per session could only ever
    // answer with a fresh snapshot.
    let mut endpoint = ResumeFixtureEndpoint::new();

    // Serve sessions until the peer stops coming back. The accept task must
    // not own the carrier; see `graphshell::carrier`.
    for attempt in 1.. {
        println!("  waiting for session {attempt}...");
        let outcome = {
            let ledger = revocations.read().expect("ledger lock");
            accept_projection_session(&carrier, &policy, &ledger, now_ms(), 0).await
        }
        .map_err(|e| format!("accept: {e}"))?;
        let mut session = match outcome {
            Ok(session) => session,
            Err(refusal) => {
                println!("  refused: {refusal:?}");
                return Ok(());
            }
        };
        println!(
            "  session {attempt}: admitted subject {} for {}",
            hex8(&session.principal.subject),
            session.principal.action.action
        );

        // `retain_admitted`, not `retain`: carrier code takes the whole
        // admitted session so it cannot accidentally drop the chain the
        // conclusion was drawn from, which is what leaves a session blind to
        // later revocation.
        let authority = SessionAuthority::retain_admitted(&session);

        // Revoked before the loop, so the ledger the loop consults already
        // carries it. A real owner revokes out of band; the effect is the
        // Revoked after session 2, so the peer's *next* request is the one
        // refused. Session 3's first request is an `IntentInvocation`, which
        // is the verb G5's done-when actually names: the earlier arrangement
        // refused whatever came first, and that was always an `Open`.
        if revoked
            && attempt == 3
            && let Some(certificate) = session.claims.delegations.first()
        {
            let statement = SignedDelegationRevocation::issue(
                &owner,
                DelegationRevocation::new(
                    certificate.certificate.id(),
                    owner.master_public_key().to_bytes(),
                    certificate.certificate.scope.clone(),
                    now_ms(),
                    [12; 32],
                ),
            )
            .expect("issue revocation");
            assert!(
                revocations.write().expect("ledger lock").fold(&statement),
                "the owner's own revocation verifies"
            );
            println!("  grant revoked");
        }

        let mut resume = |endpoint: &mut ResumeFixtureEndpoint, request| {
            ResumableProjectionSource::resume(endpoint, request).map_err(|error| error.to_string())
        };
        let summary = serve_admitted_session(
            &mut session,
            &authority,
            &revocations,
            &mut endpoint,
            &mut resume,
            now_ms,
        )
        .await
        .map_err(|e| format!("serve: {e}"))?;

        println!(
            "  session {attempt}: served {} request(s); ended {:?}",
            summary.answered, summary.end
        );
        if matches!(summary.end, graphshell::session_loop::SessionEnd::Lapsed(_)) {
            println!("  authority lapsed; not accepting further sessions");
            return Ok(());
        }
    }
    Ok(())
}

/// How this run learned which peer to dial.
///
/// The distinction matters and is easy to overstate. p2panda's mDNS populates
/// the address book so a `connect` to a **known** peer id succeeds without an
/// explicit `add_peer`. It does **not** answer "who is on this LAN":
/// `mere-transport` exposes no way to enumerate what discovery found, so
/// `Discovered` still derives the peer id from a shared name. mDNS removes the
/// need to exchange an *address*, not the need to know *who*.
enum PeerSource {
    /// A ticket carried by hand: id and address together, and the only form
    /// that works off this LAN.
    Ticket(String),
    /// A peer id known in advance, with mDNS expected to resolve its address.
    Discovered(transport::PeerID),
}

async fn connect(
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

fn intent_body() -> CarrierRequestBody {
    CarrierRequestBody::Intent(IntentInvocation {
        session: ProjectionSession(RESUME_SESSION.into()),
        target: InstanceId(1),
        observed_epoch: SceneEpoch(3),
        observed_revision: Revision(3),
        intent: "fixture.inspect".to_string(),
        payload: Vec::new(),
    })
}

/// The endpoint's own projection session, which is independent of admission:
/// two separate admissions resume the same projection, which is what makes an
/// interruption survivable at all.
const RESUME_SESSION: &str = "loopback:g2-resume";

fn open_body() -> CarrierRequestBody {
    CarrierRequestBody::Open(Box::new(SessionOpen {
        version: ProtocolVersion::V1,
        capabilities: CapabilityProfile::default(),
    }))
}

fn projection_request() -> graphshell_protocol::ProjectionRequest {
    graphshell_protocol::ProjectionRequest {
        version: ProtocolVersion::V1,
        session: ProjectionSession(RESUME_SESSION.into()),
        score: Score::new(Arrangement::Spiral(Default::default())),
    }
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
    let mut stream = carrier
        .connect(peer, projection_alpn())
        .await
        .map_err(|e| format!("connect: {e}"))?;

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

fn summarize(body: &graphshell_protocol::CarrierResponseBody) -> String {
    use graphshell_protocol::CarrierResponseBody as B;
    match body {
        B::Opened(opened) => format!(
            "opened, status {:?}, expires {:?}",
            opened.status, opened.expires_at_ms
        ),
        B::Descriptor(descriptor) => format!(
            "descriptor {:?} with {} projection(s)",
            descriptor.label,
            descriptor.projections.len()
        ),
        B::Snapshot(snapshot) => {
            format!("snapshot of {} item(s)", snapshot.scene.active_item_count())
        }
        B::Resource(_) => "resource".to_string(),
        // Which reply matters: replayed diffs are the thing that makes this a
        // resume rather than a reconnect that started over.
        B::Resume(reply) => match reply {
            graphshell_protocol::ResumeReply::Diffs(diffs) => format!(
                "resumed by replaying {} contiguous diff(s), revisions {}",
                diffs.len(),
                diffs
                    .iter()
                    .map(|d| format!("{}->{}", d.scene.base.0, d.scene.revision.0))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            graphshell_protocol::ResumeReply::Snapshot(snapshot) => format!(
                "resumed by full snapshot at revision {} (history could not bridge the gap)",
                snapshot.scene.revision.0
            ),
            graphshell_protocol::ResumeReply::Current(ack) => {
                format!("already current at revision {}", ack.revision.0)
            }
        },
        B::Intent(result) => format!("intent {result:?}"),
        B::Closed => "closed".to_string(),
        B::Suspended => "suspended".to_string(),
    }
}

/// The carrier and the Personae identity must be the same key.
///
/// D6 requires the claimed subject to *be* the peer the carrier authenticated,
/// and the proof binding is derived from that identity on both sides. If these
/// two ever diverge the failure surfaces far away, as `SessionProofInvalid`
/// during admission, so it is asserted here where the cause is visible.
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

fn hex8(bytes: &[u8; 32]) -> String {
    bytes.iter().take(4).map(|b| format!("{b:02x}")).collect()
}
