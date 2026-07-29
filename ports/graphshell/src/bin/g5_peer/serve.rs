//! The serving half: accept admitted sessions until the peer stops returning.
//!
//! `--revoked` folds a revocation of the peer's own grant before the loop
//! starts, so the next request it makes is refused with the reason and the
//! session ends. Mid-session revocation is not simulated here: the loop takes
//! the ledger by reference, so a revocation arriving *during* a session needs
//! a shared ledger the loop can re-read. The refusal path itself is identical
//! either way, because the check is per request rather than per connection.

use std::sync::RwLock;

use graphshell::carrier::{accept_projection_session, projection_alpn, projection_policy};
use graphshell::lifecycle::SessionAuthority;
use graphshell::resume::ResumeFixtureEndpoint;
use graphshell::session_loop::serve_admitted_session;
use graphshell_endpoint::ResumableProjectionSource;
use notochord::{NetworkId, RevocationLedger, TrustedRoot};
use personae::delegation::{DelegationRevocation, SignedDelegationRevocation};
use personae::{IdentityProvider, InMemoryProvider};
use transport::p2panda_transport::{MdnsDiscoveryMode, P2pandaTransport};

use crate::ROOT_AUTHORITY;
use crate::identity::{assert_same_key, hex8, now_ms, profile};

pub(crate) async fn serve(
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
