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

use std::time::{SystemTime, UNIX_EPOCH};

use graphshell::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE, open_session};
use graphshell::canary::FixtureEndpoint;
use graphshell::carrier::{accept_projection_session, projection_alpn, projection_policy};
use graphshell::lifecycle::SessionAuthority;
use graphshell::session_loop::serve_admitted_session;
use graphshell_protocol::{
    CapabilityProfile, CarrierRequest, CarrierRequestBody, CarrierResponse, ProtocolVersion,
    SessionOpen,
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
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use transport::p2panda_transport::P2pandaTransport;
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
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--peer" => peer_ticket = args.next(),
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
        "connect" => {
            let ticket = peer_ticket.ok_or("connect needs --peer <ticket>")?;
            connect(owner, me, seed, network, ticket).await
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
    let carrier = P2pandaTransport::builder_from_seed(seed)
        .alpns(vec![projection_alpn()])
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
        println!("  the peer's grant will be revoked before its first request");
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
    let mut ledger = RevocationLedger::new();

    // The accept task must not own the carrier; see `graphshell::carrier`.
    let outcome = accept_projection_session(&carrier, &policy, &ledger, now_ms(), 0)
        .await
        .map_err(|e| format!("accept: {e}"))?;
    let mut session = match outcome {
        Ok(session) => session,
        Err(refusal) => {
            println!("  refused: {refusal:?}");
            return Ok(());
        }
    };
    println!(
        "  admitted subject {} for {}",
        hex8(&session.principal.subject),
        session.principal.action.action
    );

    // `retain_admitted`, not `retain`: carrier code takes the whole admitted
    // session so it cannot accidentally drop the chain the conclusion was
    // drawn from, which is what leaves a session blind to later revocation.
    let authority = SessionAuthority::retain_admitted(&session);

    // Revoked before the loop, so the ledger the loop consults already carries
    // it. A real owner revokes out of band; the effect on this session is the
    // same, because the check runs per request rather than at admission.
    if revoked {
        if let Some(certificate) = session.claims.delegations.first() {
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
                ledger.fold(&statement),
                "the owner's own revocation verifies"
            );
            println!("  grant revoked");
        }
    }

    let mut endpoint = FixtureEndpoint::new();
    let mut resume = |_: &mut FixtureEndpoint, _| Err("this fixture does not resume".to_string());
    let summary = serve_admitted_session(
        &mut session,
        &authority,
        &ledger,
        &mut endpoint,
        &mut resume,
        now_ms,
    )
    .await
    .map_err(|e| format!("serve: {e}"))?;

    println!(
        "  served {} request(s); ended {:?}",
        summary.answered, summary.end
    );
    Ok(())
}

async fn connect(
    owner: InMemoryProvider,
    me: InMemoryProvider,
    seed: [u8; 32],
    network: NetworkId,
    ticket: String,
) -> Result<(), String> {
    let carrier = P2pandaTransport::builder_from_seed(seed)
        .alpns(vec![projection_alpn()])
        .bind()
        .await
        .map_err(|e| format!("bind: {e}"))?;
    assert_same_key(&carrier, &me)?;

    let peer = carrier
        .add_peer_ticket(&ticket)
        .await
        .map_err(|e| format!("ticket: {e}"))?;
    println!("g5_peer connect");
    println!("  dialling {}", hex8(&peer.to_bytes()));

    let mut stream = carrier
        .connect(peer, projection_alpn())
        .await
        .map_err(|e| format!("connect: {e}"))?;

    let subject = me.master_public_key().to_bytes();
    let local = transport::PeerID::from_bytes(&subject).map_err(|e| format!("peer id: {e}"))?;
    let binding = initiator_binding(&projection_alpn(), local);
    let hello = open_session(
        &me,
        network,
        profile(),
        TrafficClass::Interactive,
        [5; 32],
        &binding,
        vec![grant(&owner, subject, network, now_ms() + 3_600_000)],
    )
    .map_err(|e| format!("hello: {e}"))?;

    let limits = Default::default();
    match initiate_session(&mut stream, &hello, &limits)
        .await
        .map_err(|e| format!("handshake: {e}"))?
    {
        SessionReply::Reject { reason } => {
            println!("  refused at admission: {reason:?}");
            return Ok(());
        }
        SessionReply::Accept { .. } => println!("  admitted"),
    }

    // The session plane an authenticated carrier can actually answer.
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();

    let script = [
        (
            1,
            CarrierRequestBody::Open(Box::new(SessionOpen {
                version: ProtocolVersion::V1,
                capabilities: CapabilityProfile::default(),
            })),
        ),
        (2, CarrierRequestBody::Discover),
        (3, CarrierRequestBody::Close),
    ];

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
            }
            None => {
                println!("  #{id} -> the endpoint closed without answering");
                break;
            }
        }
    }
    Ok(())
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
        B::Resume(_) => "resume".to_string(),
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
