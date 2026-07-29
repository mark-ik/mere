//! Shared identity and admission fixtures for the two-machine H6 proof.
//!
//! The connecting side derives the owner key so it can issue its own short
//! grant. This proves carrier admission and revocation, not grant delivery.

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
            now_ms().saturating_sub(60_000),
            now_ms().saturating_sub(60_000),
            Some(expires_at_ms),
            1,
            [11; 32],
        ),
    )
    .expect("issue certificate")
}

pub(crate) fn assert_same_key(
    carrier: &P2pandaTransport,
    me: &InMemoryProvider,
) -> Result<(), String> {
    let carried = carrier.local_peer_id().to_bytes();
    let claimed = me.master_public_key().to_bytes();
    if carried != claimed {
        return Err("carrier identity and Personae identity diverged".to_string());
    }
    Ok(())
}

pub(crate) fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn short(bytes: &[u8; 32]) -> String {
    bytes
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn persona(owner: &InMemoryProvider) -> String {
    format!(
        "personae://persona/{}",
        hex(&owner.master_public_key().to_bytes())
    )
}

pub(crate) fn device(key: &[u8; 32]) -> String {
    format!("personae://device/{}", hex(key))
}

pub(crate) fn graph(key: &[u8; 32]) -> String {
    format!("graphshell://graph/{}", hex(key))
}
