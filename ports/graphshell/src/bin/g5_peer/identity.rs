//! Identity, clock, and the grant this rehearsal mints for itself.
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

use graphshell::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
use notochord::{NetworkId, ProfileRef};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use personae::{IdentityProvider, InMemoryProvider};
use transport::Transport;
use transport::p2panda_transport::P2pandaTransport;

use crate::ROOT_AUTHORITY;

pub(crate) fn env_hash(var: &str) -> Result<[u8; 32], String> {
    let value = std::env::var(var).map_err(|_| format!("set {var} (any string)"))?;
    Ok(*blake3::hash(value.as_bytes()).as_bytes())
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_millis() as u64
}

pub(crate) fn profile() -> ProfileRef {
    ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }
}

/// The grant the owner issues for projections on `network`.
pub(crate) fn grant(
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

/// The carrier and the Personae identity must be the same key.
///
/// D6 requires the claimed subject to *be* the peer the carrier authenticated,
/// and the proof binding is derived from that identity on both sides. If these
/// two ever diverge the failure surfaces far away, as `SessionProofInvalid`
/// during admission, so it is asserted here where the cause is visible.
pub(crate) fn assert_same_key(
    carrier: &P2pandaTransport,
    me: &InMemoryProvider,
) -> Result<(), String> {
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

pub(crate) fn hex8(bytes: &[u8; 32]) -> String {
    bytes.iter().take(4).map(|b| format!("{b:02x}")).collect()
}
